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

#![allow(dead_code)]

/// Safe wrappers around xfce4util/xfce4ui

use std::ffi::{CStr, CString, OsStr, c_char};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;

use gtk::glib;
use gtk::glib::ffi::{GError, GFALSE, GTRUE, g_free, g_strfreev, gboolean};
use gtk::glib::translate::{Stash, ToGlibPtr};
use gtk::prelude::*;

use crate::xfce::ffi::xfce4util;
use crate::xfce::ffi::xfce4ui;


/// The only group of an autostart `.desktop` file.
pub const DESKTOP_ENTRY: &CStr = c"Desktop Entry";

fn glib_bool(value: bool) -> gboolean {
    if value { GTRUE } else { GFALSE }
}


mod ffi {

    pub(in crate::xfce) mod xfce4util {
        use std::ffi::{c_char, c_int};
        use gtk::glib::ffi::gboolean;


        /// `XFCE_RESOURCE_CONFIG` of `XfceResourceType`: `$XDG_CONFIG_HOME` first,
        /// then the `$XDG_CONFIG_DIRS` fallbacks.
        pub const XFCE_RESOURCE_CONFIG: c_int = 1;

        #[repr(C)]
        pub struct XfceRc {
            _opaque: [u8; 0],
        }

        #[link(name = "xfce4util")]
        unsafe extern "C" {

            pub(in crate::xfce) fn xfce_resource_match(
                type_: c_int,
                pattern: *const c_char,
                unique: gboolean,
            ) -> *mut *mut c_char;

            pub(in crate::xfce) fn xfce_resource_lookup_all(type_: c_int, filename: *const c_char) -> *mut *mut c_char;

            /// The `gchar *` is the caller's, hence `g_free`.
            pub(in crate::xfce) fn xfce_resource_save_location(
                type_: c_int,
                rel_path: *const c_char,
                create: gboolean,
            ) -> *mut c_char;

            pub(in crate::xfce) fn xfce_rc_config_open(
                type_: c_int,
                resource: *const c_char,
                readonly: gboolean,
            ) -> *mut XfceRc;

            pub(in crate::xfce) fn xfce_rc_simple_open(filename: *const c_char, readonly: gboolean) -> *mut XfceRc;

            pub(in crate::xfce) fn xfce_rc_close(rc: *mut XfceRc);

            pub(in crate::xfce) fn xfce_rc_flush(rc: *mut XfceRc);

            pub(in crate::xfce) fn xfce_rc_rollback(rc: *mut XfceRc);

            pub(in crate::xfce) fn xfce_rc_is_dirty(rc: *const XfceRc) -> gboolean;

            pub(in crate::xfce) fn xfce_rc_is_readonly(rc: *const XfceRc) -> gboolean;

            pub(in crate::xfce) fn xfce_rc_set_group(rc: *mut XfceRc, group: *const c_char);

            pub(in crate::xfce) fn xfce_rc_read_entry(
                rc: *const XfceRc,
                key: *const c_char,
                fallback: *const c_char,
            ) -> *const c_char;

            pub(in crate::xfce) fn xfce_rc_read_int_entry(rc: *const XfceRc, key: *const c_char, fallback: c_int) -> c_int;

            pub(in crate::xfce) fn xfce_rc_read_bool_entry(
                rc: *const XfceRc,
                key: *const c_char,
                fallback: gboolean,
            ) -> gboolean;

            pub(in crate::xfce) fn xfce_rc_read_list_entry(
                rc: *const XfceRc,
                key: *const c_char,
                delimiter: *const c_char,
            ) -> *mut *mut c_char;

            pub(in crate::xfce) fn xfce_rc_write_entry(rc: *mut XfceRc, key: *const c_char, value: *const c_char);

            pub(in crate::xfce) fn xfce_rc_write_int_entry(rc: *mut XfceRc, key: *const c_char, value: c_int);

            pub(in crate::xfce) fn xfce_rc_write_bool_entry(rc: *mut XfceRc, key: *const c_char, value: gboolean);

            /// `value` is a `NULL` terminated `gchar **`, only read.
            pub(in crate::xfce) fn xfce_rc_write_list_entry(
                rc: *mut XfceRc,
                key: *const c_char,
                value: *mut *mut c_char,
                separator: *const c_char,
            );

            pub(in crate::xfce) fn xfce_rc_delete_entry(rc: *mut XfceRc, key: *const c_char, global: gboolean);

            pub(in crate::xfce) fn xfce_rc_delete_group(rc: *mut XfceRc, group: *const c_char, global: gboolean);
        }

    }


