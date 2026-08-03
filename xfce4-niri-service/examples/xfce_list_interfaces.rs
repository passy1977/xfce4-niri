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

//! Example: enumerate every service on the session bus whose well-known name
//! starts with "org.xfce." and print the D-Bus interfaces each one exposes,
//! using the blocking API from the `dbus` crate (plain libdbus bindings, no
//! async runtime).
//!
//! There is no directory service to ask "give me every object path this
//! service has"; XFCE registers one object per service, and - on this
//! machine at least - the object path is always the well-known name with
//! dots turned into slashes (eg. `org.xfce.Panel` -> `/org/xfce/Panel`).
//! This example rebuilds the path from the name on that basis, then walks
//! any child nodes reported by introspection in case a given XFCE version
//! nests further objects under that root (none observed here, but the
//! introspection XML carries that information for free).
//!
//! Introspection alone misses one thing: XFCE's configuration store,
//! Xfconf, doesn't expose one D-Bus member per setting. Every component's
//! settings (eg. `xfce4-power-manager`'s `presentation-mode`) live as
//! key/value pairs inside a "channel", all reachable only through the
//! single `org.xfce.Xfconf` interface's `ListChannels`/`GetAllProperties`
//! methods - so this example also dumps those channels explicitly.
//!
//! Run with:
//!   cargo run --example xfce_list_interfaces
//!
//! Compare against, from a shell:
//!   busctl --user list | grep org.xfce
//!   busctl --user introspect org.xfce.Panel /org/xfce/Panel
//!   xfconf-query -c xfce4-power-manager -l

use std::error::Error;
use std::time::Duration;

use dbus::arg::PropMap;
use dbus::blocking::Connection;

const TIMEOUT: Duration = Duration::from_millis(5000);

/// Pulls every `marker"...."` attribute value out of an introspection XML
/// blob. `Introspect()` output is simple, well-formed XML with no nested
/// quotes in the attributes we care about, so a full XML parser would be
/// overkill just to read two attribute names.
fn extract_attrs<'a>(xml: &'a str, marker: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(marker) {
        rest = &rest[start + marker.len()..];
        match rest.find('"') {
            Some(end) => {
                out.push(&rest[..end]);
                rest = &rest[end + 1..];
            }
            None => break,
        }
    }
    out
}

fn walk(conn: &Connection, dest: &str, path: &str, depth: usize) -> Result<(), Box<dyn Error>> {
    let proxy = conn.with_proxy(dest, path, TIMEOUT);
    let (xml,): (String,) =
        proxy.method_call("org.freedesktop.DBus.Introspectable", "Introspect", ())?;

    println!("--- Start introspection of {path} ---\n{xml}\n--- End introspection\n");

    let indent = "  ".repeat(depth);
    println!("{indent}{path}");
    for iface in extract_attrs(&xml, "<interface name=\"") {
        // Introspectable/Peer/Properties are on every object; skip them so
        // the interesting, service-specific interfaces stand out.
        if iface.starts_with("org.freedesktop.DBus.") {
            continue;
        }
        println!("{indent}  - {iface}");
    }

    for child in extract_attrs(&xml, "<node name=\"") {
        let child_path = if path == "/" {
            format!("/{child}")
        } else {
            format!("{path}/{child}")
        };
        walk(conn, dest, &child_path, depth + 1)?;
    }
    Ok(())
}

/// Every setting under every Xfconf channel, printed via the same
/// `GetAllProperties` call `xfconf-query -c <channel> -l` uses internally.
fn print_xfconf_channels(conn: &Connection) -> Result<(), Box<dyn Error>> {
    let xfconf = conn.with_proxy("org.xfce.Xfconf", "/org/xfce/Xfconf", TIMEOUT);

    let (mut channels,): (Vec<String>,) =
        xfconf.method_call("org.xfce.Xfconf", "ListChannels", ())?;
    channels.sort();

    println!("=== org.xfce.Xfconf channels (settings, not separate D-Bus interfaces) ===");
    for channel in channels {
        let result: Result<(PropMap,), _> = xfconf.method_call("org.xfce.Xfconf", "GetAllProperties", (&channel, "/"));
        match result {
            Ok((props,)) => {
                println!("  [{channel}] ({} properties)", props.len());
                let mut keys: Vec<&String> = props.keys().collect();
                keys.sort();
                for key in keys {
                    println!("    {key} = {:?}", props[key].0);
                }
            }
            Err(e) => println!("  [{channel}] (failed to read: {e})"),
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let conn = Connection::new_session()?;

    let bus = conn.with_proxy("org.freedesktop.DBus", "/org/freedesktop/DBus", TIMEOUT);
    let (names,): (Vec<String>,) = bus.method_call("org.freedesktop.DBus", "ListNames", ())?;

    let mut xfce_names: Vec<&String> = names.iter().filter(|n| n.starts_with("org.xfce.")).collect();
    xfce_names.sort();

    if xfce_names.is_empty() {
        println!("No org.xfce.* service is currently on the session bus.");
        return Ok(());
    }

    for name in xfce_names {
        let root_path = format!("/{}", name.replace('.', "/"));
        println!("=== {name} ===");
        if let Err(e) = walk(&conn, name, &root_path, 0) {
            println!("  (failed to introspect {root_path}: {e})");
        }
        println!();
    }

    if let Err(e) = print_xfconf_channels(&conn) {
        println!("  (org.xfce.Xfconf not available: {e})");
    }

    Ok(())
}
