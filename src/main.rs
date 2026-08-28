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

extern crate xfce4_niri_lib;

use std::error::Error;
use xfce4_niri_lib::lock::Lock;
use xfce4_niri_lib::syslog::{Options, Priority, SysLog};



fn main() -> Result<(), Box<dyn Error>> {
    let log = SysLog::open(Options::LogPid as i32 | Options::LogNDelay as i32); 

    let _ = match Lock::acquire(None) {
        Ok(lock) => lock,
        Err(e) => {
            let msg = e.to_string();
            log.syslog(Priority::LogInfo, &msg);
            return Err(msg.into());
        }
    };

    Ok(())
}