    pub(in crate::xfce) mod xfce4ui {
        use std::ffi::{c_char, c_int};
        use gtk::ffi::GtkWindow;
        use gtk::glib::ffi::GError;


        #[link(name = "xfce4ui-2")]
        unsafe extern "C" {

            /// Variadic `printf` style, hence the `...`: the wrapper always passes
            /// `"%s"` plus one argument so a message can never be read as a format.
            pub(in crate::xfce) fn xfce_dialog_show_error(
                parent: *mut GtkWindow,
                error: *const GError,
                primary_format: *const c_char,
                ...
            );
        }

        pub const R_OK: c_int = 0x04;
        pub const W_OK: c_int = 0x02;
        pub const X_OK: c_int = 0x01;

        unsafe extern "C" {
            pub(in crate::xfce) fn access(path: *const c_char, mode: c_int) -> c_int;
        }

    }
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

        g_strfreev(strv);
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
        from_strv(xfce4util::xfce_resource_match(
            xfce4util::XFCE_RESOURCE_CONFIG,
            pattern.as_ptr(),
            glib_bool(unique),
        ))
    }
}

/// `xfce_resource_lookup_all (XFCE_RESOURCE_CONFIG, rel_path)`: the absolute
/// path of `rel_path` in every config directory that holds a copy of it.
pub fn resource_lookup_all(rel_path: &str) -> Vec<String> {

    let Ok(rel_path) = CString::new(rel_path) else {
        return Vec::new()
    };

    unsafe {
        from_strv(xfce4util::xfce_resource_lookup_all(
            xfce4util::XFCE_RESOURCE_CONFIG,
            rel_path.as_ptr(),
        ))
    }
}

/// `xfce_resource_save_location (XFCE_RESOURCE_CONFIG, rel_path, create)`: the
/// path under `$XDG_CONFIG_HOME` a new file goes to, its directories made
/// when `create`.
pub fn resource_save_location(rel_path: &str, create: bool) -> Option<PathBuf> {

    let rel_path = CString::new(rel_path).ok()?;

    let path = unsafe {
        xfce4util::xfce_resource_save_location(
            xfce4util::XFCE_RESOURCE_CONFIG,
            rel_path.as_ptr(),
            glib_bool(create),
        )
    };

    if path.is_null() {
        return None
    }

    // Copied out, then handed back to `g_free`.
    let out = PathBuf::from(OsStr::from_bytes(unsafe { CStr::from_ptr(path) }.to_bytes()));
    unsafe { g_free(path.cast()) };

    Some(out)
}

/// `access (path, R_OK | W_OK | X_OK)`, the test `xfae_item_is_removable`
/// runs on the directory holding a `.desktop` file.
pub fn is_accessible_dir(path: &Path) -> bool {

    let Ok(path) = CString::new(path.as_os_str().as_encoded_bytes()) else {
        return false
    };

    unsafe { xfce4ui::access(path.as_ptr(), xfce4ui::R_OK | xfce4ui::W_OK | xfce4ui::X_OK) == 0 }
}

/// `xfce_dialog_show_error (parent, error, "%s", message)`.
pub fn show_error(parent: Option<&gtk::Window>, error: Option<&glib::Error>, message: &str) {

    let parent = parent.map_or(ptr::null_mut(), |it| it.as_ptr());

    // The stash owns the borrowed `GError *` for the length of the call.
    let error: Option<Stash<'_, *mut GError, glib::Error>> = error.map(ToGlibPtr::to_glib_none);
    let error = error.as_ref().map_or(ptr::null(), |it| it.0.cast_const());

    let message = CString::new(message).unwrap_or_default();

    unsafe { xfce4ui::xfce_dialog_show_error(parent, error, c"%s".as_ptr(), message.as_ptr()) };
}

/// An open `XfceRc`, closed on drop.
pub struct Rc(*mut xfce4util::XfceRc);

