use std::sync::mpsc::Sender;
use hyprland::event_listener::EventListener;
use hyprland::data::{Client, Monitors};
use hyprland::shared::{HyprData, HyprDataActiveOptional};
use crate::state::OverlayMessage;

pub fn normalize_class(class: &str) -> String {
    class.trim().to_lowercase()
}

/// Blocks the calling thread. Spawn on a dedicated std::thread.
pub fn start_listener(sender: Sender<OverlayMessage>) -> anyhow::Result<()> {
    let mut listener = EventListener::new();

    listener.add_active_window_changed_handler(move |data| {
        let class = data
            .map(|w| normalize_class(&w.class))
            .filter(|c| !c.is_empty());
        let monitor_info = Client::get_active()
            .map_err(|e| eprintln!("hyprcut: active client query failed: {e}"))
            .ok()
            .flatten()
            .and_then(|client| {
                Monitors::get()
                    .map_err(|e| eprintln!("hyprcut: monitors query failed: {e}"))
                    .ok()
                    .and_then(|monitors| {
                        monitors.into_iter()
                            .find(|m| Some(m.id) == client.monitor)
                            .map(|m| (m.name.clone(), m.scale as f64))
                    })
            });
        let monitor_name = monitor_info.as_ref().map(|(n, _)| n.clone());
        let display_scale = monitor_info.map(|(_, s)| s).unwrap_or(1.0);
        let _ = sender.send(OverlayMessage::ActiveWindowChanged(class, monitor_name, display_scale));
    });

    listener.start_listener()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_class_lowercases_and_trims() {
        assert_eq!(normalize_class("  Firefox  "), "firefox");
    }

    #[test]
    fn normalize_class_empty_string() {
        assert_eq!(normalize_class(""), "");
    }

    #[test]
    fn normalize_class_already_lowercase() {
        assert_eq!(normalize_class("kitty"), "kitty");
    }
}
