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

//! Example: IPC over a filesystem-backed Unix domain socket, using only
//! `std::os::unix::net` - no extra dependency, no async runtime.
//!
//! A Unix socket is addressed by a path on the filesystem, so the usual
//! filesystem rules apply: the directory must exist before `bind()`, the
//! resulting node stays behind after the process dies (nothing unlinks it for
//! you), and the socket's own mode bits decide who may connect. That makes it
//! the natural transport for a per-session daemon like xfce4-niri-service: the
//! socket lives in $XDG_RUNTIME_DIR, which is already private to the user
//! (mode 0700, wiped at logout), so only the session owner can talk to it.
//!
//! Compared to D-Bus it buys you a private, dependency-free channel; what you
//! give up is discovery, activation and a type system - the wire format is
//! yours to define. Here it is the simplest thing that works: newline
//! delimited text, one request per line, one reply per line.
//!
//!   PING            -> PONG
//!   SET <key> <val> -> OK
//!   GET <key>       -> VALUE <val> | NOTFOUND
//!   LIST            -> KEYS <k1,k2,...>
//!   QUIT            -> BYE, then the connection is closed
//!   SHUTDOWN        -> BYE, and the server stops accepting
//!
//! Three details are what actually make this robust, and all three are easy to
//! get wrong:
//!
//! 1. STALE SOCKETS. `bind()` fails with EADDRINUSE when the path exists, even
//!    if the process that created it is long gone. Blindly unlinking first
//!    would let a second server steal the socket from a running one, so probe
//!    it: a `connect()` that succeeds means somebody is listening (bail out), a
//!    `connect()` refused with ECONNREFUSED means the node is stale and can be
//!    removed.
//!
//! 2. PERMISSIONS. The mode of the socket file is applied at `bind()` time from
//!    the process umask, so tighten it right after binding (0600 here). On
//!    Linux both the socket's mode and the search permission on every parent
//!    directory are checked on `connect()`.
//!
//! 3. CLEANUP. Nothing removes the node when the process exits. `SocketGuard`
//!    below unlinks it on `Drop`, which covers a normal return and a panic that
//!    unwinds - but *not* SIGINT/SIGTERM, which kill the process outright. A
//!    real daemon installs a signal handler that flips the same flag the accept
//!    loop watches; the stale-socket probe in (1) is the backstop for a SIGKILL.
//!
//! Authentication is available too, though not from stable `std` yet:
//! `UnixStream::peer_cred()` is still gated behind the unstable
//! `peer_credentials_unix_socket` feature (rust-lang/rust#42839). On stable the
//! same kernel-recorded uid/gid/pid is one `getsockopt` away, with `libc` as a
//! dependency:
//!
//! ```ignore
//! let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
//! let mut len = size_of::<libc::ucred>() as libc::socklen_t;
//! let rc = unsafe {
//!     libc::getsockopt(
//!         stream.as_raw_fd(),
//!         libc::SOL_SOCKET,
//!         libc::SO_PEERCRED,
//!         (&raw mut cred).cast(),
//!         &raw mut len,
//!     )
//! };
//! // rc == 0 -> cred.uid / cred.gid / cred.pid, stamped by the kernel at
//! // connect() time and unforgeable by the client.
//! ```
//!
//! That is the standard way for a session daemon to refuse commands coming from
//! another user. Here the 0600 mode on the socket already does that job, so the
//! example stays dependency-free and only logs the connection.
//!
//! Run the server in one terminal:
//!   cargo run --example unix_socket_ipc -- server
//!
//! and drive it from another:
//!   cargo run --example unix_socket_ipc -- client PING
//!   cargo run --example unix_socket_ipc -- client "SET theme dark" "GET theme"
//!   cargo run --example unix_socket_ipc -- client SHUTDOWN
//!
//! Any other client works too, which is handy while debugging:
//!   socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/xfce4-niri/ipc.sock
//!
//! Note for the record: Linux also has *abstract* sockets, whose name starts
//! with a NUL byte and lives in a separate namespace with no filesystem entry -
//! no stale nodes and no cleanup, but also no permission bits (only network
//! namespaces isolate them) and no portability. This example deliberately uses
//! the filesystem variant.

use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Where the socket lives. $XDG_RUNTIME_DIR is per-user and per-session and is
/// cleaned up at logout, which is exactly the lifetime an IPC socket wants.
fn socket_path() -> PathBuf {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/tmp/xfce4-niri-{}", current_uid())));

    runtime_dir.join("xfce4-niri").join("ipc.sock")
}