impl Rc {

    /// `xfce_rc_config_open (XFCE_RESOURCE_CONFIG, rel_path, readonly)`, the
    /// system and user copies of `rel_path` merged into one view.
    ///
    /// Opened for writing, the changes land in the user copy: a system entry
    /// is shadowed rather than edited.
    pub fn config_open(rel_path: &str, readonly: bool) -> Option<Self> {

        let rel_path = CString::new(rel_path).ok()?;

        let rc = unsafe {
            xfce4util::xfce_rc_config_open(
                xfce4util::XFCE_RESOURCE_CONFIG,
                rel_path.as_ptr(),
                glib_bool(readonly),
            )
        };

        (!rc.is_null()).then_some(Self(rc))
    }

    /// `xfce_rc_simple_open (path, readonly)`, one file and no merging: how a
    /// new `.desktop` file is written, on the [`resource_save_location`] path.
    pub fn simple_open(path: &Path, readonly: bool) -> Option<Self> {

        let path = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;

        let rc = unsafe { xfce4util::xfce_rc_simple_open(path.as_ptr(), glib_bool(readonly)) };

        (!rc.is_null()).then_some(Self(rc))
    }

    /// `xfce_rc_set_group (rc, group)`.
    pub fn set_group(&self, group: &CStr) {
        unsafe { xfce4util::xfce_rc_set_group(self.0, group.as_ptr()) };
    }

    /// `xfce_rc_is_readonly (rc)`, what every write below refuses on: the
    /// library would answer with a `CRITICAL`.
    pub fn is_readonly(&self) -> bool {
        unsafe { xfce4util::xfce_rc_is_readonly(self.0) != GFALSE }
    }

    /// `xfce_rc_is_dirty (rc)`: written, not on disk yet.
    pub fn is_dirty(&self) -> bool {
        unsafe { xfce4util::xfce_rc_is_dirty(self.0) != GFALSE }
    }

    /// `xfce_rc_read_entry (rc, key, NULL)`, copied out of the library.
    pub fn read_entry(&self, key: &CStr) -> Option<String> {

        let value = unsafe { xfce4util::xfce_rc_read_entry(self.0, key.as_ptr(), ptr::null()) };

        (!value.is_null())
            .then(|| unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned())
    }

    /// `xfce_rc_read_int_entry (rc, key, fallback)`.
    pub fn read_int_entry(&self, key: &CStr, fallback: i32) -> i32 {
        unsafe { xfce4util::xfce_rc_read_int_entry(self.0, key.as_ptr(), fallback) }
    }

    /// `xfce_rc_read_bool_entry (rc, key, fallback)`.
    pub fn read_bool_entry(&self, key: &CStr, fallback: bool) -> bool {
        unsafe { xfce4util::xfce_rc_read_bool_entry(self.0, key.as_ptr(), glib_bool(fallback)) != GFALSE }
    }

    /// `xfce_rc_read_list_entry (rc, key, ";")`, empty when the key is unset.
    pub fn read_list_entry(&self, key: &CStr) -> Vec<String> {
        unsafe { from_strv(xfce4util::xfce_rc_read_list_entry(self.0, key.as_ptr(), c";".as_ptr())) }
    }

    /// `xfce_rc_write_entry (rc, key, value)`, `false` on a read only view or
    /// a `NUL` C cannot be handed. In memory until [`flush`](Self::flush).
    pub fn write_entry(&self, key: &CStr, value: &str) -> bool {

        let Ok(value) = CString::new(value) else {
            return false
        };

        if self.is_readonly() {
            return false
        }

        unsafe { xfce4util::xfce_rc_write_entry(self.0, key.as_ptr(), value.as_ptr()) };

        true
    }

    /// `xfce_rc_write_int_entry (rc, key, value)`.
    pub fn write_int_entry(&self, key: &CStr, value: i32) -> bool {

        if self.is_readonly() {
            return false
        }

        unsafe { xfce4util::xfce_rc_write_int_entry(self.0, key.as_ptr(), value) };

        true
    }

    /// `xfce_rc_write_bool_entry (rc, key, value)`, the `Hidden` toggle.
    pub fn write_bool_entry(&self, key: &CStr, value: bool) -> bool {

        if self.is_readonly() {
            return false
        }

        unsafe { xfce4util::xfce_rc_write_bool_entry(self.0, key.as_ptr(), glib_bool(value)) };

        true
    }

