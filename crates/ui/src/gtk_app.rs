#[cfg(feature = "gtk-ui")]
use gtk::{prelude::*, Orientation};
use gdk::{Screen, EventButton};
use gdk_pixbuf::{PixbufLoader, Pixbuf};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
#[cfg(feature = "gtk-ui")]
use glib::{Cast, ObjectExt};
#[cfg(feature = "gtk-ui")]
use gtk::gdk::ModifierType;
use std::{cell::RefCell, rc::Rc};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
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
struct UiConfig { dark: bool, opacity: f64, max_preview_chars: usize }

fn load_ui_config() -> UiConfig {
    let mut cfg = UiConfig { dark: true, opacity: 0.93, max_preview_chars: 200_000 };
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let path = std::path::Path::new(&home).join(".config/clipdash/config.toml");
    if let Ok(s) = std::fs::read_to_string(path) {
        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let mut parts = line.splitn(2, '=');
            let k = parts.next().map(|v| v.trim()).unwrap_or("");
            let v = parts.next().map(|v| v.trim()).unwrap_or("");
            if k.eq_ignore_ascii_case("ui.dark") {
                cfg.dark = matches!(v, "true"|"1"|"on"|"yes");
            } else if k.eq_ignore_ascii_case("ui.opacity") {
                if let Ok(f) = v.trim_matches('"').parse::<f64>() { cfg.opacity = f.clamp(0.0, 1.0); }
            } else if k.eq_ignore_ascii_case("ui.max_preview_chars") {
                if let Ok(n) = v.trim_matches('"').parse::<usize>() { cfg.max_preview_chars = n.max(10_000).min(2_000_000); }
            }
        }
    }
    cfg
}

