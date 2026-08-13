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

//! Example: the xfce4-session "Application Autostart" panel, ported to Rust.
//!
//! The widget tree, the columns, the dialog and the run hooks are a one to one
//! translation of `xfae-window.c`, `xfae-dialog.c` and `xfae-model.c` from
//! xfce4-session 4.20.4, transcribed in `APPLICATION_AUTOSTART_CODE.md`. Every
//! ported function keeps the name of the C one it comes from.
//!
//! Two languages meet here:
//!
//! * GTK 3 is used through the `gtk` 0.18 bindings, so no FFI is written by
//!   hand for the widgets, the tree model or the signals.
//! * `libxfce4util` (the `XfceRc` `.desktop` reader and the resource lookup)
//!   and `libxfce4ui` (`xfce_dialog_show_error`) have no Rust bindings, so they
//!   are declared by hand in [`ffi`] and wrapped in the [`xfce`] module. The
//!   two shared libraries are pulled in by `#[link]`, which is why no build
//!   script is needed.
//!
//! The app is presentational: it *reads* the autostart entries so the list has
//! something in it, but it never writes anything. Where the C code calls
//! `xfae_model_add`, `xfae_model_edit`, `xfae_model_toggle` or
//! `xfae_item_remove` — all of which rewrite or unlink a `.desktop` file — this
//! example stops at the store, and a comment marks the spot.
//!
//! Run with:
//!   cargo run -p xfce4-niri-autostart --example xfae_gui
//!
//! Needs GTK 3, libxfce4util and libxfce4ui (and their development files, for
//! `libxfce4util.so` / `libxfce4ui-2.so`) plus a Wayland or X11 display.

use std::cmp::Ordering;
use std::path::Path;

use gtk::gio;
use gtk::glib;
use gtk::glib::IntoGStr;
use gtk::prelude::*;
// `TreeViewColumn` inherits `pack_start`/`add_attribute` from both
// `CellLayoutExt` and `TreeViewColumnExt`: name the one we mean.
use gtk::prelude::TreeViewColumnExt as Column;

const APP_ID: &str = "org.xfce.niri.XfaeGuiExample";

/// `xfce_rc_read_entry (rc, "Icon", "application-x-executable")`.
const DEFAULT_ICON: &str = "application-x-executable";

/// The desktop the C code filters `OnlyShowIn` / `NotShowIn` against.
const DESKTOP: &str = "XFCE";

//
// --- libxfce4util / libxfce4ui FFI -----------------------------------------
//

/// Raw declarations of the C entry points listed under "Analisi Dipendenze
/// Effettive" in `APPLICATION_AUTOSTART_CODE.md`, plus the POSIX `access()`
/// that `xfae_item_is_removable` uses.
///
/// Only the reading half of `XfceRc` is declared: nothing here writes.
mod ffi {

    use std::ffi::{c_char, c_int};

    use gtk::ffi::GtkWindow;
    use gtk::glib::ffi::{GError, gboolean};

    /// `XFCE_RESOURCE_CONFIG` of `XfceResourceType`: `$XDG_CONFIG_HOME` first,
    /// then the `$XDG_CONFIG_DIRS` fallbacks.
    pub const XFCE_RESOURCE_CONFIG: c_int = 1;

    /// `XfceRc`, opaque: it is only ever passed back to the library.
    #[repr(C)]
    pub struct XfceRc {
        _opaque: [u8; 0],
    }

    #[link(name = "xfce4util")]
    unsafe extern "C" {

        pub fn xfce_resource_match(
            type_: c_int,
            pattern: *const c_char,
            unique: gboolean,
        ) -> *mut *mut c_char;

        pub fn xfce_resource_lookup_all(type_: c_int, filename: *const c_char) -> *mut *mut c_char;

        pub fn xfce_rc_config_open(
            type_: c_int,
            resource: *const c_char,
            readonly: gboolean,
        ) -> *mut XfceRc;

        pub fn xfce_rc_close(rc: *mut XfceRc);

        pub fn xfce_rc_set_group(rc: *mut XfceRc, group: *const c_char);

        pub fn xfce_rc_read_entry(
            rc: *const XfceRc,
            key: *const c_char,
            fallback: *const c_char,
        ) -> *const c_char;

        pub fn xfce_rc_read_int_entry(rc: *const XfceRc, key: *const c_char, fallback: c_int) -> c_int;

        pub fn xfce_rc_read_bool_entry(
            rc: *const XfceRc,
            key: *const c_char,
            fallback: gboolean,
        ) -> gboolean;

        pub fn xfce_rc_read_list_entry(
            rc: *const XfceRc,
            key: *const c_char,
            delimiter: *const c_char,
        ) -> *mut *mut c_char;
    }

