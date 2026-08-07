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
use std::fs::read_to_string;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use dbus::channel;
use osal_rs::access_static_option;
use osal_rs::os::types::{EventBits, TickType};
use osal_rs::os::{EventGroup, EventGroupFn, Mutex, MutexFn, Thread, ThreadFn, ThreadParam};
use osal_rs::utils::{Error, Result};

use crate::data::Data;
use crate::dbus::DBus;
use crate::os::syslog::{Options, SysLog};


static mut EVENT_GROUP: Option<EventGroup> = None;

#[derive(Clone, Debug)]
struct LockScreenData {
    presentation_mode: u64,
    dpms_on_ac_sleep: u64,
    dpms_enabled: bool,
    dpms_on_ac_off: u64,
    dpms_on_battery_off: u64,
    is_desktop: bool,
    battery_or_ac: bool,
    has_battery: bool,
    state: u32
}

impl Default for LockScreenData {
    fn default() -> Self {
        Self { 
            presentation_mode: Default::default(), 
            dpms_on_ac_sleep: Default::default(),
            dpms_enabled: Default::default(), 
            dpms_on_ac_off: Default::default(), 
            dpms_on_battery_off: Default::default(), 
            is_desktop: Default::default(),
            battery_or_ac: Default::default(),
            has_battery:  Default::default(),
            state:  Default::default()
        }
    }
}

pub(crate) struct LockScreen {
    thread: Thread,
    data: Arc<Mutex<LockScreenData>>,
    child: Arc<Mutex<Option<Child>>>
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

    const XFCE4_PM_CHANNEL: &str = "xfce4-power-manager";
    const XFCE4_PM_PROPERTY_PRESENTATION_MODE: &str = "/xfce4-power-manager/presentation-mode";
    const XFCE4_PM_PROPERTY_DPMS_ENABLED: &str = "/xfce4-power-manager/dpms-enabled";
    const XFCE4_PM_PROPERTY_DPMS_ON_AC_SLEEP: &str = "/xfce4-power-manager/dpms-on-ac-sleep";
    const XFCE4_PM_PROPERTY_DPMS_ON_AC_OFF: &str = "/xfce4-power-manager/dpms-on-ac-off";
    const XFCE4_PM_PROPERTY_DPMS_ON_BATTERY_OFF: &str = "/xfce4-power-manager/dpms-on-battery-off";

    const UPOWER_DEST: &str = "org.freedesktop.UPower";
    const UPOWER_PATH: &str = "/org/freedesktop/UPower";
    const UPOWER_IFACE: &str = "org.freedesktop.UPower";
    const UPOWER_DEVICE_IFACE: &str = "org.freedesktop.UPower.Device";

    const LOCK_SCREEN_AFTER_IN_MINUTES: u64 = 10;

    pub(crate) fn new() -> Self {

        unsafe {
            if (*&raw const EVENT_GROUP).is_none() {
                if let Ok(event_group) = EventGroup::new() {
                    EVENT_GROUP = Some(event_group);
                }
            }
        }

        let int = read_to_string("/sys/class/dmi/id/chassis_type")
                        .unwrap_or("0".to_string())
                        .trim()
                        .parse::<u32>()
                        .unwrap_or(8);
        
        let is_desktop = match int {
            3 | 4 | 5 | 6 | 7 | 13 | 23 | 24 => true,   // desktop, tower, all-in-one, rack
            8 | 9 | 10 | 11 | 14 | 30 | 31 | 32 => false, // portable, laptop, notebook, tablet
            _ => false,                                   // 1=other, 2=unknown, VM
        };

        Self {
            thread: Thread::new("brightness_thd", 0, 0),
            data: Mutex::new_arc(LockScreenData {
                is_desktop,
                ..Default::default()
            }),
            child: Mutex::new_arc(None)
        }
    }

