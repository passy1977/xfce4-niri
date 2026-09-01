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
use std::io::{BufRead, BufReader, ErrorKind};
use std::ffi::c_int;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::lock::Lock;
use crate::syslog::{Options, Priority, SysLog};

use osal_rs::os::{Mutex, MutexFn, Thread, ThreadFn};
use osal_rs::utils::{Error, Result};

pub type OnRequest = Arc<Mutex<dyn Fn(&[String]) + Send + Sync + 'static>>;
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

    pub const LOCK_FILE: &str = Lock::LOCK_FILE;

    pub fn new(unix_socket: PathBuf) -> Self {
        Socket{
            unix_socket,
            on_request: Mutex::new_arc(|_request: &[String]| {}),
            thread: Thread::new("lock_screen_thd", 0, 0)
        }
    }

    pub fn start_server(&mut self, lock: &Lock, on_request: OnRequest) -> Result<()> {
        if !lock.exists().map_err(|e| Error::UnhandledOwned(e.to_string()))? {
            return Err(Error::UnhandledOwned("fxce4-niri-service seems down".into()))
        }

        let Some(parent) = self.unix_socket.parent() else {
            return Err(Error::UnhandledOwned("invalid socket file path".into()))
        };

        self.on_request = on_request;

        let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

        let mut full_path = PathBuf::from(parent);
        full_path.push(&self.unix_socket);


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
            let on_request = Arc::clone(&self.on_request);
            self.thread.spawn_simple(move || {
                let on_request = Arc::clone(&on_request);
                if let Err(e) = Self::handle_client(stream.as_ref().unwrap().try_clone().unwrap(), &running, on_request) {
                    eprintln!("connection error: {e}");
                }

                Ok(Arc::new(()))
            })?;
        }

        Ok(())
    }


    
    fn handle_client(stream: UnixStream, _running: &Arc<AtomicBool>, on_request: OnRequest) -> Result<()> {
        // Handle the client connection here.

        let reader = BufReader::new(stream);

        for line in reader.lines() {
            let line = line.map_err(|e| Error::UnhandledOwned(e.to_string()))?;
            let args: Vec<String> = line
                                        .trim()
                                        .splitn(3, ' ')
                                        .map(|s| s.to_string())
                                        .collect();

            (on_request.lock().unwrap())(&args);
        }


        Ok(())
    }

}