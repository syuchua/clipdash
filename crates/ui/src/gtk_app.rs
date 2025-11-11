use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use gdk::{EventButton, Screen};
use gdk_pixbuf::{Pixbuf, PixbufLoader};
#[cfg(feature = "gtk-ui")]
use glib::{clone, Cast, ObjectExt};
#[cfg(feature = "gtk-ui")]
use gtk::gdk::ModifierType;
#[cfg(feature = "gtk-ui")]
use gtk::{prelude::*, Orientation};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::{cell::RefCell, rc::Rc};
#[cfg(feature = "gtk-ui")]
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

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
#[derive(Clone, Copy)]
enum AcrylicMode {
    Off,
    Fake,
    Auto,
}

struct UiConfig {
    dark: bool,
    opacity: f64,
    max_preview_chars: usize,
    max_image_preview_bytes: usize,
    preview_height: i32,
    preview_min_height: i32,
    acrylic: AcrylicMode,
    blur_strength: f32,
    open_preview_by_default: bool,
    remember_window: bool,
    last_window_w: i32,
    last_window_h: i32,
    remember_pane: bool,
    last_pane_pos: i32,
}

fn load_ui_config() -> UiConfig {
    let mut cfg = UiConfig {
        dark: true,
        opacity: 1.0,
        max_preview_chars: 200_000,
        max_image_preview_bytes: 10_000_000,
        preview_height: 360,
        preview_min_height: 180,
        acrylic: AcrylicMode::Fake,
        blur_strength: 0.4,
        open_preview_by_default: false,
        remember_window: true,
        last_window_w: 700,
        last_window_h: 480,
        remember_pane: true,
        last_pane_pos: 360,
    };
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let path = std::path::Path::new(&home).join(".config/clipdash/config.toml");
    if let Ok(s) = std::fs::read_to_string(path) {
        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(2, '=');
            let k = parts.next().map(|v| v.trim()).unwrap_or("");
            let v = parts.next().map(|v| v.trim()).unwrap_or("");
            if k.eq_ignore_ascii_case("ui.dark") {
                cfg.dark = matches!(v, "true" | "1" | "on" | "yes");
            } else if k.eq_ignore_ascii_case("ui.opacity") {
                if let Ok(f) = v.trim_matches('"').parse::<f64>() {
                    cfg.opacity = f.clamp(0.0, 1.0);
                }
            } else if k.eq_ignore_ascii_case("ui.max_preview_chars") {
                if let Ok(n) = v.trim_matches('"').parse::<usize>() {
                    cfg.max_preview_chars = n.max(10_000).min(2_000_000);
                }
            } else if k.eq_ignore_ascii_case("ui.max_image_preview_bytes") {
                if let Ok(n) = v.trim_matches('"').parse::<usize>() {
                    cfg.max_image_preview_bytes = n.max(200_000).min(50_000_000);
                }
            } else if k.eq_ignore_ascii_case("ui.preview_height") {
                if let Ok(n) = v.trim_matches('"').parse::<i32>() {
                    cfg.preview_height = n.clamp(120, 2000);
                }
            } else if k.eq_ignore_ascii_case("ui.preview_min_height") {
                if let Ok(n) = v.trim_matches('"').parse::<i32>() {
                    cfg.preview_min_height = n.clamp(80, 1000);
                }
            } else if k.eq_ignore_ascii_case("ui.acrylic") {
                let vv = v.trim_matches('"').to_ascii_lowercase();
                cfg.acrylic = match vv.as_str() {
                    "off" => AcrylicMode::Off,
                    "fake" => AcrylicMode::Fake,
                    "auto" => AcrylicMode::Auto,
                    _ => AcrylicMode::Fake,
                };
            } else if k.eq_ignore_ascii_case("ui.blur_strength") {
                if let Ok(f) = v.trim_matches('"').parse::<f32>() {
                    cfg.blur_strength = f.clamp(0.0, 1.0);
                }
            } else if k.eq_ignore_ascii_case("ui.open_preview_by_default") {
                cfg.open_preview_by_default = matches!(v, "true"|"1"|"on"|"yes");
            } else if k.eq_ignore_ascii_case("ui.remember_window") {
                cfg.remember_window = matches!(v, "true"|"1"|"on"|"yes");
            } else if k.eq_ignore_ascii_case("ui.window_w") {
                if let Ok(n) = v.trim_matches('"').parse::<i32>() { cfg.last_window_w = n.max(480).min(4096); }
            } else if k.eq_ignore_ascii_case("ui.window_h") {
                if let Ok(n) = v.trim_matches('"').parse::<i32>() { cfg.last_window_h = n.max(320).min(4096); }
            } else if k.eq_ignore_ascii_case("ui.remember_pane") {
                cfg.remember_pane = matches!(v, "true"|"1"|"on"|"yes");
            } else if k.eq_ignore_ascii_case("ui.pane_pos") {
                if let Ok(n) = v.trim_matches('"').parse::<i32>() { cfg.last_pane_pos = n.max(80).min(2000); }
            }
        }
    }
    cfg
}

