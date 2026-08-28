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
use std::io::ErrorKind;
use std::ffi::c_int;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::lock::Lock;
use crate::syslog::{Options, Priority, SysLog};

use osal_rs::os::{Thread, ThreadFn};
use osal_rs::utils::{Error, Result};

struct SocketGuard(PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub struct Socket(PathBuf, Thread);

impl Socket {

    const APP_TAG: &str = "xfce4-niri";

    pub const LOCK_FILE: &str = Lock::LOCK_FILE;

    pub fn new(unix_socket: PathBuf) -> Self {
        Socket(unix_socket, Thread::new("lock_screen_thd", 0, 0))
    }

    pub fn start_server(&mut self, socket_file: &str) -> Result<()> {
        let lock_file = Lock::get_path(None)?;

        if !fs::exists(&lock_file).map_err(|e| Error::UnhandledOwned(e.to_string()))? {
            return Err(Error::UnhandledOwned("fxce4-niri-service seems down".into()))
        }

        
        let Some(parent) = self.0.parent() else {
            return Err(Error::UnhandledOwned("invalid socket file path".into()))
        };

        let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

        let mut full_path = PathBuf::from(parent);
        full_path.push(socket_file);


        match UnixStream::connect(&full_path) {
            // Somebody answered: this is a running server, so do not touch it.
            Ok(_) => Err(Error::UnhandledOwned(format!("a server is already listening on {}", full_path.display()))),
            // Nobody is listening on it any more - the node is stale.
            Err(e) if e.kind() == ErrorKind::ConnectionRefused => fs::remove_file(&full_path).map_err(|e| Error::UnhandledOwned(e.to_string())),
            Err(e) => Err( Error::UnhandledOwned(e.to_string()))
        }?;

        let listener = UnixListener::bind(&full_path).map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        let _ = SocketGuard(full_path.clone());

        // The mode comes from the umask at bind() time, so narrow it now: owner
        // only. Do it before announcing the socket, not after.
        fs::set_permissions(&full_path, fs::Permissions::from_mode(0o600)).map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        log.syslog(Self::APP_TAG, Priority::LogDebug, &format!("listening on {}", full_path.display()));

        let running = Arc::new(AtomicBool::new(true));

        for stream in listener.incoming() {
            if !running.load(Ordering::SeqCst) {
                break;
            }

            let stream = match stream {
                Ok(stream) => stream,
                // A client that vanished between connect() and accept() is normal
                // traffic, not a reason to tear the server down.
                Err(e) if e.kind() == ErrorKind::ConnectionAborted => continue,
                Err(e) => Err( Error::UnhandledOwned(e.to_string()))?
            };

            // let store = Arc::clone(&store);
            let running = Arc::clone(&running);

            // One thread per connection keeps the accept loop responsive. For a
            // handful of clients this is cheaper and clearer than an event loop.
            let stream = stream.try_clone();
            self.1.spawn_simple(move || {
                if let Err(e) = Self::handle_client(stream.as_ref().unwrap().try_clone().unwrap(), &running) {
                    eprintln!("connection error: {e}");
                }

                Ok(Arc::new(()))
            })?;
        }

        Ok(())
    }


    
    fn handle_client(_stream: UnixStream, _running: &Arc<AtomicBool>) -> Result<(), Error<'_>> {
        // Handle the client connection here.
        Ok(())
    }

}