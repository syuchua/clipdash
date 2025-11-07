use std::{fmt::Write as _, env, fs, io::{Write, BufRead, BufReader}, os::unix::net::{UnixListener, UnixStream}, path::PathBuf, thread, sync::{Arc, Mutex}};
use clipdash_backend::ClipKind;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

use clipdash_core::{history::{History, HistoryConfig}, Item, ItemKind};
use clipdash_store::FileStore;

#[derive(Clone, Debug)]
struct DaemonConfig {
    // watch toggles
    watch_text: bool,
    watch_html: bool,
    watch_image: bool,
    // history
    max_items: usize,
    max_text_bytes: usize,
    max_image_bytes: usize,
    ttl_secs: u64,
    // cache quotas
    cache_images_max_bytes: u64,
    cache_html_max_bytes: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            watch_text: true,
            watch_html: true,
            watch_image: true,
            max_items: 200,
            max_text_bytes: 100_000,
            max_image_bytes: 2_000_000,
            ttl_secs: 0,
            cache_images_max_bytes: 100 * 1024 * 1024,
            cache_html_max_bytes: 50 * 1024 * 1024,
        }
    }
}

fn config_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/clipdash/config.toml")
}

fn load_config() -> DaemonConfig {
    let mut cfg = DaemonConfig::default();
    if let Ok(s) = fs::read_to_string(config_path()) {
        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let mut p = line.splitn(2, '=');
            let k = p.next().map(|v| v.trim()).unwrap_or("");
            let v = p.next().map(|v| v.trim()).unwrap_or("");
            let v_str = v.trim_matches('"');
            match k.to_ascii_lowercase().as_str() {
                "watch.text" => { cfg.watch_text = matches!(v_str, "1"|"true"|"on"|"yes"); }
                "watch.html" => { cfg.watch_html = matches!(v_str, "1"|"true"|"on"|"yes"); }
                "watch.image" => { cfg.watch_image = matches!(v_str, "1"|"true"|"on"|"yes"); }
                "history.max_items" => { if let Ok(n) = v_str.parse::<usize>() { cfg.max_items = n.max(10).min(10_000); } }
                "history.max_text_bytes" => { if let Ok(n) = v_str.parse::<usize>() { cfg.max_text_bytes = n.max(1024).min(10_000_000); } }
                "history.max_image_bytes" => { if let Ok(n) = v_str.parse::<usize>() { cfg.max_image_bytes = n.max(10_000).min(200_000_000); } }
                "history.ttl_secs" => { if let Ok(n) = v_str.parse::<u64>() { cfg.ttl_secs = n; } }
                "cache.images.max_bytes" => { if let Ok(n) = v_str.parse::<u64>() { cfg.cache_images_max_bytes = n.max(1_000_000).min(10_000_000_000); } }
                "cache.html.max_bytes" => { if let Ok(n) = v_str.parse::<u64>() { cfg.cache_html_max_bytes = n.max(1_000_000).min(10_000_000_000); } }
                _ => {}
            }
        }
    }
    cfg
}

pub struct State {
    pub history: History,
    persist: Option<FileStore>,
}

impl State {
    pub fn new_default() -> Self {
        Self { history: History::with_config(HistoryConfig::default()), persist: None }
    }

