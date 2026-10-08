//! End to end through the agent installer: fresh install → serve → insert →
//! query → restart → query again.
//!
//! Drives the built `loka` binary exactly as an agent would: `install-agent
//! --json` in an empty directory, then the serve command it reports, then plain
//! HTTP. No mocks.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BIN: &str = env!("CARGO_BIN_EXE_loka");

/// Kills the server when the test ends, pass or fail.
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

fn fresh_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("loka-ia-e2e-{}-{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Minimal HTTP/1.1 request; returns (status code, body).
fn http(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Option<(u16, String)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .ok()?;
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    stream.write_all(req.as_bytes()).ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let text = String::from_utf8_lossy(&raw).to_string();
    let status = text.split_whitespace().nth(1)?.parse().ok()?;
    let body = text.split_once("\r\n\r\n").map(|(_, b)| b.to_string())?;
    Some((status, body))
}

fn start_server(dir: &Path, data_dir: &str, port: u16) -> Server {
    let child = Command::new(BIN)
        .args(["serve", "--port", &port.to_string(), "--data-dir", data_dir])
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn loka serve");
    let server = Server(child);
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if let Some((200, _)) = http(port, "GET", "/health", &[], "") {
            return server;
        }
        sleep(Duration::from_millis(200));
    }
    panic!("loka serve did not become healthy on port {port}");
}

fn query_value(port: u16) -> String {
    let (status, body) = http(
        port,
        "POST",
        "/sparql",
        &[
            ("Content-Type", "application/sparql-query"),
            ("Accept", "application/sparql-results+json"),
        ],
        "SELECT ?o WHERE { <http://example.org/e2e/a> <http://example.org/e2e/p> ?o }",
    )
    .expect("sparql request");
    assert_eq!(status, 200, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).expect("sparql json");
    let bindings = json["results"]["bindings"].as_array().expect("bindings");
    assert_eq!(bindings.len(), 1, "{body}");
    bindings[0]["o"]["value"].as_str().unwrap().to_string()
}

#[test]
fn fresh_install_insert_query_restart_query() {
    let dir = fresh_dir();
    let port = free_port();

    // 1. Install, as an agent would: structured output, no blocking server.
    let out = Command::new(BIN)
        .args([
            "install-agent",
            "e2e",
            "--json",
            "--port",
            &port.to_string(),
        ])
        .current_dir(&dir)
        .output()
        .expect("run install-agent");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("install-agent --json prints JSON");
    assert_eq!(report["name"], "e2e");
    assert_eq!(report["served"], false);
    assert_eq!(report["port"], port);
    let data_dir = report["data_dir"].as_str().unwrap().to_string();
    let notes = report["notes_file"].as_str().unwrap().to_string();
    assert!(dir.join(&data_dir).is_dir(), "data dir {data_dir} created");
    let notes_text = std::fs::read_to_string(dir.join(&notes)).expect("notes file written");
    assert!(notes_text.contains("e2e"), "notes file names the database");

    // 2. Serve the installed database, insert, query back.
    {
        let _server = start_server(&dir, &data_dir, port);
        let (status, body) = http(
            port,
            "POST",
            "/triples",
            &[("Content-Type", "application/n-triples")],
            "<http://example.org/e2e/a> <http://example.org/e2e/p> \"hello\" .\n",
        )
        .expect("insert request");
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("\"inserted\":1"), "{body}");
        assert_eq!(query_value(port), "hello");
        // Past the store's flush interval before the server is stopped.
        sleep(Duration::from_secs(3));
    }

    // 3. Restart on the same data dir: the triple is still there.
    {
        let _server = start_server(&dir, &data_dir, port);
        assert_eq!(query_value(port), "hello");
    }

    let _ = std::fs::remove_dir_all(&dir);
}