    pub(super) fn start(&mut self, dbus: &DBus) -> Result<()> {



        let _log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

        let self_data = self.data.clone();
        dbus.register_signal_with_initial::<u64>(Self::XFCE4_PM_CHANNEL, Self::XFCE4_PM_PROPERTY_PRESENTATION_MODE, Mutex::new_arc(
            move |presentation_mode| {


                update_power_manager_data!(self_data, presentation_mode);

            })
        )?;

        let self_data = self.data.clone();
        dbus.register_signal_with_initial::<bool>(Self::XFCE4_PM_CHANNEL, Self::XFCE4_PM_PROPERTY_DPMS_ENABLED, Mutex::new_arc(
            move |dpms_enabled| {
                
                update_power_manager_data!(self_data, dpms_enabled);
                
        }))?;


        let Ok(data) = self.data.lock() else {
            return Err(Error::Unhandled("Failed to lock data mutex"));
        };

        if data.is_desktop {

            let self_data = self.data.clone();
            dbus.register_signal_with_initial::<u64>(Self::XFCE4_PM_CHANNEL, Self::XFCE4_PM_PROPERTY_DPMS_ON_AC_SLEEP, Mutex::new_arc(
                move |dpms_on_ac_sleep| {

                    update_power_manager_data!(self_data, dpms_on_ac_sleep);
                    
            }))?;

        } else {

            let self_data = self.data.clone();
            dbus.register_signal_with_initial::<u64>(Self::XFCE4_PM_CHANNEL, Self::XFCE4_PM_PROPERTY_DPMS_ON_AC_OFF, Mutex::new_arc(
                move |dpms_on_ac_off| {
                    
                    update_power_manager_data!(self_data, dpms_on_ac_off);

            }))?;

            let self_data = self.data.clone();
            dbus.register_signal_with_initial::<u64>(Self::XFCE4_PM_CHANNEL, Self::XFCE4_PM_PROPERTY_DPMS_ON_BATTERY_OFF, Mutex::new_arc(
                move |dpms_on_battery_off| {

                    update_power_manager_data!(self_data, dpms_on_battery_off);
                    
            }))?;

            let self_data = self.data.clone();
            dbus.register_upower_signals(
                Self::UPOWER_DEST,
                Self::UPOWER_PATH,
                Self::UPOWER_IFACE,
                Self::UPOWER_DEVICE_IFACE,
                Mutex::new_arc(
                move |battery_or_ac, has_battery, state| {

                    let mut data = self_data.lock().unwrap();
                    data.battery_or_ac = battery_or_ac;
                    data.has_battery = has_battery;
                    data.state = state;

                    access_static_option!(EVENT_GROUP).set((1 << data.presentation_mode as u8) as EventBits);
            }))?;

        }

        let self_data: Option<ThreadParam> = Some( Arc::new((self.data.clone(), self.child.clone()) ));

        self.thread.spawn(self_data, |_, self_data| {


            let arc_tuple = self_data
                        .and_then(|p| p.downcast::<(Arc<Mutex<LockScreenData>>, Arc<Mutex<Option<Child>>>)>().ok())
                        .ok_or_else(|| Error::Unhandled("Missing or not valid data parameter"))?;

            loop {
                let mask = access_static_option!(EVENT_GROUP).wait(0x3, false, TickType::MAX);
                if mask > 0 {
                    access_static_option!(EVENT_GROUP).clear(mask);
                    let data = arc_tuple.0.lock().unwrap();
                    
                    if data.presentation_mode > 0 {
                        //let mut child = arc_tuple.1.lock();    
                    } else {
                        let mut child = arc_tuple.1.lock().unwrap();

                        if let Some(mut child) = child.take() {
                            child.kill().expect("Command couldn't be killed");
                        }


                    }
                    

                        // let child = Command::new(&Data::share().lock_screen_file)
                        //     .stdout(Stdio::piped())
                        //     .stderr(Stdio::piped())
                        //     .arg(format!("{}", data.dpms_enabled))
                        //     .spawn()
                        //     .expect("Failed to execute command");

                        // //println!("{:?}", String::from_utf8_lossy(&output.stdout));
                        
                        // let pid = child.id();          // u32 — disponibile subito, processo ancora vivo
                        // println!("lock screen pid: {pid}");

                        // let output = child.wait_with_output().expect("Failed to wait command");
                        // println!("{:?}", String::from_utf8_lossy(&output.stdout));

                     


                    match mask {
                        0b0000001 => {
                            println!("Enable presentation mode:{:?}", *data)
                        }
                        0b0000010 => {
                            println!("Disable presentation mode:{:?}", *data)
                        }
                        _ => ()
                    }

                    
                }
            }
            
        })?;

        Ok(())
    }




 }