#!/usr/bin/env bash
#
# Build and install xfce4-niri for the current user (no root required).
#
#   - compiles the workspace binaries and installs them into a bin
#     directory already on $PATH (~/.local/bin preferred, ~/bin as fallback)
#   - installs xfce4-niri-config/niri            -> ~/.config/niri
#   - installs xfce4-niri-config/applications/*  -> ~/.local/share/applications
#
# Run with --help for options.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

PROFILE=release
ASSUME_YES=0
BIN_DIR_OVERRIDE=""

usage() {
    cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --debug         build in debug mode instead of release
  --bin-dir DIR   install binaries into DIR instead of auto-detecting
  -y, --yes       overwrite an existing ~/.config/niri without asking
  -h, --help      show this help
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --debug) PROFILE=debug ;;
        --bin-dir) BIN_DIR_OVERRIDE="${2:?--bin-dir requires an argument}"; shift ;;
        -y|--yes) ASSUME_YES=1 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
    esac
    shift
done

command -v cargo >/dev/null 2>&1 || {
    echo "error: cargo not found. Install Rust (>= 1.85) via https://rustup.rs" >&2
    exit 1
}
command -v pkg-config >/dev/null 2>&1 || {
    echo "warning: pkg-config not found, the build will likely fail." >&2
    echo "         see README.md 'Install' section for the required system packages." >&2
}

path_has() {
    case ":${PATH}:" in
        *":$1:"*) return 0 ;;
        *) return 1 ;;
    esac
}

if [ -n "$BIN_DIR_OVERRIDE" ]; then
    BIN_DIR="$BIN_DIR_OVERRIDE"
elif path_has "$HOME/.local/bin"; then
    BIN_DIR="$HOME/.local/bin"
elif path_has "$HOME/bin"; then
    BIN_DIR="$HOME/bin"
elif [ -d "$HOME/.local/bin" ]; then
    BIN_DIR="$HOME/.local/bin"
elif [ -d "$HOME/bin" ]; then
    BIN_DIR="$HOME/bin"
else
    BIN_DIR="$HOME/.local/bin"
fi

CONFIG_DIR="$HOME/.config/niri"
APPS_DIR="$HOME/.local/share/applications"

echo "==> Building workspace (--$PROFILE)"
if [ "$PROFILE" = release ]; then
    cargo build --workspace --release
    TARGET_DIR="$SCRIPT_DIR/target/release"
else
    cargo build --workspace
    TARGET_DIR="$SCRIPT_DIR/target/debug"
fi

echo "==> Installing binaries to $BIN_DIR"
mkdir -p "$BIN_DIR"
for bin in xfce4-niri xfce4-niri-service xfce4-niri-autostart; do
    install -m 755 "$TARGET_DIR/$bin" "$BIN_DIR/$bin"
done

if ! path_has "$BIN_DIR"; then
    echo "warning: $BIN_DIR is not in your PATH." >&2
    echo "         add e.g. 'export PATH=\"$BIN_DIR:\$PATH\"' to your shell profile," >&2
    echo "         otherwise niri won't find xfce4-niri-service at startup." >&2
fi

backup_if_exists() {
    local target="$1"
    if [ -e "$target" ]; then
        local backup
        backup="${target}.bak-$(date +%Y%m%d%H%M%S)"
        echo "==> Backing up existing $target -> $backup"
        mv "$target" "$backup"
    fi
}

install_niri_config=1
if [ -e "$CONFIG_DIR" ] && [ "$ASSUME_YES" -ne 1 ]; then
    read -r -p "$CONFIG_DIR already exists, back it up and overwrite? [y/N] " reply
    case "$reply" in
        [yY]*) ;;
        *) echo "Skipping niri config install."; install_niri_config=0 ;;
    esac
fi

if [ "$install_niri_config" -eq 1 ]; then
    echo "==> Installing niri config to $CONFIG_DIR"
    backup_if_exists "$CONFIG_DIR"
    mkdir -p "$CONFIG_DIR"
    cp -a xfce4-niri-config/niri/. "$CONFIG_DIR/"
    chmod +x "$CONFIG_DIR"/bin/* 2>/dev/null || true

    # the shipped config hardcodes the packager's own home directory
    # (e.g. the swaylock path in niri.d/60-keys.kdl); point it at $HOME instead
    while IFS= read -r f; do
        sed -i "s#/home/antoniosalsi#$HOME#g" "$f"
    done < <(grep -rl '/home/antoniosalsi' "$CONFIG_DIR" 2>/dev/null || true)
fi

echo "==> Installing application launchers to $APPS_DIR"
mkdir -p "$APPS_DIR"
for f in xfce4-niri-config/applications/*.desktop; do
    dest="$APPS_DIR/$(basename "$f")"
    sed "s#/home/antoniosalsi#$HOME#g" "$f" > "$dest"
    chmod 644 "$dest"
done

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true
fi

cat <<EOF

Done.
  binaries:      $BIN_DIR/{xfce4-niri,xfce4-niri-service,xfce4-niri-autostart}
  niri config:   $CONFIG_DIR
  app launchers: $APPS_DIR

Log into (or restart) the niri session to pick up the new startup config.
EOF
