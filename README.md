# xfce4-niri
A utility to better integrate Xfce4 applications with the niri window manager, creating a minimal, lightweight, and stable desktop environment.

## Install

Debian package
```sh
sudo apt install \
    pkg-config \
    libgtk-3-dev \
    libglib2.0-dev \
    libdbus-1-dev \
    xdg-user-dirs \
    xdg-desktop-portal \
    dbus \
    xfce4-power-manager \
    upower \
    swayidle \
    swaylock \
    xfce4-session \
    xfce4-panel \
    xfce4-appfinder \
    xfce4-notifyd \
    xfce4-pulseaudio-plugin \
    xfce4-settings \
    xfce4-whiskermenu-plugin
```

`niri` is not packaged in Debian/Ubuntu's default repositories yet; install it from your distro's community repo or build it from [source](https://github.com/YaLTeR/niri). Rust (`rustc`/`cargo` >= 1.85) should be installed via [rustup](https://rustup.rs) rather than `apt`, since distro packages usually lag behind the required version.

Void Linux package
```sh
sudo xbps-install -S \
    pkg-config \
    rust \
    gtk+3-devel \
    glib-devel \
    dbus-devel \
    xdg-user-dirs \
    xdg-desktop-portal \
    dbus \
    xfce4-power-manager \
    upower \
    swayidle \
    swaylock \
    elogind \
    xfce4-session \
    xfce4-panel \
    libxfce4ui \
    libxfce4util \
    libxfce4windowing \
    xfce4-appfinder \
    xfce4-notifyd \
    xfce4-pulseaudio-plugin \
    xfce4-settings \
    xfce4-whiskermenu-plugin \
    niri
```

## Build & install

Once the system packages above are installed, build the workspace and install
everything for the current user (no root needed) with:

```sh
./install.sh
```

This:
- builds `xfce4-niri`, `xfce4-niri-service`, `xfce4-niri-autostart` (`cargo build --workspace --release`)
- installs the binaries into `~/.local/bin` (or `~/bin`, whichever is already on `$PATH`)
- installs [xfce4-niri-config/niri](xfce4-niri-config/niri) into `~/.config/niri`, backing up an existing one first
- installs [xfce4-niri-config/applications](xfce4-niri-config/applications) `.desktop` files into `~/.local/share/applications`

Options: `./install.sh --help` (`--debug` for a debug build, `--bin-dir DIR` to
override the binaries directory, `-y`/`--yes` to skip the overwrite prompt).

## xfce4-niri-autostart

`niri` does not process `~/.config/autostart/*.desktop` the way a full
session manager does, so `xfce4-niri-autostart` is a standalone GTK
re-implementation of xfce4-session's "Application Autostart" tab, letting you
manage autostart entries the same way you would on stock Xfce.

![xfce4-niri-autostart](media/xfce4-niri-autostart.png)

- **Program list** — every autostart entry (icon, name and comment), each
  with a checkbox to enable/disable it; hovering a row shows the exact
  command it runs in a tooltip (e.g. `flatpak run com.borgbase.Vorta
  --daemonize`).
- **Toolbar (Add / Remove / Edit)** — add a new autostart entry, remove one,
  or edit an existing entry's name, description and command.
- **Close** — dismisses the window; changes are written to the underlying
  `.desktop` file as soon as they're made, so nothing needs to be saved
  explicitly.

Launch it with `Mod+A` (see [niri's
keybindings](xfce4-niri-config/niri/README.md#keybindings)).

## Dependencies

### Build
pkg-config
rustc / cargo (>= 1.85)
gtk3
glib2
dbus-1 >= 1.6

### Runtime
xdg-user-dirs
xdg-desktop-portal
niri
dbus
xfce4-power-manager
upower
swayidle
swaylock
systemd (or elogind, for loginctl)
xfce4-session (optional, for autostart desktop entries)

### For an DE minimal
libxfce4panel-4.20
libxfce4ui-4.20
libxfce4util-4.20
libxfce4windowing-4.20
xfce4-appfinder-4.20
xfce4-notifyd-0.9
xfce4-panel-4.20
xfce4-power-manager-4.20
xfce4-pulseaudio-plugin
xfce4-settings-4.20
xfce4-whiskermenu-plugin