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

//! Example: watch xfce4-power-manager's "presentation-mode" Xfconf property
//! for changes, using the blocking API from the `dbus` crate.
//!
//! Xfconf channels aren't separate D-Bus objects, and their properties
//! aren't standard D-Bus properties either - everything lives behind the
//! single `org.xfce.Xfconf` interface at `/org/xfce/Xfconf` (see
//! `xfce_list_interfaces.rs`), and changes are announced through that
//! interface's own `PropertyChanged(channel, property, value)` signal, not
//! the generic `org.freedesktop.DBus.Properties.PropertiesChanged` one. That
//! means there's no ready-made binding for it in the `dbus` crate, so this
//! example defines the small struct needed to read it - the same shape
//! `dbus-codegen-rust` would produce from the introspection XML (compare
//! with the crate's own `examples/match_signal.rs`).
//!
//! Run with:
//!   cargo run --example xfce_watch_presentation_mode
//!
//! Then, from another shell, flip the value to see a signal come in:
//!   xfconf-query -c xfce4-power-manager -p /presentation-mode -s true
//!   xfconf-query -c xfce4-power-manager -p /presentation-mode -s false

use std::error::Error;
use std::time::Duration;

use dbus::arg::{self, RefArg, Variant};
use dbus::blocking::Connection;
use dbus::Message;

const TIMEOUT: Duration = Duration::from_millis(5000);

const XFCONF_DEST: &str = "org.xfce.Xfconf";
const XFCONF_PATH: &str = "/org/xfce/Xfconf";
const CHANNEL: &str = "xfce4-power-manager";
const PROPERTY: &str = "/presentation-mode";
const SIGNAL_PROPERTY: &str = "/xfce4-power-manager/presentation-mode";

/// `org.xfce.Xfconf.PropertyChanged(channel, property, value)` - Xfconf's
/// own change-notification signal.
#[derive(Debug)]
struct XfconfPropertyChanged {
    channel: String,
    property: String,
    value: Variant<Box<dyn RefArg>>,
}

impl arg::ReadAll for XfconfPropertyChanged {
    fn read(i: &mut arg::Iter) -> Result<Self, arg::TypeMismatchError> {
        Ok(XfconfPropertyChanged {
            channel: i.read()?,
            property: i.read()?,
            value: i.read()?,
        })
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let conn = Connection::new_session()?;

    // A single proxy for the whole Xfconf object: it's reused both to read
    // the current value up front and, further down, to subscribe to the
    // PropertyChanged signal - there is no per-property object path to
    // connect to.
    let xfconf = conn.with_proxy(XFCONF_DEST, XFCONF_PATH, TIMEOUT);

    // Xfconf channels are sparse: a property that was never explicitly set
    // simply doesn't exist yet, so this call can legitimately fail even
    // though the channel itself is fine - that's not a reason to give up on
    // watching for a future change.
    let initial: Result<(Variant<Box<dyn RefArg>>,), _> =
        xfconf.method_call("org.xfce.Xfconf", "GetProperty", (CHANNEL, PROPERTY));
    match initial {
        Ok((value,)) => println!("[{CHANNEL}]{PROPERTY} = {:?} (initial value)", value.0),
        Err(e) => println!("[{CHANNEL}]{PROPERTY} not set yet ({e})"),
    }



    xfconf.match_signal(|signal: XfconfPropertyChanged, _: &Connection, _: &Message| {
        if signal.channel == CHANNEL && signal.property == SIGNAL_PROPERTY {
            println!("[{}]{} = {:?} (changed)", signal.channel, signal.property, signal.value.0);
        }
        true
    })?;

    println!("Watching for changes, Ctrl+C to stop...");
    loop {
        conn.process(Duration::from_millis(1000))?;
    }
}
