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