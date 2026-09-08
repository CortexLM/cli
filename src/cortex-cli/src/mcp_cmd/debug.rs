//! Diagnostics use exactly the active engine client; no separate permissive probe.
use super::macros::safe_println;
use super::{
    config::get_mcp_server,
    types::{DebugArgs, ToolsArgs},
    validation::validate_server_name,
};
use anyhow::{Result, anyhow, bail};
use cortex_engine::mcp::{McpClient, McpServerConfig, TransportType};
use serde_json::json;
use std::io::Write;
use std::time::Duration;

pub(crate) async fn run_debug(args: DebugArgs) -> Result<()> {
    validate_server_name(&args.name)?;
    let server =
        get_mcp_server(&args.name)?.ok_or_else(|| anyhow!("MCP server is not configured"))?;
    let result = probe(&args.name, &server, args.timeout).await;
    let output = match &result {
        Ok(info) => {
            json!({"name":args.name,"connection":{"success":true},"capabilities":info["capabilities"],"tools":info["tools"],"resources":info["resources"],"prompts":info["prompts"],"cached":false})
        }
        Err(error) => {
            json!({"name":args.name,"connection":{"success":false},"error":error.to_string(),"cached":false})
        }
    };
    if args.json {
        safe_println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        safe_println!(
            "{}",
            if result.is_ok() {
                "MCP initialization and discovery succeeded"
            } else {
                "MCP initialization or discovery failed"
            }
        );
    }
    result.map(|_| ())
}

pub(crate) async fn run_tools(args: ToolsArgs) -> Result<()> {
    validate_server_name(&args.name)?;
    let server =
        get_mcp_server(&args.name)?.ok_or_else(|| anyhow!("MCP server is not configured"))?;
    let info = probe(&args.name, &server, args.timeout).await?;
    if args.json {
        safe_println!("{}", serde_json::to_string_pretty(&info["tools"])?);
    } else if let Some(tools) = info["tools"].as_array() {
        for tool in tools {
            safe_println!("{}", tool["name"].as_str().unwrap_or("unnamed"));
        }
        if tools.is_empty() {
            safe_println!("The server reports no tools");
        }
    }
    Ok(())
}

/// Probe a configured MCP server and return the JSON report used by `mcp debug --json`.
pub(crate) async fn probe_named(name: &str, timeout: u64) -> Result<serde_json::Value> {
    validate_server_name(name)?;
    let server = get_mcp_server(name)?.ok_or_else(|| anyhow!("MCP server is not configured"))?;
    match probe(name, &server, timeout).await {
        Ok(info) => Ok(json!({
            "name": name,
            "connection": {"success": true},
            "capabilities": info["capabilities"],
            "tools": info["tools"],
            "resources": info["resources"],
            "prompts": info["prompts"],
            "cached": false
        })),
        Err(error) => Ok(json!({
            "name": name,
            "connection": {"success": false},
            "error": error.to_string(),
            "cached": false
        })),
    }
}

async fn probe(name: &str, value: &toml::Value, timeout: u64) -> Result<serde_json::Value> {
    if timeout == 0 {
        bail!("MCP timeout must be positive");
    }
    let client =
        McpClient::with_timeout(runtime_config(name, value)?, Duration::from_secs(timeout));
    let result = async {
        client.connect().await?;
        let info = client.server_info().await.ok_or_else(|| anyhow!("MCP initialization did not complete"))?;
        let prompts = if info.capabilities.prompts.is_some() { client.list_prompts().await? } else { vec![] };
        Ok(json!({"capabilities":info.capabilities,"tools":client.tools().await,"resources":client.resources().await,"prompts":prompts}))
    }.await;
    let closed = client.disconnect().await;
    match result {
        Ok(info) => {
            closed?;
            Ok(info)
        }
        Err(error) => Err(error),
    }
}

