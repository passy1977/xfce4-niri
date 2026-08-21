/***************************************************************************
 *
 * xfce4-niri
 * Copyright (C) 2026 Antonio Salsi <passy.linux@zresa.it>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2.1 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, see <https://www.gnu.org/licenses/>.
 *
 ***************************************************************************/

use std::cmp::Ordering;
use std::ffi::CStr;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use gtk::gio::{Icon, ThemedIcon};
use gtk::glib::{self, Cast, Error, FileError, IntoGStr, markup_escape_text};
use crate::xfce::{Rc, is_accessible_dir, resource_lookup_all, resource_save_location};

use xfce4_niri_lib::binary_exists;
// use crate::gui::{DEFAULT_ICON, DESKTOP, binary_exists, is_accessible_dir};
use crate::models::run_hook::RunHook;


/// Port of `XfaeItem`: one `autostart/*.desktop` file, as read through
/// `XfceRc` — so a user file overriding a system one reads as a single entry.
pub(crate) struct Item {
    pub(crate) name: String,
    pub(crate) icon: Icon,
    pub(crate) comment: String,
    pub(crate) rel_path: String,
    pub(crate) hidden: bool,
    pub(crate) tooltip: String,
    pub(crate) run_hook: RunHook,
    pub(crate) show_in_xfce: bool,
    pub(crate) show_in_override: bool
}

/// What a user copy under `~/.config/autostart` exists to say over the system
/// file it shadows: the two things the list lets one change on an entry, the
/// toggle of the first column and the trigger of the last.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct State {
    pub(crate) enabled: bool,
    pub(crate) run_hook: RunHook,
}


impl Item {
    /// The only group of an autostart `.desktop` file.
    pub(crate) const DESKTOP_ENTRY: &CStr = c"Desktop Entry";

    /// Port of `xfae_item_new`: `None` for anything the C code skips, i.e. a
    /// non `Application` entry, one hidden from this desktop by `NotShowIn`,
    /// or one whose `TryExec` binary is missing.
    pub(crate) fn new(rel_path: &str, desktop: &str, default_icon: &str) -> Option<Self> {

        let rc = Rc::config_open(rel_path, true)?;
        rc.set_group(Self::DESKTOP_ENTRY);

        if !rc.read_entry(c"Type").is_some_and(|it| it.eq_ignore_ascii_case("Application")) {
            return None
        }

        let icon = rc.read_entry(c"Icon").unwrap_or_else(|| default_icon.to_string());
        let command = rc.read_entry(c"Exec").unwrap_or_default();

        let mut item = Self {
            name: rc.read_entry(c"Name").unwrap_or_default(),
            icon: ThemedIcon::with_default_fallbacks(&icon).upcast(),
            comment: rc.read_entry(c"Comment").unwrap_or_default(),
            rel_path: rel_path.to_string(),
            hidden: rc.read_bool_entry(c"Hidden", false),
            tooltip: format!("<b>Command:</b> {}", markup_escape_text(&command)),
            run_hook: RunHook::from_value(rc.read_int_entry(c"RunHook", RunHook::Login.value())),
            show_in_xfce: false,
            show_in_override: rc.read_bool_entry(c"X-XFCE-Autostart-Override", false),
        };

        if rc.read_list_entry(c"NotShowIn").iter().any(|it| it.eq_ignore_ascii_case(desktop)) {
            return None
        }

        // No `OnlyShowIn` at all means "every desktop", so the entry is ours.
        let only_show_in = rc.read_list_entry(c"OnlyShowIn");
        item.show_in_xfce = only_show_in.is_empty()
            || only_show_in.iter().any(|it| it.eq_ignore_ascii_case(desktop));

        if let Some(try_exec) = rc.read_entry(c"TryExec")
            && !binary_exists(&try_exec) {
            return None
        }

        Some(item)
    }

    
    /// Port of `xfae_model_get`: reads back the entry behind a row, which is what
    /// the edit dialog is filled with.
    pub(crate) fn get(rel_path: &str) -> Result<(String, String, String, RunHook), Error> {

        let Some(rc) = Rc::config_open(rel_path, true) else {
            return Err(Error::new(
                FileError::Io,
                &format!("Failed to open {rel_path} for reading"),
            ))
        };

        rc.set_group(Self::DESKTOP_ENTRY);

        Ok((
            rc.read_entry(c"Name").unwrap_or_default(),
            rc.read_entry(c"Comment").unwrap_or_default(),
            rc.read_entry(c"Exec").unwrap_or_default(),
            RunHook::from_value(rc.read_int_entry(c"RunHook", RunHook::Login.value())),
        ))
    }