    pub fn with_file_persist(path: PathBuf, cfg: &DaemonConfig) -> Self {
        let fs = FileStore::new(&path);
        let mut s = Self { history: History::with_config(HistoryConfig {
            max_items: cfg.max_items,
            max_text_bytes: cfg.max_text_bytes,
            max_image_bytes: cfg.max_image_bytes,
            ttl_secs: cfg.ttl_secs,
        }), persist: Some(fs) };
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
                let id = self.history.try_push(Item { id: 0, kind: ItemKind::Text, data: text.as_bytes().to_vec(), pinned: false, ts_ms: 0, mime: Some("text/plain".into()), file_path: None });
                match id { Some(id) => { self.persist_if_needed(); format!("OK {}", id) }, None => format!("ERR text too large") }
            }
            "ADD_HTML" => {
                let html = parts.next().unwrap_or("");
                let id = self.history.try_push(Item { id: 0, kind: ItemKind::Html, data: html.as_bytes().to_vec(), pinned: false, ts_ms: 0, mime: Some("text/html".into()), file_path: None });
                match id { Some(id) => { self.persist_if_needed(); format!("OK {}", id) }, None => format!("ERR too large") }
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
                        rows.push((it.id, &it.kind, it.pinned, it.title(), it.mime.as_deref().unwrap_or(match it.kind { ItemKind::Text => "text/plain", ItemKind::Html => "text/html", ItemKind::Image => "image/png" })));
                        if rows.len() == limit { break; }
                    }
                }
                let _ = write!(&mut out, "OK {}\n", rows.len());
                for (id, kind, pinned, title, mime) in rows {
                    let k = match kind { ItemKind::Text => "Text", ItemKind::Image => "Image", ItemKind::Html => "Html" };
                    let _ = write!(&mut out, "{}\t{}\t{}\t{}\t{}\n", id, k, if pinned {1}else{0}, title, mime);
                }
                out
            }
            "GET" => {
                if let Some(id_s) = parts.next() {
                    if let Ok(id) = id_s.parse::<u64>() {
                        if let Some(it) = self.history.all().iter().find(|i| i.id==id) {
                            return match it.kind {
                                ItemKind::Text => format!("TEXT\n{}", String::from_utf8_lossy(&it.data)),
                                ItemKind::Html => format!("HTML\n{}", String::from_utf8_lossy(&it.data)),
                                ItemKind::Image => {
                                    let mime = it.mime.as_deref().unwrap_or("image/png");
                                    let bytes = if it.data.is_empty() {
                                        if let Some(path) = &it.file_path { std::fs::read(path).unwrap_or_default() } else { Vec::new() }
                                    } else { it.data.clone() };
                                    let b64 = B64.encode(&bytes);
                                    format!("IMAGE\n{}\n{}", mime, b64)
                                }
                            };
                        }
                    }
                }
                "ERR not found".into()
            }
            "PASTE" => {
                if let Some(id_s) = parts.next() {
                    if let Ok(id) = id_s.parse::<u64>() {
                        if let Some(it) = self.history.all().iter().find(|i| i.id==id) {
                            return match it.kind {
                                ItemKind::Text => match write_clipboard_text(&String::from_utf8_lossy(&it.data)) { Ok(()) => "OK".into(), Err(e) => format!("ERR {}", e) },
                                ItemKind::Html => {
                                    let mut html = String::from_utf8_lossy(&it.data).to_string();
                                    if html.is_empty() {
                                        if let Some(path) = &it.file_path { if let Ok(s) = std::fs::read_to_string(path) { html = s; } }
                                    }
                                    match write_clipboard_html(&html) { Ok(()) => "OK".into(), Err(e) => format!("ERR {}", e) }
                                },
                                ItemKind::Image => {
                                    let mime = it.mime.as_deref().unwrap_or("image/png");
                                    let mut bytes = it.data.clone();
                                    if bytes.is_empty() { if let Some(path) = &it.file_path { if let Ok(b) = std::fs::read(path) { bytes = b; } } }
                                    match write_clipboard_image(&bytes, mime) { Ok(()) => "OK".into(), Err(e) => format!("ERR {}", e) }
                                },
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

fn cache_root() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = PathBuf::from(home).join(".local/share/clipdash/cache");
    fs::create_dir_all(&dir).ok();
    dir
}

fn cleanup_cache_dir(dir: &PathBuf, max_bytes: u64) {
    if let Ok(read) = fs::read_dir(dir) {
        let mut files: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
        for e in read.flatten() {
            let p = e.path();
            if let Ok(meta) = fs::metadata(&p) { if meta.is_file() {
                let sz = meta.len();
                let mt = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                files.push((p, sz, mt));
            }}
        }
        let mut total: u64 = files.iter().map(|(_, sz, _)| *sz).sum();
        if total > max_bytes {
            files.sort_by_key(|(_, _, mt)| *mt); // oldest first
            for (p, sz, _) in files {
                if total <= max_bytes { break; }
                let _ = fs::remove_file(&p);
                total = total.saturating_sub(sz);
            }
        }
    }
}

fn handle_client(mut stream: UnixStream, state: &Arc<Mutex<State>>) {
    // Read a single line command to avoid read-to-EOF deadlocks
    let mut line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        if reader.read_line(&mut line).is_err() {
            return;
        }
    }
    let resp = state.lock().unwrap().handle_command(line.trim_end());
    let _ = stream.write_all(resp.as_bytes());
}

pub fn run_server_forever() {
    let cfg = load_config();
    let path = socket_path();
    if path.exists() { let _ = fs::remove_file(&path); }
    let listener = UnixListener::bind(&path).expect("bind unix socket");
    println!("clipdashd: listening on {}", path.display());
    let state = Arc::new(Mutex::new(State::with_file_persist(data_path(), &cfg)));
    // Cleanup caches on startup (100MB images, 50MB html)
    let root = cache_root();
    let img_dir = root.join("images"); let _ = fs::create_dir_all(&img_dir);
    let html_dir = root.join("html"); let _ = fs::create_dir_all(&html_dir);
    cleanup_cache_dir(&img_dir, cfg.cache_images_max_bytes);
    cleanup_cache_dir(&html_dir, cfg.cache_html_max_bytes);
    // spawn clipboard watcher (best-effort)
    spawn_clipboard_watcher(state.clone(), cfg.clone());
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

fn have_cmd(cmd: &str) -> bool {
    std::process::Command::new(cmd).arg("--version").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok()
}

fn read_clipboard_text() -> Option<String> {
    // Try Wayland wl-paste first
    if have_cmd("wl-paste") {
        if let Ok(out) = std::process::Command::new("wl-paste").arg("--no-newline").output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).to_string();
                if !s.is_empty() { return Some(s); }
            }
        }
    }
    // Fallback to xclip
    if have_cmd("xclip") {
        if let Ok(out) = std::process::Command::new("xclip").args(["-selection","clipboard","-out"]).output() {
            if out.status.success() {
                let mut s = String::from_utf8_lossy(&out.stdout).to_string();
                if s.ends_with('\n') { s.pop(); }
                if !s.is_empty() { return Some(s); }
            }
        }
    }
    None
}

fn write_clipboard_text(text: &str) -> std::io::Result<()> {
    // Try Wayland wl-copy
    if have_cmd("wl-copy") {
        let mut child = std::process::Command::new("wl-copy")
            .args(["--type","text/plain;charset=utf-8"])
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        if let Some(stdin) = child.stdin.as_mut() { stdin.write_all(text.as_bytes())?; }
        let status = child.wait()?;
        if status.success() { return Ok(()); }
    }
    // Fallback to xclip
    if have_cmd("xclip") {
        let mut child = std::process::Command::new("xclip")
            .args(["-selection","clipboard","-in"]) 
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        if let Some(stdin) = child.stdin.as_mut() { stdin.write_all(text.as_bytes())?; }
        let status = child.wait()?;
        if status.success() { return Ok(()); }
    }
    Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no clipboard tool (wl-copy/xclip)"))
}

fn looks_like_html(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() { return false; }
    let lower = t.to_lowercase();
    if lower.starts_with("<!doctype html") { return true; }
    for key in ["<html", "<body", "<head", "<div", "<span", "<p", "<br", "</"] {
        if lower.contains(key) { return true; }
    }
    if let Some(idx) = t.find('<') {
        let rest = &t[idx+1..];
        if let Some(c) = rest.chars().next() {
            if c.is_ascii_alphabetic() && rest.contains('>') { return true; }
        }
    }
    false
}

fn read_clipboard_html() -> Option<String> {
    // Try Wayland first
    if have_cmd("wl-paste") {
        if let Ok(out) = std::process::Command::new("wl-paste").args(["--no-newline","--type","text/html"]).output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).to_string();
                if !s.is_empty() && looks_like_html(&s) { return Some(s); }
            }
        }
    }
    // Fallback xclip
    if have_cmd("xclip") {
        if let Ok(out) = std::process::Command::new("xclip").args(["-selection","clipboard","-o","-t","text/html"]).output() {
            if out.status.success() {
                let mut s = String::from_utf8_lossy(&out.stdout).to_string();
                if s.ends_with('\n') { s.pop(); }
                if !s.is_empty() && looks_like_html(&s) { return Some(s); }
            }
        }
    }
    None
}

