/***************************************************************************
 *
 * xfce-niri
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

use std::ffi::{c_int, CStr, CString};

const APP_TAG: &CStr = c"xfce-niri-service";

pub(crate) enum Options {

    LogPid = 0x01,
    LogCons = 0x02,
    LogODelay = 0x04,
    LogNDelay = 0x08
}

enum Facility {
    LogUser = 1 << 3
}

pub(crate) enum Priority {
    LogEmerg = 0,
    LogAlert = 1,
    LogCrit = 2,
    LogErr = 3,
    LogWarning = 4,
    LogNotice = 5,
    LogInfo = 6,
    LogDebug = 7
}

pub(super)  mod ffi {
    use std::ffi::{c_char, c_int};

    unsafe extern "C" {    
        pub(super)fn openlog(ident: *const c_char, option: c_int, facility: c_int);
        pub(super)fn syslog(priority: c_int, fmt: *const c_char, ...);
        pub(super)fn closelog();
    }
}


pub(crate) fn open_log(option: c_int) {
    unsafe {
        ffi::openlog(APP_TAG.as_ptr(), option, Facility::LogUser as c_int);
    }
}

pub(crate) fn syslog(priority: c_int, msg: &str) {
    let Ok(msg) = CString::new(msg) else {
        return;
    };
    unsafe {
        ffi::syslog(priority, c"%s".as_ptr(), msg.as_ptr());
    }
}

pub(crate) fn close_log() {
    unsafe {
        ffi::closelog();
    }
}

