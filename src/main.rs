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
use osal_rs::utils::Result;
use xfce4_niri_lib::lock::Lock;
use xfce4_niri_lib::socket::Socket;
use xfce4_niri_lib::syslog::{Options, Priority, SysLog};

const APP_TAG: &str = "Xfce4Niri";

fn main() -> Result<(), Box<dyn Error>> {
    let log = SysLog::open(Options::LogPid as i32 | Options::LogNDelay as i32); 

    let lock_file = match xfce4_niri_lib::get_safe_path(None) {
        Ok(path) => path,
        Err(e) => {
            let msg = e.to_string();
            log.syslog(APP_TAG, Priority::LogCrit, &msg);
            return Err(msg.into());
        }
    };

    let lock_file = Lock::from_path(&lock_file);
    if !lock_file.exists().unwrap_or(false) {
        let msg = format!("fxce4-niri-service not running");
        log.syslog(APP_TAG, Priority::LogCrit, &msg);
        return Err(msg.into());
    }

    let Ok(unix_socket) = xfce4_niri_lib::get_safe_path(Some("xfce4-niri-service.sock")) else {
        let msg = "Failed to get safe path for unix socket";
        log.syslog(APP_TAG, Priority::LogCrit, &msg);
        return Err(msg.into());
    };

    if let Err(e) = Socket::new(unix_socket).run_client(&lock_file, &vec![]) {
        let msg = e.to_string();
        log.syslog(APP_TAG, Priority::LogCrit, &msg);
        return Err(msg.into())
    }

    Ok(())
}
