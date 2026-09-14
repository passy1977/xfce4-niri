use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use xfce4_niri_lib::socket::Socket;
use xfce4_niri_lib::test_support::{EnvGuard, TempDir};

fn seen() -> &'static Mutex<Vec<Vec<String>>> {
    static SEEN: OnceLock<Mutex<Vec<Vec<String>>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(Vec::new()))
}

fn record(request: &[String]) -> xfce4_niri_lib::Result<()> {
    seen().lock().unwrap().push(request.to_vec());
    Ok(())
}

#[test]
fn the_server_answers_a_client() {

    let dir = TempDir::new();
    let mut env = EnvGuard::new();
    env.set("XDG_RUNTIME_DIR", dir.path());

    let path = xfce4_niri_lib::get_safe_path(Some("smoke.sock")).unwrap();

    let mut socket = Socket::new(path.clone());
    socket.start_server(&record).expect("start_server failed");

    // `start_server` binds on its own thread, so the socket node can lag a
    // moment behind the call returning.
    let deadline = Instant::now() + Duration::from_secs(1);
    while !path.exists() {
        assert!(Instant::now() < deadline, "the socket node never appeared");
        std::thread::sleep(Duration::from_millis(10));
    }

    // A second server on the same path has to be refused, not silently allowed.
    assert!(Socket::new(path.clone()).start_server(&record).is_err());

    // Each `run_client` call is one connection sending one joined line, the
    // way the CLI sends the one command it was invoked with.
    Socket::new(path.clone())
        .run_client(&["PING".to_string()])
        .expect("run_client failed");

    Socket::new(path.clone())
        .run_client(&["SET".to_string(), "a".to_string(), "b".to_string()])
        .expect("run_client failed");

    let observed = seen().lock().unwrap().clone();
    assert_eq!(observed, vec![vec!["PING".to_string()], vec!["SET".to_string(), "a".to_string(), "b".to_string()]]);

    socket.stop();
}