pub(crate) async fn call_named(
    name: &str,
    tool: &str,
    arguments: serde_json::Value,
    timeout: u64,
) -> Result<serde_json::Value> {
    validate_server_name(name)?;
    let server = get_mcp_server(name)?.ok_or_else(|| anyhow!("MCP server is not configured"))?;
    if timeout == 0 {
        bail!("MCP timeout must be positive");
    }
    let started = std::time::Instant::now();
    let client =
        McpClient::with_timeout(runtime_config(name, &server)?, Duration::from_secs(timeout));
    let result = async {
        client.connect().await?;
        client.call_tool(tool, Some(arguments)).await
    }
    .await;
    let closed = client.disconnect().await;
    match result {
        Ok(call) => {
            closed?;
            Ok(json!({
                "result": call,
                "duration_ms": started.elapsed().as_millis(),
            }))
        }
        Err(error) => Err(error),
    }
}

fn runtime_config(name: &str, server: &toml::Value) -> Result<McpServerConfig> {
    if server.get("enabled").and_then(toml::Value::as_bool) == Some(false) {
        bail!("MCP server is disabled");
    }
    let transport = server
        .get("transport")
        .ok_or_else(|| anyhow!("MCP transport is required"))?;
    let get = |key| transport.get(key).or_else(|| server.get(key));
    let kind = transport
        .get("type")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| anyhow!("MCP transport type is required"))?;
    let mut config = McpServerConfig::new(name, "");
    config.transport = match kind {
        "stdio" => TransportType::Stdio,
        "http" | "streamable_http" | "streamable-http" => TransportType::Http,
        "sse" => bail!("Legacy MCP SSE is unsupported; configure Streamable HTTP"),
        _ => bail!("Unsupported MCP transport"),
    };
    if config.transport == TransportType::Stdio {
        config.command = get("command")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| anyhow!("MCP command is required"))?
            .into();
        if let Some(args) = get("args") {
            config.args = args
                .as_array()
                .ok_or_else(|| anyhow!("MCP args must be an array"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| anyhow!("MCP args must be strings"))
                })
                .collect::<Result<_>>()?;
        }
    } else {
        config.sse_url = Some(
            get("url")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| anyhow!("MCP URL is required"))?
                .into(),
        );
    }
    config.cwd = get("cwd").and_then(toml::Value::as_str).map(Into::into);
    for (key, target) in [("env", &mut config.env), ("headers", &mut config.headers)] {
        if let Some(values) = get(key) {
            for (name, value) in values
                .as_table()
                .ok_or_else(|| anyhow!("MCP {key} must be a table"))?
            {
                target.insert(
                    name.clone(),
                    value
                        .as_str()
                        .ok_or_else(|| anyhow!("MCP {key} values must be strings"))?
                        .into(),
                );
            }
        }
    }
    config.bearer_token_env_var = get("bearer_token_env_var")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_configuration_preserves_argument_boundaries_and_context() {
        let config: toml::Value = toml::from_str(
            r#"
            [transport]
            type = "stdio"
            command = "configured-command"
            args = ["one two", "", "three"]
            cwd = "/workspace"
            [transport.env]
            EXPLICIT_FIXTURE = "value"
        "#,
        )
        .unwrap();
        let parsed = runtime_config("fixture", &config).unwrap();
        assert_eq!(parsed.args, ["one two", "", "three"]);
        assert_eq!(parsed.env["EXPLICIT_FIXTURE"], "value");
        assert_eq!(parsed.cwd.unwrap().to_str(), Some("/workspace"));
    }
    #[tokio::test]
    async fn broken_peer_is_a_failed_probe_not_empty_success() {
        let value: toml::Value =
            toml::from_str("[transport]\ntype = 'stdio'\ncommand = '/nonexistent/mcp-fixture'")
                .unwrap();
        assert!(probe("fixture", &value, 1).await.is_err());
    }

    #[tokio::test]
    async fn probe_named_rejects_empty_and_missing_servers() {
        assert!(probe_named("", 1).await.is_err());
        assert!(probe_named("missing-verify-peer", 1).await.is_err());
    }

    #[tokio::test]
    async fn call_named_rejects_empty_missing_and_zero_timeout() {
        assert!(call_named("", "tool", json!({}), 1).await.is_err());
        assert!(
            call_named("missing-verify-peer", "tool", json!({}), 1)
                .await
                .is_err()
        );
        assert!(
            call_named("missing-verify-peer", "tool", json!({}), 0)
                .await
                .is_err()
        );
    }
}