    #[link(name = "xfce4ui-2")]
    unsafe extern "C" {

        /// Variadic `printf` style, hence the `...`: the wrapper always passes
        /// `"%s"` plus one argument so a message can never be read as a format.
        pub fn xfce_dialog_show_error(
            parent: *mut GtkWindow,
            error: *const GError,
            primary_format: *const c_char,
            ...
        );
    }

    pub const R_OK: c_int = 4;
    pub const W_OK: c_int = 2;
    pub const X_OK: c_int = 1;

    unsafe extern "C" {
        pub fn access(path: *const c_char, mode: c_int) -> c_int;
    }
}

/// Safe wrappers around [`ffi`]: every unsafe block in the example lives here.
mod xfce {

    use std::ffi::{CStr, CString, c_char};
    use std::path::Path;
    use std::ptr;

    use gtk::glib;
    use gtk::glib::ffi::{GFALSE, GTRUE, gboolean};
    use gtk::glib::translate::{Stash, ToGlibPtr};
    use gtk::prelude::*;

    use super::ffi;

    /// The only group of an autostart `.desktop` file.
    pub const DESKTOP_ENTRY: &CStr = c"Desktop Entry";

    fn glib_bool(value: bool) -> gboolean {
        if value { GTRUE } else { GFALSE }
    }

    /// Copies a `NULL` terminated `gchar **` owned by the caller into Rust
    /// strings, then hands it back to `g_strfreev`.
    ///
    /// # Safety
    /// `strv` must be `NULL` or a `g_strfreev`-able string vector.
    unsafe fn from_strv(strv: *mut *mut c_char) -> Vec<String> {

        if strv.is_null() {
            return Vec::new()
        }

        let mut out = Vec::new();

        unsafe {

            let mut cursor = strv;
            while !(*cursor).is_null() {
                out.push(CStr::from_ptr(*cursor).to_string_lossy().into_owned());
                cursor = cursor.add(1);
            }

            glib::ffi::g_strfreev(strv);
        }

        out
    }

    /// `xfce_resource_match (XFCE_RESOURCE_CONFIG, pattern, unique)`: the
    /// relative paths of every config file matching `pattern`.
    pub fn resource_match(pattern: &str, unique: bool) -> Vec<String> {

        let Ok(pattern) = CString::new(pattern) else {
            return Vec::new()
        };

        unsafe {
            from_strv(ffi::xfce_resource_match(
                ffi::XFCE_RESOURCE_CONFIG,
                pattern.as_ptr(),
                glib_bool(unique),
            ))
        }
    }

    /// `xfce_resource_lookup_all (XFCE_RESOURCE_CONFIG, relpath)`: the absolute
    /// path of `relpath` in every config directory that holds a copy of it.
    pub fn resource_lookup_all(relpath: &str) -> Vec<String> {

        let Ok(relpath) = CString::new(relpath) else {
            return Vec::new()
        };

        unsafe {
            from_strv(ffi::xfce_resource_lookup_all(
                ffi::XFCE_RESOURCE_CONFIG,
                relpath.as_ptr(),
            ))
        }
    }

    /// `access (path, R_OK | W_OK | X_OK)`, the test `xfae_item_is_removable`
    /// runs on the directory holding a `.desktop` file.
    pub fn is_accessible_dir(path: &Path) -> bool {

        let Ok(path) = CString::new(path.as_os_str().as_encoded_bytes()) else {
            return false
        };

        unsafe { ffi::access(path.as_ptr(), ffi::R_OK | ffi::W_OK | ffi::X_OK) == 0 }
    }

    /// `xfce_dialog_show_error (parent, error, "%s", message)`.
    pub fn show_error(parent: Option<&gtk::Window>, error: Option<&glib::Error>, message: &str) {

        let parent = parent.map_or(ptr::null_mut(), |it| it.as_ptr());

        // The stash owns the borrowed `GError *` for the length of the call.
        let error: Option<Stash<'_, *mut glib::ffi::GError, glib::Error>> =
            error.map(ToGlibPtr::to_glib_none);
        let error = error.as_ref().map_or(ptr::null(), |it| it.0.cast_const());

        let message = CString::new(message).unwrap_or_default();

        unsafe { ffi::xfce_dialog_show_error(parent, error, c"%s".as_ptr(), message.as_ptr()) };
    }

    /// An open `XfceRc`, closed on drop.
    pub struct Rc(*mut ffi::XfceRc);

    impl Rc {

        /// `xfce_rc_config_open (XFCE_RESOURCE_CONFIG, relpath, readonly)`, the
        /// system and user copies of `relpath` merged into one view.
        ///
        /// Always opened read only here: writing back is the one thing this
        /// example does not do.
        pub fn config_open(relpath: &str, readonly: bool) -> Option<Self> {

            let relpath = CString::new(relpath).ok()?;

            let rc = unsafe {
                ffi::xfce_rc_config_open(
                    ffi::XFCE_RESOURCE_CONFIG,
                    relpath.as_ptr(),
                    glib_bool(readonly),
                )
            };

            (!rc.is_null()).then_some(Self(rc))
        }

