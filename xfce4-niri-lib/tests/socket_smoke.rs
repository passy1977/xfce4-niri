use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use xfce4_niri_lib::lock::Lock;
use xfce4_niri_lib::socket::Socket;
use xfce4_niri_lib::test_support::{EnvGuard, TempDir};

fn seen() -> &'static Mutex<Vec<Vec<String>>> {
    static SEEN: OnceLock<Mutex<Vec<Vec<String>>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(Vec::new()))
}

fn record_request(request: &[String]) -> osal_rs::utils::Result<()> {
    seen().lock().unwrap().push(request.to_vec());
    Ok(())
}

fn ignore_request(_: &[String]) -> osal_rs::utils::Result<()> {
    Ok(())
}

/// `start_server` hands off to its accept-loop thread and returns before it
/// has necessarily bound the socket node, so give it a moment rather than
/// racing it.
fn wait_for(path: &std::path::Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        assert!(Instant::now() < deadline, "{} never appeared", path.display());
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_server_answers_a_client() {

    let dir = TempDir::new();
    let mut env = EnvGuard::new();
    env.set("XDG_RUNTIME_DIR", dir.path());

    let mut locked_by = None;
    let _lock = Lock::acquire(None, &mut locked_by).expect("cannot take the lock");

    let path = xfce4_niri_lib::get_safe_path(Some("smoke.sock")).unwrap();

    let mut socket = Socket::new(path.clone());
    socket
        .start_server(&record_request)
        .expect("start_server failed");

    wait_for(&path, Duration::from_secs(1));

    // A second server on the same path has to be refused, not silently allowed.
    assert!(Socket::new(path.clone()).start_server(&ignore_request).is_err());

    Socket::new(path.clone()).run_client(&["PING".to_string()]).expect("run_client failed");
    Socket::new(path.clone())
        .run_client(&["SET".to_string(), "a".to_string(), "b".to_string()])
        .expect("run_client failed");

    let seen = seen().lock().unwrap().clone();
    assert_eq!(seen, vec![vec!["PING".to_string()], vec!["SET".to_string(), "a".to_string(), "b".to_string()]]);

    socket.stop();
}
