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

use std::io::{self, BufRead, BufReader, Result, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) struct NiriCheck;

impl NiriCheck {

    const TIMEOUT: Duration = Duration::from_millis(500);

    pub fn socket_path() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("NIRI_SOCKET") {
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }

        let dir = std::env::var("XDG_RUNTIME_DIR").ok()?;
        let wayland = std::env::var("WAYLAND_DISPLAY").ok();

        let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .filter(|e| {
                let name = e.file_name();
                let Some(name) = name.to_str() else {
                    return false;
                };
                let Some(rest) = name.strip_prefix("niri.") else {
                    return false;
                };
                if !rest.ends_with(".sock") {
                    return false;
                }

                wayland
                    .as_deref()
                    .is_none_or(|w| rest.starts_with(&format!("{w}.")))
            })
            .map(|e| e.path())
            .collect();

        candidates.sort();
        candidates.pop()
    }


    fn request(line: &str) -> Result<String> {
        let path = Self::socket_path().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Socket niri not found")
        })?;

        let mut stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(Self::TIMEOUT))?;
        stream.set_write_timeout(Some(Self::TIMEOUT))?;

        stream.write_all(line.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        
        
        // End request to niri 
        stream.shutdown(Shutdown::Write)?;

        let mut response = String::new();
        BufReader::new(stream).read_line(&mut response)?;
        Ok(response)
    }

    pub fn version() -> Option<String> {
        let response = Self::request("\"Version\"").ok()?;
        
        let rest = response.split_once("\"Version\":\"")?.1;
        Some(rest.split_once('"')?.0.to_owned())
    }

}