    pub(crate) fn free_rel_path(name: &str) -> Result<(String, PathBuf), glib::Error> {

        let name = name.replace('/', "-");

        let mut n = 0;

        loop {

            let rel_path = if n == 0 {
                format!("autostart/{name}.desktop")
            } else {
                format!("autostart/{name}-{n}.desktop")
            };

            if resource_lookup_all(&rel_path).is_empty() {

                // `create`: `~/.config/autostart` può non esistere ancora.
                let path = resource_save_location(&rel_path, true).ok_or_else(|| {
                    glib::Error::new(
                        FileError::Io,
                        &format!("Failed to find a save location for {rel_path}"),
                    )
                })?;

                return Ok((rel_path, path))
            }

            n += 1;
        }
    }


    pub(crate) fn store(
        path: &Path,
        name: &str,
        comment: &str,
        exec: &str,
        run_hook: RunHook,
    ) -> Result<(), Error> {

        let Some(rc) = Rc::simple_open(path, false) else {
            return Err(Error::new(
                FileError::Io,
                &format!("Failed to open {} for writing", path.display()),
            ))
        };

        rc.set_group(Self::DESKTOP_ENTRY);

        let written = rc.write_entry(c"Encoding", "UTF-8")
            && rc.write_entry(c"Version", "0.1.0")
            && rc.write_entry(c"Type", "Application")
            && rc.write_entry(c"Name", name)
            && rc.write_entry(c"Comment", comment)
            && rc.write_entry(c"Exec", exec)
            && rc.write_bool_entry(c"StartupNotify", false)
            && rc.write_bool_entry(c"Terminal", false)
            && rc.write_bool_entry(c"Hidden", false)
            && rc.write_int_entry(c"RunHook", run_hook.value());

        if !written {
            rc.rollback();

            return Err(Error::new(
                FileError::Inval,
                &format!("Failed to write {}", path.display()),
            ))
        }

        // `Drop` flusherebbe comunque, così però un errore resta visibile qui.
        rc.flush();

        Ok(())
    }


    /// Port of `xfae_model_remove`: unlinks every copy of `rel_path`, the same
    /// list [`Self::is_removable`] has checked sits in writable directories. A
    /// copy vanished in the meantime is not an error, the row goes away either way.
    pub(crate) fn remove(rel_path: &str) -> Result<(), Error> {

        let files = resource_lookup_all(rel_path);

        if files.is_empty() {
            return Err(Error::new(
                FileError::Noent,
                &format!("Failed to find {rel_path}"),
            ))
        }

        for file in &files {
            match fs::remove_file(file) {
                Ok(()) => (),
                Err(error) if error.kind() == ErrorKind::NotFound => (),
                Err(error) => return Err(Error::new(
                    FileError::Io,
                    &format!("Failed to remove {file}: {error}"),
                )),
            }
        }

        Ok(())
    }