        /// `xfce_rc_set_group (rc, group)`.
        pub fn set_group(&self, group: &CStr) {
            unsafe { ffi::xfce_rc_set_group(self.0, group.as_ptr()) };
        }

        /// `xfce_rc_read_entry (rc, key, NULL)`, copied out of the library.
        pub fn read_entry(&self, key: &CStr) -> Option<String> {

            let value = unsafe { ffi::xfce_rc_read_entry(self.0, key.as_ptr(), ptr::null()) };

            (!value.is_null())
                .then(|| unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned())
        }

        /// `xfce_rc_read_int_entry (rc, key, fallback)`.
        pub fn read_int_entry(&self, key: &CStr, fallback: i32) -> i32 {
            unsafe { ffi::xfce_rc_read_int_entry(self.0, key.as_ptr(), fallback) }
        }

        /// `xfce_rc_read_bool_entry (rc, key, fallback)`.
        pub fn read_bool_entry(&self, key: &CStr, fallback: bool) -> bool {
            unsafe { ffi::xfce_rc_read_bool_entry(self.0, key.as_ptr(), glib_bool(fallback)) != GFALSE }
        }

        /// `xfce_rc_read_list_entry (rc, key, ";")`, empty when the key is unset.
        pub fn read_list_entry(&self, key: &CStr) -> Vec<String> {
            unsafe { from_strv(ffi::xfce_rc_read_list_entry(self.0, key.as_ptr(), c";".as_ptr())) }
        }
    }

    impl Drop for Rc {

        /// `xfce_rc_close (rc)`.
        fn drop(&mut self) {
            unsafe { ffi::xfce_rc_close(self.0) };
        }
    }
}

//
// --- XfsmRunHook -----------------------------------------------------------
//

/// Port of `XfsmRunHook`, the trigger an entry is started on.
///
/// The C side registers it as a `GEnum` only to get the labels and the integer
/// written to the `RunHook` key; the discriminants below are those integers and
/// [`RunHook::nick`] returns the `value_nick` strings shown in the UI.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum RunHook {
    #[default]
    Login,
    Logout,
    Shutdown,
    Restart,
    Suspend,
    Hibernate,
    HybridSleep,
    SwitchUser,
}

impl RunHook {

    /// In `GEnumValue` order, the order the combo boxes list them in.
    const ALL: [Self; 8] = [
        Self::Login,
        Self::Logout,
        Self::Shutdown,
        Self::Restart,
        Self::Suspend,
        Self::Hibernate,
        Self::HybridSleep,
        Self::SwitchUser,
    ];

    /// `g_enum_get_value (klass, value)`, falling back to the C default of
    /// `XFSM_RUN_HOOK_LOGIN` for a value no longer in the enum.
    fn from_value(value: i32) -> Self {
        usize::try_from(value)
            .ok()
            .and_then(|it| Self::ALL.get(it))
            .copied()
            .unwrap_or_default()
    }

    /// The position in [`RunHook::ALL`], which is what the `RunHook` key holds.
    fn value(self) -> i32 {
        Self::ALL.iter().position(|it| *it == self).unwrap_or_default() as i32
    }

    /// `GEnumValue.value_nick`.
    fn nick(self) -> &'static str {
        match self {
            Self::Login => "on login",
            Self::Logout => "on logout",
            Self::Shutdown => "on shutdown",
            Self::Restart => "on restart",
            Self::Suspend => "on suspend",
            Self::Hibernate => "on hibernate",
            Self::HybridSleep => "on hybrid sleep",
            Self::SwitchUser => "on switch user",
        }
    }
}

//
// --- XfaeModel -------------------------------------------------------------
//

/// The columns of `XfaeModelColumn`, in the same order.
///
/// The C model is a hand written `GtkTreeModel` over a `GList` of `XfaeItem`;
/// a [`gtk::ListStore`] holds the same columns without the subclassing, so
/// [`RELPATH`](col::RELPATH) is added to keep the one field of `XfaeItem` the
/// callbacks still need.
mod col {
    pub const ICON: u32 = 0;
    pub const NAME: u32 = 1;
    pub const ENABLED: u32 = 2;
    pub const REMOVABLE: u32 = 3;
    pub const TOOLTIP: u32 = 4;
    pub const RUN_HOOK: u32 = 5;
    pub const RELPATH: u32 = 6;
}

/// Port of `XfaeItem`: one `autostart/*.desktop` file, as read through
/// `XfceRc` — so a user file overriding a system one reads as a single entry.
struct Item {
    name: String,
    icon: gio::Icon,
    comment: String,
    relpath: String,
    hidden: bool,
    tooltip: String,
    run_hook: RunHook,
    show_in_xfce: bool,
    show_in_override: bool,
}