fn is_valid_image_bytes(mime: &str, b: &[u8]) -> bool {
    if b.len() < 12 { return false; }
    match mime {
        m if m.contains("png") => {
            // PNG signature: 89 50 4E 47 0D 0A 1A 0A
            let sig = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
            b.starts_with(&sig)
        }
        m if m.contains("jpeg") || m.contains("jpg") => {
            // JPEG starts with FF D8 FF
            b.len() >= 3 && b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF
        }
        m if m.contains("webp") => {
            // WebP: RIFF....WEBP
            b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP"
        }
        _ => false,
    }
}

fn read_clipboard_image() -> Option<(Vec<u8>, String)> {
    // Try some common image types in order
    const MIMES: &[&str] = &["image/png", "image/jpeg", "image/webp"];
    if have_cmd("wl-paste") {
        for &m in MIMES {
            if let Ok(out) = std::process::Command::new("wl-paste").args(["--type", m]).output() {
                if out.status.success() && !out.stdout.is_empty() {
                    if is_valid_image_bytes(m, &out.stdout) { return Some((out.stdout, m.to_string())); }
                }
            }
        }
    }
    if have_cmd("xclip") {
        for &m in MIMES {
            if let Ok(out) = std::process::Command::new("xclip").args(["-selection","clipboard","-o","-t", m]).output() {
                if out.status.success() && !out.stdout.is_empty() {
                    if is_valid_image_bytes(m, &out.stdout) { return Some((out.stdout, m.to_string())); }
                }
            }
        }
    }
    None
}

