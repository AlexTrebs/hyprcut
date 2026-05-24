#!/usr/bin/env bash
set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# ── Dependency check ──────────────────────────────────────────────────────────
missing=()
for pkg in gtk4 gtk4-layer-shell; do
    if ! pkg-config --exists "$pkg" 2>/dev/null; then
        missing+=("$pkg")
    fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
    echo -e "${RED}Missing system libraries:${NC} ${missing[*]}"
    echo ""
    echo "On Arch Linux:"
    echo "  sudo pacman -S gtk4"
    echo "  yay -S gtk4-layer-shell   # or paru"
    echo ""
    echo "See README.md for other distros."
    exit 1
fi

# ── Build ─────────────────────────────────────────────────────────────────────
echo -e "${GREEN}Building...${NC}"
cargo build --release

# ── Install binary ────────────────────────────────────────────────────────────
mkdir -p "$HOME/.local/bin"
cp target/release/hyprcut "$HOME/.local/bin/hyprcut"
echo -e "${GREEN}Installed:${NC} ~/.local/bin/hyprcut"

# ── PATH ──────────────────────────────────────────────────────────────────────
if ! echo "$PATH" | grep -q "$HOME/.local/bin"; then
    echo ""
    echo -e "${YELLOW}~/.local/bin is not in your PATH.${NC}"
    for rc in "$HOME/.zshrc" "$HOME/.bashrc"; do
        if [[ -f "$rc" ]]; then
            read -r -p "Add to $rc? [y/N] " ans
            if [[ "$ans" =~ ^[Yy]$ ]]; then
                echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$rc"
                echo -e "${GREEN}Added.${NC} Run: source $rc"
            fi
            break
        fi
    done
fi

# ── Hyprland config ───────────────────────────────────────────────────────────
hypr_conf=""
for candidate in "$HOME/.config/hypr/hyprland.conf" "$HOME/.config/hyprland/hyprland.conf"; do
    if [[ -f "$candidate" ]]; then
        hypr_conf="$candidate"
        break
    fi
done

if [[ -n "$hypr_conf" ]]; then
    if ! grep -q "exec-once = hyprcut" "$hypr_conf"; then
        echo ""
        read -r -p "Add 'exec-once = hyprcut' to $hypr_conf? [y/N] " ans
        if [[ "$ans" =~ ^[Yy]$ ]]; then
            echo "exec-once = hyprcut" >> "$hypr_conf"
            echo -e "${GREEN}Added.${NC}"
        fi
    fi
else
    echo ""
    echo "Add to your Hyprland config manually:"
    echo "  exec-once = hyprcut"
fi

# ── Done ──────────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}Done!${NC}"
echo "Config: ~/.config/hyprcut/config.toml (created on first run)"
echo "Find window classes: hyprctl activewindow | grep class"