impl Item {

    /// Port of `xfae_item_new`: `None` for anything the C code skips, i.e. a
    /// non `Application` entry, one hidden from this desktop by `NotShowIn`,
    /// or one whose `TryExec` binary is missing.
    fn new(relpath: &str) -> Option<Self> {

        let rc = xfce::Rc::config_open(relpath, true)?;
        rc.set_group(xfce::DESKTOP_ENTRY);

        if !rc.read_entry(c"Type").is_some_and(|it| it.eq_ignore_ascii_case("Application")) {
            return None
        }

        let icon = rc.read_entry(c"Icon").unwrap_or_else(|| DEFAULT_ICON.to_string());
        let command = rc.read_entry(c"Exec").unwrap_or_default();

        let mut item = Self {
            name: rc.read_entry(c"Name").unwrap_or_default(),
            icon: gio::ThemedIcon::with_default_fallbacks(&icon).upcast(),
            comment: rc.read_entry(c"Comment").unwrap_or_default(),
            relpath: relpath.to_string(),
            hidden: rc.read_bool_entry(c"Hidden", false),
            tooltip: format!("<b>Command:</b> {}", glib::markup_escape_text(&command)),
            run_hook: RunHook::from_value(rc.read_int_entry(c"RunHook", RunHook::Login.value())),
            show_in_xfce: false,
            show_in_override: rc.read_bool_entry(c"X-XFCE-Autostart-Override", false),
        };

        if rc.read_list_entry(c"NotShowIn").iter().any(|it| it.eq_ignore_ascii_case(DESKTOP)) {
            return None
        }

        // No `OnlyShowIn` at all means "every desktop", so the entry is ours.
        let only_show_in = rc.read_list_entry(c"OnlyShowIn");
        item.show_in_xfce = only_show_in.is_empty()
            || only_show_in.iter().any(|it| it.eq_ignore_ascii_case(DESKTOP));

        if let Some(try_exec) = rc.read_entry(c"TryExec")
            && !binary_exists(&try_exec) {
            return None
        }

        Some(item)
    }

    /// Port of `xfae_item_is_enabled`: an entry not meant for this desktop
    /// needs the `X-XFCE-Autostart-Override` opt in on top of not being hidden.
    fn is_enabled(&self) -> bool {
        if self.show_in_xfce {
            !self.hidden
        } else {
            !self.hidden && self.show_in_override
        }
    }

    /// Port of `xfae_item_is_removable`: removable only while every directory
    /// holding a copy of the file can be written to.
    fn is_removable(&self) -> bool {
        xfce::resource_lookup_all(&self.relpath)
            .iter()
            .all(|file| Path::new(file).parent().is_some_and(xfce::is_accessible_dir))
    }

    /// The markup `xfae_model_get_value` builds for `XFAE_MODEL_COLUMN_NAME`:
    /// "name (comment)", in italics when the entry is not shown on this desktop.
    fn markup(&self) -> String {

        let name = glib::markup_escape_text(&self.name);

        let name = if self.comment.is_empty() {
            name.to_string()
        } else {
            format!("{name} ({})", glib::markup_escape_text(&self.comment))
        };

        if self.show_in_xfce { name } else { format!("<i>{name}</i>") }
    }

    /// Port of `xfae_item_sort_default`: entries for this desktop first, then
    /// by name.
    fn sort_default(a: &Self, b: &Self) -> Ordering {
        match (a.show_in_xfce, b.show_in_xfce) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => a.name.as_str().run_with_gstr(|name| name.collate(b.name.as_str())),
        }
    }
}

/// `g_shell_parse_argv` + `g_find_program_in_path` on a `TryExec` value.
fn binary_exists(try_exec: &str) -> bool {

    let Ok(argv) = glib::shell_parse_argv(try_exec) else {
        return true // Unparsable: the C code leaves the entry alone.
    };

    let Some(program) = argv.first() else {
        return true
    };

    Path::new(program).exists() || glib::find_program_in_path(program).is_some()
}