pub fn run() -> Result<(), String> {
    gtk::init().map_err(|e| format!("gtk init: {}", e))?;
    let ui_cfg = load_ui_config();
    // CSS provider and initial theme
    let provider = gtk::CssProvider::new();
    apply_css_with_provider(&provider, ui_cfg.dark);

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Clipdash");
    window.set_default_size(700, 480);
    window.set_position(gtk::WindowPosition::Center);
    // Try semi-transparency; may be ignored on Wayland
    if std::env::var("CLIPDASH_UI_NO_OPACITY").ok().as_deref() != Some("1") {
        window.set_opacity(ui_cfg.opacity);
    }

    let vbox = gtk::Box::new(Orientation::Vertical, 6);
    vbox.style_context().add_class("surface");
    // Info bar for transient messages
    let infobar = gtk::InfoBar::new();
    infobar.set_no_show_all(true);
    infobar.hide();
    let info_label = gtk::Label::new(None);
    let area = infobar.content_area();
    area.add(&info_label);
    let entry = gtk::SearchEntry::new();
    entry.set_placeholder_text(Some("Search..."));
    // Toolbar with action buttons
    let toolbar = gtk::Box::new(Orientation::Horizontal, 6);
    let btn_copy = gtk::Button::with_label("Copy");
    let btn_pin = gtk::Button::with_label("Pin/Unpin");
    let btn_del = gtk::Button::with_label("Delete");
    let btn_clear = gtk::Button::with_label("Clear");
    let btn_prev = gtk::Button::with_label("Preview");
    let btn_theme = gtk::Button::with_label("Theme");
    let btn_fit = gtk::Button::with_label("Fit");
    let btn_actual = gtk::Button::with_label("100%");
    toolbar.pack_start(&btn_copy, false, false, 0);
    toolbar.pack_start(&btn_pin, false, false, 0);
    toolbar.pack_start(&btn_del, false, false, 0);
    toolbar.pack_start(&btn_clear, false, false, 0);
    toolbar.pack_start(&btn_prev, false, false, 0);
    toolbar.pack_start(&btn_fit, false, false, 0);
    toolbar.pack_start(&btn_actual, false, false, 0);
    toolbar.pack_start(&btn_theme, false, false, 0);

    let scroller = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    let list = gtk::ListBox::new();
    list.set_activate_on_single_click(true);
    scroller.add(&list);

    // Preview area (stack + revealer)
    let preview_text = gtk::TextView::new();
    preview_text.set_wrap_mode(gtk::WrapMode::Word);
    preview_text.set_editable(false);
    let preview_image = gtk::Image::new();
    let image_scroller = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    image_scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    image_scroller.add(&preview_image);
    let preview_stack = gtk::Stack::new();
    preview_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    preview_stack.add_named(&preview_text, "text");
    preview_stack.add_named(&image_scroller, "image");
    preview_stack.set_visible_child_name("text");
    let preview_frame = gtk::Frame::new(Some("Preview"));
    preview_frame.add(&preview_stack);
    let preview_revealer = gtk::Revealer::new();
    preview_revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
    preview_revealer.set_transition_duration(160);
    preview_revealer.add(&preview_frame);
    preview_revealer.set_reveal_child(false);

    // Async preview pipeline (avoid blocking GTK main thread)
    enum PreviewMsg {
        Text(String),
        Html(String),
        Image { mime: String, bytes: Vec<u8> },
        Error(String),
    }
    let (txp, rxp) = glib::MainContext::channel::<(u64, PreviewMsg)>(glib::PRIORITY_DEFAULT);
    let preview_seq = Arc::new(AtomicU64::new(0));

    vbox.pack_start(&entry, false, false, 0);
    vbox.pack_start(&infobar, false, false, 0);
    vbox.pack_start(&toolbar, false, false, 0);
    // Stack for list/empty placeholder
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    let empty_box = gtk::Box::new(Orientation::Vertical, 6);
    let empty_lbl = gtk::Label::new(Some("No clipboard items yet"));
    empty_lbl.style_context().add_class("empty");
    empty_lbl.set_halign(gtk::Align::Center);
    empty_lbl.set_valign(gtk::Align::Center);
    empty_box.pack_start(&empty_lbl, true, true, 0);
    stack.add_named(&scroller, "list");
    stack.add_named(&empty_box, "empty");
    stack.set_visible_child_name("list");
    vbox.pack_start(&stack, true, true, 0);
    vbox.pack_start(&preview_revealer, false, true, 0);
    window.add(&vbox);

    // Channel to update list from worker thread
    let (tx, rx) = glib::MainContext::channel::<Vec<(u64, String, bool, String, String)>>(glib::PRIORITY_DEFAULT);
    // Error channel for connection issues
    let (txe, rxe) = glib::MainContext::channel::<String>(glib::PRIORITY_DEFAULT);
    let q_state = Rc::new(RefCell::new(String::new()));
    {
        let list = list.clone();
        let stack = stack.clone();
        let q_state = q_state.clone();
        rx.attach(None, move |items| {
            for child in list.children() { list.remove(&child); }
            // pinned first
            let mut pinned_rows: Vec<gtk::ListBoxRow> = Vec::new();
            let mut normal_rows: Vec<gtk::ListBoxRow> = Vec::new();
            let q = q_state.borrow().clone();
            for (id, title, pinned, kind, mime) in items {
                let row = gtk::ListBoxRow::new();
                let hbox = gtk::Box::new(Orientation::Horizontal, 6);
                let id_label = gtk::Label::new(Some(&format!("{}", id)));
                id_label.set_xalign(0.0);
                id_label.style_context().add_class("dim-label");
                let title_label = gtk::Label::new(None);
                title_label.set_use_markup(true);
                let icon = match kind.as_str() { "Image" => "🖼 ", "Html" => "</> ", _ => if mime.starts_with("image/") { "🖼 " } else if mime == "text/html" { "</> " } else { "T " } };
                title_label.set_markup(&markup_highlight(&format!("{}{}{}", if pinned {"★ "} else {""}, icon, title), &q));
                // Tooltip shows mime when available
                if !mime.is_empty() { title_label.set_tooltip_text(Some(&mime)); }
                title_label.set_xalign(0.0);
                title_label.set_line_wrap(true);
                title_label.set_max_width_chars(80);
                hbox.pack_start(&id_label, false, false, 6);
                hbox.pack_start(&title_label, true, true, 6);
                // Card container for rounded background and spacing
                let card = gtk::EventBox::new();
                card.style_context().add_class("card");
                card.set_margin_top(6);
                card.set_margin_bottom(6);
                card.set_margin_start(8);
                card.set_margin_end(8);
                card.add(&hbox);
                row.add(&card);
                row.set_widget_name(&format!("id:{}|p:{}", id, if pinned {1} else {0}));
                if pinned { pinned_rows.push(row); } else { normal_rows.push(row); }
            }
            for r in pinned_rows { list.add(&r); }
            for r in normal_rows { list.add(&r); }
            // Toggle empty state
            let count = list.children().len();
            stack.set_visible_child_name(if count == 0 { "empty" } else { "list" });
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
            unsafe { d.destroy(); }
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
                let mut items: Vec<(u64, String, bool, String, String)> = Vec::new();
                let mut lines = resp.lines();
                if let Some(h) = lines.next() { if !h.starts_with("OK ") { let _ = tx.send(Vec::new()); return; } }
                for l in lines {
                    let mut p = l.splitn(5, '\t');
                    let id = p.next(); let kind = p.next(); let pinned = p.next(); let title = p.next(); let mime = p.next();
                    if let (Some(id), Some(kind), Some(pinned), Some(title)) = (id, kind, pinned, title) {
                        let mime_s = mime.unwrap_or("");
                        if let Ok(idn) = id.parse() { items.push((idn, title.to_string(), pinned == "1", kind.to_string(), mime_s.to_string())); }
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
        let timer: std::rc::Rc<std::cell::RefCell<Option<glib::SourceId>>> = std::rc::Rc::new(std::cell::RefCell::new(None));
        let timer_c = timer.clone();
        entry.connect_changed(move |e| {
            let q = e.text().to_string();
            *q_state.borrow_mut() = q.clone();
            if let Some(id) = timer_c.borrow_mut().take() { glib::source::source_remove(id); }
            let refresh = refresh.clone();
            *timer_c.borrow_mut() = Some(glib::timeout_add_local(std::time::Duration::from_millis(150), {
                let refresh = refresh.clone();
                let q = q.clone();
                move || { refresh(q.clone()); glib::Continue(false) }
            }));
        });
    }

    // Status message helper
    let show_status = {
        let infobar = infobar.clone();
        let info_label = info_label.clone();
        move |text: &str, kind: gtk::MessageType| {
            info_label.set_text(text);
            infobar.set_message_type(kind);
            infobar.show();
            let ib = infobar.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(1200), move || { ib.hide(); glib::Continue(false) });
        }
    };

    // Zoom state for image preview
    let zoom_fit = Rc::new(RefCell::new(true));
    let last_pix: Rc<RefCell<Option<Pixbuf>>> = Rc::new(RefCell::new(None));

    // Receive preview results on UI thread
    {
        let preview_stack_ui = preview_stack.clone();
        let preview_text_ui = preview_text.clone();
        let preview_image_ui = preview_image.clone();
        let zoom_fit_ui = zoom_fit.clone();
        let last_pix_ui = last_pix.clone();
        let seq_ui = preview_seq.clone();
        rxp.attach(None, move |(seqn, msg)| {
            if seqn != seq_ui.load(Ordering::SeqCst) { return glib::Continue(true); }
            match msg {
                PreviewMsg::Text(s) | PreviewMsg::Html(s) => {
                    if let Some(buf) = preview_text_ui.buffer() { buf.set_text(&s); }
                    preview_stack_ui.set_visible_child_name("text");
                }
                PreviewMsg::Image { mime: _mime, bytes } => {
                    let loader = PixbufLoader::new();
                    let _ = loader.write(&bytes);
                    let _ = loader.close();
                    if let Some(pix) = loader.pixbuf() {
                        *last_pix_ui.borrow_mut() = Some(pix.clone());
                        let alloc = preview_stack_ui.allocation();
                        let max_w = (alloc.width - 24).max(200);
                        let max_h = 420;
                        let scaled = if *zoom_fit_ui.borrow() { scale_pixbuf_fit(&pix, max_w, max_h) } else { pix.clone() };
                        preview_image_ui.set_from_pixbuf(Some(&scaled));
                        preview_stack_ui.set_visible_child_name("image");
                    } else {
                        if let Some(buf) = preview_text_ui.buffer() { buf.set_text("[image preview unavailable]"); }
                        preview_stack_ui.set_visible_child_name("text");
                    }
                }
                PreviewMsg::Error(e) => {
                    if let Some(buf) = preview_text_ui.buffer() { buf.set_text(&format!("[preview error] {}", e)); }
                    preview_stack_ui.set_visible_child_name("text");
                }
            }
            glib::Continue(true)
        });
    }

    // Helper: request preview asynchronously for current selection
    let request_preview: std::rc::Rc<dyn Fn()> = {
        let list_rp = list.clone();
        let txp = txp.clone();
        let seq = preview_seq.clone();
        let max_chars_cfg = ui_cfg.max_preview_chars;
        std::rc::Rc::new(move || {
            if let Some(id) = current_selected_id(&list_rp) {
                let my = seq.fetch_add(1, Ordering::SeqCst).saturating_add(1);
                let txp_outer = txp.clone();
                std::thread::spawn(move || {
                    let resp = match send(&format!("GET {}", id)) { Ok(s) => s, Err(e) => { let _ = txp_outer.send((my, PreviewMsg::Error(format!("{}", e)))); return; } };
                    if let Some(text) = resp.strip_prefix("TEXT\n") {
                        let s = if text.len() > max_chars_cfg { format!("{}\n… [truncated]", &text[..max_chars_cfg]) } else { text.to_string() };
                        let _ = txp_outer.send((my, PreviewMsg::Text(s)));
                    } else if let Some(html) = resp.strip_prefix("HTML\n") {
                        // Show raw HTML text for now (avoid WebKit by default)
                        let s = if html.len() > max_chars_cfg { format!("{}\n… [truncated]", &html[..max_chars_cfg]) } else { html.to_string() };
                        let _ = txp_outer.send((my, PreviewMsg::Html(s)));
                    } else if let Some(rest) = resp.strip_prefix("IMAGE\n") {
                        let mut lines = rest.lines();
                        let mime = lines.next().unwrap_or("image/png").to_string();
                        let b64 = lines.collect::<Vec<_>>().join("\n");
                        match B64.decode(b64) {
                            Ok(bytes) => { let _ = txp_outer.send((my, PreviewMsg::Image { mime, bytes })); }
                            Err(e) => { let _ = txp_outer.send((my, PreviewMsg::Error(format!("base64: {}", e)))); }
                        }
                    } else {
                        let _ = txp_outer.send((my, PreviewMsg::Error("unknown response".into())));
                    }
                });
            }
        })
    };

    // Buttons actions
    {
        let list_c = list.clone();
        let show = show_status.clone();
        btn_copy.connect_clicked(move |_| {
            if let Some(id) = current_selected_id(&list_c) { let _ = send(&format!("PASTE {}", id)); show("Copied", gtk::MessageType::Info); }
        });

        let list_p = list.clone();
        let entry_p = entry.clone();
        let refresh_p = refresh.clone();
        let show = show_status.clone();
        btn_pin.connect_clicked(move |_| { pin_toggle(&list_p); refresh_p(entry_p.text().to_string()); show("Toggled pin", gtk::MessageType::Other); });

        let list_d = list.clone();
        let entry_d = entry.clone();
        let refresh_d = refresh.clone();
        let show = show_status.clone();
        btn_del.connect_clicked(move |_| { delete_selected(&list_d); refresh_d(entry_d.text().to_string()); show("Deleted", gtk::MessageType::Other); });

        let entry_c2 = entry.clone();
        let refresh_c2 = refresh.clone();
        let show = show_status.clone();
        btn_clear.connect_clicked(move |_| { clear_all(); refresh_c2(entry_c2.text().to_string()); show("Cleared", gtk::MessageType::Warning); });

        let preview_revealer_btn = preview_revealer.clone();
        let req = request_preview.clone();
        btn_prev.connect_clicked(move |_| {
            preview_revealer_btn.set_reveal_child(!preview_revealer_btn.reveals_child());
            if preview_revealer_btn.reveals_child() { (*req)(); }
        });

        // Fit button: scale to fit container
        let preview_stack_fit = preview_stack.clone();
        let preview_image_fit = preview_image.clone();
        let zoom_fit_fit = zoom_fit.clone();
        let last_pix_fit = last_pix.clone();
        btn_fit.connect_clicked(move |_| {
            *zoom_fit_fit.borrow_mut() = true;
            if let Some(pix) = last_pix_fit.borrow().clone() {
                let alloc = preview_stack_fit.allocation();
                let max_w = (alloc.width - 24).max(200);
                let max_h = 420;
                let scaled = scale_pixbuf_fit(&pix, max_w, max_h);
                preview_image_fit.set_from_pixbuf(Some(&scaled));
                preview_stack_fit.set_visible_child_name("image");
            }
        });

        // 100% button: show original size
        let preview_stack_100 = preview_stack.clone();
        let preview_image_100 = preview_image.clone();
        let zoom_fit_100 = zoom_fit.clone();
        let last_pix_100 = last_pix.clone();
        btn_actual.connect_clicked(move |_| {
            *zoom_fit_100.borrow_mut() = false;
            if let Some(pix) = last_pix_100.borrow().clone() {
                preview_image_100.set_from_pixbuf(Some(&pix));
                preview_stack_100.set_visible_child_name("image");
            }
        });
    }

    // Update preview when selection changes (if visible)
    {
        let preview_revealer_c = preview_revealer.clone();
        let req = request_preview.clone();
        list.connect_row_selected(move |lb, row_opt| {
            // Update selected style on cards
            for child in lb.children() {
                if let Ok(r) = child.clone().downcast::<gtk::ListBoxRow>() {
                    if let Some(w) = r.child() {
                        if let Ok(card) = w.downcast::<gtk::EventBox>() { card.style_context().remove_class("selected-card"); }
                    }
                }
            }
            if let Some(row) = row_opt {
                if let Some(w) = row.child() {
                    if let Ok(card) = w.downcast::<gtk::EventBox>() { card.style_context().add_class("selected-card"); }
                }
            }
            if preview_revealer_c.reveals_child() { (*req)(); }
        });
    }

    // Theme toggle button
    {
        let provider_c = provider.clone();
        let dark_state = Rc::new(RefCell::new(ui_cfg.dark));
        let dark_ref = dark_state.clone();
        btn_theme.connect_clicked(move |_| {
            let new_dark = !*dark_ref.borrow();
            *dark_ref.borrow_mut() = new_dark;
            apply_css_with_provider(&provider_c, new_dark);
        });
    }

    // Activate row -> copy (PASTE) and close
    {
        let win = window.clone();
        list.connect_row_activated(move |_, row| {
            let name = row.widget_name();
            if let Some(id_str) = name.strip_prefix("id:") {
                let id_part = id_str.split('|').next().unwrap_or(id_str);
                if let Ok(id) = id_part.parse::<u64>() {
                    let _ = send(&format!("PASTE {}", id));
                    win.close();
                }
            }
        });
    }

    // Context menu on right-click
    {
        let entry_c = entry.clone();
        let refresh_c = refresh.clone();
        let preview_revealer_menu = preview_revealer.clone();
        // removed unused clones for menu preview
        let req_menu = request_preview.clone();
        list.connect_button_press_event(move |lb, ev: &EventButton| {
            if ev.button() == 3 { // right click
                let (_x, y) = ev.position();
                if let Some(row) = lb.row_at_y(y as i32) {
                    lb.select_row(Some(&row));
                    // build menu
                    let menu = gtk::Menu::new();
                    let _id_opt = current_selected_id(lb);
                    let mut currently_pinned = false;
                    if let Some(r) = lb.selected_row() { let name = r.widget_name(); currently_pinned = name.contains("|p:1"); }
                    let mi_copy = gtk::MenuItem::with_label("Copy");
                    let mi_pin = gtk::MenuItem::with_label(if currently_pinned { "Unpin" } else { "Pin" });
                    let mi_del = gtk::MenuItem::with_label("Delete");
                    let mi_prev = gtk::MenuItem::with_label("Preview");
                    menu.append(&mi_copy);
                    menu.append(&mi_pin);
                    menu.append(&mi_del);
                    menu.append(&mi_prev);
                    menu.show_all();

                    // Actions
                    let lb_c1 = lb.clone();
                    let show = show_status.clone();
                    mi_copy.connect_activate(move |_| { if let Some(id) = current_selected_id(&lb_c1) { let _ = send(&format!("PASTE {}", id)); show("Copied", gtk::MessageType::Info); } });

                    let lb_c2 = lb.clone();
                    let entry_c2 = entry_c.clone();
                    let refresh_c2 = refresh_c.clone();
                    let show = show_status.clone();
                    mi_pin.connect_activate(move |_| { pin_toggle(&lb_c2); refresh_c2(entry_c2.text().to_string()); show("Toggled pin", gtk::MessageType::Other); });

                    let lb_c3 = lb.clone();
                    let entry_c3 = entry_c.clone();
                    let refresh_c3 = refresh_c.clone();
                    let show = show_status.clone();
                    mi_del.connect_activate(move |_| { delete_selected(&lb_c3); refresh_c3(entry_c3.text().to_string()); show("Deleted", gtk::MessageType::Other); });

                    let prev_rev4 = preview_revealer_menu.clone();
                    let req_call = req_menu.clone();
                    mi_prev.connect_activate(move |_| {
                        prev_rev4.set_reveal_child(!prev_rev4.reveals_child());
                        if prev_rev4.reveals_child() { (*req_call)(); }
                    });

                    // Popup
                    menu.popup_easy(ev.button(), ev.time());
                }
                Inhibit(true)
            } else { Inhibit(false) }
        });
    }

    // Key handling: Up/Down to navigate, Enter to paste, Escape to close
    {
        let list_nav = list.clone();
        let win = window.clone();
        let preview_revealer_key = preview_revealer.clone();
        // removed unused preview clones
        let refresh_cb = refresh.clone();
        let entry_c = entry.clone();
        let req = request_preview.clone();
        entry.connect_key_press_event(move |_, ev| {
            use gtk::gdk::keys::constants as kc;
            let key = ev.keyval();
            match key {
                k if k == kc::Up => { move_selection(&list_nav, -1); if preview_revealer_key.reveals_child() { (*req)(); } Inhibit(true) }
                k if k == kc::Down => { move_selection(&list_nav, 1); if preview_revealer_key.reveals_child() { (*req)(); } Inhibit(true) }
                k if k == kc::Return => { activate_selected(&list_nav, &win); Inhibit(true) }
                k if k == kc::KP_Enter => { activate_selected(&list_nav, &win); Inhibit(true) }
                // Toggle preview with Space
                k if k == kc::space => { preview_revealer_key.set_reveal_child(!preview_revealer_key.reveals_child()); if preview_revealer_key.reveals_child() { (*req)(); } Inhibit(true) }
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
        let preview_revealer_win = preview_revealer.clone();
        // removed unused preview clones
        let refresh_cb = refresh.clone();
        let entry_w = entry.clone();
        let req = request_preview.clone();
        window.connect_key_press_event(move |w, ev| {
            use gtk::gdk::keys::constants as kc;
            let key = ev.keyval();
            match key {
                k if k == kc::Escape => { w.close(); Inhibit(true) }
                k if k == kc::Up => { move_selection(&list_nav2, -1); if preview_revealer_win.reveals_child() { (*req)(); } Inhibit(true) }
                k if k == kc::Down => { move_selection(&list_nav2, 1); if preview_revealer_win.reveals_child() { (*req)(); } Inhibit(true) }
                k if k == kc::Return || k == kc::KP_Enter => { activate_selected(&list_nav2, w); Inhibit(true) }
                k if k == kc::space => { preview_revealer_win.set_reveal_child(!preview_revealer_win.reveals_child()); if preview_revealer_win.reveals_child() { (*req)(); } Inhibit(true) }
                k if k == kc::p => { pin_toggle(&list_nav2); refresh_cb(entry_w.text().to_string()); Inhibit(true) }
                k if k == kc::Delete => { delete_selected(&list_nav2); refresh_cb(entry_w.text().to_string()); Inhibit(true) }
                k if k == kc::l && ev.state().contains(ModifierType::CONTROL_MASK) => { clear_all(); refresh_cb(entry_w.text().to_string()); Inhibit(true) }
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
            let id_part = id_str.split('|').next().unwrap_or(id_str);
            if let Ok(id) = id_part.parse::<u64>() {
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
        name.strip_prefix("id:")
            .and_then(|s| s.split('|').next())
            .and_then(|s| s.parse::<u64>().ok())
    })
}

#[cfg(feature = "gtk-ui")]
fn pin_toggle(list: &gtk::ListBox) {
    if let Some(id) = current_selected_id(list) {
        if let Some(sel) = list.selected_row() {
            let name = sel.widget_name();
            let cur = name.contains("|p:1");
            let newv = if cur { 0 } else { 1 };
            let _ = send(&format!("PIN {} {}", id, newv));
        }
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

// update_preview now handled asynchronously via request_preview closure above

#[cfg(feature = "gtk-ui")]
fn scale_pixbuf_fit(pix: &gdk_pixbuf::Pixbuf, max_w: i32, max_h: i32) -> gdk_pixbuf::Pixbuf {
    let w = pix.width();
    let h = pix.height();
    if w <= 0 || h <= 0 { return pix.clone(); }
    let rw = max_w as f64 / w as f64;
    let rh = max_h as f64 / h as f64;
    let r = rw.min(rh).min(1.0);
    let nw = (w as f64 * r).round() as i32;
    let nh = (h as f64 * r).round() as i32;
    pix.scale_simple(nw.max(1), nh.max(1), gdk_pixbuf::InterpType::Bilinear).unwrap_or_else(|| pix.clone())
}

#[cfg(feature = "gtk-ui")]
fn markup_highlight(s: &str, q: &str) -> String {
    if q.is_empty() { return glib::markup_escape_text(s).to_string(); }
    let s_lower = s.to_lowercase();
    let q_lower = q.to_lowercase();
    if let Some(pos) = s_lower.find(&q_lower) {
        let end = pos + q_lower.len();
        let before = glib::markup_escape_text(&s[..pos]).to_string();
        let mid = glib::markup_escape_text(&s[pos..end]).to_string();
        let after = glib::markup_escape_text(&s[end..]).to_string();
        // Use a yellow-ish background for contrast in both themes
        format!("{}<span background='#ffed7f' foreground='#202124'>{}</span>{}", before, mid, after)
    } else {
        glib::markup_escape_text(s).to_string()
    }
}

#[cfg(feature = "gtk-ui")]
fn css_for_theme(dark: bool) -> String {
    if dark {
        return r#"
        .surface {
            background-color: rgba(24,24,28,0.82);
            padding: 8px;
            border-radius: 14px;
        }
        .card {
            background-color: rgba(42,42,48,0.88);
            border-radius: 10px;
            box-shadow: 0 6px 16px rgba(0,0,0,0.35);
            border: 1px solid rgba(255,255,255,0.06);
            transition: background-color 120ms ease, border-color 120ms ease;
        }
        .card:hover { background-color: rgba(52,52,58,0.92); }
        .selected-card { background-color: rgba(60,60,66,0.95); border-color: rgba(255,255,255,0.18); }
        .dim-label { color: #9aa0a6; }
        .empty { color: #b0b6bd; font-size: 14pt; }
        "#.to_string();
    }
    // light theme
    r#"
    .surface {
        background-color: rgba(250,250,252,0.86);
        padding: 8px;
        border-radius: 14px;
    }
    .card {
        background-color: rgba(255,255,255,0.92);
        border-radius: 10px;
        box-shadow: 0 6px 16px rgba(0,0,0,0.15);
        border: 1px solid rgba(0,0,0,0.06);
        transition: background-color 120ms ease, border-color 120ms ease;
    }
    .card:hover { background-color: rgba(255,255,255,1.0); }
    .selected-card { background-color: rgba(245,245,248,1.0); border-color: rgba(0,0,0,0.18); }
    .dim-label { color: #5f6368; }
    .empty { color: #6b7280; font-size: 14pt; }
    "#.to_string()
}

#[cfg(feature = "gtk-ui")]
fn apply_css_with_provider(provider: &gtk::CssProvider, dark: bool) {
    if let Some(settings) = gtk::Settings::default() {
        let _ = settings.set_property("gtk-application-prefer-dark-theme", &dark);
        let _ = settings.set_property("gtk-enable-animations", &true);
    }
    let css = css_for_theme(dark);
    let _ = provider.load_from_data(css.as_bytes());
    if let Some(screen) = Screen::default() {
        gtk::StyleContext::add_provider_for_screen(&screen, provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
}
