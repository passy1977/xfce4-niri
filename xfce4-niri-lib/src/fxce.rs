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

mod ffi {

    pub(in crate::fxce) mod xfce4util {
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

            pub(in crate::fxce) fn xfce_resource_match(
                type_: c_int,
                pattern: *const c_char,
                unique: gboolean,
            ) -> *mut *mut c_char;

            pub(in crate::fxce) fn xfce_resource_lookup_all(type_: c_int, filename: *const c_char) -> *mut *mut c_char;

            pub(in crate::fxce) fn xfce_rc_config_open(
                type_: c_int,
                resource: *const c_char,
                readonly: gboolean,
            ) -> *mut XfceRc;

            pub(in crate::fxce) fn xfce_rc_close(rc: *mut XfceRc);

            pub(in crate::fxce) fn xfce_rc_set_group(rc: *mut XfceRc, group: *const c_char);

            pub(in crate::fxce) fn xfce_rc_read_entry(
                rc: *const XfceRc,
                key: *const c_char,
                fallback: *const c_char,
            ) -> *const c_char;

            pub(in crate::fxce) fn xfce_rc_read_int_entry(rc: *const XfceRc, key: *const c_char, fallback: c_int) -> c_int;

            pub(in crate::fxce) fn xfce_rc_read_bool_entry(
                rc: *const XfceRc,
                key: *const c_char,
                fallback: gboolean,
            ) -> gboolean;

            pub(in crate::fxce) fn xfce_rc_read_list_entry(
                rc: *const XfceRc,
                key: *const c_char,
                delimiter: *const c_char,
            ) -> *mut *mut c_char;
        }

    }


    pub(in crate::fxce) mod xfce4ui {
        use std::ffi::{c_char, c_int};
        use gtk::ffi::GtkWindow;
        use gtk::glib::ffi::GError;


        #[link(name = "xfce4ui-2")]
        unsafe extern "C" {

            /// Variadic `printf` style, hence the `...`: the wrapper always passes
            /// `"%s"` plus one argument so a message can never be read as a format.
            pub(in crate::fxce) fn xfce_dialog_show_error(
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
            pub(in crate::fxce) fn access(path: *const c_char, mode: c_int) -> c_int;
        }

    }
}

/// Safe wrappers around xfce4util/xfce4ui
pub mod xfce {

    use std::ffi::{CStr, CString, c_char};
    use std::path::Path;
    use std::ptr;

    use gtk::glib;
    use gtk::glib::ffi::{GError, GFALSE, GTRUE, g_strfreev, gboolean};
    use gtk::glib::translate::{Stash, ToGlibPtr};
    use gtk::prelude::*;

    use super::ffi::xfce4util;
    use super::ffi::xfce4ui;
    
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

    /// `xfce_resource_lookup_all (XFCE_RESOURCE_CONFIG, relpath)`: the absolute
    /// path of `relpath` in every config directory that holds a copy of it.
    pub fn resource_lookup_all(relpath: &str) -> Vec<String> {

        let Ok(relpath) = CString::new(relpath) else {
            return Vec::new()
        };

        unsafe {
            from_strv(xfce4util::xfce_resource_lookup_all(
                xfce4util::XFCE_RESOURCE_CONFIG,
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

        /// `xfce_rc_config_open (XFCE_RESOURCE_CONFIG, relpath, readonly)`, the
        /// system and user copies of `relpath` merged into one view.
        ///
        /// Always opened read only here: writing back is the one thing this
        /// example does not do.
        pub fn config_open(relpath: &str, readonly: bool) -> Option<Self> {

            let relpath = CString::new(relpath).ok()?;

            let rc = unsafe {
                xfce4util::xfce_rc_config_open(
                    xfce4util::XFCE_RESOURCE_CONFIG,
                    relpath.as_ptr(),
                    glib_bool(readonly),
                )
            };

            (!rc.is_null()).then_some(Self(rc))
        }

        /// `xfce_rc_set_group (rc, group)`.
        pub fn set_group(&self, group: &CStr) {
            unsafe { xfce4util::xfce_rc_set_group(self.0, group.as_ptr()) };
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
    }

    impl Drop for Rc {

        /// `xfce_rc_close (rc)`.
        fn drop(&mut self) {
            unsafe { xfce4util::xfce_rc_close(self.0) };
        }
    }
}