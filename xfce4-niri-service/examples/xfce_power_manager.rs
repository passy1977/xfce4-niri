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

//! Example: talk to xfce4-power-manager over D-Bus using the blocking API
//! from the `dbus` crate (plain libdbus bindings, no async runtime).
//!
//! xfce4-power-manager publishes itself on the session bus as:
//!   bus name:  org.xfce.PowerManager
//!   object path: /org/xfce/PowerManager
//!   interface: org.xfce.Power.Manager
//!
//! Its method set differs across versions/distros. On this machine
//! introspection shows only: Quit, Restart, GetConfig, GetInfo - no
//! GetBrightness/SetBrightness/Lock, which some older docs mention. That is
//! why this example always introspects first and prints the XML, instead of
//! hardcoding methods that might not exist. You can do the same from a
//! shell with:
//!
//!   busctl --user introspect org.xfce.PowerManager /org/xfce/PowerManager
//!   dbus-send --session --print-reply --dest=org.xfce.PowerManager \
//!       /org/xfce/PowerManager org.freedesktop.DBus.Introspectable.Introspect
//!
//! Run with:
//!   cargo run --example xfce_power_manager
//!   cargo run --example xfce_power_manager -- --lock   # also locks the screen
//!
//! Requires a running session bus and xfce4-power-manager (it must own its
//! bus name for these calls to have anything to talk to).

use std::error::Error;
use std::time::Duration;

use dbus::blocking::Connection;

const TIMEOUT: Duration = Duration::from_millis(5000);

const POWER_DEST: &str = "org.xfce.PowerManager";
const POWER_PATH: &str = "/org/xfce/PowerManager";
const POWER_IFACE: &str = "org.xfce.Power.Manager";

// Screen locking isn't part of xfce4-power-manager's own interface; it lives
// on the freedesktop.org standard screen saver interface instead.
const SCREENSAVER_DEST: &str = "org.freedesktop.ScreenSaver";
const SCREENSAVER_PATH: &str = "/org/freedesktop/ScreenSaver";
const SCREENSAVER_IFACE: &str = "org.freedesktop.ScreenSaver";

fn introspect(proxy: &dbus::blocking::Proxy<&Connection>, path: &str) -> Result<String, Box<dyn Error>> {
    let (xml,): (String,) =
        proxy.method_call("org.freedesktop.DBus.Introspectable", "Introspect", ())?;
    println!("--- Introspection of {path} ---\n{xml}\n");
    Ok(xml)
}

fn main() -> Result<(), Box<dyn Error>> {
    // Connect to the session bus (xfce4-power-manager is a per-user session
    // service, not a system one).
    let conn = Connection::new_session()?;

    // --- org.xfce.PowerManager ---
    // A proxy binds a connection to a specific destination + object path, so
    // every call below only needs the interface/method/args.
    let power = conn.with_proxy(POWER_DEST, POWER_PATH, TIMEOUT);
    introspect(&power, POWER_PATH)?;

    let (name, version, vendor): (String, String, String) =
        power.method_call(POWER_IFACE, "GetInfo", ())?;
    println!("GetInfo -> name={name} version={version} vendor={vendor}");

    let (config,): (std::collections::HashMap<String, String>,) =
        power.method_call(POWER_IFACE, "GetConfig", ())?;
    println!("GetConfig -> {config:#?}");

    // --- org.freedesktop.ScreenSaver (screen locking) ---
    let screensaver = conn.with_proxy(SCREENSAVER_DEST, SCREENSAVER_PATH, TIMEOUT);
    let saver_xml = introspect(&screensaver, SCREENSAVER_PATH)?;

    if std::env::args().any(|a| a == "--lock") {
        if saver_xml.contains("name=\"Lock\"") {
            screensaver.method_call::<(), _, _, _>(SCREENSAVER_IFACE, "Lock", ())?;
            println!("Lock() called");
        } else {
            println!("Lock method not found in introspection, skipping");
        }
    } else {
        println!("(pass --lock to also call Lock() and lock the screen)");
    }

    Ok(())
}
