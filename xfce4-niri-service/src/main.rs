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

#[cfg(not(feature = "disable_autostart"))]
mod autostart;
mod brightness;
mod data;
mod dbus;
#[cfg(not(feature = "disable_autostart"))]
mod desktop_entry;
mod lock_screen;
//mod syslog;

extern crate osal_rs;

use xfce4_niri_lib::lock::Lock;
use xfce4_niri_lib::socket::Socket;

use std::{error::Error, ffi::c_int};

use osal_rs::os::{System, SystemFn};
use osal_rs::utils::Result as OsalResult;
#[cfg(not(feature = "disable_autostart"))]
use crate::autostart::Autostart;
use crate::data::Data;
use crate::dbus::DBus;
use crate::brightness::Brightness;
use crate::lock_screen::LockScreen;
#[cfg(not(feature = "disable_niri_check"))]
use xfce4_niri_lib::niri::Niri;
use xfce4_niri_lib::syslog::{Options, Priority, SysLog};

const APP_TAG: &str = "Xfce4NiriService";

mod ffi {
    use std::ffi::c_int;
    pub(super) const SIGINT: c_int = 2;
    pub(super) const SIGTERM: c_int = 15;
    unsafe extern "C" { pub(super) fn signal(signum: c_int, handler: extern "C" fn(c_int)) -> usize; }
}

// Only an atomic store happens here, so it is safe to call from a signal handler.
extern "C" fn on_terminate(_signum: c_int) {
    System::stop();
}

fn handle_request(request: &[String]) -> OsalResult<()>{
    let mut count = 0;
    for str in request {
        println!("--->({count}): {:?}", str);
        count += 1;
    }
    
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {

    let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

    let mut pid = Some(String::new());
    let _lock = match Lock::acquire(None, &mut pid) {
        Ok(lock_file) => lock_file,
        Err(e) => {

            let pid = pid.unwrap();
            let msg = if pid == "" {
                e.to_string()
            } else {
                format!("Service already running by:{pid}")
            };

            log.syslog(APP_TAG, Priority::LogInfo, &msg);
            return Err(msg.into());
        }
    };

    #[cfg(not(feature = "disable_niri_check"))]
    {
        let version = match Niri::version() {
            Some(version) => version,
            None => {
                let msg = "Niri not ruining";
                log.syslog(APP_TAG, Priority::LogCrit, &msg);
                return Err(msg.into())
            }
        };
        
        log.syslog(APP_TAG, Priority::LogDebug, &format!("Niri {} running", version));
    }
    log.syslog(APP_TAG, Priority::LogDebug, &format!("Version {}", env!("CARGO_PKG_VERSION")));

    if let Err(e) = Data::share().check_persistence() {
        let msg = e.to_string();
        log.syslog(APP_TAG, Priority::LogCrit, &msg);
        return Err(msg.into())
    }

    let mut dbus = match DBus::new() {
        Ok(dbus) => dbus,
        Err(e) => {
            let msg = e.to_string();
            log.syslog(APP_TAG, Priority::LogCrit, &msg);
            return Err(msg.into())
        },
    };

    if let Err(e) = Brightness::new().start() {
        let msg = e.to_string();
        log.syslog(APP_TAG, Priority::LogCrit, &msg);
        return Err(msg.into())
    }

    if let Err(e) = LockScreen::new().start(&dbus) {
        let msg = e.to_string();
        log.syslog(APP_TAG, Priority::LogCrit, &msg);
        return Err(msg.into())
    }
    
    #[cfg(not(feature = "disable_autostart"))]
    if let Err(e) = Autostart::new().start() {
        let msg = e.to_string();
        log.syslog(APP_TAG, Priority::LogCrit, &msg);
        return Err(msg.into())
    }
    
    if let Err(e) = dbus.start() {
        let msg = e.to_string();
        log.syslog(APP_TAG, Priority::LogCrit, &msg);
        return Err(msg.into())
    }

    let Ok(unix_socket) = xfce4_niri_lib::get_safe_path(Some("xfce4-niri-service.sock")) else {
        let msg = "Failed to get safe path for unix socket";
        log.syslog(APP_TAG, Priority::LogCrit, &msg);
        return Err(msg.into());
    };

    let mut socket = Socket::new(unix_socket);
    if let Err(e) = socket.start_server(&handle_request) {
        let msg = e.to_string();
        log.syslog(APP_TAG, Priority::LogCrit, &msg);
        return Err(msg.into())
    }

    unsafe {
        ffi::signal(ffi::SIGINT, on_terminate);
        ffi::signal(ffi::SIGTERM, on_terminate);
    }

    System::start();

    socket.stop();

    Ok(())
}
