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

    #[test]
    fn test_verify_tool_inventory_meets_floor() {
        assert!(tool_specs().len() >= 20);
    }
}
