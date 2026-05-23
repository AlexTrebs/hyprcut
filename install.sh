#!/usr/bin/env bash
set -e

cargo build --release

mkdir -p "$HOME/.local/bin"
cp target/release/hyprcut "$HOME/.local/bin/hyprcut"

echo "Installed: ~/.local/bin/hyprcut"
echo ""
echo "Ensure ~/.local/bin is in your PATH, then add to hyprland.conf:"
echo "  exec-once = hyprcut"
echo ""
echo "Config lives at: ~/.config/hyprcut/config.toml"
echo "Run 'hyprctl activewindow' while focused on an app to find its class."
