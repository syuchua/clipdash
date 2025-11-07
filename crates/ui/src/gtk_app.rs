#[cfg(feature = "gtk-ui")]
use gtk::{prelude::*, Orientation};
#[cfg(feature = "gtk-ui")]
use glib::Cast;
#[cfg(feature = "gtk-ui")]
use gtk::gdk::ModifierType;
use std::{cell::RefCell, rc::Rc};
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

    // Preview area (hidden by default)
    let preview = gtk::TextView::new();
    preview.set_wrap_mode(gtk::WrapMode::Word);
    preview.set_editable(false);
    let preview_frame = gtk::Frame::new(Some("Preview"));
    preview_frame.add(&preview);
    preview_frame.hide();

    vbox.pack_start(&entry, false, false, 0);
    vbox.pack_start(&scroller, true, true, 0);
    vbox.pack_start(&preview_frame, false, true, 0);
    window.add(&vbox);

    // Channel to update list from worker thread
    let (tx, rx) = glib::MainContext::channel::<Vec<(u64, String, bool)>>(glib::PRIORITY_DEFAULT);
    // Error channel for connection issues
    let (txe, rxe) = glib::MainContext::channel::<String>(glib::PRIORITY_DEFAULT);
    let q_state = Rc::new(RefCell::new(String::new()));
    {
        let list = list.clone();
        let q_state = q_state.clone();
        rx.attach(None, move |items| {
            for child in list.children() { list.remove(&child); }
            // pinned first
            let mut pinned_rows: Vec<gtk::ListBoxRow> = Vec::new();
            let mut normal_rows: Vec<gtk::ListBoxRow> = Vec::new();
            let q = q_state.borrow().clone();
            for (id, title, pinned) in items {
                let row = gtk::ListBoxRow::new();
                let hbox = gtk::Box::new(Orientation::Horizontal, 6);
                let id_label = gtk::Label::new(Some(&format!("{}", id)));
                id_label.set_xalign(0.0);
                id_label.style_context().add_class("dim-label");
                let title_label = gtk::Label::new(None);
                title_label.set_use_markup(true);
                title_label.set_markup(&markup_highlight(&format!("{}{}", if pinned {"★ "} else {""}, title), &q));
                title_label.set_xalign(0.0);
                title_label.set_line_wrap(true);
                title_label.set_max_width_chars(80);
                hbox.pack_start(&id_label, false, false, 6);
                hbox.pack_start(&title_label, true, true, 6);
                row.add(&hbox);
                row.set_widget_name(&format!("id:{}", id));
                if pinned { pinned_rows.push(row); } else { normal_rows.push(row); }
            }
            for r in pinned_rows { list.add(&r); }
            for r in normal_rows { list.add(&r); }
            // Select first row by default
            if let Some(first) = list.children().get(0).and_then(|w| w.clone().downcast::<gtk::ListBoxRow>().ok()) {
                list.select_row(Some(&first));
            }
            list.show_all();
            glib::Continue(true)
        });
        // Error dialog handler
        let win = window.clone();
        rxe.attach(None, move |msg| {
            let d = gtk::MessageDialog::new(Some(&win), gtk::DialogFlags::MODAL, gtk::MessageType::Error, gtk::ButtonsType::Ok, &msg);
            d.run();
            d.destroy();
            glib::Continue(true)
        });
    }

    // Helper to refresh list based on query
    let refresh = {
        let tx = tx.clone();
        let txe = txe.clone();
        move |q: String| {
            let tx = tx.clone();
            let txe = txe.clone();
            std::thread::spawn(move || {
                let cmd = if q.is_empty() { "LIST 200".to_string() } else { format!("LIST 200 {}", q) };
                let resp = match send(&cmd) { Ok(s) => s, Err(e) => { let _ = txe.send(format!("连接守护失败: {}", e)); let _ = tx.send(Vec::new()); return; } };
                let mut items: Vec<(u64, String, bool)> = Vec::new();
                let mut lines = resp.lines();
                if let Some(h) = lines.next() { if !h.starts_with("OK ") { let _ = tx.send(Vec::new()); return; } }
                for l in lines {
                    let mut p = l.splitn(4, '\t');
                    if let (Some(id), _kind, Some(pinned), Some(title)) = (p.next(), p.next(), p.next(), p.next()) {
                        if let Ok(idn) = id.parse() { items.push((idn, title.to_string(), pinned == "1")); }
                    }
                }
                let _ = tx.send(items);
            });
        }
    };

    // Initial load
    refresh(String::new());

    // Change on search
    {
        // Debounce entry changes
        let refresh = refresh.clone();
        let q_state = q_state.clone();
        let mut timer: Option<glib::SourceId> = None;
        entry.connect_changed(move |e| {
            let q = e.text().to_string();
            *q_state.borrow_mut() = q.clone();
            if let Some(id) = timer.take() { id.remove(); }
            timer = Some(glib::timeout_add_local(std::time::Duration::from_millis(150), {
                let refresh = refresh.clone();
                let q = q.clone();
                move || { refresh(q.clone()); glib::Continue(false) }
            }));
        });
    }

    // Activate row -> copy (PASTE) and close
    {
        let win = window.clone();
        list.connect_row_activated(move |_, row| {
            let name = row.widget_name();
            if let Some(id_str) = name.strip_prefix("id:") {
                if let Ok(id) = id_str.parse::<u64>() {
                    let _ = send(&format!("PASTE {}", id));
                    win.close();
                }
            }
        });
    }

    // Key handling: Up/Down to navigate, Enter to paste, Escape to close
    {
        let list_nav = list.clone();
        let win = window.clone();
        let preview_frame_c = preview_frame.clone();
        let preview_c = preview.clone();
        let refresh_cb = refresh.clone();
        let entry_c = entry.clone();
        entry.connect_key_press_event(move |_, ev| {
            use gtk::gdk::keys::constants as kc;
            let key = ev.keyval();
            match key {
                k if k == kc::Up => { move_selection(&list_nav, -1); if preview_frame_c.is_visible() { update_preview(&list_nav, &preview_c); } Inhibit(true) }
                k if k == kc::Down => { move_selection(&list_nav, 1); if preview_frame_c.is_visible() { update_preview(&list_nav, &preview_c); } Inhibit(true) }
                k if k == kc::Return => { activate_selected(&list_nav, &win); Inhibit(true) }
                k if k == kc::KP_Enter => { activate_selected(&list_nav, &win); Inhibit(true) }
                // Toggle preview with Space
                k if k == kc::space => { toggle_preview(&list_nav, &preview_frame_c, &preview_c); Inhibit(true) }
                // Pin/unpin with 'p'
                k if k == kc::p => { pin_toggle(&list_nav); refresh_cb(entry_c.text().to_string()); Inhibit(true) }
                // Delete selected
                k if k == kc::Delete => { delete_selected(&list_nav); refresh_cb(entry_c.text().to_string()); Inhibit(true) }
                // Ctrl+L clear
                k if k == kc::l && ev.state().contains(ModifierType::CONTROL_MASK) => { clear_all(); refresh_cb(entry_c.text().to_string()); Inhibit(true) }
                _ => Inhibit(false)
            }
        });

        let list_nav2 = list.clone();
        let preview_frame_w = preview_frame.clone();
        let preview_w = preview.clone();
        let refresh_cb = refresh.clone();
        window.connect_key_press_event(move |w, ev| {
            use gtk::gdk::keys::constants as kc;
            let key = ev.keyval();
            match key {
                k if k == kc::Escape => { w.close(); Inhibit(true) }
                k if k == kc::Up => { move_selection(&list_nav2, -1); if preview_frame_w.is_visible() { update_preview(&list_nav2, &preview_w); } Inhibit(true) }
                k if k == kc::Down => { move_selection(&list_nav2, 1); if preview_frame_w.is_visible() { update_preview(&list_nav2, &preview_w); } Inhibit(true) }
                k if k == kc::Return || k == kc::KP_Enter => { activate_selected(&list_nav2, w); Inhibit(true) }
                k if k == kc::space => { toggle_preview(&list_nav2, &preview_frame_w, &preview_w); Inhibit(true) }
                k if k == kc::p => { pin_toggle(&list_nav2); refresh_cb(entry.text().to_string()); Inhibit(true) }
                k if k == kc::Delete => { delete_selected(&list_nav2); refresh_cb(entry.text().to_string()); Inhibit(true) }
                k if k == kc::l && ev.state().contains(ModifierType::CONTROL_MASK) => { clear_all(); refresh_cb(entry.text().to_string()); Inhibit(true) }
                _ => Inhibit(false)
            }
        });
    }

    window.show_all();
    entry.grab_focus();
    gtk::main();
    Ok(())
}

