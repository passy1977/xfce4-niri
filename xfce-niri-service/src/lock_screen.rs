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

use std::ffi::c_int;
use std::sync::Arc;

use osal_rs::os::{Mutex, MutexFn};
use osal_rs::utils::Result;

use crate::dbus::DBus;
use crate::os::syslog::{Options, SysLog};


pub(crate) struct LockScreen {
    presentation_mode: Arc<Mutex<bool>>,
 }

impl LockScreen {

    const CHANNEL: &str = "xfce4-power-manager";
    const PROPERTY: &str = "/presentation-mode";

    pub(crate) fn new() -> Self {
        Self {
            presentation_mode: Mutex::new_arc(false),
        }
    }

    pub(super) fn start(&mut self, dbus: &DBus) -> Result<()> {



        let _log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

        let self_presentation_mode = self.presentation_mode.clone();
        
        dbus.register_signal(Self::CHANNEL, Self::PROPERTY, Mutex::new_arc(
            move |presentation_mode: bool| {

                let Ok(self_presentation_mode_ref) = self_presentation_mode.lock() else {
                    return
                };


                println!("presentation_mode:{presentation_mode} --> {}", *self_presentation_mode_ref);
            })

        )?;

        Ok(())
    }

 }