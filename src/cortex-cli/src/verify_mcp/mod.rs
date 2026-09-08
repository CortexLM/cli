//! Hidden `cortex mcp-server --verify` — stdio JSON-RPC verification MCP.

use std::sync::Arc;

use anyhow::Result;
use cortex_mcp_server::{McpServerBuilder, ResourceProvider, ToolHandler};
use cortex_mcp_types::{CallToolResult, PropertySchema, Tool, ToolInputSchema};
use serde_json::Value;
use tokio::sync::Mutex;

mod api;
mod frame;
mod lock;
mod login;
mod report;
mod resources;
mod state;
mod tui;

use resources::VerifyResources;
use state::VerifyState;

/// Run the verification MCP on stdio. Logs stay on stderr.
pub async fn run() -> Result<()> {
    let state = Arc::new(Mutex::new(VerifyState::new()));
    let version = env!("CARGO_PKG_VERSION");
    let server = McpServerBuilder::new("cortex-verify", version)
        .with_tools_capability()
        .with_resources_capability()
        .instructions(
            "Hidden Cortex CLI verification MCP. Tools: tui.*, lock.*, login.run, api.*, mcp.*, report.finish. Report schema cortex-verify/1.",
        )
        .build()?;

    for spec in tool_specs() {
        server
            .register_tool(Arc::new(VerifyTool {
                spec,
                state: state.clone(),
            }))
            .await;
    }
    server
        .set_resource_provider(Arc::new(VerifyResources { state }) as Arc<dyn ResourceProvider>)
        .await;
    server.run_stdio().await
}

struct ToolSpec {
    name: &'static str,
    description: &'static str,
    schema: ToolInputSchema,
}

struct VerifyTool {
    spec: ToolSpec,
    state: Arc<Mutex<VerifyState>>,
}

#[async_trait::async_trait]
impl ToolHandler for VerifyTool {
    fn tool(&self) -> Tool {
        Tool::new(self.spec.name, self.spec.description).with_schema(self.spec.schema.clone())
    }

    async fn execute(&self, arguments: Value) -> Result<CallToolResult> {
        match dispatch(self.spec.name, arguments, &self.state).await {
            Ok(value) => Ok(CallToolResult::text(serde_json::to_string_pretty(&value)?)),
            Err(error) => Ok(CallToolResult::error(error.to_string())),
        }
    }
}

async fn dispatch(name: &str, args: Value, state: &Arc<Mutex<VerifyState>>) -> Result<Value> {
    let mut guard = state.lock().await;
    match name {
        "tui.start" => tui::start(&mut guard, &args),
        "tui.key" => tui::key(&mut guard, &args),
        "tui.type" => tui::type_text(&mut guard, &args),
        "tui.resize" => tui::resize(&mut guard, &args),
        "tui.frame" => tui::frame(&guard, &args),
        "tui.state" => tui::state_json(&guard, &args),
        "tui.assert" => tui::assert_frame(&mut guard, &args),
        "tui.slash" => tui::slash(&mut guard, &args),
        "tui.stop" => tui::stop(&mut guard, &args),
        "lock.list" => lock::list(&args),
        "lock.render" => lock::render(&mut guard, &args),
        "lock.diff_txt" => lock::diff_txt(&args),
        "lock.palette_audit" => lock::palette_audit(&mut guard, &args),
        "login.run" => login::run(&mut guard, &args).await,
        "api.models" => api::models(&mut guard, &args).await,
        "api.me" => api::me(&mut guard, &args).await,
        "api.turn" => api::turn(&mut guard, &args).await,
        "mcp.probe" => mcp_probe(&args).await,
        "mcp.call" => mcp_call(&args).await,
        "report.finish" => report::finish(&mut guard, &args),
        other => anyhow::bail!("unknown tool {other}"),
    }
}

async fn mcp_probe(args: &Value) -> Result<Value> {
    let server = args
        .get("server")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("server is required"))?;
    crate::mcp_cmd::debug::probe_named(server, 10).await
}

async fn mcp_call(args: &Value) -> Result<Value> {
    let server = args
        .get("server")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("server is required"))?;
    let tool = args
        .get("tool")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("tool is required"))?;
    let tool_args = args
        .get("args")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));
    crate::mcp_cmd::debug::call_named(server, tool, tool_args, 10).await
}