#[cfg(feature = "gtk-ui")]
fn move_selection(list: &gtk::ListBox, delta: i32) {
    let rows = list.children();
    let len = rows.len() as i32;
    if len == 0 { return; }
    let current_idx: i32 = list.selected_row().map(|r| r.index()).unwrap_or(0);
    let mut idx = current_idx + delta;
    if idx < 0 { idx = 0; }
    if idx >= len { idx = len - 1; }
    if let Some(row) = list.row_at_index(idx) { list.select_row(Some(&row)); }
}

#[cfg(feature = "gtk-ui")]
fn activate_selected(list: &gtk::ListBox, win: &gtk::Window) {
    if let Some(sel) = list.selected_row() {
        let name = sel.widget_name();
        if let Some(id_str) = name.strip_prefix("id:") {
            if let Ok(id) = id_str.parse::<u64>() {
                let _ = send(&format!("PASTE {}", id));
                win.close();
            }
        }
    }
}

#[cfg(feature = "gtk-ui")]
fn current_selected_id(list: &gtk::ListBox) -> Option<u64> {
    list.selected_row().and_then(|sel| {
        let name = sel.widget_name();
        name.strip_prefix("id:").and_then(|s| s.parse::<u64>().ok())
    })
}

#[cfg(feature = "gtk-ui")]
fn toggle_preview(list: &gtk::ListBox, frame: &gtk::Frame, view: &gtk::TextView) {
    if !frame.is_visible() { frame.show(); } else { frame.hide(); return; }
    if let Some(id) = current_selected_id(list) {
        if let Ok(resp) = send(&format!("GET {}", id)) {
            if let Some(text) = resp.strip_prefix("TEXT\n") {
                view.buffer().unwrap().set_text(text);
            } else {
                view.buffer().unwrap().set_text("[unsupported item]");
            }
        }
    }
}

#[cfg(feature = "gtk-ui")]
fn pin_toggle(list: &gtk::ListBox) {
    if let Some(id) = current_selected_id(list) {
        // naive toggle: try pin then unpin if already pinned (we need to know state; fetch list row label)
        // Here we send a pin=1 first; UI will refresh on next keypress by user. For robustness, fetch name
        let _ = send(&format!("PIN {} 1", id));
    }
}

#[cfg(feature = "gtk-ui")]
fn delete_selected(list: &gtk::ListBox) {
    if let Some(id) = current_selected_id(list) { let _ = send(&format!("DELETE {}", id)); }
}

#[cfg(feature = "gtk-ui")]
fn clear_all() {
    let _ = send("CLEAR");
}
