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

use std::ffi::c_int;
use std::sync::Arc;

use osal_rs::access_static_option;
use osal_rs::os::types::{EventBits, TickType};
use osal_rs::os::{EventGroup, EventGroupFn, Mutex, MutexFn, Thread, ThreadFn};
use osal_rs::utils::Result;

use crate::dbus::DBus;
use crate::os::syslog::{Options, SysLog};


static mut EVENT_GROUP: Option<EventGroup> = None;

pub(crate) struct LockScreen {
    thread: Thread,
    presentation_mode: Arc<Mutex<bool>>,
 }

impl LockScreen {

    const CHANNEL: &str = "xfce4-power-manager";
    const PROPERTY: &str = "/xfce4-power-manager/presentation-mode";

    pub(crate) fn new() -> Self {

        unsafe {
            if (*&raw const EVENT_GROUP).is_none() {
                if let Ok(event_group) = EventGroup::new() {
                    EVENT_GROUP = Some(event_group);
                }
            }
        }

        Self {
            thread: Thread::new("brightness_thd", 0, 0),
            presentation_mode: Mutex::new_arc(false),
        }
    }

    pub(super) fn start(&mut self, dbus: &DBus) -> Result<()> {



        let _log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

        let self_presentation_mode = self.presentation_mode.clone();

        dbus.register_signal(Self::CHANNEL, Self::PROPERTY, Mutex::new_arc(
            move |presentation_mode| {

                let Ok(mut self_presentation_mode_ref) = self_presentation_mode.lock() else {
                    return
                };

                let value = if presentation_mode == 1 { true } else { false };
                if value != *self_presentation_mode_ref {
                    *self_presentation_mode_ref = value;
                    access_static_option!(EVENT_GROUP).set((1 << value as u8) as EventBits);
                }

            })
        )?;

        self.thread.spawn(None, |_, _| {

            loop {
                let mask = access_static_option!(EVENT_GROUP).wait(0x3, false, TickType::MAX);
                if mask > 0 {
                    access_static_option!(EVENT_GROUP).clear(mask);
                    match mask {
                        0b0000001 => {
                            println!("Spento")
                        }
                        0b0000010 => {
                            println!("Acceso")
                        }
                        _ => ()
                    }
                }
            }
            
        })?;

        Ok(())
    }

 }