    /// The user copy of `rel_path` under `$XDG_CONFIG_HOME`, next to the
    /// highest priority copy of it in a system directory — `/etc/xdg/autostart`
    /// and the rest of `$XDG_CONFIG_DIRS` — when one exists. The user path is
    /// where the file *would* go, it need not be there yet.
    fn locations(rel_path: &str) -> Result<(PathBuf, Option<PathBuf>), Error> {

        let user = resource_save_location(rel_path, false).ok_or_else(|| Error::new(
            FileError::Io,
            &format!("Failed to find a save location for {rel_path}"),
        ))?;

        // `xfce_resource_lookup_all` answers in priority order, the user copy
        // first: what is left after it is the system one that gets shadowed.
        let system = resource_lookup_all(rel_path)
            .into_iter()
            .map(PathBuf::from)
            .find(|file| *file != user);

        Ok((user, system))
    }

    /// [`Self::is_enabled`] and the `RunHook` key read off one file instead of
    /// the merged view [`Self::new`] builds: the state an entry falls back to
    /// once the user copy shadowing it is gone. `None` when the file cannot be
    /// read at all.
    fn file_state(path: &Path, desktop: &str) -> Option<State> {

        let rc = Rc::simple_open(path, true)?;
        rc.set_group(Self::DESKTOP_ENTRY);

        let only_show_in = rc.read_list_entry(c"OnlyShowIn");

        Some(State {
            enabled: Self::enabled(
                rc.read_bool_entry(c"Hidden", false),
                only_show_in.is_empty()
                    || only_show_in.iter().any(|it| it.eq_ignore_ascii_case(desktop)),
                rc.read_bool_entry(c"X-XFCE-Autostart-Override", false),
            ),
            run_hook: RunHook::from_value(rc.read_int_entry(c"RunHook", RunHook::Login.value())),
        })
    }

    /// The whole of what the list writes on an entry, put where the user may
    /// write.
    ///
    /// A system entry — one out of `/etc/xdg/autostart` or another
    /// `$XDG_CONFIG_DIRS` directory — is never touched: it is copied into
    /// `~/.config/autostart` first and the copy, the one `xfce_rc_config_open`
    /// reads before the system file, carries `state`. Back on the state the
    /// system file already holds, that copy has nothing left to say and is
    /// unlinked again, so the entry is a plain system one once more.
    ///
    /// Both halves of [`State`] travel together for that last step: a copy is
    /// only spent once it agrees with the system file on the toggle *and* on
    /// the trigger, otherwise changing one back would take the other with it.
    ///
    /// An entry the user owns has no file to fall back to, so it is only ever
    /// rewritten — dropping it is what Remove is for.
    pub(crate) fn set_state(rel_path: &str, desktop: &str, state: State) -> Result<(), Error> {

        let (user, system) = Self::locations(rel_path)?;

        // Back on the state the system file gives anyway: the shadow goes, and
        // one already gone is not an error — the entry reads the same either way.
        if let Some(system) = &system
            && Self::file_state(system, desktop) == Some(state) {

            return match fs::remove_file(&user) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
                Err(error) => Err(Error::new(
                    FileError::Io,
                    &format!("Failed to remove {}: {error}", user.display()),
                )),
            }
        }

        if !user.exists() {

            let Some(system) = &system else {
                return Err(Error::new(
                    FileError::Noent,
                    &format!("Failed to find {rel_path}"),
                ))
            };

            // `create`: `~/.config/autostart` may not exist yet.
            resource_save_location(rel_path, true).ok_or_else(|| Error::new(
                FileError::Io,
                &format!("Failed to create the save location for {rel_path}"),
            ))?;

            fs::copy(system, &user).map_err(|error| Error::new(
                FileError::Io,
                &format!("Failed to copy {} to {}: {error}", system.display(), user.display()),
            ))?;

            // The mode comes over with the copy, and a system file may well be
            // read only: the user's own copy has to take the new state.
            if let Ok(metadata) = fs::metadata(&user) {
                let mut permissions = metadata.permissions();
                permissions.set_mode(permissions.mode() | 0o600);
                let _ = fs::set_permissions(&user, permissions);
            }
        }

