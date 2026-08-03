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
use dbus::message::SignalArgs;
use osal_rs::os::{Thread, ThreadFn};
use osal_rs::{os::{Mutex, MutexFn}, utils::{Error, Result}};

use crate::os::syslog::{Options, Priority, SysLog};

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

trait FromRefArg: Sized {
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
    thread: Thread, 
    conn: Arc<SyncConnection>, 
    log: SysLog
}

 impl DBus {

    const TIMEOUT: Duration = Duration::from_millis(5000);
    const DEST: &str = "org.xfce.Xfconf";
    const PATH: &str = "/org/xfce/Xfconf";


    pub(crate) fn new() -> Result<Self> {

        let conn = SyncConnection::new_session().map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        conn.set_signal_match_mode(true);

        Ok(Self{
            thread: Thread::new("dbus_thd", 0, 0),
            conn: Arc::new(conn),
            log: SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int)
        })
    }


    fn register_signal<T: FromRefArg + Default>(&self, channel: &str, property: &str, on_change: Arc<Mutex<impl FnMut(T) + Send + 'static>>) -> Result<()> {

        let signal_property: String = property.to_owned();

        let xfconf = self.conn.with_proxy(Self::DEST, Self::PATH, Self::TIMEOUT);

        let initial: T = match xfconf.method_call::<(Variant<Box<dyn RefArg>>,), _, _, _>("org.xfce.Xfconf", "GetProperty", (channel, property)) {
            Ok((value,)) => T::from_refarg(&*value.0).unwrap_or_default(),
            Err(e) => {
                let msg = format!("[{channel}]{property} not set yet ({e})");
                self.log.syslog(Priority::LogWarning, &msg);
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

    #[inline]
    pub(crate) fn register_presentation_mode_signal(&self, channel: &str, property: &str, on_presentation_mode: Arc<Mutex<impl FnMut(u64) + Send + 'static>>) -> Result<()> {
        self.register_signal(channel, property, on_presentation_mode)
    }

    #[inline]
    pub(crate) fn register_dpms_on_ac_sleep_signal(&self, channel: &str, property: &str, on_dpms_on_ac_sleep: Arc<Mutex<impl FnMut(u64) + Send + 'static>>) -> Result<()> {
        self.register_signal(channel, property, on_dpms_on_ac_sleep)
    }

    #[inline]
    pub(crate) fn register_dpms_enabled_signal(&self, channel: &str, property: &str, on_dpms_enabled: Arc<Mutex<impl FnMut(bool) + Send + 'static>>) -> Result<()> {
        self.register_signal( channel, property, on_dpms_enabled)
    }

    #[inline]
    pub(crate) fn register_dpms_on_battery_off_signal(&self, channel: &str, property: &str, on_dpms_on_battery_off: Arc<Mutex<impl FnMut(u64) + Send + 'static>>) -> Result<()> {
        self.register_signal(channel, property, on_dpms_on_battery_off)
    }

    pub(crate) fn start(&mut self) -> Result<()> {

        let conn = self.conn.clone();

        self.thread.spawn(None,move |_, _| {
                let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);
                loop {
                    if let Err(e) = conn.process(Self::TIMEOUT) {
                        log.syslog(Priority::LogWarning, &format!("dbus process error: {e}"));
                    }
                }
            })?;

        Ok(())
    }
 }
