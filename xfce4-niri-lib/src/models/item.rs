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
use std::path::Path;

use gtk::gio::{Icon, ThemedIcon};
use gtk::glib::{Cast, IntoGStr, markup_escape_text};
use crate::fxce::{DESKTOP_ENTRY, Rc, is_accessible_dir, resource_lookup_all};

use crate::binary_exists;
// use crate::gui::{DEFAULT_ICON, DESKTOP, binary_exists, is_accessible_dir};
use crate::models::run_hook::RunHook;


/// Port of `XfaeItem`: one `autostart/*.desktop` file, as read through
/// `XfceRc` — so a user file overriding a system one reads as a single entry.
pub struct Item {
    pub name: String,
    pub icon: Icon,
    pub comment: String,
    pub rel_path: String,
    pub hidden: bool,
    pub tooltip: String,
    pub run_hook: RunHook,
    pub show_in_xfce: bool,
    pub show_in_override: bool
}

impl Item {

    /// Port of `xfae_item_new`: `None` for anything the C code skips, i.e. a
    /// non `Application` entry, one hidden from this desktop by `NotShowIn`,
    /// or one whose `TryExec` binary is missing.
    pub fn new(rel_path: &str, desktop: &str, default_icon: &str) -> Option<Self> {

        let rc = Rc::config_open(rel_path, true)?;
        rc.set_group(DESKTOP_ENTRY);

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

    /// Port of `xfae_item_is_enabled`: an entry not meant for this desktop
    /// needs the `X-XFCE-Autostart-Override` opt in on top of not being hidden.
    pub fn is_enabled(&self) -> bool {
        if self.show_in_xfce {
            !self.hidden
        } else {
            !self.hidden && self.show_in_override
        }
    }

    /// Port of `xfae_item_is_removable`: removable only while every directory
    /// holding a copy of the file can be written to.
    pub fn is_removable(&self) -> bool {
        resource_lookup_all(&self.rel_path)
            .iter()
            .all(|file| Path::new(file).parent().is_some_and(is_accessible_dir))
    }

    /// The markup `xfae_model_get_value` builds for `XFAE_MODEL_COLUMN_NAME`:
    /// "name (comment)", in italics when the entry is not shown on this desktop.
    pub fn markup(&self) -> String {

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
    pub fn sort_default(a: &Self, b: &Self) -> Ordering {
        match (a.show_in_xfce, b.show_in_xfce) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => a.name.as_str().run_with_gstr(|name| name.collate(b.name.as_str())),
        }
    }
}


#[cfg(test)]
mod tests {

    use super::*;

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

        assert!(Item::new("autostart/nul\0byte.desktop", "XFCE", "icon").is_none());
        assert!(Item::new("autostart/xfce4-niri-no-such-entry.desktop", "XFCE", "icon").is_none());
    }
}