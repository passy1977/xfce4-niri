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

use std::ffi::{c_int, CStr, CString};

#[allow(unused)]
pub enum Options {

    LogPid = 0x01,
    LogCons = 0x02,
    LogODelay = 0x04,
    LogNDelay = 0x08
}

enum Facility {
    LogUser = 1 << 3
}

#[allow(unused)]
pub enum Priority {
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

pub struct SysLog;

impl Drop for SysLog {
    fn drop(&mut self) {
        self.close();
    }
}

impl SysLog {

    const APP_TAG: &CStr = c"xfce4-niri-service";

    pub fn open(option: c_int) -> Self {
        unsafe {
            ffi::openlog(Self::APP_TAG.as_ptr(), option, Facility::LogUser as c_int);
        }
        Self
    }

    pub fn syslog(&self, priority: Priority, msg: &str) {
        let Ok(msg) = CString::new(msg) else {
            return
        };
        unsafe {
            ffi::syslog(priority as c_int, c"%s".as_ptr(), msg.as_ptr());
        }
    }

    #[allow(dead_code)]
    pub fn syslog_with_tag(&self, tag: &str, priority: Priority, msg: &str) {
        let Ok(tag) = CString::new(tag) else {
            return
        }; 

        let Ok(msg) = CString::new(msg) else {
            return
        };

        unsafe {
            ffi::syslog(priority as c_int, c"%s - %s".as_ptr(), tag.as_ptr(), msg.as_ptr());
        }
    }


    pub fn close(&mut self) {
        unsafe {
            ffi::closelog();
        }
    }
}


#[cfg(test)]
mod tests {

    use super::*;

    /// The discriminants are handed to `openlog` and `syslog` as they are, so
    /// they have to be the `<syslog.h>` constants.
    #[test]
    fn the_constants_are_the_posix_ones() {

        assert_eq!(Options::LogPid as c_int, 0x01);
        assert_eq!(Options::LogCons as c_int, 0x02);
        assert_eq!(Options::LogODelay as c_int, 0x04);
        assert_eq!(Options::LogNDelay as c_int, 0x08);

        assert_eq!(Facility::LogUser as c_int, 8);

        assert_eq!(Priority::LogEmerg as c_int, 0);
        assert_eq!(Priority::LogAlert as c_int, 1);
        assert_eq!(Priority::LogCrit as c_int, 2);
        assert_eq!(Priority::LogErr as c_int, 3);
        assert_eq!(Priority::LogWarning as c_int, 4);
        assert_eq!(Priority::LogNotice as c_int, 5);
        assert_eq!(Priority::LogInfo as c_int, 6);
        assert_eq!(Priority::LogDebug as c_int, 7);
    }

    /// Opening, logging and the `closelog` on drop have to hold together; a
    /// message is written to the system log, which is what the service does.
    #[test]
    fn logging_goes_through() {

        let log = SysLog::open(Options::LogPid as c_int | Options::LogCons as c_int);

        log.syslog(Priority::LogDebug, "xfce4-niri-lib test message");
        log.syslog_with_tag("test", Priority::LogDebug, "xfce4-niri-lib tagged test message");
    }

    /// A `NUL` cannot be handed to C: the message is dropped instead.
    #[test]
    fn a_message_with_an_interior_nul_is_dropped() {

        let log = SysLog::open(Options::LogPid as c_int);

        log.syslog(Priority::LogDebug, "nul\0byte");
        log.syslog_with_tag("nul\0byte", Priority::LogDebug, "message");
        log.syslog_with_tag("tag", Priority::LogDebug, "nul\0byte");
    }
}