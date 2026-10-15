use std::sync::{Mutex, OnceLock};

use xfce4_niri_lib::lock::Lock;
use xfce4_niri_lib::socket::Socket;
use xfce4_niri_lib::test_support::{EnvGuard, TempDir};

fn seen() -> &'static Mutex<Vec<Vec<String>>> {
    static SEEN: OnceLock<Mutex<Vec<Vec<String>>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(Vec::new()))
}

#[test]
fn the_server_answers_a_client() {

    let dir = TempDir::new();
    let mut env = EnvGuard::new();
    env.set("XDG_RUNTIME_DIR", dir.path());

    let lock = Lock::acquire(None).expect("cannot take the lock");

    let path = xfce4_niri_lib::get_safe_path(Some("smoke.sock")).unwrap();

    let mut socket = Socket::new(path.clone());
    socket
        .start_server(&lock, osal_rs::os::Mutex::new_arc(|request: &[String]| {
            seen().lock().unwrap().push(request.to_vec());
        }))
        .expect("start_server failed");

    assert!(path.exists(), "the socket node must still be there after start_server");

    // A second server on the same path has to be refused, not silently allowed.
    assert!(Socket::new(path.clone()).start_server(&lock, osal_rs::os::Mutex::new_arc(|_: &[String]| {})).is_err());

    Socket::new(path.clone())
        .run_client(&lock, &["PING".to_string(), "SET a b".to_string()])
        .expect("run_client failed");

    let seen = seen().lock().unwrap().clone();
    assert_eq!(seen, vec![vec!["PING".to_string()], vec!["SET".to_string(), "a".to_string(), "b".to_string()]]);

    socket.stop();
}
