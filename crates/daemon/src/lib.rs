use std::{fmt::Write as _, env, fs, io::{Read, Write}, os::unix::net::{UnixListener, UnixStream}, path::PathBuf, thread, sync::{Arc, Mutex}};

use clipdash_core::{history::{History, HistoryConfig}, Item, ItemKind};
use clipdash_store::FileStore;

pub struct State {
    pub history: History,
    persist: Option<FileStore>,
}

impl State {
    pub fn new_default() -> Self {
        Self { history: History::with_config(HistoryConfig::default()), persist: None }
    }

    pub fn with_file_persist(path: PathBuf) -> Self {
        let fs = FileStore::new(&path);
        let mut s = Self { history: History::with_config(HistoryConfig::default()), persist: Some(fs) };
        // try load existing
        if let Some(store) = &s.persist {
            if let Ok(items) = store.load() { s.history.rebuild_from(items); }
        }
        s
    }

    fn persist_if_needed(&self) {
        if let Some(store) = &self.persist { let _ = store.save(self.history.all()); }
    }

    /// Handle a single line command and return a response string.
    /// Protocol (demo):
    /// - ADD_TEXT <text> -> OK <id> | ERR <msg>
    /// - LIST <limit> [query] -> OK <n>\n<id>\t<kind>\t<pinned>\t<title> ... | ERR
    /// - GET <id> -> TEXT\n<content> | ERR <msg>
    /// - PIN <id> <0|1> -> OK | ERR
    /// - DELETE <id> -> OK | ERR
    /// - CLEAR -> OK
    pub fn handle_command(&mut self, line: &str) -> String {
        let mut parts = line.trim_end().splitn(3, ' ');
        let cmd = parts.next().unwrap_or("").to_uppercase();
        match cmd.as_str() {
            "ADD_TEXT" => {
                let text = parts.next().unwrap_or("");
                let id = self.history.try_push(Item { id: 0, kind: ItemKind::Text, data: text.as_bytes().to_vec(), pinned: false });
                match id { Some(id) => { self.persist_if_needed(); format!("OK {}", id) }, None => format!("ERR text too large") }
            }
            "LIST" => {
                let lim_s = parts.next().unwrap_or("50");
                let limit: usize = lim_s.parse().unwrap_or(50);
                let query = parts.next().unwrap_or("");
                let q = query.to_lowercase();
                let items = self.history.all();
                let mut out = String::new();
                let mut rows = Vec::new();
                for it in items.iter().rev() { // most recent first
                    if q.is_empty() || matches_query(it, &q) {
                        rows.push((it.id, &it.kind, it.pinned, it.title()));
                        if rows.len() == limit { break; }
                    }
                }
                let _ = write!(&mut out, "OK {}\n", rows.len());
                for (id, kind, pinned, title) in rows {
                    let k = match kind { ItemKind::Text => "Text", ItemKind::Image => "Image", ItemKind::Html => "Html" };
                    let _ = write!(&mut out, "{}\t{}\t{}\t{}\n", id, k, if pinned {1}else{0}, title);
                }
                out
            }
            "GET" => {
                if let Some(id_s) = parts.next() {
                    if let Ok(id) = id_s.parse::<u64>() {
                        if let Some(it) = self.history.all().iter().find(|i| i.id==id) {
                            return match it.kind {
                                ItemKind::Text => format!("TEXT\n{}", String::from_utf8_lossy(&it.data)),
                                _ => "ERR unsupported kind".into(),
                            };
                        }
                    }
                }
                "ERR not found".into()
            }
            "PIN" => {
                let id = parts.next().and_then(|s| s.parse::<u64>().ok());
                let pv = parts.next().and_then(|s| s.parse::<u8>().ok());
                match (id, pv) {
                    (Some(id), Some(v)) => { self.history.pin(id, v!=0); self.persist_if_needed(); "OK".into() }
                    _ => "ERR invalid args".into()
                }
            }
            "DELETE" => {
                if let Some(id) = parts.next().and_then(|s| s.parse::<u64>().ok()) {
                    if self.history.delete(id) { self.persist_if_needed(); "OK".into() } else { "ERR not found".into() }
                } else { "ERR invalid args".into() }
            }
            "CLEAR" => { self.history.clear(); self.persist_if_needed(); "OK".into() }
            _ => "ERR unknown".into(),
        }
    }
}

fn socket_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = PathBuf::from(home).join(".cache/clipdash");
    fs::create_dir_all(&dir).ok();
    dir.join("daemon.sock")
}

fn data_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = PathBuf::from(home).join(".local/share/clipdash");
    fs::create_dir_all(&dir).ok();
    dir.join("history.v1")
}

fn handle_client(mut stream: UnixStream, state: &Arc<Mutex<State>>) {
    let mut buf = String::new();
    let _ = stream.read_to_string(&mut buf);
    if let Some(line) = buf.lines().next() {
        let resp = state.lock().unwrap().handle_command(line);
        let _ = stream.write_all(resp.as_bytes());
    }
}

pub fn run_server_forever() {
    let path = socket_path();
    if path.exists() { let _ = fs::remove_file(&path); }
    let listener = UnixListener::bind(&path).expect("bind unix socket");
    println!("clipdashd: listening on {}", path.display());
    let state = Arc::new(Mutex::new(State::with_file_persist(data_path())));
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let st = state.clone();
                thread::spawn(move || handle_client(stream, &st));
            }
            Err(e) => eprintln!("conn error: {}", e),
        }
    }
}

fn matches_query(it: &Item, q: &str) -> bool {
    match it.kind {
        ItemKind::Text => {
            let s = String::from_utf8_lossy(&it.data).to_lowercase();
            s.contains(q)
        }
        _ => it.title().to_lowercase().contains(q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_list_get_flow() {
        let mut s = State::new_default();
        let r = s.handle_command("ADD_TEXT hello");
        assert!(r.starts_with("OK "));
        let list = s.handle_command("LIST 10");
        assert!(list.starts_with("OK 1\n"));
        let id: u64 = list.lines().nth(1).unwrap().split('\t').next().unwrap().parse().unwrap();
        let got = s.handle_command(&format!("GET {}", id));
        assert_eq!(got, "TEXT\nhello");
    }
}
