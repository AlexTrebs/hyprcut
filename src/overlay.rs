use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, atomic::AtomicBool};
use gtk4::prelude::*;

thread_local! {
    static CURRENT_CSS_PROVIDER: RefCell<Option<gtk4::CssProvider>> = RefCell::new(None);
}
use gtk4::{Application, ApplicationWindow, Box as GtkBox, Label, Orientation};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use crate::config::{Config, ShortcutList};
use crate::state::OverlayMessage;

pub fn build_overlay(
    app: &Application,
    config: Config,
    receiver: async_channel::Receiver<OverlayMessage>,
    skip_reload: Arc<AtomicBool>,
) {
    let window = ApplicationWindow::new(app);

    window.init_layer_shell();
    window.set_namespace(Some("hyprcut"));
    window.set_layer(Layer::Overlay);
    window.set_exclusive_zone(0);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Bottom, false);
    window.set_anchor(Edge::Right, false);
    window.set_margin(Edge::Top, config.overlay.position[1]);
    window.set_margin(Edge::Left, config.overlay.position[0]);

    let vbox = GtkBox::new(Orientation::Vertical, 0);
    vbox.set_widget_name("hyprcut-overlay");
    window.set_child(Some(&vbox));

    let title_bar = GtkBox::new(Orientation::Horizontal, 0);
    title_bar.set_widget_name("hyprcut-titlebar");
    let app_label = Label::new(Some(""));
    app_label.set_widget_name("hyprcut-app-name");
    title_bar.append(&app_label);
    vbox.append(&title_bar);

    let list_box = GtkBox::new(Orientation::Vertical, 2);
    list_box.set_widget_name("hyprcut-shortcuts");
    list_box.set_can_target(false);
    vbox.append(&list_box);

    apply_css(&config, None, None);

    let config_cell = Rc::new(RefCell::new(config));
    let current_class: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let display_scale: Rc<Cell<f64>> = Rc::new(Cell::new(1.0));
    attach_drag(&title_bar, &window, Rc::clone(&config_cell), Rc::clone(&current_class), Rc::clone(&display_scale), skip_reload);

    window.set_visible(false);

    // Wire async channel to GTK main context
    let window_ref = window.clone();
    let app_label_ref = app_label.clone();
    let list_box_ref = list_box.clone();

    glib::MainContext::default().spawn_local(async move {
        while let Ok(msg) = receiver.recv().await {
            handle_message(msg, &window_ref, &app_label_ref, &list_box_ref, &config_cell, &current_class, &display_scale);
        }
    });

    window.present();
}

fn handle_message(
    msg: OverlayMessage,
    window: &ApplicationWindow,
    app_label: &Label,
    list_box: &GtkBox,
    config_cell: &Rc<RefCell<Config>>,
    current_class: &Rc<RefCell<Option<String>>>,
    display_scale: &Rc<Cell<f64>>,
) {
    match msg {
        OverlayMessage::ActiveWindowChanged(class, monitor_name, scale) => {
            display_scale.set(scale);
            if let Some(name) = monitor_name.as_deref() {
                if let Some(mon) = find_gdk_monitor(name) {
                    window.set_monitor(Some(&mon));
                }
            }
            let config = config_cell.borrow();
            match class.as_deref().and_then(|c| config.apps.get(c)) {
                Some(app_config) => {
                    let pos_x = app_config.position.map(|[x, _]| x).unwrap_or(config.overlay.position[0]);
                    let pos_y = app_config.position.map(|[_, y]| y).unwrap_or(config.overlay.position[1]);
                    window.set_margin(Edge::Left, pos_x);
                    window.set_margin(Edge::Top, pos_y);
                    app_label.set_text(&app_config.name);
                    render_shortcuts(list_box, &app_config.shortcuts);
                    apply_css(&config, app_config.bg.as_deref(), app_config.text.as_deref());
                    window.set_visible(true);
                }
                None => {
                    window.set_visible(false);
                }
            }
            *current_class.borrow_mut() = class;
        }
        OverlayMessage::ConfigReloaded(new_config) => {
            // Single borrow of current_class — snapshot to avoid repeated locks
            let current = current_class.borrow().clone();
            let app = current.as_deref().and_then(|c| new_config.apps.get(c));

            let pos_x = app.and_then(|a| a.position).map(|[x, _]| x)
                .unwrap_or(new_config.overlay.position[0]);
            let pos_y = app.and_then(|a| a.position).map(|[_, y]| y)
                .unwrap_or(new_config.overlay.position[1]);
            if pos_x != window.margin(Edge::Left) {
                window.set_margin(Edge::Left, pos_x);
            }
            if pos_y != window.margin(Edge::Top) {
                window.set_margin(Edge::Top, pos_y);
            }
            apply_css(&new_config, app.and_then(|a| a.bg.as_deref()), app.and_then(|a| a.text.as_deref()));
            // Re-render shortcuts so live edits to config.toml take effect immediately
            if window.is_visible() {
                if let Some(app_cfg) = app {
                    app_label.set_text(&app_cfg.name);
                    render_shortcuts(list_box, &app_cfg.shortcuts);
                }
            }
            *config_cell.borrow_mut() = new_config;
        }
    }
}