fn write_clipboard_html(html: &str) -> std::io::Result<()> {
    if have_cmd("wl-copy") {
        let mut child = std::process::Command::new("wl-copy")
            .args(["--type","text/html"])
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        if let Some(stdin) = child.stdin.as_mut() { stdin.write_all(html.as_bytes())?; }
        let status = child.wait()?; if status.success() { return Ok(()); }
    }
    if have_cmd("xclip") {
        let mut child = std::process::Command::new("xclip")
            .args(["-selection","clipboard","-t","text/html","-in"]) 
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        if let Some(stdin) = child.stdin.as_mut() { stdin.write_all(html.as_bytes())?; }
        let status = child.wait()?; if status.success() { return Ok(()); }
    }
    Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no html clipboard tool"))
}

fn write_clipboard_image(bytes: &[u8], mime: &str) -> std::io::Result<()> {
    if have_cmd("wl-copy") {
        let mut child = std::process::Command::new("wl-copy")
            .args(["--type", mime])
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        if let Some(stdin) = child.stdin.as_mut() { stdin.write_all(bytes)?; }
        let status = child.wait()?; if status.success() { return Ok(()); }
    }
    if have_cmd("xclip") {
        let mut child = std::process::Command::new("xclip")
            .args(["-selection","clipboard","-t", mime, "-in"]) 
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        if let Some(stdin) = child.stdin.as_mut() { stdin.write_all(bytes)?; }
        let status = child.wait()?; if status.success() { return Ok(()); }
    }
    Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no image clipboard tool"))
}

