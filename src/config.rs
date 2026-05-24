use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebouncedEventKind};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub overlay: OverlayConfig,
    #[serde(default)]
    pub apps: HashMap<String, AppConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OverlayConfig {
    pub position: [i32; 2],
    pub opacity: f64,
    pub font_size: u32,
    #[serde(alias = "bg_color")]
    pub bg: Option<String>,
    #[serde(alias = "text_color")]
    pub text: Option<String>,
    pub border_color: Option<String>,
    pub font_family: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub name: String,
    #[serde(default)]
    pub shortcuts: ShortcutList,
    #[serde(alias = "bg_color")]
    pub bg: Option<String>,
    #[serde(alias = "text_color")]
    pub text: Option<String>,
    pub position: Option<[i32; 2]>,
}

/// Supports both flat `shortcuts = [...]` and grouped `[app.shortcuts] group = [...]`
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(untagged)]
pub enum ShortcutList {
    Flat(Vec<Shortcut>),
    Grouped(HashMap<String, Vec<Shortcut>>),
    #[default]
    Empty,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Shortcut {
    pub keys: String,
    pub label: String,
}

pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    PathBuf::from(home).join(".config/hyprcut/config.toml")
}

pub fn load() -> anyhow::Result<Config> {
    let path = config_path();
    if !path.exists() {
        create_default(&path)?;
    }
    let text = fs::read_to_string(&path)?;
    let mut config: Config = toml::from_str(&text)?;
    // Normalize keys to lowercase so they match IPC window classes
    config.apps = config.apps.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
    Ok(config)
}

pub fn save(config: &Config, skip: &AtomicBool) -> anyhow::Result<()> {
    use toml_edit::{value, Array, DocumentMut, InlineTable, Item, Table};

    let mut doc = DocumentMut::new();

    // [overlay]
    let ov = doc.entry("overlay").or_insert(Item::Table(Table::new()));
    let ov = ov.as_table_mut().unwrap();
    let mut pos = Array::new();
    pos.push(config.overlay.position[0] as i64);
    pos.push(config.overlay.position[1] as i64);
    ov["position"] = value(pos);
    ov["opacity"] = value(config.overlay.opacity);
    ov["font_size"] = value(config.overlay.font_size as i64);
    if let Some(ref bg) = config.overlay.bg {
        ov["bg"] = value(bg.as_str());
    }
    if let Some(ref text) = config.overlay.text {
        ov["text"] = value(text.as_str());
    }
    if let Some(ref font) = config.overlay.font_family {
        ov["font_family"] = value(font.as_str());
    }

    // [apps.*]
    let apps = doc.entry("apps").or_insert(Item::Table(Table::new()));
    let apps = apps.as_table_mut().unwrap();
    apps.set_implicit(true);

    let mut sorted: Vec<_> = config.apps.iter().collect();
    sorted.sort_by_key(|(k, _)| k.as_str());

    for (class, app) in sorted {
        let app_t = apps.entry(class.as_str()).or_insert(Item::Table(Table::new()));
        let app_t = app_t.as_table_mut().unwrap();

        app_t["name"] = value(app.name.as_str());
        if let Some([x, y]) = app.position {
            let mut p = Array::new();
            p.push(x as i64);
            p.push(y as i64);
            app_t["position"] = value(p);
        }
        if let Some(ref bg) = app.bg {
            app_t["bg"] = value(bg.as_str());
        }
        if let Some(ref text) = app.text {
            app_t["text"] = value(text.as_str());
        }

        match &app.shortcuts {
            ShortcutList::Flat(list) => {
                let mut arr = Array::new();
                for s in list {
                    let mut t = InlineTable::new();
                    t.insert("keys", toml_edit::Value::from(s.keys.as_str()));
                    t.insert("label", toml_edit::Value::from(s.label.as_str()));
                    arr.push(t);
                }
                app_t["shortcuts"] = value(arr);
            }
            ShortcutList::Grouped(groups) => {
                let sc = app_t.entry("shortcuts").or_insert(Item::Table(Table::new()));
                let sc = sc.as_table_mut().unwrap();
                let mut sorted_groups: Vec<_> = groups.iter().collect();
                sorted_groups.sort_by_key(|(k, _)| k.as_str());
                for (group, list) in sorted_groups {
                    let mut arr = Array::new();
                    for s in list {
                        let mut t = InlineTable::new();
                        t.insert("keys", toml_edit::Value::from(s.keys.as_str()));
                        t.insert("label", toml_edit::Value::from(s.label.as_str()));
                        arr.push(t);
                    }
                    sc[group.as_str()] = value(arr);
                }
            }
            ShortcutList::Empty => {}
        }
    }

    let text = doc.to_string();
    let path = config_path();
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, &text)?;
    skip.store(true, Ordering::SeqCst);
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn create_default(path: &PathBuf) -> anyhow::Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, DEFAULT_CONFIG)?;
    Ok(())
}

