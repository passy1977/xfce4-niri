/***************************************************************************
 *
 * xfce4-niri
 * Copyright (C) 2026 Antonio Salsi <passy.linux@zresa.it>
 *
 * This program is free software; you can redistribute it and/or
 * modify it under the terms of the GNU General Public License
 * as published by the Free Software Foundation; either version 2
 * of the License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, see <https://www.gnu.org/licenses/>.
 *
 ***************************************************************************/

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixStream, UnixListener};
use std::path::PathBuf;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::ffi::c_int;

use crate::lock::Lock;
use crate::syslog::{Options, Priority, SysLog};

use osal_rs::os::{Thread, ThreadFn};

use crate::Result;

pub type OnRequest = dyn Fn(&[String]) -> Result<()> + Send + Sync + 'static;

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
    thread: Thread
}


impl Socket {

    const APP_TAG: &str = "Socket";

    const REPLY_OK: &str = "OK";
    const REPLY_KO: &str = "KO";
    const MAX_PARAM_SPLIT: usize = 3;

    pub const LOCK_FILE: &str = Lock::LOCK_FILE;

    pub fn new(unix_socket: PathBuf) -> Self {
        Socket{
            unix_socket,
            thread: Thread::new("socket_srv_thd", 0, 0),
        }
    }

