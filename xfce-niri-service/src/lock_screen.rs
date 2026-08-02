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
use std::time::Duration;

use dbus::blocking::Connection;
use osal_rs::os::{Mutex, MutexFn, System, Thread, ThreadFn, ThreadParam};
use osal_rs::utils::{Error, Result};

use crate::dbus::DBus;
use crate::os::syslog::{Options, SysLog};

 pub(crate) struct LockScreen {
    thread: Thread,
    presentation_mode: Arc<Mutex<bool>>,
 }

 impl LockScreen {

    pub(crate) fn new() -> Self {
        Self {
            thread: Thread::new("lock_screen_thd", 0, 0),
            presentation_mode: Mutex::new_arc(false),
        }
    }

    pub(super) fn start(&mut self) -> Result<()> {

        let param: Option<ThreadParam> = Option::Some(self.presentation_mode.clone());

        self.thread.spawn(param, |_, param| {
            let _log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

            let presentation_mode = param
                .and_then(|p| p.downcast::<Mutex<i32>>().ok())
                .ok_or(Error::Unhandled("Missing presentation_mode parameter"))?;

            let mut binding = presentation_mode.lock();
            let Ok(_presentation_mode_ref) = binding.as_deref_mut() else {
                return Err(Error::Unhandled("Missing presentation_mode parameter"))
            };


            loop {


                System::delay_with_to_tick(Duration::from_secs(1));
            }

        })?;


        Ok(())
    }

 }