const DEFAULT_CONFIG: &str = include_str!("../config/default.toml");

/// Starts a background thread watching the config file for changes.
/// On change, reloads and sends ConfigReloaded via the async channel.
pub fn watch(sender: async_channel::Sender<crate::state::OverlayMessage>) -> anyhow::Result<Arc<AtomicBool>> {
    let path = config_path();
    let dir = path.parent().expect("config path has no parent").to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(500), tx)?;
    debouncer.watcher().watch(&dir, RecursiveMode::NonRecursive)?;

    let skip = Arc::new(AtomicBool::new(false));
    let skip_watch = Arc::clone(&skip);

    std::thread::spawn(move || {
        let _keep_alive = debouncer;
        for result in rx {
            match result {
                Ok(events) => {
                    let changed = events.iter().any(|e| {
                        e.kind == DebouncedEventKind::Any && e.path == path
                    });
                    if changed {
                        if skip_watch.swap(false, Ordering::SeqCst) {
                            continue; // triggered by our own drag-save, skip to avoid CSS flash
                        }
                        match load() {
                            Ok(config) => {
                                let _ = sender.send_blocking(crate::state::OverlayMessage::ConfigReloaded(config));
                            }
                            Err(e) => eprintln!("hyprcut: config reload error: {e}"),
                        }
                    }
                }
                Err(e) => eprintln!("hyprcut: file watcher error: {e}"),
            }
        }
    });

    Ok(skip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flat_shortcuts() {
        let raw = r#"
[overlay]
position = [100, 50]
opacity = 0.75
font_size = 14

[apps.firefox]
name = "Firefox"
shortcuts = [
    { keys = "Ctrl+L", label = "Focus address bar" },
    { keys = "Ctrl+T", label = "New tab" },
]
"#;
        let config: Config = toml::from_str(raw).unwrap();
        let app = config.apps.get("firefox").unwrap();
        assert_eq!(app.name, "Firefox");
        match &app.shortcuts {
            ShortcutList::Flat(list) => {
                assert_eq!(list.len(), 2);
                assert_eq!(list[0].keys, "Ctrl+L");
            }
            _ => panic!("expected flat"),
        }
    }

    #[test]
    fn parse_grouped_shortcuts() {
        let raw = r#"
[overlay]
position = [0, 0]
opacity = 0.75
font_size = 14

[apps.kitty]
name = "Kitty"

[apps.kitty.shortcuts]
navigation = [
    { keys = "Ctrl+Shift+T", label = "New tab" },
]
"#;
        let config: Config = toml::from_str(raw).unwrap();
        let app = config.apps.get("kitty").unwrap();
        match &app.shortcuts {
            ShortcutList::Grouped(groups) => {
                assert!(groups.contains_key("navigation"));
                assert_eq!(groups["navigation"][0].keys, "Ctrl+Shift+T");
            }
            _ => panic!("expected grouped"),
        }
    }

    #[test]
    fn missing_shortcuts_field_defaults_to_empty() {
        let raw = r#"
[overlay]
position = [0, 0]
opacity = 0.75
font_size = 14

[apps.bare]
name = "Bare"
"#;
        let config: Config = toml::from_str(raw).unwrap();
        let app = config.apps.get("bare").unwrap();
        assert!(matches!(app.shortcuts, ShortcutList::Empty));
    }
}
