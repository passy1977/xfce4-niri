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

use std::{error::Error, ffi::c_int};

use osal_rs::os::{System, SystemFn};

use crate::{brightness::Brightness, data::Data, dbus::DBus, lock_screen::LockScreen, os::syslog::{Options, Priority, SysLog}};


mod brightness;
mod data;
mod dbus;
mod lock_screen;
mod os;

extern crate osal_rs;

fn main() -> Result<(), Box<dyn Error>> {

    let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

    if let Err(e) = Data::share().check_persistence() {
        let msg = e.to_string();
        log.syslog(Priority::LogCrit, &msg);
        return Err(msg.into());
    }

    let dbus = match DBus::new() {
        Ok(dbus) => dbus,
        Err(e) => {
            let msg = e.to_string();
            log.syslog(Priority::LogCrit, &msg);
            return Err(msg.into());
        },
    };

    if let Err(e) = Brightness::new().start() {
        let msg = e.to_string();
        log.syslog(Priority::LogCrit, &msg);
        return Err(msg.into());
    }

    if let Err(e) = LockScreen::new().start(&dbus) {
        let msg = e.to_string();
        log.syslog(Priority::LogCrit, &msg);
        return Err(msg.into());
    }

    System::start();

    Ok(())
}
