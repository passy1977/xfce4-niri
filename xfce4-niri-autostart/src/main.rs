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

extern crate osal_rs;
extern crate xfce4_niri_lib;

mod data;
mod gui;

use std::error::Error;
use std::ffi::c_int;

use xfce4_niri_lib::niri_check::NiriCheck;
use xfce4_niri_lib::syslog::{Options, Priority, SysLog};

use crate::data::Data;

fn main() -> Result<(), Box<dyn Error>> {

    let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);


    let version = match NiriCheck::version() {
        Some(version) => version,
        None => {
            let msg = "Niri not ruining";
            log.syslog(Priority::LogCrit, &msg);
            return Err(msg.into())
        }
    };
    log.syslog(Priority::LogDebug, &format!("Version {}", env!("CARGO_PKG_VERSION")));
    log.syslog(Priority::LogDebug, &format!("Niri {} running", version));

    if let Err(e) = Data::share().check_persistence() {
        let msg = e.to_string();
        log.syslog(Priority::LogCrit, &msg);
        return Err(msg.into());
    }

    Ok(())
}