/// `std` has no uid accessor, and this example does not pull in `libc` just for
/// the fallback path above, so shell out to the one value we need.
fn current_uid() -> u32 {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Unlinks the socket path when the server drops it, so a normal exit (or an
/// unwinding panic) does not leave a stale node behind.
struct SocketGuard(PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Shared state the handlers mutate - stands in for whatever the real service
/// exposes (window list, presentation mode, ...).
type Store = Arc<Mutex<HashMap<String, String>>>;

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);

    match args.next().as_deref() {
        Some("server") => run_server(),
        Some("client") => {
            let commands: Vec<String> = args.collect();
            if commands.is_empty() {
                run_client(&["PING".to_string()])
            } else {
                run_client(&commands)
            }
        }
        _ => {
            eprintln!("usage: unix_socket_ipc <server|client [command ...]>");
            std::process::exit(2);
        }
    }
}

/* -------------------------------------------------------------------------- */
/* server                                                                      */
/* -------------------------------------------------------------------------- */

fn run_server() -> io::Result<()> {
    let path = socket_path();

    // bind() does not create directories; $XDG_RUNTIME_DIR/xfce4-niri may well
    // not exist yet on a fresh session.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    remove_stale_socket(&path)?;

    let listener = UnixListener::bind(&path)?;
    let _guard = SocketGuard(path.clone());

    // The mode comes from the umask at bind() time, so narrow it now: owner
    // only. Do it before announcing the socket, not after.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;

    println!("listening on {}", path.display());

    let store: Store = Arc::new(Mutex::new(HashMap::new()));
    let running = Arc::new(AtomicBool::new(true));

    // accept() blocks, so SHUTDOWN both flips the flag and pokes the listener
    // with a throwaway connection to make the loop come back around and notice.
    // A real daemon would use the same flag from its SIGTERM handler.
    for stream in listener.incoming() {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        let stream = match stream {
            Ok(stream) => stream,
            // A client that vanished between connect() and accept() is normal
            // traffic, not a reason to tear the server down.
            Err(e) if e.kind() == ErrorKind::ConnectionAborted => continue,
            Err(e) => return Err(e),
        };

        let store = Arc::clone(&store);
        let running = Arc::clone(&running);

        // One thread per connection keeps the accept loop responsive. For a
        // handful of clients this is cheaper and clearer than an event loop.
        thread::spawn(move || {
            if let Err(e) = handle_client(stream, store, &running) {
                eprintln!("connection error: {e}");
            }
        });
    }

    println!("shutting down");
    Ok(())
}

/// Decides whether an existing socket node belongs to a live server or is
/// leftover from one that died. See point (1) in the module docs.
fn remove_stale_socket(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    match UnixStream::connect(path) {
        // Somebody answered: this is a running server, so do not touch it.
        Ok(_) => Err(io::Error::new(
            ErrorKind::AddrInUse,
            format!("a server is already listening on {}", path.display()),
        )),
        // Nobody is listening on it any more - the node is stale.
        Err(e) if e.kind() == ErrorKind::ConnectionRefused => fs::remove_file(path),
        Err(e) => Err(e),
    }
}

fn handle_client(stream: UnixStream, store: Store, running: &AtomicBool) -> io::Result<()> {
    // This is where a daemon would check SO_PEERCRED (see the module docs) and
    // drop the connection if the peer uid is not its own.
    println!("client connected");

    // Reader and writer need separate handles; try_clone() dups the fd.
    let writer = stream.try_clone()?;
    let reader = BufReader::new(stream);
    let mut writer = io::BufWriter::new(writer);

    // lines() ends on EOF, which is what a client dropping its end looks like.
    for line in reader.lines() {
        let line = line?;
        let mut parts = line.trim().splitn(3, ' ');
        let reply = match parts.next() {
            Some("PING") => "PONG".to_string(),
            Some("SET") => match (parts.next(), parts.next()) {
                (Some(key), Some(value)) => {
                    store.lock().unwrap().insert(key.to_string(), value.to_string());
                    "OK".to_string()
                }
                _ => "ERR usage: SET <key> <value>".to_string(),
            },
            Some("GET") => match parts.next() {
                Some(key) => match store.lock().unwrap().get(key) {
                    Some(value) => format!("VALUE {value}"),
                    None => "NOTFOUND".to_string(),
                },
                None => "ERR usage: GET <key>".to_string(),
            },
            Some("LIST") => {
                let store = store.lock().unwrap();
                let keys: Vec<&str> = store.keys().map(String::as_str).collect();
                format!("KEYS {}", keys.join(","))
            }
            Some("QUIT") => {
                writeln!(writer, "BYE")?;
                writer.flush()?;
                break;
            }
            Some("SHUTDOWN") => {
                running.store(false, Ordering::SeqCst);
                writeln!(writer, "BYE")?;
                writer.flush()?;

                // Unblock accept(): the listener is still up, so this dummy
                // connect makes the loop iterate once more and see the flag.
                let _ = UnixStream::connect(socket_path());
                break;
            }
            Some("") | None => continue,
            Some(other) => format!("ERR unknown command: {other}"),
        };

        // BufWriter would otherwise sit on the reply until the buffer fills;
        // in a request/response protocol every reply has to be flushed.
        writeln!(writer, "{reply}")?;
        writer.flush()?;
    }

    println!("client disconnected");
    Ok(())
}

/* -------------------------------------------------------------------------- */
/* client                                                                      */
/* -------------------------------------------------------------------------- */

fn run_client(commands: &[String]) -> io::Result<()> {
    let path = socket_path();

    let stream = UnixStream::connect(&path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("cannot connect to {}: {e} (is the server running?)", path.display()),
        )
    })?;

    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    for command in commands {
        writeln!(writer, "{command}")?;
        writer.flush()?;

        let mut reply = String::new();
        if reader.read_line(&mut reply)? == 0 {
            eprintln!("server closed the connection");
            break;
        }

        println!("{command} -> {}", reply.trim_end());
    }

    Ok(())
}
