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

pub mod fxce; 
pub mod models;
pub mod niri_check;
pub mod syslog;

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::{env, fs};

use gtk::glib;

fn is_program(path: impl AsRef<Path>) -> bool {

    let Ok(metadata) = fs::metadata(path) else {
        return false
    };

    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}


pub fn binary_exists(binary: &str) -> bool {
    const DEFAULT_PATH: &str = "/bin:/usr/bin:.";
    
    let Ok(argv) = glib::shell_parse_argv(binary) else {
        return true // Unparsable: the C code leaves the entry alone.
    };

    let Some(program) = argv.first() else {
        return true // Parsed to no words at all: same.
    };

    // A `/` anywhere makes it a path. `to_string_lossy` cannot lose one on the
    // way: `/` never appears inside a multi byte sequence.
    if program.to_string_lossy().contains('/') {
        return is_program(program)
    }

    let path = env::var_os("PATH")
        .filter(|it| !it.is_empty())
        .unwrap_or_else(|| DEFAULT_PATH.into());

    // Bytes, not `str`: a `PATH` element is not required to be UTF-8.
    path.as_bytes()
        .split(|it| *it == b':')
        .map(|dir| if dir.is_empty() { Path::new(".") } else { Path::new(OsStr::from_bytes(dir)) })
        .any(|dir| is_program(dir.join(program)))
}

