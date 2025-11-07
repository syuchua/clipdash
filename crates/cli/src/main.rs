use std::{env, io::{Read, Write}, os::unix::net::UnixStream, path::PathBuf};

fn socket_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".cache/clipdash/daemon.sock")
}

fn send(cmd: &str) -> std::io::Result<String> {
    let mut s = UnixStream::connect(socket_path())?;
    s.write_all(cmd.as_bytes())?;
    let mut buf = String::new();
    s.read_to_string(&mut buf)?;
    Ok(buf)
}

fn usage() {
    eprintln!("clipdash CLI\nCommands:\n  daemon (run daemon)\n  add-text <text>\n  list [limit]\n  get <id>\n  pin <id> <0|1>\n  delete <id>\n  clear");
}

fn main() {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else { usage(); return; };
    match cmd.as_str() {
        "daemon" => {
            clipdash_daemon::run_server_forever();
        }
        "add-text" => {
            let text: String = args.collect::<Vec<_>>().join(" ");
            if text.is_empty() { eprintln!("empty text"); return; }
            match send(&format!("ADD_TEXT {}", text)) { Ok(r) => print!("{}", r), Err(e) => eprintln!("{}", e) }
        }
        "list" => {
            let limit = args.next().unwrap_or("50".into());
            match send(&format!("LIST {}", limit)) { Ok(r) => print!("{}", r), Err(e) => eprintln!("{}", e) }
        }
        "get" => {
            let Some(id)= args.next() else { usage(); return; };
            match send(&format!("GET {}", id)) { Ok(r) => print!("{}", r), Err(e) => eprintln!("{}", e) }
        }
        "pin" => {
            let Some(id)= args.next() else { usage(); return; };
            let Some(v)= args.next() else { usage(); return; };
            match send(&format!("PIN {} {}", id, v)) { Ok(r) => print!("{}", r), Err(e) => eprintln!("{}", e) }
        }
        "delete" => {
            let Some(id)= args.next() else { usage(); return; };
            match send(&format!("DELETE {}", id)) { Ok(r) => print!("{}", r), Err(e) => eprintln!("{}", e) }
        }
        "clear" => {
            match send("CLEAR") { Ok(r) => print!("{}", r), Err(e) => eprintln!("{}", e) }
        }
        _ => usage(),
    }
}