    /// `xfce_rc_write_list_entry (rc, key, values, ";")`, the counterpart of
    /// [`read_list_entry`](Self::read_list_entry). Joined, so without the
    /// trailing `;` the spec makes optional.
    pub fn write_list_entry<S: AsRef<str>>(&self, key: &CStr, values: &[S]) -> bool {

        let Ok(values) = values.iter().map(|it| CString::new(it.as_ref())).collect::<Result<Vec<_>, _>>()
        else {
            return false
        };

        if self.is_readonly() {
            return false
        }

        // `NULL` terminated, borrowed from the `CString`s for the call only.
        let mut strv: Vec<*mut c_char> = values.iter().map(|it| it.as_ptr().cast_mut()).collect();
        strv.push(ptr::null_mut());

        unsafe {
            xfce4util::xfce_rc_write_list_entry(self.0, key.as_ptr(), strv.as_mut_ptr(), c";".as_ptr())
        };

        true
    }

    /// `xfce_rc_delete_entry (rc, key, global)`, `global` for every file
    /// behind the view.
    pub fn delete_entry(&self, key: &CStr, global: bool) -> bool {

        if self.is_readonly() {
            return false
        }

        unsafe { xfce4util::xfce_rc_delete_entry(self.0, key.as_ptr(), glib_bool(global)) };

        true
    }

    /// `xfce_rc_delete_group (rc, group, global)`.
    pub fn delete_group(&self, group: &CStr, global: bool) -> bool {

        if self.is_readonly() {
            return false
        }

        unsafe { xfce4util::xfce_rc_delete_group(self.0, group.as_ptr(), glib_bool(global)) };

        true
    }

    /// `xfce_rc_flush (rc)`: the pending writes onto disk, view kept open.
    pub fn flush(&self) {
        unsafe { xfce4util::xfce_rc_flush(self.0) };
    }

    /// `xfce_rc_rollback (rc)`: drops the pending writes, so [`Drop`] has
    /// nothing to flush.
    pub fn rollback(&self) {
        unsafe { xfce4util::xfce_rc_rollback(self.0) };
    }
}

impl Drop for Rc {

    /// `xfce_rc_close (rc)`, which flushes the pending writes first.
    fn drop(&mut self) {
        unsafe { xfce4util::xfce_rc_close(self.0) };
    }
}


#[cfg(test)]
mod tests {

    use std::fs;

    use super::*;
    use crate::test_support::{TempDir, xfce_resource_ready};

    #[test]
    fn glib_bool_maps_onto_the_c_constants() {
        assert_eq!(glib_bool(true), GTRUE);
        assert_eq!(glib_bool(false), GFALSE);
    }

    #[test]
    fn from_strv_reads_a_null_vector_as_empty() {
        assert!(unsafe { from_strv(ptr::null_mut()) }.is_empty());
    }

    /// Copies the strings out and hands the vector back to `g_strfreev`; run
    /// under a leak checker this is the one that would catch a missing free.
    #[test]
    fn from_strv_copies_every_string() {

        let source = vec!["first".to_string(), "second".to_string()];
        let strv: *mut *mut c_char = source.to_glib_full();

        assert_eq!(unsafe { from_strv(strv) }, source);
    }

    /// A `NUL` cannot be handed to C, and the C side never sees the call.
    #[test]
    fn an_interior_nul_is_refused_everywhere() {

        assert!(resource_match("autostart/*\0.desktop", true).is_empty());
        assert!(resource_lookup_all("autostart/nul\0byte.desktop").is_empty());
        assert!(Rc::config_open("autostart/nul\0byte.desktop", true).is_none());
        assert!(!is_accessible_dir(Path::new(OsStr::from_bytes(b"/tmp/nul\0byte"))));
    }

    /// `xfce_resource_match` matches nothing rather than failing on a pattern
    /// no config directory can satisfy.
    #[test]
    fn resource_match_finds_nothing_for_an_unknown_pattern() {
        xfce_resource_ready();
        assert!(resource_match("xfce4-niri-no-such-dir/*.desktop", true).is_empty());
    }