fn is_safe_css_color(s: &str) -> bool {
    let s = s.trim();
    if matches!(s, "transparent" | "none") { return true; }
    if s.starts_with('#') {
        return matches!(s.len(), 4 | 5 | 7 | 9) && s[1..].chars().all(|c| c.is_ascii_hexdigit());
    }
    if s.starts_with("rgb(") || s.starts_with("rgba(") {
        return !s.contains([';', '{', '}', '"', '\'', '\\']);
    }
    !s.contains([';', '{', '}', '"', '\'', '\\', '<', '>'])
}

fn apply_css(config: &Config, app_bg: Option<&str>, app_text: Option<&str>) {
    let font_size = config.overlay.font_size;
    let small = font_size.saturating_sub(2);
    let font = config.overlay.font_family.as_deref().unwrap_or("monospace");
    let bg_raw = app_bg.or(config.overlay.bg.as_deref()).unwrap_or("transparent");
    let text_raw = app_text.or(config.overlay.text.as_deref()).unwrap_or("rgba(255, 255, 255, 0.75)");
    let text_muted_raw = app_text.or(config.overlay.text.as_deref()).unwrap_or("rgba(255, 255, 255, 0.45)");
    let bg = if is_safe_css_color(bg_raw) { bg_raw } else { eprintln!("hyprcut: unsafe bg color ignored"); "transparent" };
    let text = if is_safe_css_color(text_raw) { text_raw } else { eprintln!("hyprcut: unsafe text color ignored"); "rgba(255, 255, 255, 0.75)" };
    let text_muted = if is_safe_css_color(text_muted_raw) { text_muted_raw } else { "rgba(255, 255, 255, 0.45)" };
    let border_css = match config.overlay.border_color.as_deref() {
        Some(c) if is_safe_css_color(c) => format!("border: 1px solid {c};"),
        _ => "border: none;".to_string(),
    };
    let css = format!(
        r#"
window, .background {{
    background-color: transparent;
}}
#hyprcut-overlay {{
    background-color: {bg};
    {border_css}
    padding: 4px;
}}
#hyprcut-titlebar {{
    padding: 2px 4px;
}}
#hyprcut-app-name {{
    color: {text};
    font-weight: bold;
    font-size: {font_size}px;
    font-family: {font};
}}
#hyprcut-shortcuts {{
    padding: 2px 4px;
}}
#hyprcut-keys {{
    color: {text};
    font-weight: bold;
    font-size: {font_size}px;
    font-family: {font};
    min-width: 140px;
}}
#hyprcut-label {{
    color: {text};
    font-size: {font_size}px;
}}
#hyprcut-group-header {{
    color: {text_muted};
    font-size: {small}px;
    margin-top: 4px;
}}
"#,
    );

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(&css);

    let display = gdk4::Display::default().expect("no GDK display — is WAYLAND_DISPLAY set?");

    CURRENT_CSS_PROVIDER.with(|cell| {
        if let Some(old) = cell.borrow().as_ref() {
            gtk4::style_context_remove_provider_for_display(&display, old);
        }
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        *cell.borrow_mut() = Some(provider);
    });
}