fn spawn_clipboard_watcher(state: Arc<Mutex<State>>, cfg: DaemonConfig) {
    thread::spawn(move || {
        let mut last_kind: Option<ClipKind> = None;
        let mut last_bytes: Vec<u8> = Vec::new();
        loop {
            // Prefer image -> html -> text
            if cfg.watch_image {
                if let Some((bytes, mime)) = read_clipboard_image() {
                if !(matches!(last_kind, Some(ClipKind::Image)) && bytes == last_bytes) {
                    last_kind = Some(ClipKind::Image); last_bytes = bytes.clone();
                    // Decide inline or externalize by size
                    let cache_dir = cache_root().join("images");
                    let _ = fs::create_dir_all(&cache_dir);
                    let mut item = Item{ id:0, kind: ItemKind::Image, data: Vec::new(), pinned: false, ts_ms: 0, mime: Some(mime.clone()), file_path: None};
                    if bytes.len() <= 200_000 { // inline threshold ~200KB
                        item.data = bytes;
                    } else {
                        let ts = {
                            use std::time::{SystemTime, UNIX_EPOCH};
                            let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
                            (d.as_secs() as i64)*1000 + (d.subsec_millis() as i64)
                        };
                        let ext = if mime.contains("png") { "png" } else if mime.contains("jpeg") || mime.contains("jpg") { "jpg" } else if mime.contains("webp") { "webp" } else { "bin" };
                        let path = cache_dir.join(format!("img-{}.{}", ts, ext));
                        if std::fs::write(&path, &last_bytes).is_ok() { item.file_path = Some(path.to_string_lossy().to_string()); } else { item.data = bytes; }
                        cleanup_cache_dir(&cache_dir, cfg.cache_images_max_bytes);
                    }
                    let mut st = state.lock().unwrap();
                    let _ = st.history.try_push(item);
                    st.persist_if_needed();
                }
                }
            }
            if cfg.watch_html {
                if let Some(html) = read_clipboard_html() {
                let b = html.as_bytes().to_vec();
                if !(matches!(last_kind, Some(ClipKind::Html)) && b == last_bytes) {
                    last_kind = Some(ClipKind::Html); last_bytes = b.clone();
                    // Externalize large html
                    let cache_dir = cache_root().join("html");
                    let _ = fs::create_dir_all(&cache_dir);
                    let mut item = Item{ id:0, kind: ItemKind::Html, data: Vec::new(), pinned: false, ts_ms: 0, mime: Some("text/html".into()), file_path: None};
                    if b.len() <= 100_000 { item.data = b; } else {
                        let ts = {
                            use std::time::{SystemTime, UNIX_EPOCH};
                            let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
                            (d.as_secs() as i64)*1000 + (d.subsec_millis() as i64)
                        };
                        let path = cache_dir.join(format!("html-{}.html", ts));
                        if std::fs::write(&path, &last_bytes).is_ok() { item.file_path = Some(path.to_string_lossy().to_string()); } else { item.data = b; }
                        cleanup_cache_dir(&cache_dir, cfg.cache_html_max_bytes);
                    }
                    let mut st = state.lock().unwrap();
                    let _ = st.history.try_push(item);
                    st.persist_if_needed();
                }
                }
            }
            if cfg.watch_text {
                if let Some(s) = read_clipboard_text() {
                let b = s.as_bytes().to_vec();
                if !(matches!(last_kind, Some(ClipKind::Text)) && b == last_bytes) {
                    last_kind = Some(ClipKind::Text); last_bytes = b.clone();
                    let mut st = state.lock().unwrap();
                    let _ = st.history.try_push(Item{ id:0, kind: ItemKind::Text, data: b, pinned: false, ts_ms: 0, mime: Some("text/plain".into()), file_path: None});
                    st.persist_if_needed();
                }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
    });
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
