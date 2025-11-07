#[cfg(feature = "gtk-ui")]
use gtk::{prelude::*, Orientation};
#[cfg(feature = "gtk-ui")]
use std::{io::{Read, Write}, os::unix::net::UnixStream, path::PathBuf};

#[cfg(feature = "gtk-ui")]
fn socket_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".cache/clipdash/daemon.sock")
}

#[cfg(feature = "gtk-ui")]
fn send(cmd: &str) -> std::io::Result<String> {
    use std::net::Shutdown;
    let mut s = UnixStream::connect(socket_path())?;
    s.write_all(cmd.as_bytes())?;
    s.write_all(b"\n")?;
    let _ = s.shutdown(Shutdown::Write);
    let mut buf = String::new();
    s.read_to_string(&mut buf)?;
    Ok(buf)
}

#[cfg(feature = "gtk-ui")]
pub fn run() -> Result<(), String> {
    gtk::init().map_err(|e| format!("gtk init: {}", e))?;

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Clipdash");
    window.set_default_size(700, 480);
    window.set_position(gtk::WindowPosition::Center);

    let vbox = gtk::Box::new(Orientation::Vertical, 6);
    let entry = gtk::SearchEntry::new();
    entry.set_placeholder_text(Some("Search..."));
    let scroller = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    let list = gtk::ListBox::new();
    list.set_activate_on_single_click(true);
    scroller.add(&list);

    vbox.pack_start(&entry, false, false, 0);
    vbox.pack_start(&scroller, true, true, 0);
    window.add(&vbox);

    // Helper to refresh list based on query
    let refresh = {
        let list = list.clone();
        move |q: String| {
            let list = list.clone();
            std::thread::spawn(move || {
                let cmd = if q.is_empty() { "LIST 200".to_string() } else { format!("LIST 200 {}", q) };
                let resp = send(&cmd).unwrap_or_else(|_| "OK 0".into());
                let mut items: Vec<(u64, String)> = Vec::new();
                let mut lines = resp.lines();
                if let Some(h) = lines.next() { if !h.starts_with("OK ") { return; } }
                for l in lines { let mut p = l.splitn(4, '\t'); if let (Some(id), _, _, Some(title)) = (p.next(), p.next(), p.next(), p.next()) { if let Ok(idn)=id.parse(){ items.push((idn,title.to_string())); } } }
                glib::idle_add_local(move || {
                    for child in list.get_children() { list.remove(&child); }
                    for (id, title) in items {
                        let row = gtk::ListBoxRow::new();
                        let hbox = gtk::Box::new(Orientation::Horizontal, 6);
                        let id_label = gtk::Label::new(Some(&format!("{}", id)));
                        id_label.set_xalign(0.0);
                        id_label.get_style_context().add_class("dim-label");
                        let title_label = gtk::Label::new(Some(&title));
                        title_label.set_xalign(0.0);
                        title_label.set_line_wrap(true);
                        title_label.set_max_width_chars(80);
                        // keep simple wrapping without ellipsize to avoid extra deps
                        hbox.pack_start(&id_label, false, false, 6);
                        hbox.pack_start(&title_label, true, true, 6);
                        row.add(&hbox);
                        // store id in widget name for retrieval
                        row.set_widget_name(&format!("id:{}", id));
                        list.add(&row);
                    }
                    list.show_all();
                    glib::Continue(false)
                });
            });
        }
    };

    // Initial load
    refresh(String::new());

    // Change on search
    {
        let refresh = refresh.clone();
        entry.connect_changed(move |e| {
            let q = e.get_text().to_string();
            refresh(q);
        });
    }

    // Activate row -> copy (PASTE) and close
    {
        let win = window.clone();
        list.connect_row_activated(move |_, row| {
            if let Some(name) = row.get_widget_name() {
                if let Some(id_str) = name.strip_prefix("id:") {
                    if let Ok(id) = id_str.parse::<u64>() {
                        let _ = send(&format!("PASTE {}", id));
                        win.close();
                    }
                }
            }
        });
    }

    window.show_all();
    gtk::main();
    Ok(())
}