fn find_gdk_monitor(connector: &str) -> Option<gdk4::Monitor> {
    let display = gdk4::Display::default()?;
    let monitors = display.monitors();
    for i in 0..monitors.n_items() {
        if let Some(m) = monitors.item(i).and_downcast::<gdk4::Monitor>() {
            if m.connector().as_deref() == Some(connector) {
                return Some(m);
            }
        }
    }
    None
}

fn render_shortcuts(list_box: &GtkBox, shortcuts: &ShortcutList) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
    match shortcuts {
        ShortcutList::Flat(list) => {
            for s in list {
                list_box.append(&make_shortcut_row(&s.keys, &s.label));
            }
        }
        ShortcutList::Grouped(groups) => {
            let mut sorted: Vec<_> = groups.iter().collect();
            sorted.sort_by_key(|(k, _)| k.as_str());
            for (group_name, list) in sorted {
                list_box.append(&make_group_header(group_name));
                for s in list {
                    list_box.append(&make_shortcut_row(&s.keys, &s.label));
                }
            }
        }
        ShortcutList::Empty => {}
    }
}

fn make_shortcut_row(keys: &str, label: &str) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 8);
    let keys_label = Label::new(Some(keys));
    keys_label.set_widget_name("hyprcut-keys");
    keys_label.set_xalign(0.0);
    let desc_label = Label::new(Some(label));
    desc_label.set_widget_name("hyprcut-label");
    desc_label.set_xalign(0.0);
    row.append(&keys_label);
    row.append(&desc_label);
    row
}

fn make_group_header(name: &str) -> Label {
    let label = Label::new(Some(&format!("── {name} ")));
    label.set_widget_name("hyprcut-group-header");
    label.set_xalign(0.0);
    label
}

fn attach_drag(title_bar: &GtkBox, window: &ApplicationWindow, config_cell: Rc<RefCell<Config>>, current_class: Rc<RefCell<Option<String>>>, display_scale: Rc<Cell<f64>>, skip_reload: Arc<AtomicBool>) {
    let drag = gtk4::GestureDrag::new();

    let drag_base: Rc<Cell<(i32, i32)>> = Rc::new(Cell::new((0, 0)));

    let drag_base_begin = Rc::clone(&drag_base);
    let window_begin = window.clone();
    drag.connect_drag_begin(move |_gesture, _x, _y| {
        drag_base_begin.set((window_begin.margin(Edge::Left), window_begin.margin(Edge::Top)));
    });

    // No live movement during drag — GestureDrag offsets are surface-local, so moving the
    // window mid-drag creates a feedback loop that prevents 1:1 tracking. Position is applied
    // only on release, where the window hasn't moved and offset == true screen displacement.
    drag.connect_drag_update(move |_gesture, _offset_x, _offset_y| {});

    let window_end = window.clone();
    let current_class_end = Rc::clone(&current_class);
    let display_scale_end = Rc::clone(&display_scale);
    drag.connect_drag_end(move |_gesture, offset_x, offset_y| {
        let (base_x, base_y) = drag_base.get();
        let buf_scale = display_scale_end.get().ceil() as i32;
        let new_x = (base_x + offset_x as i32 * buf_scale).max(0);
        let new_y = (base_y + offset_y as i32 * buf_scale).max(0);
        window_end.set_margin(Edge::Left, new_x);
        window_end.set_margin(Edge::Top, new_y);
        let mut config = config_cell.borrow_mut();
        let saved = if let Some(class) = current_class_end.borrow().as_ref() {
            if let Some(app) = config.apps.get_mut(class) {
                app.position = Some([new_x, new_y]);
                true
            } else { false }
        } else { false };
        if !saved {
            config.overlay.position = [new_x, new_y];
        }
        if let Err(e) = crate::config::save(&config, &skip_reload) {
            eprintln!("hyprcut: failed to save position: {e}");
        }
    });

    title_bar.add_controller(drag);
}

