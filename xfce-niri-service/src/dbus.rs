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

use dbus::Message;
use dbus::arg::{self, RefArg, Variant};
use dbus::blocking::SyncConnection;
use dbus::message::SignalArgs;
use osal_rs::os::{Thread, ThreadFn};
use osal_rs::{os::{Mutex, MutexFn}, utils::{Error, Result}};

use crate::os::syslog::{Options, Priority, SysLog};

#[derive(Debug)]
struct XfconfPropertyChanged {
    channel: String,
    property: String,
    _value: Variant<Box<dyn RefArg>>,
}

impl arg::ReadAll for XfconfPropertyChanged {
    fn read(i: &mut arg::Iter) -> Result<Self, arg::TypeMismatchError> {
        Ok(XfconfPropertyChanged {
            channel: i.read()?,
            property: i.read()?,
            _value: i.read()?,
        })
    }
}

impl SignalArgs for XfconfPropertyChanged {
    const NAME: &'static str = "PropertyChanged";
    const INTERFACE: &'static str = "org.xfce.Xfconf";
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

        Ok(Self{
            thread: Thread::new("dbus_thd", 0, 0),
            conn: Arc::new(SyncConnection::new_session().map_err(|e| Error::UnhandledOwned(e.to_string()))?),
            log: SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int)
        })
    }

    pub(crate) fn register_signal(&self, channel: &str, property: &str, on_presentation_mode: Arc<Mutex<impl FnMut(bool) + Send + 'static>>) -> Result<()> {

        let signal_property: String = format!("/{channel}{property}");

        let xfconf = self.conn.with_proxy(Self::DEST, Self::PATH, Self::TIMEOUT);

        let initial: bool = match xfconf.method_call("org.xfce.Xfconf", "GetProperty", (channel, property)) {
            Ok((value,)) => value,
            Err(e) => {
                let msg = format!("[{channel}]{property} not set yet ({e})");
                self.log.syslog(Priority::LogWarning, &msg);
                false
            }
        };
        (on_presentation_mode.lock().unwrap())(initial);


        let on_presentation_mode = on_presentation_mode.clone();
        let channel = channel.to_owned();

        xfconf.match_signal(move |signal: XfconfPropertyChanged, _: &SyncConnection, _: &Message| {

            if signal.channel == channel && signal.property == signal_property {
                (on_presentation_mode.lock().unwrap())(initial);
            }
            true
        }).map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        Ok(())
    }

    /// Starts a dedicated thread that pumps the connection forever,
    /// dispatching incoming signals to the callbacks registered via
    /// `register_signal`. Without this nothing ever reads from the D-Bus
    /// socket, so registered match rules never fire.
    ///
    /// The connection is shared through an `Arc<SyncConnection>`, so
    /// `register_signal` can still be called on `self` after `start()` to
    /// subscribe to further signals.
    pub(crate) fn start(&mut self) -> Result<()> {

        let connection = self.conn.clone();

        self.thread.spawn(None,move |_, _| {
                let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);
                loop {
                    if let Err(e) = connection.process(Self::TIMEOUT) {
                        log.syslog(Priority::LogWarning, &format!("dbus process error: {e}"));
                    }
                }
            })?;

        Ok(())
    }
 }
