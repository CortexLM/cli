//! Loopback HTTP endpoints shared by the CLI subprocess contract tests.
//!
//! These never reach a real service. A [`Loopback`] answers only the exact
//! read-only GET paths it was given from local fixtures and refuses everything
//! else, so no test can perform or simulate a successful remote write.

// Shared by several test binaries; each uses a subset of the helpers.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};

pub struct Loopback {
    address: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Loopback {
    /// Serve `routes` (path without query string) and record every request
    /// line. Unknown paths and every non-GET method get `status`.
    pub fn start(routes: Vec<(String, serde_json::Value)>, status: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut line = String::new();
                let _ = BufReader::new(stream.try_clone().unwrap()).read_line(&mut line);
                let request = line.trim().to_string();
                recorded.lock().unwrap().push(request.clone());
                let body = request.strip_prefix("GET ").and_then(|rest| {
                    let path = rest.split_whitespace().next().unwrap_or("");
                    let path = path.split('?').next().unwrap_or("");
                    routes
                        .iter()
                        .find(|(route, _)| route == path)
                        .map(|(_, value)| value.to_string())
                });
                let response = match body {
                    Some(body) => format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                         content-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    ),
                    None => format!(
                        "HTTP/1.1 {status}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                    ),
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { address, requests }
    }

    /// An endpoint that refuses everything, so no remote read can succeed.
    pub fn refusing() -> Self {
        Self::start(Vec::new(), "503 Service Unavailable")
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.address.port())
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    /// Assert that nothing but read-only GET requests were ever attempted.
    pub fn assert_read_only(&self) {
        let requests = self.requests();
        assert!(
            requests.iter().all(|line| line.starts_with("GET ")),
            "a remote write was attempted: {requests:?}"
        );
    }
}
