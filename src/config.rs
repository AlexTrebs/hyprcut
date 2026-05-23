use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
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
    pub position_x: i32,
    pub position_y: i32,
    pub opacity: f64,
    pub font_size: u32,
    pub text_color: Option<String>,
    pub border_color: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub name: String,
    #[serde(default)]
    pub shortcuts: ShortcutList,
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
    Ok(toml::from_str(&text)?)
}

pub fn save(config: &Config) -> anyhow::Result<()> {
    let text = toml::to_string_pretty(config)?;
    fs::write(config_path(), text)?;
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
pub fn watch(sender: async_channel::Sender<crate::state::OverlayMessage>) -> anyhow::Result<()> {
    let path = config_path();
    let dir = path.parent().expect("config path has no parent").to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(500), tx)?;
    debouncer.watcher().watch(&dir, RecursiveMode::NonRecursive)?;

    std::thread::spawn(move || {
        let _keep_alive = debouncer;
        for result in rx {
            match result {
                Ok(events) => {
                    let changed = events.iter().any(|e| {
                        e.kind == DebouncedEventKind::Any && e.path == path
                    });
                    if changed {
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flat_shortcuts() {
        let raw = r#"
[overlay]
position_x = 100
position_y = 50
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
position_x = 0
position_y = 0
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
position_x = 0
position_y = 0
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
