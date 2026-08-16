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

pub struct NiriCheck;

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


#[cfg(test)]
mod tests {

    use std::os::unix::net::UnixListener;
    use std::path::Path;
    use std::thread::{self, JoinHandle};

    use super::*;
    use crate::test_support::{EnvGuard, TempDir};

    /// A one shot stand in for niri: reads the request line, answers with
    /// `response`, and gives the request back on join.
    fn serve(path: &Path, response: &'static str) -> JoinHandle<String> {

        let listener = UnixListener::bind(path).expect("cannot bind the socket");

        thread::spawn(move || {

            let (stream, _) = listener.accept().expect("no client");

            let mut request = String::new();
            BufReader::new(&stream).read_line(&mut request).expect("no request");

            let mut stream = &stream;
            stream.write_all(response.as_bytes()).expect("cannot answer");
            stream.write_all(b"\n").expect("cannot answer");

            request
        })
    }

    #[test]
    fn socket_path_prefers_the_environment() {

        let mut env = EnvGuard::new();
        env.set("NIRI_SOCKET", "/run/user/1000/niri.chosen.sock")
            .set("XDG_RUNTIME_DIR", "/run/user/1000")
            .unset("WAYLAND_DISPLAY");

        assert_eq!(NiriCheck::socket_path(), Some(PathBuf::from("/run/user/1000/niri.chosen.sock")));
    }

    #[test]
    fn socket_path_is_none_without_a_runtime_dir() {

        let mut env = EnvGuard::new();
        env.set("NIRI_SOCKET", "").unset("XDG_RUNTIME_DIR");

        assert_eq!(NiriCheck::socket_path(), None);
        assert!(NiriCheck::version().is_none());
    }

    #[test]
    fn socket_path_is_none_when_the_runtime_dir_holds_no_socket() {

        let dir = TempDir::new();
        dir.file("not-niri.sock", 0o644);
        dir.file("niri.wayland-1.1.txt", 0o644);

        let mut env = EnvGuard::new();
        env.unset("NIRI_SOCKET").set("XDG_RUNTIME_DIR", dir.path()).unset("WAYLAND_DISPLAY");

        assert_eq!(NiriCheck::socket_path(), None);
    }

    /// With a display set only its own sockets are candidates, and the last one
    /// in order wins.
    #[test]
    fn socket_path_keeps_the_sockets_of_this_display() {

        let dir = TempDir::new();
        dir.file("niri.wayland-1.123.sock", 0o644);
        dir.file("niri.wayland-1.9.sock", 0o644);
        dir.file("niri.wayland-2.5.sock", 0o644);
        dir.file("other.sock", 0o644);

        let mut env = EnvGuard::new();
        env.unset("NIRI_SOCKET").set("XDG_RUNTIME_DIR", dir.path()).set("WAYLAND_DISPLAY", "wayland-1");

        assert_eq!(NiriCheck::socket_path(), Some(dir.path().join("niri.wayland-1.9.sock")));

        // No display: every `niri.*.sock` is a candidate.
        env.unset("WAYLAND_DISPLAY");
        assert_eq!(NiriCheck::socket_path(), Some(dir.path().join("niri.wayland-2.5.sock")));
    }

    #[test]
    fn request_sends_one_line_and_reads_one_back() {

        let dir = TempDir::new();
        let socket = dir.path().join("niri.sock");
        let server = serve(&socket, "{\"Ok\":\"pong\"}");

        let mut env = EnvGuard::new();
        env.set("NIRI_SOCKET", &socket);

        assert_eq!(NiriCheck::request("\"ping\"").unwrap(), "{\"Ok\":\"pong\"}\n");
        assert_eq!(server.join().unwrap(), "\"ping\"\n");
    }

    #[test]
    fn request_fails_without_a_socket() {

        let dir = TempDir::new();

        let mut env = EnvGuard::new();
        env.set("NIRI_SOCKET", dir.path().join("niri.sock"));

        assert!(NiriCheck::request("\"Version\"").is_err(), "nothing is listening");
    }

    #[test]
    fn version_reads_the_field_out_of_the_reply() {

        let dir = TempDir::new();
        let socket = dir.path().join("niri.sock");
        let server = serve(&socket, "{\"Ok\":{\"Version\":\"25.05.1\"}}");

        let mut env = EnvGuard::new();
        env.set("NIRI_SOCKET", &socket);

        assert_eq!(NiriCheck::version().as_deref(), Some("25.05.1"));
        assert_eq!(server.join().unwrap(), "\"Version\"\n");
    }

    #[test]
    fn version_is_none_when_the_reply_holds_no_version() {

        let dir = TempDir::new();
        let socket = dir.path().join("niri.sock");
        let server = serve(&socket, "{\"Err\":\"unknown request\"}");

        let mut env = EnvGuard::new();
        env.set("NIRI_SOCKET", &socket);

        assert_eq!(NiriCheck::version(), None);

        server.join().unwrap();
    }
}