    #[test]
    fn resource_lookup_all_finds_nothing_for_an_unknown_file() {
        xfce_resource_ready();
        assert!(resource_lookup_all("autostart/xfce4-niri-no-such-entry.desktop").is_empty());
    }

    /// With no file in any config directory the open still hands back a view,
    /// an empty one: what makes `Item::new` give up is the missing `Type` key,
    /// not the open itself. Every read falls back.
    #[test]
    fn config_open_reads_nothing_when_the_file_is_nowhere() {

        xfce_resource_ready();

        let rc = Rc::config_open("autostart/xfce4-niri-no-such-entry.desktop", true)
            .expect("an empty view, not a failure");

        rc.set_group(DESKTOP_ENTRY);

        assert_eq!(rc.read_entry(c"Type"), None);
        assert_eq!(rc.read_int_entry(c"RunHook", 42), 42);
        assert!(rc.read_bool_entry(c"Hidden", true));
        assert!(!rc.read_bool_entry(c"Hidden", false));
        assert!(rc.read_list_entry(c"OnlyShowIn").is_empty());
    }

    /// The round trip the `.desktop` writer needs: every kind of entry out,
    /// then back in through a second view.
    #[test]
    fn simple_open_writes_every_entry_and_reads_it_back() {

        let dir = TempDir::new();
        let path = dir.path().join("written.desktop");

        {
            let rc = Rc::simple_open(&path, false).expect("a writable view");
            rc.set_group(DESKTOP_ENTRY);

            assert!(!rc.is_readonly());

            assert!(rc.write_entry(c"Type", "Application"));
            assert!(rc.write_entry(c"Name", "Niri"));
            assert!(rc.write_int_entry(c"RunHook", 1));
            assert!(rc.write_bool_entry(c"Hidden", true));
            assert!(rc.write_list_entry(c"OnlyShowIn", &["XFCE", "Niri"]));

            assert!(rc.is_dirty());
            rc.flush();
            assert!(!rc.is_dirty());
        }

        let rc = Rc::simple_open(&path, true).expect("the file just written");
        rc.set_group(DESKTOP_ENTRY);

        assert_eq!(rc.read_entry(c"Type").as_deref(), Some("Application"));
        assert_eq!(rc.read_entry(c"Name").as_deref(), Some("Niri"));
        assert_eq!(rc.read_int_entry(c"RunHook", 42), 1);
        assert!(rc.read_bool_entry(c"Hidden", false));
        assert_eq!(rc.read_list_entry(c"OnlyShowIn"), ["XFCE", "Niri"]);
    }

    /// What lands on disk is a plain `.desktop` group.
    #[test]
    fn what_is_written_is_a_desktop_file() {

        let dir = TempDir::new();
        let path = dir.path().join("shape.desktop");

        {
            let rc = Rc::simple_open(&path, false).expect("a writable view");
            rc.set_group(DESKTOP_ENTRY);

            rc.write_entry(c"Type", "Application");
            rc.write_bool_entry(c"Hidden", false);
            rc.write_list_entry(c"OnlyShowIn", &["XFCE"]);
            rc.write_list_entry(c"NotShowIn", &["GNOME", "KDE"]);
        }

        let written = fs::read_to_string(&path).expect("the file the view flushed on close");
        let lines: Vec<&str> = written.lines().collect();

        assert!(lines.contains(&"[Desktop Entry]"), "{written}");
        assert!(lines.contains(&"Type=Application"), "{written}");
        assert!(lines.contains(&"Hidden=false"), "{written}");

        // Joined, so no terminating `;`.
        assert!(lines.contains(&"OnlyShowIn=XFCE"), "{written}");
        assert!(lines.contains(&"NotShowIn=GNOME;KDE"), "{written}");
    }

