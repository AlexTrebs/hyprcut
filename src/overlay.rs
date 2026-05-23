use std::cell::{Cell, RefCell};
use std::rc::Rc;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, Label, Orientation};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use crate::config::{Config, ShortcutList};
use crate::state::OverlayMessage;

pub fn build_overlay(
    app: &Application,
    config: Config,
    receiver: async_channel::Receiver<OverlayMessage>,
) {
    let window = ApplicationWindow::new(app);

    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_exclusive_zone(0);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Bottom, false);
    window.set_anchor(Edge::Right, false);
    window.set_margin(Edge::Top, config.overlay.position_y);
    window.set_margin(Edge::Left, config.overlay.position_x);

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

    apply_css(&config);

    let config_cell = Rc::new(RefCell::new(config));
    attach_drag(&title_bar, &window, Rc::clone(&config_cell));

    window.set_visible(false);

    // Wire async channel to GTK main context
    let window_ref = window.clone();
    let app_label_ref = app_label.clone();
    let list_box_ref = list_box.clone();

    glib::MainContext::default().spawn_local(async move {
        while let Ok(msg) = receiver.recv().await {
            handle_message(msg, &window_ref, &app_label_ref, &list_box_ref, &config_cell);
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
) {
    match msg {
        OverlayMessage::ActiveWindowChanged(class) => {
            let config = config_cell.borrow();
            match class.as_deref().and_then(|c| config.apps.get(c)) {
                Some(app_config) => {
                    app_label.set_text(&app_config.name);
                    render_shortcuts(list_box, &app_config.shortcuts);
                    window.set_visible(true);
                }
                None => {
                    window.set_visible(false);
                }
            }
        }
        OverlayMessage::ConfigReloaded(new_config) => {
            apply_css(&new_config);
            *config_cell.borrow_mut() = new_config;
        }
    }
}

fn apply_css(config: &Config) {
    let text = config.overlay.text_color.as_deref().unwrap_or("#ffffff");
    let border = config.overlay.border_color.as_deref().unwrap_or("#000000");
    let css = format!(
        r#"
#hyprcut-overlay {{
    background-color: rgba(20, 20, 20, {opacity});
    border: 1px solid {border};
    border-radius: 6px;
    padding: 4px;
}}
#hyprcut-titlebar {{
    padding: 4px 8px;
    border-bottom: 1px solid {border};
}}
#hyprcut-app-name {{
    color: {text};
    font-weight: bold;
    font-size: {font_size}px;
    font-family: monospace;
}}
#hyprcut-shortcuts {{
    padding: 4px;
}}
#hyprcut-keys {{
    color: {text};
    font-weight: bold;
    font-size: {font_size}px;
    font-family: monospace;
    min-width: 140px;
}}
#hyprcut-label {{
    color: {text};
    font-size: {font_size}px;
}}
#hyprcut-group-header {{
    color: rgba(200,200,200,0.5);
    font-size: {small}px;
    margin-top: 4px;
}}
"#,
        opacity = config.overlay.opacity,
        border = border,
        text = text,
        font_size = config.overlay.font_size,
        small = config.overlay.font_size.saturating_sub(2),
    );

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(&css);
    gtk4::style_context_add_provider_for_display(
        &gdk4::Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
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

fn attach_drag(title_bar: &GtkBox, window: &ApplicationWindow, config_cell: Rc<RefCell<Config>>) {
    let drag = gtk4::GestureDrag::new();

    // Snapshot the base position when the drag starts, so update/end offsets
    // are always relative to the same origin.
    let drag_base: Rc<Cell<(i32, i32)>> = Rc::new(Cell::new((0, 0)));

    let drag_base_begin = Rc::clone(&drag_base);
    let config_begin = Rc::clone(&config_cell);
    drag.connect_drag_begin(move |_gesture, _x, _y| {
        let config = config_begin.borrow();
        drag_base_begin.set((config.overlay.position_x, config.overlay.position_y));
    });

    let window_update = window.clone();
    let drag_base_update = Rc::clone(&drag_base);
    drag.connect_drag_update(move |_gesture, offset_x, offset_y| {
        let (base_x, base_y) = drag_base_update.get();
        let new_x = (base_x + offset_x as i32).max(0);
        let new_y = (base_y + offset_y as i32).max(0);
        window_update.set_margin(Edge::Left, new_x);
        window_update.set_margin(Edge::Top, new_y);
    });

    let window_end = window.clone();
    drag.connect_drag_end(move |_gesture, offset_x, offset_y| {
        let (base_x, base_y) = drag_base.get();
        let new_x = (base_x + offset_x as i32).max(0);
        let new_y = (base_y + offset_y as i32).max(0);
        window_end.set_margin(Edge::Left, new_x);
        window_end.set_margin(Edge::Top, new_y);
        let mut config = config_cell.borrow_mut();
        config.overlay.position_x = new_x;
        config.overlay.position_y = new_y;
        if let Err(e) = crate::config::save(&config) {
            eprintln!("hyprcut: failed to save position: {e}");
        }
    });

    title_bar.add_controller(drag);
}

/// Pure Rust helper — testable without GTK display.
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
}