/// Compute new overlay position from drag base and cumulative gesture offset.
/// Both base and offset are in the same GTK logical-pixel coordinate space.
#[allow(dead_code)]
pub fn apply_drag_offset(base: (i32, i32), offset: (f64, f64)) -> (i32, i32) {
    let x = (base.0 + offset.0 as i32).max(0);
    let y = (base.1 + offset.1 as i32).max(0);
    (x, y)
}

/// Pure Rust helper — testable without GTK display.
#[allow(dead_code)]
pub fn shortcut_rows(shortcuts: &ShortcutList) -> Vec<(String, String)> {
    match shortcuts {
        ShortcutList::Flat(list) => list.iter().map(|s| (s.keys.clone(), s.label.clone())).collect(),
        ShortcutList::Grouped(groups) => groups
            .values()
            .flat_map(|list| list.iter().map(|s| (s.keys.clone(), s.label.clone())))
            .collect(),
        ShortcutList::Empty => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Shortcut, ShortcutList};
    use std::collections::HashMap;

    #[test]
    fn shortcut_rows_flat() {
        let shortcuts = ShortcutList::Flat(vec![
            Shortcut { keys: "Ctrl+T".into(), label: "New tab".into() },
            Shortcut { keys: "Ctrl+W".into(), label: "Close tab".into() },
        ]);
        let rows = shortcut_rows(&shortcuts);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "Ctrl+T");
    }

    #[test]
    fn shortcut_rows_grouped_flattens_all_entries() {
        let mut groups = HashMap::new();
        groups.insert("nav".into(), vec![
            Shortcut { keys: "G".into(), label: "Go".into() },
        ]);
        groups.insert("edit".into(), vec![
            Shortcut { keys: ":w".into(), label: "Save".into() },
        ]);
        let shortcuts = ShortcutList::Grouped(groups);
        let rows = shortcut_rows(&shortcuts);
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn shortcut_rows_empty() {
        let rows = shortcut_rows(&ShortcutList::Empty);
        assert!(rows.is_empty());
    }

    #[test]
    fn drag_offset_moves_1_to_1() {
        // Base at (100, 200), offset of 50 in each axis → should move exactly 50.
        assert_eq!(apply_drag_offset((100, 200), (50.0, 30.0)), (150, 230));
    }

    #[test]
    fn drag_offset_clamps_to_zero() {
        // Negative offset that would put position below 0 must clamp.
        assert_eq!(apply_drag_offset((10, 10), (-50.0, -50.0)), (0, 0));
    }

    #[test]
    fn drag_offset_truncates_fractional() {
        // f64 offsets are truncated to i32 (not rounded).
        assert_eq!(apply_drag_offset((100, 100), (0.9, 1.9)), (100, 101));
    }

    #[test]
    fn drag_offset_large_values() {
        assert_eq!(apply_drag_offset((1600, 20), (100.0, 5.0)), (1700, 25));
    }

    // Documents the buf_scale formula: Hyprland fractional scale → ceil → buffer multiplier.
    // Runtime empirical result: eDP-1 at 1.33 gives effective scale=2 (margins placed at X/2).
    #[test]
    fn buf_scale_ceil_formula() {
        assert_eq!((1.33_f64).ceil() as i32, 2); // eDP-1 1.33 → 2
        assert_eq!((1.0_f64).ceil() as i32, 1);  // 1:1 display
        assert_eq!((2.0_f64).ceil() as i32, 2);  // HiDPI
        assert_eq!((1.5_f64).ceil() as i32, 2);  // other fractional
    }
}
