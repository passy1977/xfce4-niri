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
use osal_rs::os::{EventGroup, EventGroupFn, Mutex, MutexFn, Thread, ThreadFn, ThreadParam};
use osal_rs::utils::{Error, Result};

use crate::dbus::DBus;
use crate::os::syslog::{Options, SysLog};


static mut EVENT_GROUP: Option<EventGroup> = None;

#[derive(Clone, Debug)]
struct LockScreenData {
    presentation_mode: u64,
    dpms_enabled: u64,
    dpms_on_ac_off: u64,
    dpms_on_battery_off: u64,
}

impl Default for LockScreenData {
    fn default() -> Self {
        Self { presentation_mode: Default::default(), dpms_enabled: Default::default(), dpms_on_ac_off: Default::default(), dpms_on_battery_off: Default::default() }
    }
}

pub(crate) struct LockScreen {
    thread: Thread,
    data: Arc<Mutex<LockScreenData>>,
}

macro_rules! update_power_manager_data {
    ($self_data:expr, $field:ident) => {{
        let Ok(self_data_ref) = $self_data.lock() else {
            return
        };

        let mut guard = $self_data.lock();
        let guard = guard.as_mut().unwrap();
        guard.$field = $field;

        access_static_option!(EVENT_GROUP).set((1 << (*self_data_ref).presentation_mode as u8) as EventBits);
    }};
}

impl LockScreen {

    const CHANNEL: &str = "xfce4-power-manager";
    const PROPERTY_PRESENTATION_MODE: &str = "/xfce4-power-manager/presentation-mode";
    const PROPERTY_DPMS_ENABLED: &str = "/xfce4-power-manager/dpms-enabled";
    const PROPERTY_DPMS_ON_AC_OFF: &str = "/xfce4-power-manager/dpms-on-ac-off";
    const PROPERTY_DPMS_ON_BATTERY_OFF: &str = "/xfce4-power-manager/dpms-on-battery-off";

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
            data: Mutex::new_arc(Default::default()),
        }
    }

    pub(super) fn start(&mut self, dbus: &DBus) -> Result<()> {



        let _log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

        let self_data = self.data.clone();
        dbus.register_presentation_mode_signal(Self::CHANNEL, Self::PROPERTY_PRESENTATION_MODE, Mutex::new_arc(
            move |presentation_mode| {


                update_power_manager_data!(self_data, presentation_mode);

            })
        )?;

        let self_data = self.data.clone();
        dbus.register_dpms_enabled_signal(Self::CHANNEL, Self::PROPERTY_DPMS_ENABLED, Mutex::new_arc(
            move |dpms_enabled| {
                
                update_power_manager_data!(self_data, dpms_enabled);
                
        }))?;

        let self_data = self.data.clone();
        dbus.register_dpms_on_ac_sleep_signal(Self::CHANNEL, Self::PROPERTY_DPMS_ON_AC_OFF, Mutex::new_arc(
            move |dpms_on_ac_off| {
                
                update_power_manager_data!(self_data, dpms_on_ac_off);

        }))?;

        let self_data = self.data.clone();
        dbus.register_dpms_on_battery_off_signal(Self::CHANNEL, Self::PROPERTY_DPMS_ON_BATTERY_OFF, Mutex::new_arc(
            move |dpms_on_battery_off| {

                update_power_manager_data!(self_data, dpms_on_battery_off);

        }))?;


        let self_data: Option<ThreadParam> = Some(self.data.clone());

        self.thread.spawn(self_data, |_, param| {


            let param = param
                        .and_then(|p| p.downcast::<Mutex<LockScreenData>>().ok())
                        .ok_or(Error::Unhandled("Missing current_brightness parameter"))?;

            loop {
                let mask = access_static_option!(EVENT_GROUP).wait(0x3, false, TickType::MAX);
                if mask > 0 {
                    access_static_option!(EVENT_GROUP).clear(mask);
                    let data = param.lock().unwrap();
                    match mask {
                        0b0000001 => {
                            println!("Switch on PowerManagerData:{:?}", *data)
                        }
                        0b0000010 => {
                            println!("Switch off PowerManagerData:{:?}", *data)
                        }
                        _ => ()
                    }
                }
            }
            
        })?;

        Ok(())
    }

 }