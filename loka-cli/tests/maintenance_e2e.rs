//! `loka serve --maintenance-idle-secs` really runs the background rebuild:
//! tombstone a vector over HTTP, go idle, and see the index rebuilt.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_loka");
const EMB: &str = "http://example.org/emb";

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn http(port: u16, method: &str, path: &str, body: &str) -> Option<(u16, String)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .ok()?;
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let text = String::from_utf8_lossy(&raw).to_string();
    let status = text.split_whitespace().nth(1)?.parse().ok()?;
    let body = text.split_once("\r\n\r\n").map(|(_, b)| b.to_string())?;
    Some((status, body))
}

fn ok(port: u16, method: &str, path: &str, body: &str) -> serde_json::Value {
    let (status, body) = http(port, method, path, body).expect("request");
    assert_eq!(status, 200, "{method} {path}: {body}");
    serde_json::from_str(&body).unwrap_or(serde_json::Value::Null)
}

#[test]
fn idle_server_rebuilds_a_tombstoned_index() {
    let port = free_port();
    let child = Command::new(BIN)
        .args([
            "serve",
            "--memory-only",
            "--port",
            &port.to_string(),
            "--maintenance-idle-secs",
            "1",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn loka serve");
    let _server = Server(child);
    let deadline = Instant::now() + Duration::from_secs(60);
    while http(port, "GET", "/health", "").map(|r| r.0) != Some(200) {
        assert!(Instant::now() < deadline, "server did not start");
        sleep(Duration::from_millis(200));
    }

    ok(
        port,
        "POST",
        "/vectors/declare",
        &format!(r#"{{"predicate":"{EMB}","dimensions":4}}"#),
    );
    for (s, v) in [("a", "[1,0,0,0]"), ("b", "[0,1,0,0]")] {
        ok(
            port,
            "POST",
            "/vectors",
            &format!(r#"{{"predicate":"{EMB}","subject":"http://example.org/{s}","vector":{v}}}"#),
        );
    }
    ok(
        port,
        "POST",
        "/retract",
        r#"{"iri":"http://example.org/a","commit":true}"#,
    );

    // Each health poll counts as activity, so poll slowly enough that the
    // server is idle (1 s) and the loop checks (every 1 s) in between.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        sleep(Duration::from_secs(3));
        let health = ok(port, "GET", "/vectors/health", "");
        if health["maintenance"]["rebuild_cycles"].as_u64() >= Some(1) {
            assert_eq!(health["indexes"][0]["total_nodes"], 1, "{health}");
            assert_eq!(health["indexes"][0]["active_nodes"], 1, "{health}");
            assert_eq!(health["maintenance"]["tombstones_removed"], 1, "{health}");
            break;
        }
        assert!(Instant::now() < deadline, "no rebuild: {health}");
    }
}
