#!/usr/bin/env bash
#
# Build and install xfce4-niri for the current user (no root required).
#
#   - compiles the workspace binaries and installs them into a bin
#     directory already on $PATH (~/.local/bin preferred, ~/bin as fallback)
#   - installs scripts/*                         -> the same bin directory
#   - installs xfce4-niri-config/niri            -> ~/.config/niri
#   - installs xfce4-niri-config/applications/*  -> ~/.local/share/applications
#
# With --uninstall, removes everything listed above instead (the niri config
# is moved to a timestamped backup rather than deleted).
#
# Run with --help for options.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

PROFILE=release
ASSUME_YES=0
BIN_DIR_OVERRIDE=""
SKIP_TESTS=0
UNINSTALL=0

usage() {
    cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --debug         build in debug mode instead of release
  --bin-dir DIR   install binaries into DIR instead of auto-detecting
  --skip-tests    skip running the test suite before building
  --uninstall     remove a previous installation (binaries, scripts,
                  application launchers; ~/.config/niri is backed up)
  -y, --yes       overwrite (or, with --uninstall, back up and remove)
                  an existing ~/.config/niri without asking
  -h, --help      show this help
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --debug) PROFILE=debug ;;
        --bin-dir) BIN_DIR_OVERRIDE="${2:?--bin-dir requires an argument}"; shift ;;
        --skip-tests) SKIP_TESTS=1 ;;
        --uninstall) UNINSTALL=1 ;;
        -y|--yes) ASSUME_YES=1 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
    esac
    shift
done

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
BINARIES=(xfce4-niri xfce4-niri-service xfce4-niri-autostart)

backup_if_exists() {
    local target="$1"
    if [ -e "$target" ]; then
        local backup
        backup="${target}.bak-$(date +%Y%m%d%H%M%S)"
        echo "==> Backing up existing $target -> $backup"
        mv "$target" "$backup"
    fi
}

remove_file() {
    if [ -e "$1" ] || [ -L "$1" ]; then
        echo "    removing $1"
        rm -f "$1"
    fi
}

if [ "$UNINSTALL" -eq 1 ]; then
    echo "==> Removing binaries from $BIN_DIR"
    for bin in "${BINARIES[@]}"; do
        remove_file "$BIN_DIR/$bin"
    done

    echo "==> Removing helper scripts from $BIN_DIR"
    for script in scripts/*; do
        [ -f "$script" ] || continue
        remove_file "$BIN_DIR/$(basename "$script")"
    done

    echo "==> Removing application launchers from $APPS_DIR"
    for f in xfce4-niri-config/applications/*.desktop; do
        remove_file "$APPS_DIR/$(basename "$f")"
    done
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true
    fi

    if [ -e "$CONFIG_DIR" ]; then
        remove_niri_config=1
        if [ "$ASSUME_YES" -ne 1 ]; then
            read -r -p "Back up and remove $CONFIG_DIR? [y/N] " reply
            case "$reply" in
                [yY]*) ;;
                *) echo "Keeping niri config."; remove_niri_config=0 ;;
            esac
        fi
        if [ "$remove_niri_config" -eq 1 ]; then
            backup_if_exists "$CONFIG_DIR"
        fi
    fi

    echo
    echo "Done. xfce4-niri has been uninstalled."
    exit 0
fi

command -v cargo >/dev/null 2>&1 || {
    echo "error: cargo not found. Install Rust (>= 1.85) via https://rustup.rs" >&2
    exit 1
}
command -v pkg-config >/dev/null 2>&1 || {
    echo "warning: pkg-config not found, the build will likely fail." >&2
    echo "         see README.md 'Install' section for the required system packages." >&2
}

if [ "$SKIP_TESTS" -ne 1 ]; then
    echo "==> Running test suite"
    cargo test --workspace
else
    echo "==> Skipping test suite (--skip-tests)"
fi

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
for bin in "${BINARIES[@]}"; do
    install -m 755 "$TARGET_DIR/$bin" "$BIN_DIR/$bin"
done

echo "==> Installing helper scripts to $BIN_DIR"
for script in scripts/*; do
    [ -f "$script" ] || continue
    install -m 755 "$script" "$BIN_DIR/$(basename "$script")"
done

if ! path_has "$BIN_DIR"; then
    echo "warning: $BIN_DIR is not in your PATH." >&2
    echo "         add e.g. 'export PATH=\"$BIN_DIR:\$PATH\"' to your shell profile," >&2
    echo "         otherwise niri won't find xfce4-niri-service at startup." >&2
fi

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
  scripts:       $BIN_DIR/{$(cd scripts && ls | paste -sd, -)}
  niri config:   $CONFIG_DIR
  app launchers: $APPS_DIR

Log into (or restart) the niri session to pick up the new startup config.
EOF
