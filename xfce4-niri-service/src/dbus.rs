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
use std::time::Duration;

use dbus::Message;
use dbus::arg::{self, RefArg, Variant};
use dbus::blocking::SyncConnection;
use dbus::blocking::stdintf::org_freedesktop_dbus::{Properties, PropertiesPropertiesChanged};
use dbus::message::SignalArgs;
use osal_rs::os::{Thread, ThreadFn, Mutex, MutexFn};
use osal_rs::utils::{Error, Result};

use xfce4_niri_lib::syslog::{Options, Priority, SysLog};

macro_rules! handle_power_source_error {
    ($msg:expr) => {{
        let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);
        log.syslog(Priority::LogWarning, $msg);
        Error::UnhandledOwned($msg.to_string())
    }};
}

#[derive(Debug)]
struct XFConfPropertyChanged {
    channel: String,
    property: String,
    value: Variant<Box<dyn RefArg>>,
}

impl arg::ReadAll for XFConfPropertyChanged {
    fn read(i: &mut arg::Iter) -> Result<Self, arg::TypeMismatchError> {
        Ok(XFConfPropertyChanged {
            channel: i.read()?,
            property: i.read()?,
            value: i.read()?,
        })
    }
}

impl SignalArgs for XFConfPropertyChanged {
    const NAME: &'static str = "PropertyChanged";
    const INTERFACE: &'static str = "org.xfce.Xfconf";
}

pub(crate) trait FromRefArg: Sized {
    fn from_refarg(value: &dyn RefArg) -> Option<Self>;
}

impl FromRefArg for u64 {
    fn from_refarg(value: &dyn RefArg) -> Option<Self> {
        value.as_u64()
    }
}

impl FromRefArg for bool {
    fn from_refarg(value: &dyn RefArg) -> Option<Self> {
        value.as_u64().map(|v| v != 0)
    }
}

