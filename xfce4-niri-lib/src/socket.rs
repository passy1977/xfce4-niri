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
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixStream, UnixListener};
use std::path::PathBuf;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::ffi::c_int;
use std::sync::Arc;

use crate::lock::Lock;
use crate::syslog::{Options, Priority, SysLog};

use osal_rs::os::{Mutex, MutexFn, Thread, ThreadFn};
use osal_rs::utils::{Error, Result};

pub type OnRequest = Arc<Mutex<dyn Fn(&[String]) + Send + Sync + 'static>>;

/// Unlinks the socket node when the accept loop that owns it goes away, so the
/// next start does not find a stale one.
struct SocketGuard(PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub struct Socket{
    unix_socket: PathBuf,
    on_request: OnRequest,
    thread: Thread
}


impl Socket {

    const APP_TAG: &str = "xfce4-niri";

    /// What the server writes back for every line it takes in: `run_client`
    /// reads one reply per command, so every command has to be answered.
    const REPLY_OK: &str = "OK";

    pub const LOCK_FILE: &str = Lock::LOCK_FILE;

    pub fn new(unix_socket: PathBuf) -> Self {
        Socket{
            unix_socket,
            on_request: Mutex::new_arc(|_request: &[String]| {}),
            thread: Thread::new("socket_srv_thd", 0, 0),
        }
    }

    pub fn start_server(&mut self, lock: &Lock, on_request: OnRequest) -> Result<()> {
        if !lock.exists().map_err(|e| Error::UnhandledOwned(e.to_string()))? {
            return Err(Error::UnhandledOwned("fxce4-niri-service seems down".into()))
        }

        let Some(parent) = self.unix_socket.parent() else {
            return Err(Error::UnhandledOwned("invalid socket file path".into()))
        };

        fs::create_dir_all(parent).map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        self.on_request = on_request;

        let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

        let path = self.unix_socket.clone();

        if path.exists() {
            match UnixStream::connect(&path) {
                // Somebody answered: this is a running server, so do not touch it.
                Ok(_) => Err(Error::UnhandledOwned(format!("a server is already listening on {}", path.display()))),
                // Nobody is listening on it any more - the node is stale.
                Err(e) if e.kind() == ErrorKind::ConnectionRefused => fs::remove_file(&path).map_err(|e| Error::UnhandledOwned(e.to_string())),
                Err(e) => Err( Error::UnhandledOwned(e.to_string()))
            }?;
        }

        log.syslog(Self::APP_TAG, Priority::LogDebug, &format!("listening on {}", path.display()));

        let on_request = Arc::clone(&self.on_request);

        self.thread.spawn_simple(move || {
            let listener = UnixListener::bind(&path).map_err(|e| Error::UnhandledOwned(e.to_string()))?;

            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|e| Error::UnhandledOwned(e.to_string()))?;


            let _guard = SocketGuard(path.clone());

            loop {
                match listener.accept() {
                    Ok((stream, _addr)) => {
                        println!("New connection accepted");
                        if let Err(e) = Self::handle_client(&stream, &on_request) {
                            eprintln!("Error handling client: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Accept error: {}", e)
                }
            }
        })?;

        Ok(())
    }

    pub fn stop(&self) {
        let _ = UnixStream::connect(&self.unix_socket);
    }

    fn handle_client(stream: &UnixStream, on_request: &OnRequest) -> Result<()> {

        // Reading and writing side both borrow the same fd: `&UnixStream` is
        // itself `Read` and `Write`, so there is nothing to dup.
        let mut writer = stream;
        let reader = BufReader::new(stream);

        // lines() ends on EOF, which is what a client dropping its end looks like.
        for line in reader.lines() {
            let line = line.map_err(|e| Error::UnhandledOwned(e.to_string()))?;
            let args: Vec<String> = line
                                        .trim()
                                        .splitn(3, ' ')
                                        .map(|s| s.to_string())
                                        .collect();

            let Ok(callback) = on_request.lock() else {
                return Err(Error::Unhandled("failed to take the request callback"))
            };

            callback(&args);
            drop(callback);

            // The client reads one line back per command it wrote: with no
            // answer it would sit on read_line() until the service goes away.
            writeln!(writer, "{}", Self::REPLY_OK).map_err(|e| Error::UnhandledOwned(e.to_string()))?;
            writer.flush().map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        }


        Ok(())
    }

     
    pub fn run_client(&self, lock: &Lock, commands: &[String]) -> Result<()> {
        if !lock.exists().map_err(|e| Error::UnhandledOwned(e.to_string()))? {
            return Err(Error::UnhandledOwned("fxce4-niri-service seems down".into()))
        }

        let stream = UnixStream::connect(&self.unix_socket)
            .map_err(|e| Error::UnhandledOwned(format!("cannot connect to {}: {e} (is the service running?)", self.unix_socket.display())))?;

        let mut writer = stream.try_clone().map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        let mut reader = BufReader::new(stream);

        for command in commands {
            writeln!(writer, "{command}").map_err(|e| Error::UnhandledOwned(e.to_string()))?;
            writer.flush().map_err(|e| Error::UnhandledOwned(e.to_string()))?;

            let mut reply = String::new();
            if reader.read_line(&mut reply).map_err(|e| Error::UnhandledOwned(e.to_string()))? == 0 {
                return Err(Error::UnhandledOwned("server closed the connection".into()));
            }

            println!("{}", reply.trim_end());
        }

        Ok(())
    }

}
