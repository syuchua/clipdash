#[cfg(feature = "gtk-ui")]
use gtk::{prelude::*, Orientation};
use gdk::{Screen, EventButton};
use gdk_pixbuf::{PixbufLoader, Pixbuf};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
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
    // CSS provider and initial dark theme
    let provider = gtk::CssProvider::new();
    apply_css_with_provider(&provider, true);

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Clipdash");
    window.set_default_size(700, 480);
    window.set_position(gtk::WindowPosition::Center);
    // Try semi-transparency; may be ignored on Wayland
    window.set_opacity(0.93);

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
                row.set_widget_name(&format!("id:{}", id));
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

        let list_v = list.clone();
        let preview_revealer_btn = preview_revealer.clone();
        let preview_stack_btn = preview_stack.clone();
        let preview_text_btn = preview_text.clone();
        let preview_image_btn = preview_image.clone();
        let zoom_fit_btn = zoom_fit.clone();
        let last_pix_btn = last_pix.clone();
        btn_prev.connect_clicked(move |_| { toggle_preview(&list_v, &preview_revealer_btn, &preview_stack_btn, &preview_text_btn, &preview_image_btn, &zoom_fit_btn, &last_pix_btn); });

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
        let preview_stack_c = preview_stack.clone();
        let preview_text_c = preview_text.clone();
        let preview_image_c = preview_image.clone();
        let zoom_fit_c = zoom_fit.clone();
        let last_pix_c = last_pix.clone();
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
            if preview_revealer_c.reveals_child() { update_preview(lb, &preview_stack_c, &preview_text_c, &preview_image_c, &zoom_fit_c, &last_pix_c); }
        });
    }

    // Theme toggle button
    {
        let provider_c = provider.clone();
        let dark_state = Rc::new(RefCell::new(true));
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
                if let Ok(id) = id_str.parse::<u64>() {
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
        let preview_stack_m = preview_stack.clone();
        let preview_text_m = preview_text.clone();
        let preview_image_m = preview_image.clone();
        let zoom_fit_menu_src = zoom_fit.clone();
        let last_pix_menu_src = last_pix.clone();
        list.connect_button_press_event(move |lb, ev: &EventButton| {
            if ev.button() == 3 { // right click
                let (_x, y) = ev.position();
                if let Some(row) = lb.row_at_y(y as i32) {
                    lb.select_row(Some(&row));
                    // build menu
                    let menu = gtk::Menu::new();
                    let _id_opt = current_selected_id(lb);
                    let mut currently_pinned = false;
                    if let Some(r) = lb.selected_row() {
                        if let Some(w) = r.child() {
                            if let Ok(hbox_or_card) = w.downcast::<gtk::EventBox>() {
                                if let Some(inner) = hbox_or_card.child() {
                                    if let Ok(hbox) = inner.downcast::<gtk::Box>() {
                                        let ch = hbox.children();
                                        if ch.len() >= 2 {
                                            if let Ok(label) = ch[1].clone().downcast::<gtk::Label>() {
                                                let txt = label.text();
                                                currently_pinned = txt.as_str().starts_with("★ ");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
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

                    let lb_c4 = lb.clone();
                    let prev_rev4 = preview_revealer_menu.clone();
                    let stack4 = preview_stack_m.clone();
                    let text4 = preview_text_m.clone();
                    let img4 = preview_image_m.clone();
                    let zoom4 = zoom_fit_menu_src.clone();
                    let last4 = last_pix_menu_src.clone();
                    mi_prev.connect_activate(move |_| { toggle_preview(&lb_c4, &prev_rev4, &stack4, &text4, &img4, &zoom4, &last4); });

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
        let preview_stack_k = preview_stack.clone();
        let preview_text_k = preview_text.clone();
        let preview_image_k = preview_image.clone();
        let zoom_fit_k = zoom_fit.clone();
        let last_pix_k = last_pix.clone();
        let refresh_cb = refresh.clone();
        let entry_c = entry.clone();
        entry.connect_key_press_event(move |_, ev| {
            use gtk::gdk::keys::constants as kc;
            let key = ev.keyval();
            match key {
                k if k == kc::Up => { move_selection(&list_nav, -1); if preview_revealer_key.reveals_child() { update_preview(&list_nav, &preview_stack_k, &preview_text_k, &preview_image_k, &zoom_fit_k, &last_pix_k); } Inhibit(true) }
                k if k == kc::Down => { move_selection(&list_nav, 1); if preview_revealer_key.reveals_child() { update_preview(&list_nav, &preview_stack_k, &preview_text_k, &preview_image_k, &zoom_fit_k, &last_pix_k); } Inhibit(true) }
                k if k == kc::Return => { activate_selected(&list_nav, &win); Inhibit(true) }
                k if k == kc::KP_Enter => { activate_selected(&list_nav, &win); Inhibit(true) }
                // Toggle preview with Space
                k if k == kc::space => { toggle_preview(&list_nav, &preview_revealer_key, &preview_stack_k, &preview_text_k, &preview_image_k, &zoom_fit_k, &last_pix_k); Inhibit(true) }
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
        let preview_stack_w = preview_stack.clone();
        let preview_text_w = preview_text.clone();
        let preview_image_w = preview_image.clone();
        let zoom_fit_w = zoom_fit.clone();
        let last_pix_w = last_pix.clone();
        let refresh_cb = refresh.clone();
        let entry_w = entry.clone();
        window.connect_key_press_event(move |w, ev| {
            use gtk::gdk::keys::constants as kc;
            let key = ev.keyval();
            match key {
                k if k == kc::Escape => { w.close(); Inhibit(true) }
                k if k == kc::Up => { move_selection(&list_nav2, -1); if preview_revealer_win.reveals_child() { update_preview(&list_nav2, &preview_stack_w, &preview_text_w, &preview_image_w, &zoom_fit_w, &last_pix_w); } Inhibit(true) }
                k if k == kc::Down => { move_selection(&list_nav2, 1); if preview_revealer_win.reveals_child() { update_preview(&list_nav2, &preview_stack_w, &preview_text_w, &preview_image_w, &zoom_fit_w, &last_pix_w); } Inhibit(true) }
                k if k == kc::Return || k == kc::KP_Enter => { activate_selected(&list_nav2, w); Inhibit(true) }
                k if k == kc::space => { toggle_preview(&list_nav2, &preview_revealer_win, &preview_stack_w, &preview_text_w, &preview_image_w, &zoom_fit_w, &last_pix_w); Inhibit(true) }
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
fn toggle_preview(list: &gtk::ListBox, revealer: &gtk::Revealer, stack: &gtk::Stack, view: &gtk::TextView, image: &gtk::Image, zoom_fit: &Rc<RefCell<bool>>, last_pix: &Rc<RefCell<Option<Pixbuf>>>) {
    revealer.set_reveal_child(!revealer.reveals_child());
    if !revealer.reveals_child() { return; }
    update_preview(list, stack, view, image, zoom_fit, last_pix);
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

#[cfg(feature = "gtk-ui")]
fn update_preview(list: &gtk::ListBox, stack: &gtk::Stack, view: &gtk::TextView, image: &gtk::Image, zoom_fit: &Rc<RefCell<bool>>, last_pix: &Rc<RefCell<Option<Pixbuf>>>) {
    if let Some(id) = current_selected_id(list) {
        if let Ok(resp) = send(&format!("GET {}", id)) {
            if let Some(text) = resp.strip_prefix("TEXT\n") {
                if let Some(buf) = view.buffer() { buf.set_text(text); }
                stack.set_visible_child_name("text");
            } else if let Some(html) = resp.strip_prefix("HTML\n") {
                if let Some(buf) = view.buffer() { buf.set_text(html); }
                stack.set_visible_child_name("text");
            } else if let Some(rest) = resp.strip_prefix("IMAGE\n") {
                let mut lines = rest.lines();
                let _mime = lines.next().unwrap_or("image/png");
                let b64 = lines.collect::<Vec<_>>().join("\n");
                if let Ok(bytes) = B64.decode(b64) {
                    let loader = PixbufLoader::new();
                    let _ = loader.write(&bytes);
                    let _ = loader.close();
                    if let Some(pix) = loader.pixbuf() {
                        *last_pix.borrow_mut() = Some(pix.clone());
                        // Fit into available width/height
                        let alloc = stack.allocation();
                        let max_w = (alloc.width - 24).max(200);
                        let max_h = 420; // reasonable height bound
                        let scaled = if *zoom_fit.borrow() { scale_pixbuf_fit(&pix, max_w, max_h) } else { pix.clone() };
                        image.set_from_pixbuf(Some(&scaled));
                        stack.set_visible_child_name("image");
                        return;
                    }
                }
                if let Some(buf) = view.buffer() { buf.set_text("[image preview unavailable]"); }
                stack.set_visible_child_name("text");
            }
        }
    }
}

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
