use std::collections::HashMap;
use crate::config::{AppConfig, Config};

/// Messages sent from background threads to the GTK main loop.
/// Must be Send — Config and its fields are all Send.
pub enum OverlayMessage {
    ActiveWindowChanged(Option<String>),
    ConfigReloaded(Config),
}

pub fn resolve_class<'a>(class: &str, apps: &'a HashMap<String, AppConfig>) -> Option<&'a AppConfig> {
    apps.get(class)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, Shortcut, ShortcutList};
    use std::collections::HashMap;

    fn make_app(name: &str) -> AppConfig {
        AppConfig {
            name: name.to_string(),
            shortcuts: ShortcutList::Flat(vec![
                Shortcut { keys: "Ctrl+T".into(), label: "Test".into() },
            ]),
        }
    }

    #[test]
    fn resolve_class_returns_matching_app() {
        let mut apps = HashMap::new();
        apps.insert("firefox".to_string(), make_app("Firefox"));
        let result = resolve_class("firefox", &apps);
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "Firefox");
    }

    #[test]
    fn resolve_class_returns_none_for_unknown() {
        let apps = HashMap::new();
        assert!(resolve_class("unknown", &apps).is_none());
    }

    #[test]
    fn resolve_class_is_case_sensitive() {
        let mut apps = HashMap::new();
        apps.insert("firefox".to_string(), make_app("Firefox"));
        assert!(resolve_class("Firefox", &apps).is_none());
    }
}
