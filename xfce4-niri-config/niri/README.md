# niri configuration

Personal [niri](https://niri-wm.github.io/niri/) (scrollable-tiling Wayland
compositor) setup, running on **Void Linux** (`xbps` package manager, `runit`
init — not systemd). That last detail matters throughout this config: niri
does not process `/etc/xdg/autostart/*.desktop` files, and there is no
`systemd-backlight@.service`, so a few things that "just work" on a systemd
desktop are handled explicitly here instead.

## Layout

```
config.kdl              entry point: environment, hotkey-overlay, screenshot path
niri.d/
  _index.kdl            includes every module below, in load order
  10-inputs.kdl          keyboard (it layout), touchpad, mouse, trackpoint
  20-outputs.kdl         monitor mode/scale/transform
  30-appearance.kdl      gaps, borders, focus-ring, shadows
  40-rules.kdl           per-window rules (floating exceptions, workarounds)
  50-startup.kdl         spawn-at-startup: portals, panel, tray apps, backup...
  60-keys.kdl            all keybindings
  animations/            swappable animation presets (stock is active)
bin/
  lockscreen             swayidle wrapper, toggles on xfce4-power-manager's presentation-mode
  brightness-daemon      persists backlight level across reboots (see udev rule below)
swaylock/config          styled lock screen, palette matched to the GTK theme
```

## Applications in use

Installed packages checked with `xbps-query`; versions as of this system's
current state.