        let Some(rc) = Rc::simple_open(&user, false) else {
            return Err(Error::new(
                FileError::Io,
                &format!("Failed to open {} for writing", user.display()),
            ))
        };

        rc.set_group(Self::DESKTOP_ENTRY);

        // `X-XFCE-Autostart-Override` is what [`Self::is_enabled`] reads on an
        // entry `OnlyShowIn` keeps out of this desktop, and only on such a one.
        let only_show_in = rc.read_list_entry(c"OnlyShowIn");
        let show_in_xfce = only_show_in.is_empty()
            || only_show_in.iter().any(|it| it.eq_ignore_ascii_case(desktop));

        let written = rc.write_bool_entry(c"Hidden", !state.enabled)
            && (show_in_xfce || rc.write_bool_entry(c"X-XFCE-Autostart-Override", state.enabled))
            && rc.write_int_entry(c"RunHook", state.run_hook.value());

        if !written {
            rc.rollback();

            return Err(Error::new(
                FileError::Inval,
                &format!("Failed to write {}", user.display()),
            ))
        }

        rc.flush();

        Ok(())
    }

    /// Port of `xfae_item_is_enabled`: an entry not meant for this desktop
    /// needs the `X-XFCE-Autostart-Override` opt in on top of not being hidden.
    pub(crate) fn is_enabled(&self) -> bool {
        Self::enabled(self.hidden, self.show_in_xfce, self.show_in_override)
    }

    /// The three keys [`Self::is_enabled`] answers on, away from an [`Item`]:
    /// the same question [`Self::file_enabled`] asks of a single file.
    fn enabled(hidden: bool, show_in_xfce: bool, show_in_override: bool) -> bool {
        !hidden && (show_in_xfce || show_in_override)
    }

    /// Port of `xfae_item_is_removable`: removable only while every directory
    /// holding a copy of the file can be written to.
    pub(crate) fn is_removable(&self) -> bool {
        Self::removable(&self.rel_path)
    }

    /// [`Self::is_removable`] on a `rel_path` alone: what a row has to be told
    /// again once [`Self::set_enabled`] has added or dropped the user copy.
    pub(crate) fn removable(rel_path: &str) -> bool {
        resource_lookup_all(rel_path)
            .iter()
            .all(|file| Path::new(file).parent().is_some_and(is_accessible_dir))
    }

    /// The markup `xfae_model_get_value` builds for `XFAE_MODEL_COLUMN_NAME`:
    /// "name (comment)", in italics when the entry is not shown on this desktop.
    pub(crate) fn markup(&self) -> String {

        let name = markup_escape_text(&self.name);

        let name = if self.comment.is_empty() {
            name.to_string()
        } else {
            format!("{name} ({})", markup_escape_text(&self.comment))
        };

        if self.show_in_xfce { name } else { format!("<i>{name}</i>") }
    }

    /// Port of `xfae_item_sort_default`: entries for this desktop first, then
    /// by name.
    pub(crate) fn sort_default(a: &Self, b: &Self) -> Ordering {
        match (a.show_in_xfce, b.show_in_xfce) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => a.name.as_str().run_with_gstr(|name| name.collate(b.name.as_str())),
        }
    }
}


#[cfg(test)]
mod tests {

    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    use xfce4_niri_lib::test_support::TempDir;

    use super::*;
    use crate::test_support::xfce_resource_ready;

    /// An item built by hand: [`Item::new`] needs a real `.desktop` file under
    /// the XFCE config directories, everything below is pure logic on the fields.
    fn item(name: &str, show_in_xfce: bool) -> Item {
        Item {
            name: name.to_string(),
            icon: ThemedIcon::with_default_fallbacks("application-x-executable").upcast(),
            comment: String::new(),
            rel_path: format!("autostart/{name}.desktop"),
            hidden: false,
            tooltip: String::new(),
            run_hook: RunHook::default(),
            show_in_xfce,
            show_in_override: false,
        }
    }