    /// Deleting a key leaves the rest of the group alone.
    #[test]
    fn delete_entry_and_delete_group_drop_what_was_written() {

        let dir = TempDir::new();
        let path = dir.path().join("deleted.desktop");

        {
            let rc = Rc::simple_open(&path, false).expect("a writable view");
            rc.set_group(DESKTOP_ENTRY);

            rc.write_entry(c"Type", "Application");
            rc.write_entry(c"Name", "Niri");

            assert!(rc.delete_entry(c"Name", true));
        }

        {
            let rc = Rc::simple_open(&path, true).expect("the file just written");
            rc.set_group(DESKTOP_ENTRY);

            assert_eq!(rc.read_entry(c"Type").as_deref(), Some("Application"));
            assert_eq!(rc.read_entry(c"Name"), None);
        }

        {
            let rc = Rc::simple_open(&path, false).expect("a writable view");
            assert!(rc.delete_group(DESKTOP_ENTRY, true));
        }

        let rc = Rc::simple_open(&path, true).expect("the file just written");
        rc.set_group(DESKTOP_ENTRY);

        assert_eq!(rc.read_entry(c"Type"), None);
    }

    /// Nothing left to flush, so the file is never created.
    #[test]
    fn rollback_drops_the_pending_writes() {

        let dir = TempDir::new();
        let path = dir.path().join("rolled-back.desktop");

        let rc = Rc::simple_open(&path, false).expect("a writable view");
        rc.set_group(DESKTOP_ENTRY);

        rc.write_entry(c"Type", "Application");
        assert!(rc.is_dirty());

        rc.rollback();
        assert!(!rc.is_dirty());

        drop(rc);
        assert!(!path.exists());
    }

    /// A read only view refuses every write, and nothing reaches the file.
    #[test]
    fn a_read_only_view_refuses_every_write() {

        let dir = TempDir::new();
        let path = dir.path().join("read-only.desktop");

        {
            let rc = Rc::simple_open(&path, false).expect("a writable view");
            rc.set_group(DESKTOP_ENTRY);
            rc.write_entry(c"Type", "Application");
        }

        let rc = Rc::simple_open(&path, true).expect("the file just written");
        rc.set_group(DESKTOP_ENTRY);

        assert!(rc.is_readonly());

        assert!(!rc.write_entry(c"Name", "Niri"));
        assert!(!rc.write_int_entry(c"RunHook", 1));
        assert!(!rc.write_bool_entry(c"Hidden", true));
        assert!(!rc.write_list_entry(c"OnlyShowIn", &["XFCE"]));
        assert!(!rc.delete_entry(c"Type", true));
        assert!(!rc.delete_group(DESKTOP_ENTRY, true));

        assert!(!rc.is_dirty());
        assert_eq!(rc.read_entry(c"Type").as_deref(), Some("Application"));
    }

    /// The write side of `an_interior_nul_is_refused_everywhere`.
    #[test]
    fn an_interior_nul_is_refused_by_the_writes() {

        let dir = TempDir::new();
        let path = dir.path().join("nul.desktop");

        let rc = Rc::simple_open(&path, false).expect("a writable view");
        rc.set_group(DESKTOP_ENTRY);

        assert!(!rc.write_entry(c"Name", "nul\0byte"));
        assert!(!rc.write_list_entry(c"OnlyShowIn", &["XFCE", "nul\0byte"]));
        assert!(!rc.is_dirty());

        assert!(Rc::simple_open(Path::new(OsStr::from_bytes(b"/tmp/nul\0byte.desktop")), false).is_none());
    }

    /// Only the shape of the path is asserted: libxfce4util builds its
    /// directory list once per process, so an `$XDG_CONFIG_HOME` set from a
    /// test comes too late, and `create` would make the real directory.
    #[test]
    fn resource_save_location_is_an_absolute_path_under_the_config_home() {

        xfce_resource_ready();

        let rel_path = "autostart/xfce4-niri-save-location.desktop";

        let path = resource_save_location(rel_path, false).expect("a path to write to");

        assert!(path.is_absolute(), "{}", path.display());
        assert!(path.ends_with(rel_path), "{}", path.display());

        assert_eq!(resource_save_location("autostart/nul\0byte.desktop", false), None);
    }

    #[test]
    fn is_accessible_dir_wants_read_write_and_execute() {

        let dir = TempDir::new();

        assert!(is_accessible_dir(dir.path()));
        assert!(!is_accessible_dir(&dir.path().join("missing")));

        // Readable only: no write and no execute bit, which not even root gets
        // past for `X_OK`.
        assert!(!is_accessible_dir(&dir.file("read-only", 0o444)));
    }
}