/// Port of `xfae_model_new` and `xfae_model_init`: every
/// `autostart/*.desktop` under the config directories, sorted the same way.
fn xfae_model_new() -> gtk::ListStore {

    let model = gtk::ListStore::new(&[
        gio::Icon::static_type(), // ICON
        String::static_type(),    // NAME (markup)
        bool::static_type(),      // ENABLED
        bool::static_type(),      // REMOVABLE
        String::static_type(),    // TOOLTIP (markup)
        String::static_type(),    // RUN_HOOK (nick)
        String::static_type(),    // RELPATH
    ]);

    let mut items = xfce::resource_match("autostart/*.desktop", true)
        .iter()
        .filter_map(|relpath| Item::new(relpath))
        .collect::<Vec<_>>();

    items.sort_by(Item::sort_default);

    for item in &items {
        model.set(&model.append(), &[
            (col::ICON, &item.icon),
            (col::NAME, &item.markup()),
            (col::ENABLED, &item.is_enabled()),
            (col::REMOVABLE, &item.is_removable()),
            (col::TOOLTIP, &item.tooltip),
            (col::RUN_HOOK, &item.run_hook.nick()),
            (col::RELPATH, &item.relpath),
        ]);
    }

    model
}

/// Port of `xfae_model_get`: reads back the entry behind a row, which is what
/// the edit dialog is filled with.
fn xfae_model_get(relpath: &str) -> Result<(String, String, String, RunHook), glib::Error> {

    let Some(rc) = xfce::Rc::config_open(relpath, true) else {
        return Err(glib::Error::new(
            glib::FileError::Io,
            &format!("Failed to open {relpath} for reading"),
        ))
    };

    rc.set_group(xfce::DESKTOP_ENTRY);

    Ok((
        rc.read_entry(c"Name").unwrap_or_default(),
        rc.read_entry(c"Comment").unwrap_or_default(),
        rc.read_entry(c"Exec").unwrap_or_default(),
        RunHook::from_value(rc.read_int_entry(c"RunHook", RunHook::Login.value())),
    ))
}

//
// --- XfaeDialog ------------------------------------------------------------
//

/// Port of `XfaeDialog`: name, description, command and trigger of one entry.
struct XfaeDialog {
    dialog: gtk::Dialog,
    name_entry: gtk::Entry,
    descr_entry: gtk::Entry,
    command_entry: gtk::Entry,
    run_hook_combo: gtk::ComboBoxText,
}

impl XfaeDialog {

    /// Port of `xfae_dialog_init` and `xfae_dialog_new`: passing any of the
    /// values turns the "Add application" dialog into "Edit application".
    fn new(
        parent: Option<&gtk::Window>,
        name: Option<&str>,
        descr: Option<&str>,
        command: Option<&str>,
        run_hook: RunHook,
    ) -> Self {

        let dialog = gtk::Dialog::with_buttons(
            Some("Add application"),
            parent,
            gtk::DialogFlags::DESTROY_WITH_PARENT,
            &[("_Cancel", gtk::ResponseType::Cancel), ("_OK", gtk::ResponseType::Ok)],
        );

        dialog.set_default_response(gtk::ResponseType::Ok);
        dialog.set_response_sensitive(gtk::ResponseType::Ok, false);

        let content_area = dialog.content_area();
        content_area.set_border_width(6);

        let grid = gtk::Grid::builder()
            .row_spacing(6)
            .column_spacing(12)
            .border_width(6)
            .build();
        content_area.add(&grid);

        let entry = || gtk::Entry::builder().activates_default(true).hexpand(true).build();
        let label = |text| gtk::Label::builder().label(text).xalign(0.0).build();

        let name_entry = entry();
        grid.attach(&label("Name:"), 0, 0, 1, 1);
        grid.attach(&name_entry, 1, 0, 1, 1);

        let descr_entry = entry();
        grid.attach(&label("Description:"), 0, 1, 1, 1);
        grid.attach(&descr_entry, 1, 1, 1, 1);

        // The command entry carries a browse button next to it.
        let command_entry = entry();
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let browse = gtk::Button::builder().can_default(false).build();
        browse.add(&gtk::Image::from_icon_name(Some("document-open"), gtk::IconSize::Button));
        hbox.pack_start(&command_entry, true, true, 0);
        hbox.pack_start(&browse, false, false, 0);
        grid.attach(&label("Command:"), 0, 2, 1, 1);
        grid.attach(&hbox, 1, 2, 1, 1);

        let run_hook_combo = gtk::ComboBoxText::new();
        run_hook_combo.set_margin_bottom(5);
        for hook in RunHook::ALL {
            run_hook_combo.append_text(hook.nick());
        }
        run_hook_combo.set_active(Some(run_hook.value() as u32));
        grid.attach(&label("Trigger:"), 0, 3, 1, 1);
        grid.attach(&run_hook_combo, 1, 3, 1, 1);

        browse.connect_clicked(
            glib::clone!(@weak dialog, @weak command_entry => move |_| Self::browse(&dialog, &command_entry)),
        );

        // `xfae_dialog_update`: OK stays insensitive until both are filled in.
        name_entry.connect_text_notify(
            glib::clone!(@weak dialog, @weak command_entry => move |name_entry| {
                Self::update(&dialog, name_entry, &command_entry);
            }),
        );
        command_entry.connect_text_notify(
            glib::clone!(@weak dialog, @weak name_entry => move |command_entry| {
                Self::update(&dialog, &name_entry, command_entry);
            }),
        );

        if let Some(name) = name {
            name_entry.set_text(name);
        }
        if let Some(descr) = descr {
            descr_entry.set_text(descr);
        }
        if let Some(command) = command {
            command_entry.set_text(command);
        }
        if name.is_some() || descr.is_some() || command.is_some() {
            dialog.set_title("Edit application");
        }

        dialog.show_all();

        Self { dialog, name_entry, descr_entry, command_entry, run_hook_combo }
    }