    /// One `.desktop` file under `dir`, holding exactly the keys
    /// [`Item::file_state`] reads.
    fn desktop_file(
        dir: &TempDir,
        name: &str,
        keys: &[(&CStr, bool)],
        only_show_in: &[&str],
        run_hook: Option<RunHook>,
    ) -> PathBuf {

        let path = dir.path().join(format!("{name}.desktop"));

        let rc = Rc::simple_open(&path, false).expect("a writable view");
        rc.set_group(Item::DESKTOP_ENTRY);

        rc.write_entry(c"Type", "Application");

        for (key, value) in keys {
            rc.write_bool_entry(key, *value);
        }

        if !only_show_in.is_empty() {
            rc.write_list_entry(c"OnlyShowIn", only_show_in);
        }

        if let Some(run_hook) = run_hook {
            rc.write_int_entry(c"RunHook", run_hook.value());
        }

        drop(rc);

        path
    }

    /// The same truth table as [`Item::is_enabled`], read off a file rather
    /// than off the fields: this is what a system copy is asked when its user
    /// copy is about to go, so the two answers have to agree.
    #[test]
    fn file_state_reads_the_toggle_off_one_file() {

        let dir = TempDir::new();

        for (n, (hidden, show_in_xfce, show_in_override, expected)) in [
            (false, true, false, true),
            (false, true, true, true),
            (true, true, false, false),
            (true, true, true, false),
            (false, false, false, false),
            (false, false, true, true),
            (true, false, false, false),
            (true, false, true, false),
        ].into_iter().enumerate() {

            let path = desktop_file(
                &dir,
                &format!("state-{n}"),
                &[(c"Hidden", hidden), (c"X-XFCE-Autostart-Override", show_in_override)],
                if show_in_xfce { &[] } else { &["GNOME"] },
                None,
            );

            assert_eq!(
                Item::file_state(&path, "XFCE").map(|it| it.enabled),
                Some(expected),
                "hidden {hidden}, show_in_xfce {show_in_xfce}, override {show_in_override}",
            );
        }
    }

    /// The trigger travels with the toggle: a user copy is only spent once it
    /// agrees with the system file on both.
    #[test]
    fn file_state_reads_the_trigger_off_one_file() {

        let dir = TempDir::new();

        for (n, run_hook) in RunHook::ALL.into_iter().enumerate() {

            let path = desktop_file(&dir, &format!("hook-{n}"), &[], &[], Some(run_hook));

            assert!(Item::file_state(&path, "XFCE").expect("the file just written").run_hook == run_hook);
        }
    }

    /// Every key missing is the common case — a system `.desktop` file says
    /// nothing about `Hidden` or `RunHook` — and reads the way `Item::new` does.
    #[test]
    fn file_state_falls_back_to_enabled_on_login() {

        let dir = TempDir::new();
        let path = desktop_file(&dir, "bare", &[], &[], None);

        let state = Item::file_state(&path, "XFCE").expect("the file just written");

        assert!(state.enabled);
        assert!(state.run_hook == RunHook::Login);
    }

    /// `OnlyShowIn` is matched the way [`Item::new`] matches it, case and all.
    #[test]
    fn file_state_matches_only_show_in_ignoring_case() {

        let dir = TempDir::new();
        let path = desktop_file(&dir, "listed", &[], &["gnome", "xfce"], None);

        assert_eq!(Item::file_state(&path, "XFCE").map(|it| it.enabled), Some(true));
        assert_eq!(Item::file_state(&path, "KDE").map(|it| it.enabled), Some(false));
    }

    /// A path no file can be read from: [`Item::set_state`] keeps the user copy
    /// rather than unlinking it on a guess.
    #[test]
    fn file_state_is_none_without_a_readable_file() {
        assert!(Item::file_state(Path::new(OsStr::from_bytes(b"/tmp/nul\0byte")), "XFCE").is_none());
    }

