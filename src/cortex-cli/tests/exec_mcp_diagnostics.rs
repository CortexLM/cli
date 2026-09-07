//! `cortex mcp debug` / `cortex mcp tools` against a real local stdio MCP peer.
//!
//! The peer is a scripted process speaking the actual protocol over pipes; no
//! network, and no stubbed success. A peer that fails to initialize must be
//! reported as a failed probe with a non-zero exit, never as an empty success.

use std::process::{Command, Output};

/// A minimal but genuine MCP server. `behaviour` selects the failure to prove.
const PEER: &str = r#"
import json, sys
behaviour = sys.argv[1]
if behaviour == "silent":
    # Accept input, answer nothing: initialization never completes.
    for _ in sys.stdin:
        pass
    sys.exit(0)
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    message = json.loads(line)
    if "id" not in message:
        continue
    method = message["method"]
    if method == "initialize":
        if behaviour == "bad-version":
            result = {"protocolVersion": "1999-01-01", "capabilities": {},
                      "serverInfo": {"name": "fixture", "version": "0"}}
        elif behaviour == "no-tools":
            result = {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}},
                      "serverInfo": {"name": "fixture", "version": "0"}}
        else:
            result = {"protocolVersion": "2024-11-05",
                      "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
                      "serverInfo": {"name": "fixture", "version": "0"}}
    elif method == "tools/list":
        result = {"tools": [] if behaviour == "no-tools" else
                  [{"name": "echo", "description": "repeat input",
                    "inputSchema": {"type": "object"}}]}
    elif method == "resources/list":
        result = {"resources": []}
    elif method == "prompts/list":
        result = {"prompts": [{"name": "greet", "description": "hello"}]}
    else:
        result = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": result}) + "\n")
    sys.stdout.flush()
"#;

struct Fixture {
    home: tempfile::TempDir,
}

impl Fixture {
    /// Configure one stdio server named `fixture` running the scripted peer.
    fn with_peer(behaviour: &str) -> Self {
        let home = tempfile::tempdir().unwrap();
        let script = home.path().join("peer.py");
        std::fs::write(&script, PEER).unwrap();
        std::fs::write(
            home.path().join("config.toml"),
            format!(
                "[mcp_servers.fixture.transport]\ntype = \"stdio\"\ncommand = \"python3\"\nargs = [{:?}, {:?}]\n",
                script.to_str().unwrap(),
                behaviour
            ),
        )
        .unwrap();
        Self { home }
    }

