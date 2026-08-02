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

use std::time::Duration;

use dbus::blocking::Connection;
use osal_rs::utils::{Error, Result};

 pub(crate) struct DBus(Connection);

 impl DBus {

    const TIMEOUT: Duration = Duration::from_millis(5000);
    // pub(crate) const DEST: &str = "org.xfce.Xfconf";
    // pub(crate) const PATH: &str = "/org/xfce/Xfconf";
    const DEST: &str = "org.xfce.Xfconf";
    const PATH: &str = "/org/xfce/Xfconf";


    pub(crate) fn connection() -> Result<Self> {


        let conn = Connection::new_session().map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        let xfconf = conn.with_proxy(Self::DEST, Self::DEST, Self::TIMEOUT);



        Ok(Self(conn))
    }

 }