pub fn run() -> Result<(), String> {
    gtk::init().map_err(|e| format!("gtk init: {}", e))?;
    let ui_cfg = load_ui_config();
    let ui_cfg_cell = Rc::new(RefCell::new(ui_cfg));
    // CSS provider and initial theme
    let provider = gtk::CssProvider::new();
    apply_css_with_provider(&provider, &ui_cfg_cell.borrow());

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Clipdash");
    if ui_cfg_cell.borrow().remember_window {
        let sz = (ui_cfg_cell.borrow().last_window_w, ui_cfg_cell.borrow().last_window_h);
        window.set_default_size(sz.0, sz.1);
    } else {
        window.set_default_size(700, 480);
    }
    window.set_position(gtk::WindowPosition::Center);
    // Try semi-transparency; may be ignored on Wayland
    if std::env::var("CLIPDASH_UI_NO_OPACITY").ok().as_deref() != Some("1") {
        window.set_opacity(ui_cfg_cell.borrow().opacity);
    }

    // Xorg 下尝试启用 RGBA 透明背景（每像素 alpha）
    // 要求：合成器可用（大多数 GNOME/KDE 在 Xorg 默认有），并且启用了 ARGB visual
    if std::env::var("XDG_SESSION_TYPE").ok().as_deref() == Some("x11") {
        if let Some(screen) = Screen::default() {
            if screen.is_composited() {
                if let Some(vis) = screen.rgba_visual() {
                    window.set_app_paintable(true);
                    window.set_visual(Some(&vis));
                    // 窗口背景设为透明；其余由 .surface/.card 负责绘制半透明面板
                    let tp = gtk::CssProvider::new();
                    let _ = tp.load_from_data(b"window { background-color: transparent; }\n");
                    gtk::StyleContext::add_provider_for_screen(
                        &screen,
                        &tp,
                        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                    );
                }
            }
        }
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
    // Menubar (Actions, View, Preferences)
    let menubar = gtk::MenuBar::new();
    let m_actions = gtk::MenuItem::with_label("Actions");
    let m_view = gtk::MenuItem::with_label("View");
    let m_prefs = gtk::MenuItem::with_label("Preferences");
    let menu_actions = gtk::Menu::new();
    let mi_copy = gtk::MenuItem::with_label("Copy");
    let mi_pin = gtk::MenuItem::with_label("Pin/Unpin");
    let mi_del = gtk::MenuItem::with_label("Delete");
    let mi_clear = gtk::MenuItem::with_label("Clear");
    menu_actions.append(&mi_copy);
    menu_actions.append(&mi_pin);
    menu_actions.append(&mi_del);
    menu_actions.append(&mi_clear);
    m_actions.set_submenu(Some(&menu_actions));
    let menu_view = gtk::Menu::new();
    let mi_preview = gtk::CheckMenuItem::with_label("Preview");
    let mi_fit = gtk::MenuItem::with_label("Fit");
    let mi_actual = gtk::MenuItem::with_label("100%");
    let mi_theme = gtk::CheckMenuItem::with_label("Dark Theme");
    menu_view.append(&mi_preview);
    menu_view.append(&mi_fit);
    menu_view.append(&mi_actual);
    menu_view.append(&mi_theme);
    m_view.set_submenu(Some(&menu_view));
    menubar.append(&m_actions);
    menubar.append(&m_view);
    menubar.append(&m_prefs);

    let scroller = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    let list = gtk::ListBox::new();
    list.set_activate_on_single_click(true);
    scroller.add(&list);

    // Preview area (stack + revealer)
    let preview_text = gtk::TextView::new();
    preview_text.set_wrap_mode(gtk::WrapMode::Word);
    preview_text.set_editable(false);
    let preview_image = gtk::Image::new();
    let image_scroller =
        gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    image_scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    image_scroller.add(&preview_image);
    let preview_stack = gtk::Stack::new();
    // 禁用复杂过渡动画，减少合成器负担
    preview_stack.set_transition_type(gtk::StackTransitionType::None);
    preview_stack.add_named(&preview_text, "text");
    preview_stack.add_named(&image_scroller, "image");
    #[cfg(feature = "html-webkit")]
    let webview = {
        use webkit2gtk::prelude::*;
        let v = webkit2gtk::WebView::new();
        if let Some(settings) = v.settings() {
            settings.set_enable_javascript(false);
            settings.set_enable_plugins(false);
            settings.set_enable_write_console_messages_to_stdout(false);
            settings.set_enable_developer_extras(false);
        }
        preview_stack.add_named(&v, "html");
        v
    };
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
        ImageTooLarge { mime: String, size: usize },
        Error(String),
    }
    let (txp, rxp) = glib::MainContext::channel::<(u64, PreviewMsg)>(glib::PRIORITY_DEFAULT);
    let preview_seq = Arc::new(AtomicU64::new(0));

    // Initialize View→Preview state from config
    if ui_cfg_cell.borrow().open_preview_by_default { mi_preview.set_active(true); }
    vbox.pack_start(&menubar, false, false, 0);
    vbox.pack_start(&entry, false, false, 0);
    vbox.pack_start(&infobar, false, false, 0);
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
    // 通过可拖拽的垂直分割增强预览区域可伸缩性
    let pane = gtk::Paned::new(Orientation::Vertical);
    pane.add1(&stack);
    pane.add2(&preview_revealer);
    // 预览区最小高度，初始高度
    preview_frame.set_size_request(-1, ui_cfg_cell.borrow().preview_min_height);
    if ui_cfg_cell.borrow().remember_pane && ui_cfg_cell.borrow().last_pane_pos > 0 {
        pane.set_position(ui_cfg_cell.borrow().last_pane_pos);
    } else {
        pane.set_position(ui_cfg_cell.borrow().preview_height);
    }
    vbox.pack_start(&pane, true, true, 0);
    window.add(&vbox);

    // Preferences dialog binding (after pane/frame constructed)
    {
        let prefs_action_window = window.clone();
        let prefs_action_provider = provider.clone();
        let prefs_action_pane = pane.clone();
        let prefs_action_preview_frame = preview_frame.clone();
        let prefs_cfg = ui_cfg_cell.clone();
        m_prefs.connect_activate(move |_| {
            open_preferences_dialog(
                &prefs_action_window,
                &prefs_action_provider,
                &prefs_action_pane,
                &prefs_action_preview_frame,
                &prefs_cfg,
            );
        });
    }

    // Helper: adjust window height when toggling preview (non-additive)
    let initial_base = if ui_cfg_cell.borrow().remember_window {
        let mut b = ui_cfg_cell.borrow().last_window_h;
        if ui_cfg_cell.borrow().open_preview_by_default {
            b = (b - ui_cfg_cell.borrow().preview_height).max(420);
        }
        b
    } else { 480 };
    let base_h = Rc::new(RefCell::new(initial_base));
    let adjust_on_toggle = {
        let window = window.clone();
        let pane = pane.clone();
        let cfg = ui_cfg_cell.clone();
        let base_h = base_h.clone();
        move |revealed: bool| {
            let (cur_w, cur_h) = (window.allocation().width, window.allocation().height);
            if revealed {
                let base = *base_h.borrow();
                let target = base + cfg.borrow().preview_height.max(cfg.borrow().preview_min_height);
                let mut new_h = target;
                if let Some(screen) = Screen::default() {
                    let (_sw, sh) = (screen.width(), screen.height());
                    let cap = (sh as f64 * 0.9) as i32;
                    if new_h > cap { new_h = cap; }
                }
                window.resize(cur_w.max(700), new_h.max(480));
                pane.set_position(cfg.borrow().preview_height);
            } else {
                let sub = cfg.borrow().preview_height;
                let new_base = (cur_h - sub).max(420);
                *base_h.borrow_mut() = new_base;
                window.resize(cur_w, new_base);
            }
        }
    };

    // Save window/pane positions on close
    {
        let cfg_save = ui_cfg_cell.clone();
        let pane_ref = pane.clone();
        window.connect_delete_event(move |w, _| {
            let alloc = w.allocation();
            let mut cfg = cfg_save.borrow_mut();
            if cfg.remember_window {
                cfg.last_window_w = alloc.width;
                cfg.last_window_h = alloc.height;
            }
            if cfg.remember_pane {
                cfg.last_pane_pos = pane_ref.position();
            }
            let _ = save_ui_config(&cfg);
            Inhibit(false)
        });
    }

    // If configured, open preview by default on startup
    if ui_cfg_cell.borrow().open_preview_by_default {
        preview_revealer.set_reveal_child(true);
        // window expand
        (adjust_on_toggle.clone())(true);
    }

    // Channel to update list from worker thread
    let (tx, rx) = glib::MainContext::channel::<Vec<(u64, String, bool, String, String)>>(
        glib::PRIORITY_DEFAULT,
    );
    // Error channel for connection issues
    let (txe, rxe) = glib::MainContext::channel::<String>(glib::PRIORITY_DEFAULT);
    let q_state = Rc::new(RefCell::new(String::new()));
    {
        let list = list.clone();
        let stack = stack.clone();
        let q_state = q_state.clone();
        rx.attach(None, move |items| {
            for child in list.children() {
                list.remove(&child);
            }
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
                let icon = match kind.as_str() {
                    "Image" => "🖼 ",
                    "Html" => "</> ",
                    _ => {
                        if mime.starts_with("image/") {
                            "🖼 "
                        } else if mime == "text/html" {
                            "</> "
                        } else {
                            "T "
                        }
                    }
                };
                title_label.set_markup(&markup_highlight(
                    &format!("{}{}{}", if pinned { "★ " } else { "" }, icon, title),
                    &q,
                ));
                // Tooltip shows mime when available
                if !mime.is_empty() {
                    title_label.set_tooltip_text(Some(&mime));
                }
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
                row.set_widget_name(&format!("id:{}|p:{}", id, if pinned { 1 } else { 0 }));
                if pinned {
                    pinned_rows.push(row);
                } else {
                    normal_rows.push(row);
                }
            }
            for r in pinned_rows {
                list.add(&r);
            }
            for r in normal_rows {
                list.add(&r);
            }
            // Toggle empty state
            let count = list.children().len();
            stack.set_visible_child_name(if count == 0 { "empty" } else { "list" });
            // Select first row by default
            if let Some(first) = list
                .children()
                .get(0)
                .and_then(|w| w.clone().downcast::<gtk::ListBoxRow>().ok())
            {
                list.select_row(Some(&first));
            }
            list.show_all();
            glib::Continue(true)
        });
        // Error dialog handler
        let win = window.clone();
        rxe.attach(None, move |msg| {
            let d = gtk::MessageDialog::new(
                Some(&win),
                gtk::DialogFlags::MODAL,
                gtk::MessageType::Error,
                gtk::ButtonsType::Ok,
                &msg,
            );
            d.run();
            unsafe {
                d.destroy();
            }
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
                let cmd = if q.is_empty() {
                    "LIST 200".to_string()
                } else {
                    format!("LIST 200 {}", q)
                };
                let resp = match send(&cmd) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = txe.send(format!("连接守护失败: {}", e));
                        let _ = tx.send(Vec::new());
                        return;
                    }
                };
                let mut items: Vec<(u64, String, bool, String, String)> = Vec::new();
                let mut lines = resp.lines();
                if let Some(h) = lines.next() {
                    if !h.starts_with("OK ") {
                        let _ = tx.send(Vec::new());
                        return;
                    }
                }
                for l in lines {
                    let mut p = l.splitn(5, '\t');
                    let id = p.next();
                    let kind = p.next();
                    let pinned = p.next();
                    let title = p.next();
                    let mime = p.next();
                    if let (Some(id), Some(kind), Some(pinned), Some(title)) =
                        (id, kind, pinned, title)
                    {
                        let mime_s = mime.unwrap_or("");
                        if let Ok(idn) = id.parse() {
                            items.push((
                                idn,
                                title.to_string(),
                                pinned == "1",
                                kind.to_string(),
                                mime_s.to_string(),
                            ));
                        }
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
        let timer: std::rc::Rc<std::cell::RefCell<Option<glib::SourceId>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let timer_c = timer.clone();
        entry.connect_changed(move |e| {
            let q = e.text().to_string();
            *q_state.borrow_mut() = q.clone();
            if let Some(id) = timer_c.borrow_mut().take() {
                glib::source::source_remove(id);
            }
            let refresh = refresh.clone();
            *timer_c.borrow_mut() = Some(glib::timeout_add_local(
                std::time::Duration::from_millis(150),
                {
                    let refresh = refresh.clone();
                    let q = q.clone();
                    move || {
                        refresh(q.clone());
                        glib::Continue(false)
                    }
                },
            ));
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
            glib::timeout_add_local(std::time::Duration::from_millis(1200), move || {
                ib.hide();
                glib::Continue(false)
            });
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
        let image_scroller_ui = image_scroller.clone();
        let zoom_fit_ui = zoom_fit.clone();
        let last_pix_ui = last_pix.clone();
        let seq_ui = preview_seq.clone();
        #[cfg(feature = "html-webkit")]
        let webview_ui = webview.clone();
        rxp.attach(None, move |(seqn, msg)| {
            if seqn != seq_ui.load(Ordering::SeqCst) {
                return glib::Continue(true);
            }
            match msg {
                PreviewMsg::Text(s) => {
                    set_textview_with_markdown(&preview_text_ui, &s);
                    preview_stack_ui.set_visible_child_name("text");
                }
                PreviewMsg::Html(s) => {
                    // 禁用 HTML 渲染：转为纯文本并按 Markdown 样式（若有）渲染
                    let plain = html_to_text(&s);
                    set_textview_with_markdown(&preview_text_ui, &plain);
                    preview_stack_ui.set_visible_child_name("text");
                }
                PreviewMsg::ImageTooLarge { mime, size } => {
                    if let Some(buf) = preview_text_ui.buffer() {
                        buf.set_text(&format!("[image {} too large: {} bytes]", mime, size));
                    }
                    preview_stack_ui.set_visible_child_name("text");
                }
                PreviewMsg::Image { mime: _mime, bytes } => {
                    let loader = PixbufLoader::new();
                    let _ = loader.write(&bytes);
                    let _ = loader.close();
                    if let Some(pix) = loader.pixbuf() {
                        *last_pix_ui.borrow_mut() = Some(pix.clone());
                        let alloc = image_scroller_ui.allocation();
                        let max_w = (alloc.width - 24).max(100);
                        let max_h = (alloc.height - 24).max(100);
                        let scaled = if *zoom_fit_ui.borrow() {
                            scale_pixbuf_fit(&pix, max_w, max_h)
                        } else {
                            pix.clone()
                        };
                        preview_image_ui.set_from_pixbuf(Some(&scaled));
                        preview_stack_ui.set_visible_child_name("image");
                    } else {
                        if let Some(buf) = preview_text_ui.buffer() {
                            buf.set_text("[image preview unavailable]");
                        }
                        preview_stack_ui.set_visible_child_name("text");
                    }
                }
                PreviewMsg::Error(e) => {
                    if let Some(buf) = preview_text_ui.buffer() {
                        buf.set_text(&format!("[preview error] {}", e));
                    }
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
        let max_chars_cfg = ui_cfg_cell.borrow().max_preview_chars;
        let ui_cfg_for_req = ui_cfg_cell.clone();
        std::rc::Rc::new(move || {
            if let Some(id) = current_selected_id(&list_rp) {
                let my = seq.fetch_add(1, Ordering::SeqCst).saturating_add(1);
                let txp_outer = txp.clone();
                let img_max = ui_cfg_for_req.borrow().max_image_preview_bytes;
                std::thread::spawn(move || {
                    let resp = match send(&format!("GET {}", id)) {
                        Ok(s) => s,
                        Err(e) => {
                            let _ = txp_outer.send((my, PreviewMsg::Error(format!("{}", e))));
                            return;
                        }
                    };
                    if let Some(text) = resp.strip_prefix("TEXT\n") {
                        let s = if text.len() > max_chars_cfg {
                            format!("{}\n… [truncated]", &text[..max_chars_cfg])
                        } else {
                            text.to_string()
                        };
                        let _ = txp_outer.send((my, PreviewMsg::Text(s)));
                    } else if let Some(html) = resp.strip_prefix("HTML\n") {
                        // Show raw HTML text for now (avoid WebKit by default)
                        let s = if html.len() > max_chars_cfg {
                            format!("{}\n… [truncated]", &html[..max_chars_cfg])
                        } else {
                            html.to_string()
                        };
                        let _ = txp_outer.send((my, PreviewMsg::Html(s)));
                    } else if let Some(rest) = resp.strip_prefix("IMAGE\n") {
                        let mut lines = rest.lines();
                        let mime = lines.next().unwrap_or("image/png").to_string();
                        let b64 = lines.collect::<Vec<_>>().join("\n");
                        match B64.decode(b64) {
                            Ok(bytes) => {
                                let sz = bytes.len();
                                if sz > img_max {
                                    let _ = txp_outer
                                        .send((my, PreviewMsg::ImageTooLarge { mime, size: sz }));
                                } else {
                                    let _ = txp_outer.send((my, PreviewMsg::Image { mime, bytes }));
                                }
                            }
                            Err(e) => {
                                let _ = txp_outer
                                    .send((my, PreviewMsg::Error(format!("base64: {}", e))));
                            }
                        }
                    } else {
                        let _ = txp_outer.send((my, PreviewMsg::Error("unknown response".into())));
                    }
                });
            }
        })
    };

    // Menu actions and handlers
    {

        // Menu actions
        let lb_copy = list.clone();
        let show_copy = show_status.clone();
        mi_copy.connect_activate(move |_| {
            if let Some(id) = current_selected_id(&lb_copy) {
                let _ = send(&format!("PASTE {}", id));
                show_copy("Copied", gtk::MessageType::Info);
            }
        });
        let lb_pin = list.clone();
        let entry_pin = entry.clone();
        let refresh_pin = refresh.clone();
        let show_pin = show_status.clone();
        mi_pin.connect_activate(move |_| {
            pin_toggle(&lb_pin);
            refresh_pin(entry_pin.text().to_string());
            show_pin("Toggled pin", gtk::MessageType::Other);
        });
        let lb_del = list.clone();
        let entry_del = entry.clone();
        let refresh_del = refresh.clone();
        let show_del = show_status.clone();
        mi_del.connect_activate(move |_| {
            delete_selected(&lb_del);
            refresh_del(entry_del.text().to_string());
            show_del("Deleted", gtk::MessageType::Other);
        });
        let entry_cl = entry.clone();
        let refresh_cl = refresh.clone();
        let show_cl = show_status.clone();
        mi_clear.connect_activate(move |_| {
            clear_all();
            refresh_cl(entry_cl.text().to_string());
            show_cl("Cleared", gtk::MessageType::Warning);
        });
        let preview_revealer_btn = preview_revealer.clone();
        let req = request_preview.clone();
        let adjust = adjust_on_toggle.clone();
        mi_preview.connect_toggled(move |mi| {
            let reveal = mi.is_active();
            preview_revealer_btn.set_reveal_child(reveal);
            adjust(reveal);
            if reveal { (*req)(); }
        });
        // View menu Fit/100%
        let preview_stack_fit = preview_stack.clone();
        let image_scroller_fit = image_scroller.clone();
        let preview_image_fit = preview_image.clone();
        let zoom_fit_fit = zoom_fit.clone();
        let last_pix_fit = last_pix.clone();
        mi_fit.connect_activate(move |_| {
            *zoom_fit_fit.borrow_mut() = true;
            if let Some(pix) = last_pix_fit.borrow().clone() {
                let alloc = image_scroller_fit.allocation();
                let max_w = (alloc.width - 24).max(100);
                let max_h = (alloc.height - 24).max(100);
                let scaled = scale_pixbuf_fit(&pix, max_w, max_h);
                preview_image_fit.set_from_pixbuf(Some(&scaled));
                preview_stack_fit.set_visible_child_name("image");
            }
        });
        let preview_stack_100 = preview_stack.clone();
        let preview_image_100 = preview_image.clone();
        let zoom_fit_100 = zoom_fit.clone();
        let last_pix_100 = last_pix.clone();
        mi_actual.connect_activate(move |_| {
            *zoom_fit_100.borrow_mut() = false;
            if let Some(pix) = last_pix_100.borrow().clone() {
                preview_image_100.set_from_pixbuf(Some(&pix));
                preview_stack_100.set_visible_child_name("image");
            }
        });
        // Theme toggle
        let provider_c = provider.clone();
        let cfg_c = ui_cfg_cell.clone();
        mi_theme.connect_toggled(move |mi| {
            cfg_c.borrow_mut().dark = mi.is_active();
            apply_css_with_provider(&provider_c, &cfg_c.borrow());
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
                        if let Ok(card) = w.downcast::<gtk::EventBox>() {
                            card.style_context().remove_class("selected-card");
                        }
                    }
                }
            }
            if let Some(row) = row_opt {
                if let Some(w) = row.child() {
                    if let Ok(card) = w.downcast::<gtk::EventBox>() {
                        card.style_context().add_class("selected-card");
                    }
                }
            }
            if preview_revealer_c.reveals_child() {
                (*req)();
            }
        });
    }

    // 当预览容器大小变化时，若处于“适应窗口”模式则重新缩放，避免拉伸失真
    {
        let _image_scroller_rsz = image_scroller.clone();
        let preview_image_rsz = preview_image.clone();
        let zoom_fit_rsz = zoom_fit.clone();
        let last_pix_rsz = last_pix.clone();
        image_scroller.connect_size_allocate(move |sc, _| {
            if *zoom_fit_rsz.borrow() {
                if let Some(pix) = last_pix_rsz.borrow().as_ref() {
                    let alloc = sc.allocation();
                    let max_w = (alloc.width - 24).max(100);
                    let max_h = (alloc.height - 24).max(100);
                    let scaled = scale_pixbuf_fit(pix, max_w, max_h);
                    preview_image_rsz.set_from_pixbuf(Some(&scaled));
                }
            }
        });
    }

    // Theme toggle button removed; handled via menu

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
        let adjust_for_ctx = adjust_on_toggle.clone();
        let entry_c = entry.clone();
        let refresh_c = refresh.clone();
        let preview_revealer_menu = preview_revealer.clone();
        // removed unused clones for menu preview
        let req_menu = request_preview.clone();
        list.connect_button_press_event(move |lb, ev: &EventButton| {
            if ev.button() == 3 {
                // right click
                let (_x, y) = ev.position();
                if let Some(row) = lb.row_at_y(y as i32) {
                    lb.select_row(Some(&row));
                    // build menu
                    let menu = gtk::Menu::new();
                    let _id_opt = current_selected_id(lb);
                    let mut currently_pinned = false;
                    if let Some(r) = lb.selected_row() {
                        let name = r.widget_name();
                        currently_pinned = name.contains("|p:1");
                    }
                    let mi_copy = gtk::MenuItem::with_label("Copy");
                    let mi_pin =
                        gtk::MenuItem::with_label(if currently_pinned { "Unpin" } else { "Pin" });
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
                    mi_copy.connect_activate(move |_| {
                        if let Some(id) = current_selected_id(&lb_c1) {
                            let _ = send(&format!("PASTE {}", id));
                            show("Copied", gtk::MessageType::Info);
                        }
                    });

                    let lb_c2 = lb.clone();
                    let entry_c2 = entry_c.clone();
                    let refresh_c2 = refresh_c.clone();
                    let show = show_status.clone();
                    mi_pin.connect_activate(move |_| {
                        pin_toggle(&lb_c2);
                        refresh_c2(entry_c2.text().to_string());
                        show("Toggled pin", gtk::MessageType::Other);
                    });

                    let lb_c3 = lb.clone();
                    let entry_c3 = entry_c.clone();
                    let refresh_c3 = refresh_c.clone();
                    let show = show_status.clone();
                    mi_del.connect_activate(move |_| {
                        delete_selected(&lb_c3);
                        refresh_c3(entry_c3.text().to_string());
                        show("Deleted", gtk::MessageType::Other);
                    });

                    let prev_rev4 = preview_revealer_menu.clone();
                    let req_call = req_menu.clone();
                    let adjust_call = adjust_for_ctx.clone();
                    mi_prev.connect_activate(move |_| {
                        let newv = !prev_rev4.reveals_child();
                        prev_rev4.set_reveal_child(newv);
                        adjust_call(newv);
                        if newv { (*req_call)(); }
                    });

                    // Popup
                    menu.popup_easy(ev.button(), ev.time());
                }
                Inhibit(true)
            } else {
                Inhibit(false)
            }
        });
    }

    // Key handling: Up/Down to navigate, Enter to paste, Escape to close
    {
        let list_nav = list.clone();
        let win = window.clone();
        let preview_revealer_key = preview_revealer.clone();
        let adjust = adjust_on_toggle.clone();
        // removed unused preview clones
        let refresh_cb = refresh.clone();
        let entry_c = entry.clone();
        let req = request_preview.clone();
        entry.connect_key_press_event(move |_, ev| {
            use gtk::gdk::keys::constants as kc;
            let key = ev.keyval();
            match key {
                k if k == kc::Up => {
                    move_selection(&list_nav, -1);
                    if preview_revealer_key.reveals_child() {
                        (*req)();
                    }
                    Inhibit(true)
                }
                k if k == kc::Down => {
                    move_selection(&list_nav, 1);
                    if preview_revealer_key.reveals_child() {
                        (*req)();
                    }
                    Inhibit(true)
                }
                k if k == kc::Return => {
                    activate_selected(&list_nav, &win);
                    Inhibit(true)
                }
                k if k == kc::KP_Enter => {
                    activate_selected(&list_nav, &win);
                    Inhibit(true)
                }
                // Toggle preview with Space
                k if k == kc::space => { let newv = !preview_revealer_key.reveals_child(); preview_revealer_key.set_reveal_child(newv); adjust(newv); if newv { (*req)(); } Inhibit(true) }
                // Pin/unpin with 'p'
                k if k == kc::p => {
                    pin_toggle(&list_nav);
                    refresh_cb(entry_c.text().to_string());
                    Inhibit(true)
                }
                // Delete selected
                k if k == kc::Delete => {
                    delete_selected(&list_nav);
                    refresh_cb(entry_c.text().to_string());
                    Inhibit(true)
                }
                // Ctrl+L clear
                k if k == kc::l && ev.state().contains(ModifierType::CONTROL_MASK) => {
                    clear_all();
                    refresh_cb(entry_c.text().to_string());
                    Inhibit(true)
                }
                _ => Inhibit(false),
            }
        });

        let list_nav2 = list.clone();
        let preview_revealer_win = preview_revealer.clone();
        let adjust2 = adjust_on_toggle.clone();
        // removed unused preview clones
        let refresh_cb = refresh.clone();
        let entry_w = entry.clone();
        let req = request_preview.clone();
        window.connect_key_press_event(move |w, ev| {
            use gtk::gdk::keys::constants as kc;
            let key = ev.keyval();
            match key {
                k if k == kc::Escape => {
                    w.close();
                    Inhibit(true)
                }
                k if k == kc::Up => {
                    move_selection(&list_nav2, -1);
                    if preview_revealer_win.reveals_child() {
                        (*req)();
                    }
                    Inhibit(true)
                }
                k if k == kc::Down => {
                    move_selection(&list_nav2, 1);
                    if preview_revealer_win.reveals_child() {
                        (*req)();
                    }
                    Inhibit(true)
                }
                k if k == kc::Return || k == kc::KP_Enter => {
                    activate_selected(&list_nav2, w);
                    Inhibit(true)
                }
                k if k == kc::space => { let newv = !preview_revealer_win.reveals_child(); preview_revealer_win.set_reveal_child(newv); adjust2(newv); if newv { (*req)(); } Inhibit(true) }
                k if k == kc::p => {
                    pin_toggle(&list_nav2);
                    refresh_cb(entry_w.text().to_string());
                    Inhibit(true)
                }
                k if k == kc::Delete => {
                    delete_selected(&list_nav2);
                    refresh_cb(entry_w.text().to_string());
                    Inhibit(true)
                }
                k if k == kc::l && ev.state().contains(ModifierType::CONTROL_MASK) => {
                    clear_all();
                    refresh_cb(entry_w.text().to_string());
                    Inhibit(true)
                }
                _ => Inhibit(false),
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
    if len == 0 {
        return;
    }
    let current_idx: i32 = list.selected_row().map(|r| r.index()).unwrap_or(0);
    let mut idx = current_idx + delta;
    if idx < 0 {
        idx = 0;
    }
    if idx >= len {
        idx = len - 1;
    }
    if let Some(row) = list.row_at_index(idx) {
        list.select_row(Some(&row));
    }
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
    if let Some(id) = current_selected_id(list) {
        let _ = send(&format!("DELETE {}", id));
    }
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
    if w <= 0 || h <= 0 {
        return pix.clone();
    }
    let rw = max_w as f64 / w as f64;
    let rh = max_h as f64 / h as f64;
    let r = rw.min(rh).min(1.0);
    let nw = (w as f64 * r).round() as i32;
    let nh = (h as f64 * r).round() as i32;
    pix.scale_simple(nw.max(1), nh.max(1), gdk_pixbuf::InterpType::Bilinear)
        .unwrap_or_else(|| pix.clone())
}

#[cfg(feature = "gtk-ui")]
fn markup_highlight(s: &str, q: &str) -> String {
    if q.is_empty() {
        return glib::markup_escape_text(s).to_string();
    }
    let s_lower = s.to_lowercase();
    let q_lower = q.to_lowercase();
    if let Some(pos) = s_lower.find(&q_lower) {
        let end = pos + q_lower.len();
        let before = glib::markup_escape_text(&s[..pos]).to_string();
        let mid = glib::markup_escape_text(&s[pos..end]).to_string();
        let after = glib::markup_escape_text(&s[end..]).to_string();
        // Use a yellow-ish background for contrast in both themes
        format!(
            "{}<span background='#ffed7f' foreground='#202124'>{}</span>{}",
            before, mid, after
        )
    } else {
        glib::markup_escape_text(s).to_string()
    }
}

#[cfg(feature = "gtk-ui")]
fn html_to_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => {
                in_tag = true;
            }
            '>' => {
                in_tag = false;
            }
            _ => {
                if !in_tag {
                    out.push(c);
                }
            }
        }
    }
    out = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");
    out
}

#[cfg(feature = "gtk-ui")]
fn set_textview_with_markdown(view: &gtk::TextView, input: &str) {
    use gtk::pango::Style;
    use gtk::prelude::*;
    if let Some(buf) = view.buffer() {
        buf.set_text("");
        let table = match buf.tag_table() {
            Some(t) => t,
            None => {
                return;
            }
        };
        let tag_bold = gtk::TextTag::new(Some("md_bold"));
        tag_bold.set_weight(700);
        let tag_italic = gtk::TextTag::new(Some("md_italic"));
        tag_italic.set_style(Style::Italic);
        let tag_code = gtk::TextTag::new(Some("md_code"));
        tag_code.set_family(Some("monospace"));
        let tag_head = gtk::TextTag::new(Some("md_head"));
        tag_head.set_weight(700);
        tag_head.set_scale(1.2);
        table.add(&tag_bold);
        table.add(&tag_italic);
        table.add(&tag_code);
        table.add(&tag_head);

        for raw_line in input.lines() {
            let mut line = raw_line.to_string();
            let mut head = false;
            if line.starts_with("### ") {
                head = true;
                line = line[4..].to_string();
            } else if line.starts_with("## ") {
                head = true;
                line = line[3..].to_string();
            } else if line.starts_with("# ") {
                head = true;
                line = line[2..].to_string();
            }
            if line.starts_with("- ") || line.starts_with("* ") {
                line = format!("• {}", &line[2..]);
            }
            // 1. item → 1）item（简单处理）
            if line.len() > 3
                && line.chars().nth(1) == Some('.')
                && line.chars().nth(2) == Some(' ')
                && line.chars().next().unwrap_or('0').is_ascii_digit()
            {
                let mut chars = line.chars();
                let n = chars.next().unwrap();
                let rest: String = chars.skip(2).collect();
                line = format!("{}）{}", n, rest);
            }

            // inline: **bold**, `code`
            let mut i = 0usize;
            let bytes = line.as_bytes();
            let mut last = 0usize;
            while i < bytes.len() {
                if i + 1 < bytes.len() && &bytes[i..i + 2] == b"**" {
                    if last < i {
                        let mut it = buf.end_iter();
                        buf.insert(&mut it, &line[last..i]);
                    }
                    if let Some(j) = line[i + 2..].find("**") {
                        let start_off = buf.end_iter().offset();
                        let mut it = buf.end_iter();
                        let content = &line[i + 2..i + 2 + j];
                        buf.insert(&mut it, content);
                        let mut s_iter = buf.start_iter();
                        s_iter.set_offset(start_off);
                        let e_iter = buf.end_iter();
                        buf.apply_tag(&tag_bold, &s_iter, &e_iter);
                        i = i + 2 + j + 2;
                        last = i;
                        continue;
                    }
                } else if bytes[i] == b'`' {
                    if last < i {
                        let mut it = buf.end_iter();
                        buf.insert(&mut it, &line[last..i]);
                    }
                    if let Some(j) = line[i + 1..].find('`') {
                        let start_off = buf.end_iter().offset();
                        let mut it = buf.end_iter();
                        let content = &line[i + 1..i + 1 + j];
                        buf.insert(&mut it, content);
                        let mut s_iter = buf.start_iter();
                        s_iter.set_offset(start_off);
                        let e_iter = buf.end_iter();
                        buf.apply_tag(&tag_code, &s_iter, &e_iter);
                        i = i + 1 + j + 1;
                        last = i;
                        continue;
                    }
                }
                i += 1;
            }
            if last < line.len() {
                let mut it = buf.end_iter();
                buf.insert(&mut it, &line[last..]);
            }
            if head {
                let line_len = line.len() as i32;
                if line_len > 0 {
                    let end_off = buf.end_iter().offset();
                    let start_off = end_off.saturating_sub(line_len);
                    let mut s_iter = buf.start_iter();
                    s_iter.set_offset(start_off);
                    let e_iter = buf.end_iter();
                    buf.apply_tag(&tag_head, &s_iter, &e_iter);
                }
            }
            let mut it = buf.end_iter();
            buf.insert(&mut it, "\n");
        }
    }
}

#[cfg(feature = "gtk-ui")]
fn open_preferences_dialog(
    parent: &gtk::Window,
    provider: &gtk::CssProvider,
    pane: &gtk::Paned,
    preview_frame: &gtk::Frame,
    cfg_cell: &Rc<RefCell<UiConfig>>,
) {
    let dialog = gtk::Dialog::with_buttons(
        Some("Preferences"),
        Some(parent),
        gtk::DialogFlags::MODAL,
        &[("Cancel", gtk::ResponseType::Cancel), ("OK", gtk::ResponseType::Ok)],
    );
    let content = dialog.content_area();
    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(8);

    let dark_switch = gtk::Switch::new();
    dark_switch.set_active(cfg_cell.borrow().dark);
    let opacity_adj = gtk::Adjustment::new(cfg_cell.borrow().opacity, 0.0, 1.0, 0.01, 0.1, 0.0);
    let opacity_spin = gtk::SpinButton::new(Some(&opacity_adj), 0.01, 2);
    let blur_adj = gtk::Adjustment::new(cfg_cell.borrow().blur_strength as f64, 0.0, 1.0, 0.05, 0.1, 0.0);
    let blur_spin = gtk::SpinButton::new(Some(&blur_adj), 0.05, 2);
    let ph_adj = gtk::Adjustment::new(cfg_cell.borrow().preview_height as f64, 100.0, 2000.0, 10.0, 50.0, 0.0);
    let ph_spin = gtk::SpinButton::new(Some(&ph_adj), 10.0, 0);
    let pmin_adj = gtk::Adjustment::new(cfg_cell.borrow().preview_min_height as f64, 80.0, 1000.0, 10.0, 50.0, 0.0);
    let pmin_spin = gtk::SpinButton::new(Some(&pmin_adj), 10.0, 0);

    // Advanced toggles
    let open_preview_chk = gtk::CheckButton::with_label("Open preview by default");
    open_preview_chk.set_active(cfg_cell.borrow().open_preview_by_default);
    let remember_win_chk = gtk::CheckButton::with_label("Remember window size");
    remember_win_chk.set_active(cfg_cell.borrow().remember_window);
    let win_w_adj = gtk::Adjustment::new(cfg_cell.borrow().last_window_w as f64, 480.0, 4096.0, 10.0, 50.0, 0.0);
    let win_w_spin = gtk::SpinButton::new(Some(&win_w_adj), 10.0, 0);
    let win_h_adj = gtk::Adjustment::new(cfg_cell.borrow().last_window_h as f64, 320.0, 4096.0, 10.0, 50.0, 0.0);
    let win_h_spin = gtk::SpinButton::new(Some(&win_h_adj), 10.0, 0);
    win_w_spin.set_sensitive(cfg_cell.borrow().remember_window);
    win_h_spin.set_sensitive(cfg_cell.borrow().remember_window);
    let remember_pane_chk = gtk::CheckButton::with_label("Remember preview pane");
    remember_pane_chk.set_active(cfg_cell.borrow().remember_pane);
    let pane_pos_adj = gtk::Adjustment::new(cfg_cell.borrow().last_pane_pos as f64, 80.0, 2000.0, 10.0, 50.0, 0.0);
    let pane_pos_spin = gtk::SpinButton::new(Some(&pane_pos_adj), 10.0, 0);
    pane_pos_spin.set_sensitive(cfg_cell.borrow().remember_pane);

    grid.attach(&gtk::Label::new(Some("Dark theme")), 0, 0, 1, 1);
    grid.attach(&dark_switch, 1, 0, 1, 1);
    grid.attach(&gtk::Label::new(Some("Opacity")), 0, 1, 1, 1);
    grid.attach(&opacity_spin, 1, 1, 1, 1);
    grid.attach(&gtk::Label::new(Some("Acrylic strength")), 0, 2, 1, 1);
    grid.attach(&blur_spin, 1, 2, 1, 1);
    grid.attach(&gtk::Label::new(Some("Preview height")), 0, 3, 1, 1);
    grid.attach(&ph_spin, 1, 3, 1, 1);
    grid.attach(&gtk::Label::new(Some("Preview min height")), 0, 4, 1, 1);
    grid.attach(&pmin_spin, 1, 4, 1, 1);
    grid.attach(&open_preview_chk, 0, 5, 2, 1);
    grid.attach(&remember_win_chk, 0, 6, 2, 1);
    grid.attach(&gtk::Label::new(Some("Window width")), 0, 7, 1, 1);
    grid.attach(&win_w_spin, 1, 7, 1, 1);
    grid.attach(&gtk::Label::new(Some("Window height")), 0, 8, 1, 1);
    grid.attach(&win_h_spin, 1, 8, 1, 1);
    grid.attach(&remember_pane_chk, 0, 9, 2, 1);
    grid.attach(&gtk::Label::new(Some("Pane position")), 0, 10, 1, 1);
    grid.attach(&pane_pos_spin, 1, 10, 1, 1);

    // sensitivity toggles
    remember_win_chk.connect_toggled(clone!(@weak win_w_spin, @weak win_h_spin => move |chk| {
        let s = chk.is_active(); win_w_spin.set_sensitive(s); win_h_spin.set_sensitive(s);
    }));
    remember_pane_chk.connect_toggled(clone!(@weak pane_pos_spin => move |chk| {
        pane_pos_spin.set_sensitive(chk.is_active());
    }));

    content.add(&grid);
    dialog.show_all();
    let resp = dialog.run();
    if resp == gtk::ResponseType::Ok {
        let mut cfg = cfg_cell.borrow_mut();
        cfg.dark = dark_switch.is_active();
        cfg.opacity = opacity_spin.value();
        cfg.blur_strength = blur_spin.value() as f32;
        cfg.preview_height = ph_spin.value() as i32;
        cfg.preview_min_height = pmin_spin.value() as i32;
        cfg.open_preview_by_default = open_preview_chk.is_active();
        cfg.remember_window = remember_win_chk.is_active();
        cfg.last_window_w = win_w_spin.value() as i32;
        cfg.last_window_h = win_h_spin.value() as i32;
        cfg.remember_pane = remember_pane_chk.is_active();
        cfg.last_pane_pos = pane_pos_spin.value() as i32;
        drop(cfg);
        // Apply
        apply_css_with_provider(provider, &cfg_cell.borrow());
        preview_frame.set_size_request(-1, cfg_cell.borrow().preview_min_height);
        pane.set_position(cfg_cell.borrow().preview_height);
        // Persist to config
        let _ = save_ui_config(&cfg_cell.borrow());
    }
    unsafe { dialog.destroy(); }
}

#[cfg(feature = "gtk-ui")]
fn save_ui_config(cfg: &UiConfig) -> std::io::Result<()> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let path = std::path::Path::new(&home).join(".config/clipdash/config.toml");
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let _ = std::fs::create_dir_all(dir);
    // Overwrite only UI section (simple writer)
    let data = format!(
        "ui.dark = {}\nui.opacity = {}\nui.acrylic = \"{}\"\nui.blur_strength = {}\nui.preview_height = {}\nui.preview_min_height = {}\nui.max_preview_chars = {}\nui.max_image_preview_bytes = {}\nui.open_preview_by_default = {}\nui.remember_window = {}\nui.window_w = {}\nui.window_h = {}\nui.remember_pane = {}\nui.pane_pos = {}\n",
        if cfg.dark { "true" } else { "false" },
        cfg.opacity,
        match cfg.acrylic { AcrylicMode::Off => "off", AcrylicMode::Fake => "fake", AcrylicMode::Auto => "auto" },
        cfg.blur_strength,
        cfg.preview_height,
        cfg.preview_min_height,
        cfg.max_preview_chars,
        cfg.max_image_preview_bytes,
        if cfg.open_preview_by_default { "true" } else { "false" },
        if cfg.remember_window { "true" } else { "false" },
        cfg.last_window_w,
        cfg.last_window_h,
        if cfg.remember_pane { "true" } else { "false" },
        cfg.last_pane_pos
    );
    // Append or replace file: write UI block only (simplified)
    let _ = std::fs::write(&path, data);
    Ok(())
}
#[cfg(feature = "gtk-ui")]
fn sanitize_html_for_preview(input: &str) -> String {
    // 目标：避免脚本与外部资源请求；简单将高风险标签转义，保留基本结构
    // 这里采用简单替换，满足预览需求（非严格 HTML 清洗）
    let mut s = input
        .replace("<script", "&lt;script")
        .replace("</script", "&lt;/script")
        .replace("<iframe", "&lt;iframe")
        .replace("</iframe", "&lt;/iframe")
        .replace("<object", "&lt;object")
        .replace("</object", "&lt;/object")
        .replace("<embed", "&lt;embed")
        .replace("</embed", "&lt;/embed")
        .replace("<link", "&lt;link")
        .replace("<img", "&lt;img");
    // 限制整体长度，进一步保护 UI
    if s.len() > 500_000 {
        s.truncate(500_000);
        s.push_str("\n… [truncated]");
    }
    s
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
    "#
    .to_string()
}

#[cfg(feature = "gtk-ui")]
fn apply_css_with_provider(provider: &gtk::CssProvider, cfg: &UiConfig) {
    if let Some(settings) = gtk::Settings::default() {
        let _ = settings.set_property("gtk-application-prefer-dark-theme", &cfg.dark);
        let _ = settings.set_property("gtk-enable-animations", &true);
    }
    // 基于配置调整透明度（伪亚克力）。这里复用原有模板，按模式替换透明度
    let mut css = css_for_theme(cfg.dark);
    // 动态修改 alpha（简单替换，避免大改模板）。
    // 注意：这是简化实现，真实模糊仍依赖合成器，auto/fake 等价。
    let (surf_a, card_a) = match cfg.acrylic {
        AcrylicMode::Off => {
            if cfg.dark {
                (0.95, 0.98)
            } else {
                (0.96, 0.98)
            }
        }
        AcrylicMode::Fake | AcrylicMode::Auto => {
            let s = cfg.blur_strength.clamp(0.0, 1.0);
            let base_surf = if cfg.dark { 0.82 } else { 0.86 };
            let base_card = if cfg.dark { 0.88 } else { 0.92 };
            (
                (base_surf - 0.18 * s).clamp(0.58, 0.98),
                (base_card - 0.18 * s).clamp(0.60, 0.99),
            )
        }
    };
    // 仅替换主要透明度占位（与模板中的默认值匹配进行替换）
    css = css.replace(
        "rgba(24,24,28,0.82)",
        &format!("rgba(24,24,28,{:.2})", surf_a),
    );
    css = css.replace(
        "rgba(42,42,48,0.88)",
        &format!("rgba(42,42,48,{:.2})", card_a),
    );
    css = css.replace(
        "rgba(250,250,252,0.86)",
        &format!("rgba(250,250,252,{:.2})", surf_a),
    );
    css = css.replace(
        "rgba(255,255,255,0.92)",
        &format!("rgba(255,255,255,{:.2})", card_a),
    );
    let _ = provider.load_from_data(css.as_bytes());
    if let Some(screen) = Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