    fn with_config(config: &str) -> Self {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("config.toml"), config).unwrap();
        Self { home }
    }

    fn mcp(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_Cortex"))
            .arg("mcp")
            .args(args)
            .env("HOME", self.home.path())
            .env("CORTEX_HOME", self.home.path())
            .env("RUST_LOG", "off")
            .current_dir(self.home.path())
            .output()
            .unwrap()
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_working_peer_reports_its_real_capabilities_tools_and_prompts() {
    let fixture = Fixture::with_peer("ok");
    let output = fixture.mcp(&["debug", "fixture", "--json", "--timeout", "10"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["name"], "fixture");
    assert_eq!(report["connection"]["success"], true);
    assert_eq!(report["cached"], false, "diagnostics must never be cached");
    assert_eq!(report["tools"][0]["name"], "echo");
    assert!(report["capabilities"]["tools"].is_object());
    assert_eq!(
        report["prompts"][0]["name"], "greet",
        "prompts are listed only when the peer advertises the capability: {report}"
    );
}

#[test]
fn a_peer_that_never_initializes_is_a_failed_probe_with_a_non_zero_exit() {
    let fixture = Fixture::with_peer("silent");
    let output = fixture.mcp(&["debug", "fixture", "--json", "--timeout", "2"]);

    assert!(
        !output.status.success(),
        "a peer that never initializes must not exit successfully"
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["connection"]["success"], false);
    assert!(
        report["error"].as_str().is_some_and(|e| !e.is_empty()),
        "the failure must be explained: {report}"
    );
    assert!(
        report.get("tools").is_none(),
        "a failed probe must not publish a tool inventory: {report}"
    );
}

#[test]
fn an_unsupported_protocol_version_is_rejected_rather_than_negotiated_down() {
    let fixture = Fixture::with_peer("bad-version");
    let output = fixture.mcp(&["debug", "fixture", "--timeout", "5"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("MCP initialization or discovery failed"),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn tools_listing_distinguishes_an_empty_inventory_from_a_failure() {
    let empty = Fixture::with_peer("no-tools");
    let output = empty.mcp(&["tools", "fixture", "--timeout", "10"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("The server reports no tools"),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let working = Fixture::with_peer("ok");
    let output = working.mcp(&["tools", "fixture", "--timeout", "10"]);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "echo");

    let output = working.mcp(&["tools", "fixture", "--json", "--timeout", "10"]);
    let tools: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tools[0]["name"], "echo");

    // A broken peer must fail the command rather than print an empty list.
    let broken = Fixture::with_peer("silent");
    let output = broken.mcp(&["tools", "fixture", "--timeout", "2"]);
    assert!(!output.status.success());
}

#[test]
fn an_unconfigured_or_invalid_server_name_is_refused_before_any_process_starts() {
    let fixture = Fixture::with_peer("ok");
    let output = fixture.mcp(&["debug", "absent", "--timeout", "5"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("MCP server is not configured"),
        "stderr: {}",
        stderr(&output)
    );

    let output = fixture.mcp(&["tools", "../escape", "--timeout", "5"]);
    assert!(
        !output.status.success(),
        "an invalid server name must be rejected"
    );
}

#[test]
fn a_zero_timeout_is_refused_instead_of_meaning_no_limit() {
    let fixture = Fixture::with_peer("ok");
    let output = fixture.mcp(&["debug", "fixture", "--json", "--timeout", "0"]);
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["connection"]["success"], false);
    assert!(
        report["error"]
            .as_str()
            .unwrap()
            .contains("timeout must be positive"),
        "{report}"
    );
}

#[test]
fn malformed_or_unsupported_transport_configuration_fails_with_a_specific_reason() {
    for (config, expected) in [
        (
            "[mcp_servers.fixture]\nenabled = false\n[mcp_servers.fixture.transport]\ntype = \"stdio\"\ncommand = \"true\"\n",
            "MCP server is disabled",
        ),
        (
            "[mcp_servers.fixture]\ncommand = \"true\"\n",
            "MCP transport is required",
        ),
        (
            "[mcp_servers.fixture.transport]\nkind = \"stdio\"\n",
            "MCP transport type is required",
        ),
        (
            "[mcp_servers.fixture.transport]\ntype = \"sse\"\nurl = \"http://127.0.0.1:1\"\n",
            "Legacy MCP SSE is unsupported",
        ),
        (
            "[mcp_servers.fixture.transport]\ntype = \"carrier-pigeon\"\n",
            "Unsupported MCP transport",
        ),
        (
            "[mcp_servers.fixture.transport]\ntype = \"stdio\"\n",
            "MCP command is required",
        ),
        (
            "[mcp_servers.fixture.transport]\ntype = \"stdio\"\ncommand = \"true\"\nargs = \"one\"\n",
            "MCP args must be an array",
        ),
        (
            "[mcp_servers.fixture.transport]\ntype = \"stdio\"\ncommand = \"true\"\nargs = [1]\n",
            "MCP args must be strings",
        ),
        (
            "[mcp_servers.fixture.transport]\ntype = \"http\"\n",
            "MCP URL is required",
        ),
        (
            "[mcp_servers.fixture.transport]\ntype = \"stdio\"\ncommand = \"true\"\nenv = \"nope\"\n",
            "MCP env must be a table",
        ),
        (
            "[mcp_servers.fixture.transport]\ntype = \"stdio\"\ncommand = \"true\"\n[mcp_servers.fixture.transport.env]\nKEY = 1\n",
            "MCP env values must be strings",
        ),
    ] {
        let fixture = Fixture::with_config(config);
        let output = fixture.mcp(&["debug", "fixture", "--json", "--timeout", "5"]);
        assert!(
            !output.status.success(),
            "this configuration must not probe successfully: {config}"
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|_| panic!("no JSON report for: {config}"));
        assert_eq!(report["connection"]["success"], false);
        assert!(
            report["error"].as_str().unwrap().contains(expected),
            "expected {expected:?} for {config}, got {report}"
        );
    }
}

#[test]
fn a_streamable_http_endpoint_that_is_not_listening_fails_the_probe() {
    // Loopback port 1 is reserved; the probe cannot leave the machine.
    let fixture = Fixture::with_config(
        "[mcp_servers.fixture.transport]\ntype = \"streamable-http\"\nurl = \"http://127.0.0.1:1/mcp\"\n",
    );
    let output = fixture.mcp(&["debug", "fixture", "--json", "--timeout", "3"]);
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["connection"]["success"], false);
}
