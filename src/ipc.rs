use std::sync::mpsc::Sender;
use hyprland::event_listener::EventListener;
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
        let _ = sender.send(OverlayMessage::ActiveWindowChanged(class));
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