pub(crate) struct DBus {
    xfce4_power_manager_thread: Thread,
    xfce4_power_manager_conn: Arc<SyncConnection>,
    upower_thread: Thread,
    upower_conn: Arc<SyncConnection>,
}

 impl DBus {

    const TIMEOUT: Duration = Duration::from_millis(5000);
    const DEST: &str = "org.xfce.Xfconf";
    const PATH: &str = "/org/xfce/Xfconf";


    pub(crate) fn new() -> Result<Self> {

        // xfconf is a per-session service, UPower is a system service.
        let conn = SyncConnection::new_session().map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        let system_conn = SyncConnection::new_system().map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        conn.set_signal_match_mode(true);
        system_conn.set_signal_match_mode(true);

        Ok(Self{
            xfce4_power_manager_thread: Thread::new("xfce4_power_manager_thread_thd", 0, 0),
            xfce4_power_manager_conn: Arc::new(conn),
            upower_thread: Thread::new("upower_thd", 0, 0),
            upower_conn: Arc::new(system_conn)
        })
    }


    fn private_register_xfce4_power_manager_signal<T: FromRefArg + Default>(&self, channel: &str, property: &str, on_change: Arc<Mutex<impl FnMut(T) + Send + 'static>>) -> Result<()> {

        let signal_property: String = property.to_owned();

        let xfconf = self.xfce4_power_manager_conn.with_proxy(Self::DEST, Self::PATH, Self::TIMEOUT);

        let initial: T = match xfconf.method_call::<(Variant<Box<dyn RefArg>>,), _, _, _>("org.xfce.Xfconf", "GetProperty", (channel, property)) {
            Ok((value,)) => T::from_refarg(&*value.0).unwrap_or_default(),
            Err(e) => {
                handle_power_source_error!(&format!("[{channel}]{property} not set yet ({e})"));
                T::default()
            }
        };
        (on_change.lock().unwrap())(initial);


        let on_change = on_change.clone();
        let channel = channel.to_owned();

        xfconf.match_signal(move |signal: XFConfPropertyChanged, _: &SyncConnection, _: &Message| {

            if signal.channel == channel && signal.property == signal_property {
                if let Some(value) = T::from_refarg(&*signal.value.0) {
                    (on_change.lock().unwrap())(value);
                }
            }
            true
        }).map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        Ok(())
    }

    fn get_power_source_data(
        dest: &str,
        path: &str,
        iface: &'static str,
        device_iface: &'static str,
        device_path: &str,
        conn: &SyncConnection
    ) -> Result<(bool, bool, u32)> {

        const DEVICE_TYPE_BATTERY: u32 = 2;

        // OnBattery lives on the manager interface, the rest on the device one.
        let upower = conn.with_proxy(dest, path, Self::TIMEOUT);
        let battery_or_ac: bool = upower.get(iface, "OnBattery").map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        let device = conn.with_proxy(dest, device_path, Self::TIMEOUT);
        let device_type: u32 = device.get(device_iface, "Type").map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        let is_present: bool = device.get(device_iface, "IsPresent").map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        let state: u32 = device.get(device_iface, "State").map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        let has_battery = is_present && device_type == DEVICE_TYPE_BATTERY;

        Ok((battery_or_ac, has_battery, state))
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn register_xfce4_power_manager_signal<T: FromRefArg + Default>(
        &self,
        channel: &str,
        property: &str,
        on_change: Arc<Mutex<impl FnMut(T) + Send + 'static>>
    ) -> Result<()> {
        self.private_register_xfce4_power_manager_signal(channel, property, on_change)
    }


    pub(crate) fn register_upower_signals(
        &self,
        dest: &'static str,
        path: &'static str,
        iface: &'static str,
        device_iface: &'static str,
        on_upower_signals: Arc<Mutex<impl FnMut(bool, bool, u32) + Send + 'static>>,
    ) -> Result<()> {


        let upower = self.upower_conn.with_proxy(dest, path, Self::TIMEOUT);
        let (device_path,): (dbus::Path,) = upower.method_call(iface, "GetDisplayDevice", ()).map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        let device_path = device_path.to_string();

        let manager_rule = PropertiesPropertiesChanged::match_rule(
            Some(&dest.into()),
            Some(&path.into()),
        )
        .static_clone();

        let watched_path = device_path.clone();
        let value = on_upower_signals.clone();
        let _ = Self::get_power_source_data(dest, path, iface, device_iface, &watched_path, &self.upower_conn)
            .map(|(battery_or_ac, has_battery, state)| (value.lock().unwrap())(battery_or_ac, has_battery, state))
            .map_err(|e| handle_power_source_error!(&format!("Error occurred while processing power source data: {e}")))?;


        let watched_path = device_path.clone();
        let value = on_upower_signals.clone();
        self.upower_conn.add_match(manager_rule, move |changed: PropertiesPropertiesChanged, conn, _msg| {
            if changed.interface_name == iface
                && changed.changed_properties.contains_key("OnBattery")
            {

                let Ok(_) = Self::get_power_source_data(dest, path, iface, device_iface, &watched_path, conn)
                    .map(|(battery_or_ac, has_battery, state)|  (value.lock().unwrap())(battery_or_ac, has_battery, state) )
                    .map_err(|e| handle_power_source_error!(&format!("Error occurred while processing power source data: {e}"))) else {
                        return false
                    };
            
            }
            true
        }).map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        let device_rule = PropertiesPropertiesChanged::match_rule(
            Some(&dest.into()),
            Some(&device_path.clone().into()),
        )
        .static_clone();

        let watched_path = device_path.clone();
        let value = on_upower_signals.clone();
        self.upower_conn.add_match(device_rule, move |changed: PropertiesPropertiesChanged, conn, _msg| {
            if changed.interface_name == device_iface
                && (changed.changed_properties.contains_key("IsPresent")
                    || changed.changed_properties.contains_key("Type")
                    || changed.changed_properties.contains_key("State"))
            {

                let Ok(_) = Self::get_power_source_data(dest, path, iface, device_iface, &watched_path, conn)
                    .map(|(battery_or_ac, has_battery, state)|  (value.lock().unwrap())(battery_or_ac, has_battery, state) )
                    .map_err(|e| handle_power_source_error!(&format!("Error occurred while processing power source data: {e}"))) else {
                        return false
                    };
                    
            }
            true
        }).map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        Ok(())
    }


    pub(crate) fn start(&mut self) -> Result<()> {

        let conn = self.xfce4_power_manager_conn.clone();

        self.xfce4_power_manager_thread.spawn(None,move |_, _| {
                let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);
                loop {
                    if let Err(e) = conn.process(Self::TIMEOUT) {
                        log.syslog(Priority::LogWarning, &format!("DBus process error: {e}"));
                    }
                }
            })?;

        let upower_conn = self.upower_conn.clone();

        self.upower_thread.spawn(None,move |_, _| {
                let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);
                loop {
                    if let Err(e) = upower_conn.process(Self::TIMEOUT) {
                        log.syslog(Priority::LogWarning, &format!("UPower process error: {e}"));
                    }
                }
            })?;

        Ok(())
    }
 }
