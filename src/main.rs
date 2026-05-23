mod config;
mod ipc;
mod overlay;
mod state;

use gtk4::prelude::*;
use gtk4::Application;

const APP_ID: &str = "io.github.hyprcut";

fn main() -> anyhow::Result<()> {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        let config = config::load().expect("failed to load config");

        // Single async_channel for all messages to the overlay
        let (sender, receiver) = async_channel::bounded::<state::OverlayMessage>(64);

        // Config file watcher → sends ConfigReloaded directly via async_channel
        config::watch(sender.clone()).expect("failed to start config watcher");

        // IPC listener uses mpsc internally; bridge to async_channel
        let (mpsc_tx, mpsc_rx) = std::sync::mpsc::channel::<state::OverlayMessage>();
        let ipc_bridge_sender = sender.clone();
        std::thread::spawn(move || {
            while let Ok(msg) = mpsc_rx.recv() {
                let _ = ipc_bridge_sender.send_blocking(msg);
            }
        });
        std::thread::spawn(move || {
            if let Err(e) = ipc::start_listener(mpsc_tx) {
                eprintln!("hyprcut: IPC error: {e}");
            }
        });

        overlay::build_overlay(app, config, receiver);
    });

    app.run();
    Ok(())
}
