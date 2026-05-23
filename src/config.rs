use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

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