    /// Port of `xfae_dialog_update`.
    fn update(dialog: &gtk::Dialog, name_entry: &gtk::Entry, command_entry: &gtk::Entry) {
        dialog.set_response_sensitive(
            gtk::ResponseType::Ok,
            !name_entry.text().is_empty() && !command_entry.text().is_empty(),
        );
    }

    /// Port of `xfae_dialog_browse`: picks the command off the file system.
    fn browse(dialog: &gtk::Dialog, command_entry: &gtk::Entry) {

        let chooser = gtk::FileChooserDialog::with_buttons(
            Some("Select a command"),
            Some(dialog),
            gtk::FileChooserAction::Open,
            &[("Cancel", gtk::ResponseType::Cancel), ("OK", gtk::ResponseType::Accept)],
        );

        chooser.set_local_only(true);

        let command = command_entry.text();
        if command.starts_with('/') {
            chooser.set_filename(command.as_str());
        }

        if chooser.run() == gtk::ResponseType::Accept
            && let Some(filename) = chooser.filename() {
            command_entry.set_text(&filename.to_string_lossy());
        }

        unsafe { chooser.destroy() };
    }

    /// Port of `xfae_dialog_get`.
    fn get(&self) -> (String, String, String, RunHook) {
        (
            self.name_entry.text().trim().to_string(),
            self.descr_entry.text().trim().to_string(),
            self.command_entry.text().trim().to_string(),
            RunHook::from_value(self.run_hook_combo.active().unwrap_or_default() as i32),
        )
    }

    fn run(&self) -> gtk::ResponseType {
        self.dialog.run()
    }

    fn hide(&self) {
        self.dialog.hide();
    }

    fn destroy(&self) {
        unsafe { self.dialog.destroy() };
    }
}

//
// --- XfaeWindow ------------------------------------------------------------
//

/// Port of `xfae_window_new` and `xfae_window_init`: the scrolled tree view
/// plus the inline toolbar underneath it.
///
/// `XfaeWindow` is a `GtkBox` subclass holding its tree view and selection;
/// here the box is built directly and the widgets are handed to the callbacks
/// by the closures, which is all the struct was for.
fn xfae_window_new() -> gtk::Box {

    let vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .border_width(12)
        .build();

    let swin = gtk::ScrolledWindow::builder()
        .shadow_type(gtk::ShadowType::In)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .build();
    vbox.pack_start(&swin, true, true, 0);

    let model = xfae_model_new();

    let treeview = gtk::TreeView::builder()
        .model(&model)
        .headers_visible(true)
        .tooltip_column(col::TOOLTIP as i32)
        .build();
    swin.add(&treeview);

    treeview.connect_button_press_event(xfae_window_button_press_event);
    treeview.connect_realize(|treeview| treeview.columns_autosize());

    let selection = treeview.selection();
    selection.set_mode(gtk::SelectionMode::Single);

    // Column: the enabled toggle, untitled like in the C code.
    let column = gtk::TreeViewColumn::builder().reorderable(false).resizable(false).build();
    let renderer = gtk::CellRendererToggle::new();
    renderer.connect_toggled(glib::clone!(@weak model => move |_, path| {
        xfae_window_item_toggled(&model, &path);
    }));
    Column::pack_start(&column, &renderer, false);
    Column::add_attribute(&column, &renderer, "active", col::ENABLED as i32);
    column.set_sort_column_id(col::ENABLED as i32);
    treeview.append_column(&column);

    // Column: icon and name of the program.
    let column = gtk::TreeViewColumn::builder()
        .title("Program")
        .reorderable(false)
        .resizable(false)
        .expand(true)
        .build();
    let renderer = gtk::CellRendererPixbuf::new();
    Column::pack_start(&column, &renderer, false);
    Column::add_attribute(&column, &renderer, "gicon", col::ICON as i32);
    let renderer = gtk::CellRendererText::builder().ellipsize(gtk::pango::EllipsizeMode::End).build();
    Column::pack_start(&column, &renderer, true);
    Column::add_attribute(&column, &renderer, "markup", col::NAME as i32);
    column.set_sort_column_id(col::NAME as i32);
    treeview.append_column(&column);

    // Column: the trigger, editable through a combo inside the cell.
    let column = gtk::TreeViewColumn::builder()
        .title("Trigger")
        .reorderable(false)
        .resizable(false)
        .build();
    let renderer = gtk::CellRendererCombo::builder()
        .has_entry(false)
        .model(&xfae_window_create_run_hooks_combo_model())
        .text_column(0)
        .editable(true)
        .mode(gtk::CellRendererMode::Editable)
        .build();
    renderer.connect_changed(glib::clone!(@weak model => move |combo, path, combo_iter| {
        run_hook_changed(&model, combo, &path, combo_iter);
    }));
    Column::pack_start(&column, &renderer, false);
    Column::add_attribute(&column, &renderer, "text", col::RUN_HOOK as i32);
    column.set_sort_column_id(col::RUN_HOOK as i32);
    treeview.append_column(&column);

    // The inline toolbar.
    let bbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bbox.style_context().add_class("inline-toolbar");
    vbox.pack_start(&bbox, false, true, 0);

    let button = |label, icon, tooltip| {
        let button = gtk::Button::with_label(label);
        button.set_image(Some(&gtk::Image::from_icon_name(Some(icon), gtk::IconSize::Button)));
        button.set_tooltip_text(Some(tooltip));
        button
    };

    let add = button("Add", "list-add-symbolic", "Add application");
    add.connect_clicked(glib::clone!(@weak treeview => move |_| xfae_window_add(&treeview)));
    bbox.pack_start(&add, false, false, 0);

    let remove = button("Remove", "list-remove-symbolic", "Remove application");
    remove.connect_clicked(glib::clone!(@weak treeview => move |_| xfae_window_remove(&treeview)));
    bbox.pack_start(&remove, false, false, 0);

    let edit = button("Edit", "document-edit-symbolic", "Edit application");
    edit.connect_clicked(glib::clone!(@weak treeview => move |_| xfae_window_edit(&treeview)));
    bbox.pack_start(&edit, false, false, 0);

    // Both buttons follow the selection, as in `xfae_window_init`.
    selection.connect_changed(glib::clone!(@weak remove, @weak edit => move |selection| {
        xfae_window_selection_changed(selection, &remove, &edit);
    }));
    xfae_window_selection_changed(&selection, &remove, &edit);

    vbox
}

