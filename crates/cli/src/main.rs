use std::{env, io::{Read, Write}, os::unix::net::UnixStream, path::PathBuf, net::Shutdown};

fn socket_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".cache/clipdash/daemon.sock")
}

fn send(cmd: &str) -> std::io::Result<String> {
    let mut s = UnixStream::connect(socket_path())?;
    s.write_all(cmd.as_bytes())?;
    s.write_all(b"\n")?; // signal end-of-command for line-based protocol
    let _ = s.shutdown(Shutdown::Write);
    let mut buf = String::new();
    s.read_to_string(&mut buf)?;
    Ok(buf)
}

fn usage() {
    eprintln!("clipdash CLI\nCommands:\n  daemon (run daemon)\n  add-text <text>\n  list [limit] [query]\n  get <id>\n  paste <id> (print raw text)\n  copy <id> (to system clipboard)\n  menu (open rofi/wofi/dmenu UI)\n  pin <id> <0|1>\n  delete <id>\n  clear");
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
            let query = args.collect::<Vec<_>>().join(" ");
            let cmd = if query.is_empty() { format!("LIST {}", limit) } else { format!("LIST {} {}", limit, query) };
            match send(&cmd) { Ok(r) => print!("{}", r), Err(e) => eprintln!("{}", e) }
        }
        "get" => {
            let Some(id)= args.next() else { usage(); return; };
            match send(&format!("GET {}", id)) { Ok(r) => print!("{}", r), Err(e) => eprintln!("{}", e) }
        }
        "paste" => {
            let Some(id)= args.next() else { usage(); return; };
            match send(&format!("GET {}", id)) {
                Ok(r) => {
                    if let Some(rest) = r.strip_prefix("TEXT\n") { print!("{}", rest); } else { eprintln!("ERR unsupported kind or not found"); }
                }
                Err(e) => eprintln!("{}", e)
            }
        }
        "copy" => {
            let Some(id)= args.next() else { usage(); return; };
            match send(&format!("PASTE {}", id)) { Ok(r) => print!("{}", r), Err(e) => eprintln!("{}", e) }
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
        "menu" => {
            if let Err(e) = run_menu() { eprintln!("menu error: {}", e); }
        }
        _ => usage(),
    }
}

fn have_cmd(cmd: &str) -> bool {
    std::process::Command::new(cmd).arg("-v").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok()
}

fn run_menu() -> std::io::Result<()> {
    // Fetch latest items
    let resp = send("LIST 200")?;
    let mut lines = resp.lines();
    let header = lines.next().unwrap_or("");
    if !header.starts_with("OK ") { eprintln!("daemon error: {}", header); return Ok(()); }
    let mut items: Vec<(u64, String)> = Vec::new();
    for l in lines {
        // id\tkind\tpinned\ttitle
        let mut parts = l.splitn(4, '\t');
        if let (Some(id), Some(_kind), Some(_pinned), Some(title)) = (parts.next(), parts.next(), parts.next(), parts.next()) {
            if let Ok(idn) = id.parse::<u64>() {
                items.push((idn, title.to_string()));
            }
        }
    }
    if items.is_empty() { return Ok(()); }

    // Build menu input: "<id>\t<title>"
    let menu_input = items.iter().map(|(id, title)| format!("{}\t{}", id, title.replace('\n', " "))).collect::<Vec<_>>().join("\n");

    // Prefer GTK zenity first (native), then rofi -> wofi -> dmenu
    let choice = if have_cmd("zenity") {
        run_zenity_list(&items)?
    } else if have_cmd("rofi") {
        run_dmenu_like(&menu_input, &["rofi","-dmenu","-p","Clipdash"]) ?
    } else if have_cmd("wofi") {
        run_dmenu_like(&menu_input, &["wofi","--dmenu","--prompt","Clipdash"]) ?
    } else if have_cmd("dmenu") {
        run_dmenu_like(&menu_input, &["dmenu","-p","Clipdash"]) ?
    } else {
        // Fallback: print list and read a line from stdin
        eprintln!("No rofi/wofi/dmenu found. Falling back to stdin selection. Enter an id:");
        println!("{}", menu_input);
        let mut s = String::new();
        std::io::stdin().read_line(&mut s)?;
        Some(s)
    };

    if let Some(ch) = choice {
        let id_str = ch.split('\t').next().unwrap_or(ch.trim());
        if let Ok(id) = id_str.trim().parse::<u64>() {
            // ask daemon to copy to system clipboard
            match send(&format!("PASTE {}", id)) { Ok(r) => println!("{}", r), Err(e) => eprintln!("{}", e) }
        }
    }

    Ok(())
}

fn run_dmenu_like(input: &str, cmd: &[&str]) -> std::io::Result<Option<String>> {
    let (prog, args) = cmd.split_first().expect("non-empty cmd");
    let mut child = std::process::Command::new(prog)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    if let Some(stdin) = child.stdin.as_mut() { stdin.write_all(input.as_bytes())?; }
    let out = child.wait_with_output()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).to_string();
        if s.trim().is_empty() { Ok(None) } else { Ok(Some(s)) }
    } else {
        Ok(None)
    }
}

fn run_zenity_list(items: &[(u64, String)]) -> std::io::Result<Option<String>> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new("zenity");
    cmd.arg("--list")
        .arg("--title=Clipdash")
        .arg("--width=700")
        .arg("--height=480")
        .arg("--print-column=1")
        .arg("--hide-column=1")
        .arg("--column=ID")
        .arg("--column=Title")
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    for (id, title) in items {
        cmd.arg(id.to_string());
        cmd.arg(title.replace('\n', " "));
    }
    let out = cmd.output()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).to_string();
        if s.trim().is_empty() { Ok(None) } else { Ok(Some(s)) }
    } else {
        Ok(None)
    }
}
