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

//! Example: watch whether the machine has a battery and whether it is
//! currently running on AC or on that battery, using UPower over D-Bus with
//! the blocking API from the `dbus` crate.
//!
//! UPower lives on the SYSTEM bus (unlike xfce4-power-manager, which is a
//! per-session service):
//!   bus name:    org.freedesktop.UPower
//!   object path: /org/freedesktop/UPower
//!   interface:   org.freedesktop.UPower
//!
//! "am I on AC or on battery" is the manager's own OnBattery property (b):
//! true means the system is draining its battery, false means it is wall
//! powered - on a desktop with no battery at all it is always false. That is
//! the same source xfce4-power-manager uses to pick between its *-on-ac-* and
//! *-on-battery-* settings.
//!
//! "do I have a battery" has no matching HasBattery property - introspecting
//! the manager object on this machine shows only DaemonVersion, LidIsClosed,
//! LidIsPresent and OnBattery:
//!
//!   busctl introspect org.freedesktop.UPower /org/freedesktop/UPower
//!
//! Instead, UPower folds every power source into one "display device"
//! (GetDisplayDevice), and that device's own IsPresent property, together
//! with Type == 2 (Battery), is what actually answers that question. Its
//! State property (1 = charging, 2 = discharging, 4 = fully charged) then
//! tells apart "plugged in and charging" from "plugged in and topped up".
//!
//! This example prints both states once, then subscribes to
//! org.freedesktop.DBus.Properties.PropertiesChanged on the manager (for
//! OnBattery) and on the display device (for IsPresent/Type/State), so
//! plugging the charger in or out and removing/inserting a battery are both
//! reported live.
//!
//! Run with:
//!   cargo run --example upower_has_battery
//!
//! Requires upowerd to be running; reading its properties and subscribing to
//! its signals needs no special privileges on the system bus.

use std::error::Error;
use std::time::Duration;

use dbus::blocking::Connection;
use dbus::blocking::stdintf::org_freedesktop_dbus::{Properties, PropertiesPropertiesChanged};
// Brings `match_rule` into scope on PropertiesPropertiesChanged below.
use dbus::message::SignalArgs;

const TIMEOUT: Duration = Duration::from_millis(5000);

const UPOWER_DEST: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const UPOWER_IFACE: &str = "org.freedesktop.UPower";
const DEVICE_IFACE: &str = "org.freedesktop.UPower.Device";

// Per the UPower spec, Device.Type == 2 means "Battery".
const DEVICE_TYPE_BATTERY: u32 = 2;

// Per the UPower spec, Device.State values.
fn battery_state_name(state: u32) -> &'static str {
    match state {
        1 => "charging",
        2 => "discharging",
        3 => "empty",
        4 => "fully-charged",
        5 => "pending-charge",
        6 => "pending-discharge",
        _ => "unknown",
    }
}


//(battery_or_ac: bool, is_present: bool, has_battery: bool, state: u32)
fn print_power_state(conn: &Connection, device_path: &str) -> Result<(bool, bool, bool, u32), Box<dyn Error>> {
    let upower = conn.with_proxy(UPOWER_DEST, UPOWER_PATH, TIMEOUT);
    let battery_or_ac: bool = upower.get(UPOWER_IFACE, "OnBattery")?;

    let device = conn.with_proxy(UPOWER_DEST, device_path, TIMEOUT);
    let device_type: u32 = device.get(DEVICE_IFACE, "Type")?;
    let is_present: bool = device.get(DEVICE_IFACE, "IsPresent")?;
    let state: u32 = device.get(DEVICE_IFACE, "State")?;
    let has_battery = is_present && device_type == DEVICE_TYPE_BATTERY;

    let source = if battery_or_ac { "battery" } else { "ac" };

    println!(
        "has-battery: {has_battery}  power-source: {source}  battery-state: {}  \
         (device={device_path}, type={device_type}, is_present={is_present}, on_battery={battery_or_ac})",
        battery_state_name(state)
    );
    Ok((battery_or_ac, is_present, has_battery, state))
}

fn main() -> Result<(), Box<dyn Error>> {
    let conn = Connection::new_system()?;

    let upower = conn.with_proxy(UPOWER_DEST, UPOWER_PATH, TIMEOUT);
    let (device_path,): (dbus::Path,) = upower.method_call(UPOWER_IFACE, "GetDisplayDevice", ())?;
    let device_path = device_path.to_string();

    let _ = print_power_state(&conn, &device_path)?;

    // The manager announces AC <-> battery transitions through OnBattery.
    let manager_rule = PropertiesPropertiesChanged::match_rule(
        Some(&UPOWER_DEST.into()),
        Some(&UPOWER_PATH.into()),
    )
    .static_clone();

    let watched_path = device_path.clone();
    conn.add_match(manager_rule, move |changed: PropertiesPropertiesChanged, conn, _msg| {
        if changed.interface_name == UPOWER_IFACE
            && changed.changed_properties.contains_key("OnBattery")
        {
            if let Err(e) = print_power_state(conn, &watched_path) {
                eprintln!("failed to re-read power state: {e}");
            }
        }
        true
    })?;

    // The display device announces the battery itself appearing/disappearing
    // and its charge state changing.
    let device_rule = PropertiesPropertiesChanged::match_rule(
        Some(&UPOWER_DEST.into()),
        Some(&device_path.clone().into()),
    )
    .static_clone();

    let watched_path = device_path.clone();
    conn.add_match(device_rule, move |changed: PropertiesPropertiesChanged, conn, _msg| {
        if changed.interface_name == DEVICE_IFACE
            && (changed.changed_properties.contains_key("IsPresent")
                || changed.changed_properties.contains_key("Type")
                || changed.changed_properties.contains_key("State"))
        {
            if let Err(e) = print_power_state(conn, &watched_path) {
                eprintln!("failed to re-read device state: {e}");
            }
        }
        true
    })?;

    println!("Watching {UPOWER_PATH} and {device_path} for power source / battery changes (Ctrl+C to stop)...");
    loop {
        conn.process(Duration::from_millis(1000))?;
    }
}