/// Port of `xfae_window_create_run_hooks_combo_model`.
fn xfae_window_create_run_hooks_combo_model() -> gtk::ListStore {

    let model = gtk::ListStore::new(&[String::static_type()]);

    for hook in RunHook::ALL {
        model.set(&model.append(), &[(0, &hook.nick())]);
    }

    model
}

/// Port of `run_hook_changed`.
///
/// The C code writes the new trigger to the `.desktop` file through
/// `xfae_model_set_run_hook`; here it only lands in the store.
fn run_hook_changed(
    model: &gtk::ListStore,
    combo: &gtk::CellRendererCombo,
    path: &gtk::TreePath,
    combo_iter: &gtk::TreeIter,
) {

    let Some(iter) = model.iter(path) else {
        return
    };

    let Some(combo_model) = combo.model() else {
        return
    };

    let nick = cell_string(&combo_model, combo_iter, 0);
    model.set_value(&iter, col::RUN_HOOK, &nick.to_value());
}

/// Port of `xfae_window_button_press_event`: the right click menu.
///
/// The C version spins a nested `GMainLoop` to keep the menu alive; attaching
/// it to the tree view does the same job here.
fn xfae_window_button_press_event(
    treeview: &gtk::TreeView,
    event: &gtk::gdk::EventButton,
) -> glib::Propagation {

    if event.button() != 3 || event.event_type() != gtk::gdk::EventType::ButtonPress {
        return glib::Propagation::Proceed
    }

    let (x, y) = event.position();
    let Some((Some(path), ..)) = treeview.path_at_pos(x as i32, y as i32) else {
        return glib::Propagation::Proceed
    };

    let selection = treeview.selection();
    selection.select_path(&path);

    let removable = selection
        .selected()
        .is_some_and(|(model, iter)| cell_bool(&model, &iter, col::REMOVABLE));

    let menu = gtk::Menu::new();

    let add = gtk::MenuItem::with_label("Add");
    add.connect_activate(glib::clone!(@weak treeview => move |_| xfae_window_add(&treeview)));
    menu.append(&add);

    let remove = gtk::MenuItem::with_label("Remove");
    remove.connect_activate(glib::clone!(@weak treeview => move |_| xfae_window_remove(&treeview)));
    remove.set_sensitive(removable);
    menu.append(&remove);

    // Attaching also puts the menu on the screen of the tree view, which is
    // what `gtk_menu_set_screen` does in the C code. `None` pops the menu up on
    // the event being handled, the one the C code passes explicitly.
    menu.set_attach_widget(Some(treeview));
    menu.show_all();
    menu.popup_at_pointer(None);

    glib::Propagation::Stop
}