| Role | Package(s) (xbps) | Notes |
|---|---|---|
| Compositor | `niri` | |
| Terminal | `alacritty` | `Mod+T` |
| App launcher | `xfce4-appfinder` | `Mod+Space`, opened floating (`40-rules.kdl`) |
| File manager | `Thunar`, `thunar-archive-plugin`, `thunar-volman` | started as `--daemon` at startup |
| Panel / tray | `xfce4-panel` | layer-shell aware, hosts nm-applet/blueman-applet |
| Settings daemon | `xfce4-settings` (`xfsettingsd`) | applies GTK theme, cursor, fonts |
| Power management | `xfce4-power-manager` | battery, backlight, presentation-mode flag read by `bin/lockscreen` and `bin/brightness-daemon` |
| Networking | `network-manager-applet` (`nm-applet`) | |
| Bluetooth | `blueman` (`blueman-applet`) | tray icon in xfce4-panel |
| Session/seat management | `elogind`, `seatd` | provides the logind-compatible API niri/polkit/xfce4-power-manager expect under runit |
| PolicyKit agent | `polkit-gnome`, `polkit` | started with a retry loop — races elogind/polkitd at boot |
| Audio | `pulseaudio`, `pulseaudio-utils`, `pavucontrol`, `xfce4-pulseaudio-plugin` | volume keys use `pactl` (no WirePlumber/`wpctl` here) |
| Media keys | `playerctl` | MPRIS play/stop/prev/next |
| Backlight | `brightnessctl` | brightness keys; see [udev rule](#backlight-permissions-udev) |
| Wallpaper | `swaybg` | |
| Idle / lock | `swayidle`, `swaylock` | `Mod+Shift+Escape`, auto-lock in `bin/lockscreen` |
| Secrets | `gnome-keyring` | backend for the portal's `Secret` interface |
| Desktop portals | `xdg-desktop-portal`, `xdg-desktop-portal-gtk`, `xdg-desktop-portal-gnome` | see [below](#xdg-desktop-portal) |

## Keybindings

All defined in `niri.d/60-keys.kdl` (`Mod` = the Super/Windows key). Binds
commented out in that file (e.g. `Mod+Shift+E` quit, `Mod+Shift+P`
power-off-monitors) are left disabled on purpose and are not listed here.

**Apps & session**

| Bind | Action |
|---|---|
| `Mod+Shift+O` | Show hotkey overlay |
| `Mod+T` | Terminal — `alacritty` |
| `Mod+Space` | App launcher — `xfce4-appfinder` |
| `Mod+E` | File manager — `Thunar` |
| `Mod+Shift+Escape` | Lock screen — `swaylock` (skipped if presentation mode is on) |
| `Ctrl+Shift+Escape` | Mission Center (Flatpak) |
| `Mod+Escape` | Toggle keyboard-shortcuts inhibit |
| `Ctrl+Alt+Delete` | Quit niri |

**Media & hardware**

| Bind | Action |
|---|---|
| `XF86AudioRaiseVolume` / `LowerVolume` | Volume ±10% (`pactl`) |
| `XF86AudioMute` / `AudioMicMute` | Toggle sink/source mute |
| `XF86AudioPlay` / `Stop` / `Prev` / `Next` | `playerctl` media control |
| `XF86MonBrightnessUp` / `Down` | Brightness ±10% (`brightnessctl`) |

**Focus & move — windows/columns**

| Bind | Action |
|---|---|
| `Mod+←/↓/↑/→` or `Mod+H/J/K/L` | Focus column left/right, window down/up |
| `Mod+Ctrl+←/↓/↑/→` or `Mod+Ctrl+H/J/K/L` | Move column/window in that direction |
| `Mod+Home` / `End` | Focus first/last column |
| `Mod+Ctrl+Home` / `End` | Move column to first/last |
| `Mod+Comma` / `Period` | Consume window into column / expel from column |
| `Mod+è` / `Mod+plus` | Consume-or-expel window left/right |

**Monitors**

| Bind | Action |
|---|---|
| `Mod+Shift+←/↓/↑/→` or `Mod+Shift+H/J/K/L` | Focus monitor in that direction |
| `Mod+Shift+Ctrl+←/↓/↑/→` or `+H/J/K/L` | Move column to monitor in that direction |

**Workspaces**

| Bind | Action |
|---|---|
| `Mod+Page_Down/Up` or `Mod+U/I` | Focus workspace down/up |
| `Mod+Ctrl+Page_Down/Up` or `Mod+Ctrl+U/I` | Move column to workspace down/up |
| `Mod+Shift+Page_Down/Up` or `Mod+Shift+U/I` | Move whole workspace down/up |
| `Mod+[Ctrl/Shift+]WheelScroll` | Same actions, via scroll wheel |
| `Mod+1`…`9` | Focus workspace *N* |
| `Mod+Ctrl+1`…`9` | Move column to workspace *N* |

**Column/window layout**

| Bind | Action |
|---|---|
| `Mod+R` / `Shift+R` | Switch preset column width (forward/back) |
| `Mod+Ctrl+R` | Reset window height |
| `Mod+Ctrl+Shift+R` | Switch preset window height |
| `Mod+F` | Maximize column |
| `Mod+Shift+F` | Fullscreen window |
| `Mod+M` | Maximize window to edges |
| `Mod+Ctrl+F` | Expand column to available width |
| `Mod+C` / `Ctrl+C` | Center column / center visible columns |
| `Mod+Minus/Equal` | Column width −10%/+10% |
| `Mod+Shift+Minus/Equal` | Window height −10%/+10% |
| `Mod+V` | Toggle window floating |
| `Mod+Shift+V` | Switch focus floating ↔ tiling |
| `Mod+W` | Toggle tabbed display for column |
| `Mod+O` | Toggle overview |
| `Mod+Q` | Close window |

**Screenshots**

| Bind | Action |
|---|---|
| `Print` | Screenshot (interactive) |
| `Ctrl+Print` | Screenshot current screen |
| `Alt+Print` | Screenshot focused window |

## Theming

GTK/window-manager theming comes from **Minimal-Light**
(`~/.local/share/themes/Minimal-Light`, source:
[passy1977/XFCE4-theme-Minimal-Light](https://github.com/passy1977/XFCE4-theme-Minimal-Light)),
applied via `xfsettingsd`:

- `GtkTheme` / `MetacityTheme` / `xfwm4` theme: **Minimal-Light**
- Icon theme: **Zafiro-icons-Light**
- Cursor theme: **DeepinDark-cursors**

`swaylock/config` deliberately re-derives its palette from the theme's own
sources (`gtk-2.0/gtkrc`, `xfwm4/themerc`) instead of using swaylock's
defaults, so the lock screen reads as part of the same desktop:

- surface/background: `#ffffff` / `#f5f6f7`
- text/foreground: `#111521`
- accent (selection/link): `#5294e2`
- insensitive fg/bg: `#a9acb2` / `#fbfcfc`

The "wrong password" red (`#9b0000`) is intentionally the same value as
`urgent-color` in `niri.d/30-appearance.kdl`, so an urgent window border and a
failed unlock attempt use the same accent.

## xdg-desktop-portal

`config.kdl` sets `XDG_CURRENT_DESKTOP=niri`, which is what makes the portal
frontend pick up `~/.config/xdg-desktop-portal/niri-portals.conf`:

```ini
[preferred]
default=gnome;gtk;
org.freedesktop.impl.portal.Access=gtk;
org.freedesktop.impl.portal.Notification=gtk;
org.freedesktop.impl.portal.FileChooser=gtk;
org.freedesktop.impl.portal.Secret=gnome-keyring;
```

- Falls back to the **gnome** backend, then **gtk**, for anything not listed
  explicitly (niri has no portal backend of its own, so it needs to borrow
  one — GNOME's covers the Shell-specific bits like screencast/screenshot).
- **Access**, **Notification**, and **FileChooser** are pinned to `gtk`, since
  a plain GTK file picker/dialog fits a non-GNOME-Shell session better than
  the GNOME ones.
- **Secret** is pinned to `gnome-keyring`, matching the `gnome-keyring` daemon
  actually used to store credentials on this system.
- `xdg-desktop-portal` itself is spawned explicitly at startup
  (`niri.d/50-startup.kdl`), since runit won't launch it via autostart the
  way a systemd user session would.

Whenever a backlight device is added, this rule hands group **video** write
access to its `brightness` and `bl_power` sysfs files. Without it, both the
brightness keys (`XF86MonBrightnessUp`/`Down` → `brightnessctl` in
`60-keys.kdl`) and `bin/brightness-daemon` would need root just to change or
persist the screen brightness. `bin/brightness-daemon` exists because of the
same systemd/runit gap: the kernel resets brightness to max on every boot and
there is no `systemd-backlight@.service` to restore/save it, so this daemon
does that job itself, polling the sysfs file and storing the last value under
`~/.local/state/niri-brightness`.

## Disable unused icon from xfce-settings
[.local/share/applications/xfce4-accessibility-settings.desktop](../../.local/share/applications/xfce4-accessibility-settings.desktop)  
[.local/share/applications/xfce-keyboard-settings.desktop](../../.local/share/applications/xfce-keyboard-settings.desktop)  
[.local/share/applications/xfce-mouse-settings.desktop](../../.local/share/applications/xfce-mouse-settings.desktop)