    pub fn start_server(&mut self, on_request: &'static OnRequest) -> Result<()> {

        let Some(parent) = self.unix_socket.parent() else {
            return Err("invalid socket file path".into())
        };

        fs::create_dir_all(parent)?;

        let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

        let path = self.unix_socket.clone();

        if path.exists() {
            let result: Result<()> = match UnixStream::connect(&path) {
                // Somebody answered: this is a running server, so do not touch it.
                Ok(_) => Err(format!("a server is already listening on {}", path.display()).into()),
                // Nobody is listening on it any more - the node is stale.
                Err(e) if e.kind() == ErrorKind::ConnectionRefused => fs::remove_file(&path).map_err(|e| e.into()),
                Err(e) => Err(e.into())
            };
            result?;
        }

        log.syslog(Self::APP_TAG, Priority::LogDebug, &format!("listening on {}", path.display()));

        self.thread.spawn_simple(move || {
            let listener = UnixListener::bind(&path)?;

            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;


            let _guard = SocketGuard(path.clone());

            loop {
                match listener.accept() {
                    Ok((ref mut stream, _addr)) => {
                        println!("New connection accepted");
                        if let Err(e) = Self::handle_client(&stream, &on_request) {
                            let _ = stream.write(Self::REPLY_KO.as_bytes());
                            log.syslog(Self::APP_TAG, Priority::LogErr, &format!("{e}"));
                        } else {
                            let _ = stream.write(Self::REPLY_OK.as_bytes());
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

        let mut writer = stream;
        let reader = BufReader::new(stream);

        for line in reader.lines() {
            let line = line?;
            let args: Vec<String> = line
                                        .trim()
                                        .splitn(Self::MAX_PARAM_SPLIT, ' ')
                                        .map(|s| s.to_string())
                                        .collect();


            on_request(&args)?;

            writeln!(writer, "{}", Self::REPLY_OK)?;
            writer.flush()?;
        }


        Ok(())
    }

     
    pub fn run_client(&self, commands: &[String]) -> Result<()> {

        let stream = UnixStream::connect(&self.unix_socket)
            .map_err(|e| format!("cannot connect to {}: {e} (is the service running?)", self.unix_socket.display()))?;

        let mut writer = stream.try_clone()?;
        let mut reader = BufReader::new(stream);

        let command = commands.join(" ");


        writeln!(writer, "{command}")?;
        writer.flush()?;

        let mut reply = String::new();
        if reader.read_line(&mut reply)? == 0 {
            return Err("server closed the connection".into());
        }

        println!("{}", reply.trim_end());

        Ok(())
    }

}


#[cfg(test)]
mod tests {

    use std::io::Read;
    use std::net::Shutdown;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::test_support::{EnvGuard, TempDir};

    /// A callback recording every request it is handed, for the tests that
    /// need to look at what `on_request` was called with.
    fn recording() -> (Arc<Mutex<Vec<Vec<String>>>>, Box<OnRequest>) {

        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = seen.clone();

        (seen, Box::new(move |args: &[String]| {
            recorder.lock().unwrap().push(args.to_vec());
            Ok(())
        }))
    }

    #[test]
    fn handle_client_replies_ok_and_forwards_every_line() {

        let (client, server) = UnixStream::pair().unwrap();
        let (seen, on_request) = recording();

        let mut client = client;
        client.write_all(b"PING\n").unwrap();
        client.write_all(b"SET a b c d\n").unwrap();
        client.shutdown(Shutdown::Write).unwrap();

        Socket::handle_client(&server, &*on_request).unwrap();
        server.shutdown(Shutdown::Write).unwrap();

        let mut replies = String::new();
        BufReader::new(&client).read_to_string(&mut replies).unwrap();
        assert_eq!(replies, "OK\nOK\n");

        // `splitn(MAX_PARAM_SPLIT, ' ')`: only the first two words are split
        // out, the rest of the line stays together as the last argument.
        assert_eq!(*seen.lock().unwrap(), vec![
            vec!["PING".to_string()],
            vec!["SET".to_string(), "a".to_string(), "b c d".to_string()],
        ]);
    }

    /// A failing callback stops the loop and the error is returned, with no
    /// reply written for the line that failed.
    #[test]
    fn handle_client_stops_and_propagates_the_callback_error() {

        let (client, server) = UnixStream::pair().unwrap();

        let mut client = client;
        client.write_all(b"BOOM\n").unwrap();
        client.shutdown(Shutdown::Write).unwrap();

        let on_request: &OnRequest = &|_: &[String]| Err("boom".into());
        assert!(Socket::handle_client(&server, on_request).is_err());
    }

    #[test]
    fn new_stores_the_socket_path_without_touching_the_filesystem() {

        let dir = TempDir::new();
        let path = dir.path().join("does-not-exist.sock");

        let socket = Socket::new(path.clone());

        assert_eq!(socket.unix_socket, path);
        assert!(!path.exists());
    }

    /// A live server is already listening: `start_server` must not steal or
    /// remove its socket node.
    #[test]
    fn start_server_refuses_when_something_is_already_listening() {

        let dir = TempDir::new();
        let path = dir.path().join("running.sock");
        let listener = UnixListener::bind(&path).unwrap();

        let (_seen, on_request) = recording();
        let on_request: &'static OnRequest = Box::leak(on_request);

        let err = match Socket::new(path.clone()).start_server(on_request) {
            Ok(()) => panic!("a live server's socket must not be taken over"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("already listening"));

        drop(listener);
        assert!(path.exists(), "the running server's own socket must be left alone");
    }

    /// A socket node left behind by a server that is no longer running is
    /// stale: `start_server` has to clear it out of the way and bind its own.
    #[test]
    fn start_server_clears_a_stale_socket_node() {

        let (_env, dir) = env_and_dir();
        let path = dir.path().join("stale.sock");

        // Bind and drop without unlinking: exactly what a crashed server
        // leaves behind.
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists());

        let (seen, on_request) = recording();
        let on_request: &'static OnRequest = Box::leak(on_request);

        let mut socket = Socket::new(path.clone());
        socket.start_server(on_request).expect("a stale node must not block start_server");

        wait_until(|| path.exists(), "the new socket node never appeared");

        socket.run_client(&["PING".to_string()]).expect("run_client failed");
        assert_eq!(*seen.lock().unwrap(), vec![vec!["PING".to_string()]]);

        socket.stop();
    }

    fn env_and_dir() -> (EnvGuard, TempDir) {
        let dir = TempDir::new();
        let env = EnvGuard::new();
        (env, dir)
    }

    /// Polls `predicate` for up to a second: `start_server` binds on its own
    /// thread, so the socket node can lag a moment behind the call returning.
    fn wait_until(mut predicate: impl FnMut() -> bool, message: &str) {

        let deadline = Instant::now() + Duration::from_secs(1);

        while !predicate() {
            assert!(Instant::now() < deadline, "{message}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