    /// An entry meant for this desktop is enabled unless `Hidden`; one meant for
    /// another desktop needs `X-XFCE-Autostart-Override` on top of that.
    #[test]
    fn is_enabled_needs_the_override_outside_this_desktop() {

        for (show_in_xfce, hidden, show_in_override, expected) in [
            (true, false, false, true),
            (true, false, true, true),
            (true, true, false, false),
            (true, true, true, false),
            (false, false, false, false),
            (false, false, true, true),
            (false, true, false, false),
            (false, true, true, false),
        ] {
            let mut item = item("entry", show_in_xfce);
            item.hidden = hidden;
            item.show_in_override = show_in_override;

            assert_eq!(
                item.is_enabled(),
                expected,
                "show_in_xfce {show_in_xfce}, hidden {hidden}, override {show_in_override}",
            );
        }
    }

    /// No directory holds a copy, so there is nothing to refuse: the C loop over
    /// an empty list falls through to `TRUE` as well.
    #[test]
    fn is_removable_is_true_when_the_file_is_nowhere() {
        xfce_resource_ready();
        assert!(item("no-such-entry-anywhere", true).is_removable());
    }

    #[test]
    fn markup_appends_the_comment() {

        let mut item = item("Screensaver", true);
        assert_eq!(item.markup(), "Screensaver");

        item.comment = "Lock the screen".to_string();
        assert_eq!(item.markup(), "Screensaver (Lock the screen)");
    }

    /// Both halves end up inside markup, so both are escaped.
    #[test]
    fn markup_escapes_name_and_comment() {

        let mut item = item("Cut & Paste", true);
        item.comment = "<b>bold</b>".to_string();

        assert_eq!(item.markup(), "Cut &amp; Paste (&lt;b&gt;bold&lt;/b&gt;)");
    }

    #[test]
    fn markup_is_italic_outside_this_desktop() {

        let mut item = item("Elsewhere", false);
        assert_eq!(item.markup(), "<i>Elsewhere</i>");

        item.comment = "Other desktop".to_string();
        assert_eq!(item.markup(), "<i>Elsewhere (Other desktop)</i>");
    }

    #[test]
    fn sort_default_puts_this_desktop_first() {

        let ours = item("zzz", true);
        let theirs = item("aaa", false);

        assert_eq!(Item::sort_default(&ours, &theirs), Ordering::Less);
        assert_eq!(Item::sort_default(&theirs, &ours), Ordering::Greater);
    }

    #[test]
    fn sort_default_falls_back_to_the_name() {

        for show_in_xfce in [true, false] {
            let first = item("aaa", show_in_xfce);
            let second = item("bbb", show_in_xfce);

            assert_eq!(Item::sort_default(&first, &second), Ordering::Less);
            assert_eq!(Item::sort_default(&second, &first), Ordering::Greater);
            assert_eq!(Item::sort_default(&first, &first), Ordering::Equal);
        }
    }

    /// Both `None` branches of [`Item::new`] that need no config file at all: a
    /// path `XfceRc` cannot be handed, and one no config directory holds.
    #[test]
    fn new_is_none_without_a_desktop_file() {

        xfce_resource_ready();

        assert!(Item::new("autostart/nul\0byte.desktop", "XFCE", "icon").is_none());
        assert!(Item::new("autostart/xfce4-niri-no-such-entry.desktop", "XFCE", "icon").is_none());
    }

    /// The branch of [`Item::remove`] no config directory is needed for: nothing
    /// to unlink is a failure, not a silent success — the row would go from the
    /// list while its file stays on disk.
    #[test]
    fn remove_fails_when_the_file_is_nowhere() {

        xfce_resource_ready();

        assert!(Item::remove("autostart/xfce4-niri-no-such-entry.desktop").is_err());
    }

}