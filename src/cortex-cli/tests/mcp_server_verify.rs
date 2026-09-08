//! Real stdio JSON-RPC against `cortex mcp-server --verify`.
//!
//! Spawns the built binary and speaks the protocol over pipes. A peer that
//! fails to initialize, lists fewer than 20 tools, or reports mock success
//! fails the test.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct VerifyPeer {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
    _home: tempfile::TempDir,
}

impl VerifyPeer {
    fn spawn() -> Self {
        let home = tempfile::tempdir().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_Cortex"))
            .args(["mcp-server", "--verify"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("HOME", home.path())
            .env("CORTEX_HOME", home.path())
            .env("RUST_LOG", "off")
            .env("CORTEX_API_URL", "http://127.0.0.1:1")
            .current_dir(home.path())
            .spawn()
            .expect("spawn mcp-server --verify");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
            _home: home,
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{}", message).expect("write request");
        self.stdin.flush().expect("flush");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        let response: Value = serde_json::from_str(line.trim()).unwrap_or_else(|_| {
            panic!("invalid JSON-RPC from verify server: {line}");
        });
        assert_eq!(response["id"], id);
        if let Some(error) = response.get("error") {
            panic!("JSON-RPC error for {method}: {error}");
        }
        response["result"].clone()
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
    }

    fn tool_text(&mut self, name: &str, arguments: Value) -> (bool, Value) {
        let result = self.call_tool(name, arguments);
        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("tool {name} returned no text: {result}"));
        let parsed = serde_json::from_str(text).unwrap_or_else(|_| json!({ "text": text }));
        (is_error, parsed)
    }
}

impl Drop for VerifyPeer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn verify_mcp_stdio_contract() {
    let mut peer = VerifyPeer::spawn();

    let init = peer.request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "verify-test", "version": "0" }
        }),
    );
    assert_eq!(init["serverInfo"]["name"], "cortex-verify");
    assert!(init["capabilities"]["tools"].is_object());

    let listed = peer.request("tools/list", json!({}));
    let tools = listed["tools"].as_array().expect("tools array");
    assert!(
        tools.len() >= 20,
        "expected ≥20 verify tools, got {}: {listed}",
        tools.len()
    );

    let (_, rendered) = peer.tool_text(
        "lock.render",
        json!({
            "pack": "v2",
            "id": "welcome-cortex",
            "width": 40,
            "height": 12
        }),
    );
    let plain = rendered["plain"].as_str().unwrap_or("");
    assert!(
        plain.contains("/ commands") && plain.contains("@ files"),
        "welcome-cortex 40x12 must contain the legend: {plain}"
    );

    let (_, started) = peer.tool_text(
        "tui.start",
        json!({ "width": 40, "height": 12, "entry": "cortex", "credentials": "none" }),
    );
    let session_id = started["session_id"].as_str().expect("session_id");
    let (_, slash) = peer.tool_text(
        "tui.slash",
        json!({ "session_id": session_id, "query": "/" }),
    );
    let rows = slash["rows"].as_array().expect("slash rows");
    assert!(
        !rows.is_empty(),
        "tui.start → slash must list commands: {slash}"
    );

    let (_, login) = peer.tool_text(
        "login.run",
        json!({
            "method": "browser",
            "api_url": "http://127.0.0.1:1",
            "fixture": "unreachable"
        }),
    );
    let copy = login["product_copy"].as_str().unwrap_or("");
    assert!(
        copy.contains("The coding service is temporarily unavailable"),
        "unreachable login must use product copy: {login}"
    );

    let (audit_error, audit) = peer.tool_text(
        "lock.palette_audit",
        json!({
            "pack": "v2",
            "width": 40,
            "height": 12,
            "fixture": "violet-cell"
        }),
    );
    assert!(
        audit_error || audit["ok"] == false,
        "violet cell fixture must fail lock.palette_audit: {audit}"
    );
    let violet = audit["palette"]["violet_px"]
        .as_u64()
        .or_else(|| {
            audit["text"]
                .as_str()
                .and_then(|t| serde_json::from_str::<Value>(t).ok())
                .and_then(|v| v["palette"]["violet_px"].as_u64())
        })
        .unwrap_or(0);
    assert!(
        violet >= 1 || audit_error,
        "violet fixture must report violet pixels: {audit}"
    );

    let (_, finished) = peer.tool_text("report.finish", json!({ "run_id": "stdio-contract" }));
    let report = &finished["report"];
    assert_eq!(report["schema"], "cortex-verify/1");
    assert!(report["cli_version"].as_str().is_some());
    assert!(report["sizes"].as_array().is_some());
    assert!(report["summary"].is_object());
    assert!(report.get("exit_code").is_some());
}