/// Port of `xfae_window_add`.
///
/// The C version hands the result to `xfae_model_add`, which writes a new
/// `~/.config/autostart/<name>.desktop`; this example drops it.
fn xfae_window_add(treeview: &gtk::TreeView) {

    let parent = toplevel(treeview);
    let dialog = XfaeDialog::new(parent.as_ref(), None, None, None, RunHook::Login);

    if dialog.run() == gtk::ResponseType::Ok {
        dialog.hide();
        let (_name, _descr, _command, _run_hook) = dialog.get();
    }

    dialog.destroy();
}

/// Port of `xfae_window_remove`.
///
/// The C version unlinks every copy of the `.desktop` file in
/// `xfae_item_remove`; this one only drops the row.
fn xfae_window_remove(treeview: &gtk::TreeView) {

    let Some((model, iter)) = treeview.selection().selected() else {
        return
    };

    let Ok(model) = model.downcast::<gtk::ListStore>() else {
        return
    };

    model.remove(&iter);
}

/// Port of `xfae_window_edit`.
///
/// Reading the entry back goes through `XfceRc`, so a `.desktop` file that
/// disappeared under the view ends in `xfce_dialog_show_error`, exactly where
/// the C code reports it. What the dialog returns is dropped: this is where
/// `xfae_model_edit` would rewrite the file.
fn xfae_window_edit(treeview: &gtk::TreeView) {

    let parent = toplevel(treeview);

    let Some((model, iter)) = treeview.selection().selected() else {
        return
    };

    let relpath = cell_string(&model, &iter, col::RELPATH);

    let (name, descr, command, run_hook) = match xfae_model_get(&relpath) {
        Ok(entry) => entry,
        Err(error) => {
            xfce::show_error(parent.as_ref(), Some(&error), "Failed to edit item");
            return
        }
    };

    let dialog = XfaeDialog::new(
        parent.as_ref(),
        Some(&name),
        Some(&descr),
        Some(&command),
        run_hook,
    );

    if dialog.run() == gtk::ResponseType::Ok {
        dialog.hide();
        let (_name, _descr, _command, _run_hook) = dialog.get();
    }

    dialog.destroy();
}

/// Port of `xfae_window_item_toggled`.
///
/// The C version flips `Hidden` (or `X-XFCE-Autostart-Override`) in the file
/// through `xfae_model_toggle`; here the check box only moves on screen.
fn xfae_window_item_toggled(model: &gtk::ListStore, path: &gtk::TreePath) {

    let Some(iter) = model.iter(path) else {
        return
    };

    let enabled = cell_bool(model, &iter, col::ENABLED);
    model.set_value(&iter, col::ENABLED, &(!enabled).to_value());
}

/// Port of `xfae_window_selection_changed`: Remove asks for an entry every copy
/// of which sits in a writable directory, which in practice means one the user
/// owns under `~/.config/autostart`.
///
/// The C code connects this handler to the Edit button as well, so upstream
/// Edit is greyed out on every system wide entry too — the vast majority of the
/// list. Here Edit only asks for a selected row: the entry is opened read only
/// anyway, and its `Exec` is worth looking at even when the file cannot be
/// rewritten in place.
fn xfae_window_selection_changed(
    selection: &gtk::TreeSelection,
    remove: &gtk::Button,
    edit: &gtk::Button,
) {

    let selected = selection.selected();

    let removable = selected
        .as_ref()
        .is_some_and(|(model, iter)| cell_bool(model, iter, col::REMOVABLE));

    remove.set_sensitive(removable);
    edit.set_sensitive(selected.is_some());
}

fn cell_string(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter, column: u32) -> String {
    model.value(iter, column as i32).get::<String>().unwrap_or_default()
}

fn cell_bool(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter, column: u32) -> bool {
    model.value(iter, column as i32).get::<bool>().unwrap_or_default()
}

/// `gtk_widget_get_toplevel`, as far as it is a window: the parent the dialogs
/// and the error dialog are transient for.
fn toplevel(widget: &impl IsA<gtk::Widget>) -> Option<gtk::Window> {
    widget.toplevel().and_then(|it| it.downcast::<gtk::Window>().ok())
}

//
// --- main ------------------------------------------------------------------
//

fn main() -> glib::ExitCode {

    let app = gtk::Application::builder().application_id(APP_ID).build();

    app.connect_activate(build_ui);

    // The example takes no arguments of its own, and GTK would refuse the ones
    // cargo passes through.
    app.run_with_args::<&str>(&[])
}

fn build_ui(app: &gtk::Application) {

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Application Autostart")
        .default_width(600)
        .default_height(450)
        .build();

    window.add(&xfae_window_new());
    window.show_all();
}