fn string_prop(desc: &str) -> PropertySchema {
    PropertySchema::string().description(desc)
}

fn int_prop(desc: &str) -> PropertySchema {
    PropertySchema::integer().description(desc)
}

fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "tui.start",
            description: "Start a headless TUI session",
            schema: ToolInputSchema::object()
                .property("width", int_prop("columns"))
                .property("height", int_prop("rows"))
                .property("entry", string_prop("cortex or agent"))
                .property("resumed", PropertySchema::boolean())
                .property("api_url", string_prop("API origin"))
                .property("credentials", string_prop("none, env, or keyring")),
        },
        ToolSpec {
            name: "tui.key",
            description: "Dispatch crossterm key names into the session",
            schema: ToolInputSchema::object()
                .property("session_id", string_prop("session from tui.start"))
                .property("keys", PropertySchema::array(PropertySchema::string()))
                .required(vec!["session_id", "keys"]),
        },
        ToolSpec {
            name: "tui.type",
            description: "Type text into the composer",
            schema: ToolInputSchema::object()
                .property("session_id", string_prop("session from tui.start"))
                .property("text", string_prop("characters to type"))
                .required(vec!["session_id", "text"]),
        },
        ToolSpec {
            name: "tui.resize",
            description: "Resize the headless terminal and reflow",
            schema: ToolInputSchema::object()
                .property("session_id", string_prop("session from tui.start"))
                .property("width", int_prop("columns"))
                .property("height", int_prop("rows"))
                .required(vec!["session_id", "width", "height"]),
        },
        ToolSpec {
            name: "tui.frame",
            description: "Capture the current frame as plain, ansi, or cells",
            schema: ToolInputSchema::object()
                .property("session_id", string_prop("session from tui.start"))
                .property("format", string_prop("plain, ansi, or cells"))
                .required(vec!["session_id"]),
        },
        ToolSpec {
            name: "tui.state",
            description: "Structured projection of the live AppState",
            schema: ToolInputSchema::object()
                .property("session_id", string_prop("session from tui.start"))
                .required(vec!["session_id"]),
        },
        ToolSpec {
            name: "tui.assert",
            description: "Assert text, cells, legend, and banned colours",
            schema: ToolInputSchema::object()
                .property("session_id", string_prop("session from tui.start"))
                .property("checks", PropertySchema::array(PropertySchema::object()))
                .required(vec!["session_id"]),
        },
        ToolSpec {
            name: "tui.slash",
            description: "Open the slash palette and return rows",
            schema: ToolInputSchema::object()
                .property("session_id", string_prop("session from tui.start"))
                .property("query", string_prop("slash query, e.g. /"))
                .required(vec!["session_id"]),
        },
        ToolSpec {
            name: "tui.stop",
            description: "Release a headless TUI session",
            schema: ToolInputSchema::object()
                .property("session_id", string_prop("session from tui.start"))
                .required(vec!["session_id"]),
        },
        ToolSpec {
            name: "lock.list",
            description: "List lock scene ids for a pack and width",
            schema: ToolInputSchema::object()
                .property("pack", string_prop("v1 or v2"))
                .property("width", int_prop("terminal width")),
        },
        ToolSpec {
            name: "lock.render",
            description: "Render a lock scene through production widgets",
            schema: ToolInputSchema::object()
                .property("pack", string_prop("v1 or v2"))
                .property("id", string_prop("scene id"))
                .property("width", int_prop("columns"))
                .property("height", int_prop("rows"))
                .required(vec!["id"]),
        },
        ToolSpec {
            name: "lock.diff_txt",
            description: "Unified diff of the live grid vs checked-in Designer txt",
            schema: ToolInputSchema::object()
                .property("id", string_prop("scene id"))
                .property("width", int_prop("columns"))
                .property("height", int_prop("rows"))
                .required(vec!["id"]),
        },
        ToolSpec {
            name: "lock.palette_audit",
            description: "Audit accent and banned colours across a lock pack",
            schema: ToolInputSchema::object()
                .property("pack", string_prop("v1 or v2"))
                .property("width", int_prop("columns"))
                .property("height", int_prop("rows"))
                .property("fixture", string_prop("optional violet-cell fixture")),
        },
        ToolSpec {
            name: "login.run",
            description: "Render login frames; unreachable yields product copy",
            schema: ToolInputSchema::object()
                .property("method", string_prop("browser or api_key"))
                .property("api_url", string_prop("device API origin"))
                .property(
                    "fixture",
                    string_prop("ok, unreachable, denied, expired, 429"),
                ),
        },
        ToolSpec {
            name: "api.models",
            description: "GET /v1/models through the CLI client",
            schema: ToolInputSchema::object(),
        },
        ToolSpec {
            name: "api.me",
            description: "GET /v1/me through the CLI client",
            schema: ToolInputSchema::object(),
        },
        ToolSpec {
            name: "api.turn",
            description: "Stream a Code/Chat turn; reports product errors offline",
            schema: ToolInputSchema::object()
                .property("session_id", string_prop("optional code session"))
                .property("message", string_prop("turn text"))
                .property("mode", string_prop("code or chat"))
                .property("cancel_after_ms", int_prop("optional cancel")),
        },
        ToolSpec {
            name: "mcp.probe",
            description: "Reuse cortex mcp debug --json against a configured server",
            schema: ToolInputSchema::object()
                .property("server", string_prop("configured MCP server name"))
                .required(vec!["server"]),
        },
        ToolSpec {
            name: "mcp.call",
            description: "Call a tool on a configured MCP server",
            schema: ToolInputSchema::object()
                .property("server", string_prop("configured MCP server name"))
                .property("tool", string_prop("tool name"))
                .property("args", PropertySchema::object())
                .required(vec!["server", "tool"]),
        },
        ToolSpec {
            name: "report.finish",
            description: "Write a cortex-verify/1 JSON report",
            schema: ToolInputSchema::object().property("run_id", string_prop("report file stem")),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn call(name: &str, args: Value) -> Result<Value> {
        let state = Arc::new(Mutex::new(VerifyState::new()));
        dispatch(name, args, &state).await
    }

    #[test]
    fn test_verify_tool_inventory_meets_floor() {
        let specs = tool_specs();
        assert!(specs.len() >= 20);
        let names: Vec<_> = specs.iter().map(|s| s.name).collect();
        for required in [
            "tui.start",
            "tui.key",
            "tui.type",
            "tui.resize",
            "tui.frame",
            "tui.state",
            "tui.assert",
            "tui.slash",
            "tui.stop",
            "lock.list",
            "lock.render",
            "lock.diff_txt",
            "lock.palette_audit",
            "login.run",
            "api.models",
            "api.me",
            "api.turn",
            "mcp.probe",
            "mcp.call",
            "report.finish",
        ] {
            assert!(names.contains(&required), "missing tool {required}");
        }
    }

    #[tokio::test]
    async fn unknown_tool_fails_closed() {
        let err = call("not.a.tool", json!({}))
            .await
            .expect_err("unknown tool");
        assert!(err.to_string().contains("unknown tool"));
    }

    #[tokio::test]
    async fn mcp_probe_and_call_require_names() {
        let probe = call("mcp.probe", json!({})).await.expect_err("server");
        assert!(probe.to_string().contains("server is required"));
        let call_err = call("mcp.call", json!({"server": "x"}))
            .await
            .expect_err("tool");
        assert!(call_err.to_string().contains("tool is required"));
    }

    #[tokio::test]
    async fn mcp_probe_unconfigured_server_is_not_success() {
        let err = call("mcp.probe", json!({"server": "missing-verify-peer"}))
            .await
            .expect_err("unconfigured");
        assert!(!err.to_string().is_empty());
    }

    #[tokio::test]
    async fn mcp_call_unconfigured_server_is_not_success() {
        let err = call(
            "mcp.call",
            json!({"server": "missing-verify-peer", "tool": "x", "args": {}}),
        )
        .await
        .expect_err("unconfigured");
        assert!(!err.to_string().is_empty());
    }

    #[tokio::test]
    async fn verify_tool_execute_reports_errors_as_tool_errors() {
        let spec = tool_specs()
            .into_iter()
            .find(|s| s.name == "mcp.probe")
            .expect("mcp.probe");
        let tool = VerifyTool {
            spec,
            state: Arc::new(Mutex::new(VerifyState::new())),
        };
        let listed = tool.tool();
        assert_eq!(listed.name, "mcp.probe");
        let result = tool.execute(json!({})).await.expect("execute");
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn verify_tool_execute_pretty_prints_success() {
        let spec = tool_specs()
            .into_iter()
            .find(|s| s.name == "lock.list")
            .expect("lock.list");
        let tool = VerifyTool {
            spec,
            state: Arc::new(Mutex::new(VerifyState::new())),
        };
        let result = tool
            .execute(json!({"pack": "v2", "width": 40}))
            .await
            .expect("execute");
        assert_ne!(result.is_error, Some(true));
        let text = match &result.content[0] {
            cortex_mcp_types::Content::Text { text, .. } => text,
            other => panic!("expected text content, got {other:?}"),
        };
        let parsed: Value = serde_json::from_str(text).expect("json");
        assert_eq!(parsed["pack"], "v2");
        assert!(parsed["ids"].as_array().is_some_and(|ids| !ids.is_empty()));
    }

    #[tokio::test]
    async fn dispatch_report_finish_writes_schema() {
        let value = call("report.finish", json!({"run_id": "unit-dispatch"}))
            .await
            .expect("finish");
        assert_eq!(value["schema"], "cortex-verify/1");
        assert_eq!(value["report"]["schema"], "cortex-verify/1");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn dispatch_covers_tui_lock_login_and_api_tools() {
        let state = Arc::new(Mutex::new(VerifyState::new()));
        let started = dispatch(
            "tui.start",
            json!({"width": 40, "height": 12, "entry": "cortex"}),
            &state,
        )
        .await
        .expect("start");
        let session_id = started["session_id"].as_str().expect("id").to_string();
        dispatch(
            "tui.type",
            json!({"session_id": session_id, "text": "hi"}),
            &state,
        )
        .await
        .expect("type");
        dispatch(
            "tui.key",
            json!({"session_id": session_id, "keys": ["a"]}),
            &state,
        )
        .await
        .expect("key");
        dispatch(
            "tui.resize",
            json!({"session_id": session_id, "width": 60, "height": 16}),
            &state,
        )
        .await
        .expect("resize");
        dispatch(
            "tui.frame",
            json!({"session_id": session_id, "format": "plain"}),
            &state,
        )
        .await
        .expect("frame");
        dispatch("tui.state", json!({"session_id": session_id}), &state)
            .await
            .expect("state");
        dispatch("tui.assert", json!({"session_id": session_id}), &state)
            .await
            .expect("assert");
        dispatch(
            "tui.slash",
            json!({"session_id": session_id, "query": "/"}),
            &state,
        )
        .await
        .expect("slash");
        dispatch("tui.stop", json!({"session_id": session_id}), &state)
            .await
            .expect("stop");

        dispatch("lock.list", json!({"pack": "v2", "width": 40}), &state)
            .await
            .expect("list");
        dispatch(
            "lock.render",
            json!({"pack": "v2", "id": "welcome-cortex", "width": 40, "height": 12}),
            &state,
        )
        .await
        .expect("render");
        dispatch(
            "lock.diff_txt",
            json!({"id": "welcome-cortex", "width": 40, "height": 12}),
            &state,
        )
        .await
        .expect("diff");
        let audit = dispatch(
            "lock.palette_audit",
            json!({"pack": "v2", "width": 40, "height": 12, "fixture": "violet-cell"}),
            &state,
        )
        .await;
        assert!(audit.is_err());

        let previous = std::env::var("CORTEX_API_URL").ok();
        unsafe { std::env::set_var("CORTEX_API_URL", "http://127.0.0.1:1") };
        dispatch(
            "login.run",
            json!({"fixture": "unreachable", "api_url": "http://127.0.0.1:1"}),
            &state,
        )
        .await
        .expect("login");
        dispatch("api.models", json!({}), &state)
            .await
            .expect("models");
        dispatch("api.me", json!({}), &state).await.expect("me");
        dispatch("api.turn", json!({"message": "ping"}), &state)
            .await
            .expect("turn");
        match previous {
            Some(url) => unsafe { std::env::set_var("CORTEX_API_URL", url) },
            None => unsafe { std::env::remove_var("CORTEX_API_URL") },
        }
    }
}
