# xfce4-niri — Void Linux package

xbps-src template for [xfce4-niri](https://github.com/passy1977/xfce4-niri).
It is meant to be copied into a [void-packages](https://github.com/void-linux/void-packages)
checkout and built from there.

```
packaging/void/xfce4-niri/
├── template                build recipe (version, dependencies, install layout)
├── files/
│   └── xfce4-niri.desktop  "Niri (Xfce)" session for display managers
└── README.md               this file
```

## Build

Run on the Void host (not inside a distrobox container):

```sh
# once: clone void-packages and set up the build chroot
git clone --depth 1 https://github.com/void-linux/void-packages.git ~/void-packages
cd ~/void-packages && ./xbps-src binary-bootstrap

# every build: copy the template from this repository, then build
rsync -a --delete --exclude README.md \
    /path/to/xfce4-niri/packaging/void/xfce4-niri/ ~/void-packages/srcpkgs/xfce4-niri/
cd ~/void-packages
./xbps-src pkg xfce4-niri          # add -Q to also run the test suite
```

The template downloads the tagged release from GitHub, so it does not build the local working tree:
it builds the release named by `version=`.

If a previous build failed halfway, run `./xbps-src clean xfce4-niri` first.
Otherwise xbps-src resumes from the failed step and skips earlier ones such as `post_patch`.

The package ends up in `hostdir/binpkgs/xfce4-niri-<version>_<revision>.<arch>.xbps`.

## Install

```sh
cd ~/void-packages
sudo xbps-install -R hostdir/binpkgs xfce4-niri
```

Then choose **Niri (Xfce)** in the display manager, or run `startniri` from a TTY.

## Installed layout

| Path | Content |
|---|---|
| `/usr/bin/` | `xfce4-niri`, `xfce4-niri-service`, `xfce4-niri-autostart`, `startniri`, `xfce4-niri-set-cursor` |
| `/usr/libexec/xfce4-niri/lock_screen` | swayidle/swaylock wrapper started by `xfce4-niri-service` |
| `/etc/niri/` | default niri config (`config.kdl`, `niri.d/`, `swaylock/`) |
| `/usr/share/applications/` | `it.salsi.xfce-niri.AutoStart.desktop` |
| `/usr/share/xfce4-niri/applications/` | Xfce settings launchers that replace the stock ones in the niri session only |
| `/usr/share/backgrounds/xfce4-niri/` | wallpaper and lock screen images |
| `/usr/share/wayland-sessions/xfce4-niri.desktop` | display manager session entry |
| `/usr/share/doc/xfce4-niri/` | `README.md`, `niri-config.md` |

Notes:
- The files directly under `/etc/niri` and `/etc/niri/niri.d/` are `conf_files`: xbps keeps local edits when the package is updated. The animation presets in `niri.d/animations/` are not protected, and updates overwrite them.
- niri reads `~/.config/niri/config.kdl` if it exists, otherwise `/etc/niri/config.kdl`; the two files are never merged. `xfce4-niri-set-cursor` copies `/etc/niri` into `~/.config/niri` the first time it runs, if the user has no niri config of their own yet.
- The Xfce settings launchers cannot go into `/usr/share/applications`, because `xfce4-settings` already installs files with the same names there. `startniri` puts `/usr/share/xfce4-niri` first in `XDG_DATA_DIRS`, so the launchers apply only to the niri session.

## Updating to a new release

1. Tag and push the release upstream. The repository must contain an up-to-date `Cargo.lock`, because the build runs with `--locked`.
2. In `packaging/void/xfce4-niri/template`, set `version=` to the new tag and reset `revision=1`.
3. Update `checksum=` with the sha256 of the GitHub tarball:
   ```sh
   xgensum -i xfce4-niri            # needs xtools; run in ~/void-packages, then copy the template back
   # or by hand:
   curl -sL https://github.com/passy1977/xfce4-niri/archive/refs/tags/<version>.tar.gz | sha256sum
   ```
4. Copy the template into void-packages again (see [Build](#build)) and rebuild with `./xbps-src -Q pkg xfce4-niri`.

If a template change does not touch the upstream sources (for example a new dependency), bump `revision=` instead of `version=`.

## Notes for an upstream submission to void-packages

- `osal-rs` and `osal-rs-serde` are git dependencies. void-packages prefers crates published on crates.io.
- Copy only `template` and `files/` into the PR. void-packages expects nothing else in a package directory (`template`, `files/`, `patches/`, `update`), so this README stays out.
