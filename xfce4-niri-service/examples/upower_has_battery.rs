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

//! Example: watch whether the machine currently has a battery, using UPower
//! over D-Bus with the blocking API from the `dbus` crate.
//!
//! UPower lives on the SYSTEM bus (unlike xfce4-power-manager, which is a
//! per-session service):
//!   bus name:    org.freedesktop.UPower
//!   object path: /org/freedesktop/UPower
//!   interface:   org.freedesktop.UPower
//!
//! There is no single "HasBattery" property - introspecting the manager
//! object on this machine shows only DaemonVersion, LidIsClosed,
//! LidIsPresent and OnBattery:
//!
//!   busctl introspect org.freedesktop.UPower /org/freedesktop/UPower
//!
//! Instead, UPower folds every power source into one "display device"
//! (GetDisplayDevice), and that device's own IsPresent property, together
//! with Type == 2 (Battery), is what actually answers "does this machine
//! have a battery". This example prints that state once, then subscribes to
//! org.freedesktop.DBus.Properties.PropertiesChanged on the display device
//! so a battery being removed/inserted is reported live.
//!
//! Run with:
//!   cargo run --example upower_has_battery
//!
//! Requires upowerd to be running; reading its properties and subscribing to
//! its signals needs no special privileges on the system bus.

use std::error::Error;
use std::time::Duration;

use dbus::blocking::stdintf::org_freedesktop_dbus::{Properties, PropertiesPropertiesChanged};
use dbus::blocking::Connection;
use dbus::message::SignalArgs;

const TIMEOUT: Duration = Duration::from_millis(5000);

const UPOWER_DEST: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const UPOWER_IFACE: &str = "org.freedesktop.UPower";
const DEVICE_IFACE: &str = "org.freedesktop.UPower.Device";

// Per the UPower spec, Device.Type == 2 means "Battery".
const DEVICE_TYPE_BATTERY: u32 = 2;

fn print_has_battery(conn: &Connection, device_path: &str) -> Result<(), Box<dyn Error>> {
    let device = conn.with_proxy(UPOWER_DEST, device_path, TIMEOUT);
    let device_type: u32 = device.get(DEVICE_IFACE, "Type")?;
    let is_present: bool = device.get(DEVICE_IFACE, "IsPresent")?;
    let has_battery = is_present && device_type == DEVICE_TYPE_BATTERY;

    println!(
        "has-battery: {has_battery}  (device={device_path}, type={device_type}, is_present={is_present})"
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let conn = Connection::new_system()?;

    let upower = conn.with_proxy(UPOWER_DEST, UPOWER_PATH, TIMEOUT);
    let (device_path,): (dbus::Path,) = upower.method_call(UPOWER_IFACE, "GetDisplayDevice", ())?;
    let device_path = device_path.to_string();

    print_has_battery(&conn, &device_path)?;

    let rule = PropertiesPropertiesChanged::match_rule(
        Some(&UPOWER_DEST.into()),
        Some(&device_path.clone().into()),
    )
    .static_clone();

    let watched_path = device_path.clone();
    conn.add_match(rule, move |changed: PropertiesPropertiesChanged, conn, _msg| {
        if changed.interface_name == DEVICE_IFACE
            && (changed.changed_properties.contains_key("IsPresent")
                || changed.changed_properties.contains_key("Type"))
        {
            if let Err(e) = print_has_battery(conn, &watched_path) {
                eprintln!("failed to re-read device state: {e}");
            }
        }
        true
    })?;

    println!("Watching {device_path} for battery presence changes (Ctrl+C to stop)...");
    loop {
        conn.process(Duration::from_millis(1000))?;
    }
}
