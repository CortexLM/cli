//! Builder for MCP server selection.

use crate::interactive::state::{
    InlineFormField, InlineFormState, InteractiveAction, InteractiveItem, InteractiveState,
};
use crate::modal::mcp_manager::{McpServerInfo, McpStatus};

/// Build an interactive state for MCP server management.
///
/// Lock `/mcp`: servers only, status glyphs, `a` add / `r` reconnect in the
/// footer (not as list rows).
pub fn build_mcp_selector(servers: &[McpServerInfo]) -> InteractiveState {
    let items: Vec<InteractiveItem> = servers.iter().map(mcp_server_item).collect();

    let connected = servers
        .iter()
        .filter(|s| matches!(s.status, McpStatus::Running))
        .count();
    let banner = format!("MCP servers · {} of {} connected", connected, servers.len());

    InteractiveState::new("MCP Servers", items, InteractiveAction::McpServerAction)
        .with_banner(banner)
        .with_hints(vec![
            ("Enter".to_string(), "details".to_string()),
            ("r".to_string(), "reconnect".to_string()),
            ("a".to_string(), "add server".to_string()),
            ("Esc".to_string(), "close".to_string()),
        ])
}

fn mcp_server_item(server: &McpServerInfo) -> InteractiveItem {
    let (icon, description) = match server.status {
        McpStatus::Running => ('✓', format!("{} tools · connected", server.tool_count)),
        McpStatus::Starting => (
            '⠇',
            if server.requires_auth {
                "authenticating…".to_string()
            } else {
                "starting…".to_string()
            },
        ),
        McpStatus::Error => {
            let reason = server
                .error
                .as_deref()
                .map(trim_error)
                .filter(|s| !s.is_empty())
                .unwrap_or("connection lost");
            ('×', format!("failed — {reason} · r to reconnect"))
        }
        McpStatus::Stopped => ('○', "stopped · r to reconnect".to_string()),
    };

    let mut item = InteractiveItem::new(&server.name, &server.name)
        .with_icon(icon)
        .with_description(description);
    if server.requires_auth {
        item = item.with_metadata("requires_auth".to_string());
    }
    item
}

fn trim_error(error: &str) -> &str {
    error
        .strip_prefix("failed — ")
        .or_else(|| error.strip_prefix("failed: "))
        .unwrap_or(error)
        .trim()
}

/// Build a selector for choosing MCP server source (Custom or Registry).
/// This is the first step when adding an MCP server.
pub fn build_mcp_source_selector() -> InteractiveState {
    let items = vec![
        InteractiveItem::new("custom", "Custom Server")
            .with_description("Configure a server with command/URL manually")
            .with_shortcut('c'),
        InteractiveItem::new("registry", "From Registry")
            .with_description("Browse and install from MCP server registry")
            .with_shortcut('r'),
    ];

    InteractiveState::new(
        "Add MCP Server",
        items,
        InteractiveAction::Custom("mcp-source".to_string()),
    )
    .with_hints(vec![
        ("Enter".to_string(), "select".to_string()),
        ("Esc".to_string(), "back".to_string()),
    ])
}

/// Build a selector for choosing MCP transport type (stdio or HTTP).
/// This is shown after selecting "Custom Server".
pub fn build_mcp_transport_selector() -> InteractiveState {
    let items = vec![
        InteractiveItem::new("stdio", "stdio (Local Process)")
            .with_description("Run a local command (npx, uvx, binary)")
            .with_shortcut('s'),
        InteractiveItem::new("http", "HTTP (Remote Server)")
            .with_description("Connect to a remote MCP server via HTTP/SSE")
            .with_shortcut('h'),
    ];

    InteractiveState::new(
        "Transport Type",
        items,
        InteractiveAction::Custom("mcp-transport".to_string()),
    )
    .with_hints(vec![
        ("Enter".to_string(), "select".to_string()),
        ("Esc".to_string(), "back".to_string()),
    ])
}

/// Build an inline form for adding a stdio MCP server.
pub fn build_mcp_stdio_form() -> InlineFormState {
    InlineFormState::new("Add stdio Server", "mcp-add-stdio")
        .with_field(
            InlineFormField::new("name", "Name")
                .required()
                .with_placeholder("server-name"),
        )
        .with_field(
            InlineFormField::new("command", "Command")
                .required()
                .with_placeholder("npx, uvx, or path/to/binary"),
        )
        .with_field(InlineFormField::new("args", "Args").with_placeholder("arg1 arg2 ..."))
}

/// Build an inline form for adding an HTTP MCP server.
pub fn build_mcp_http_form() -> InlineFormState {
    InlineFormState::new("Add HTTP Server", "mcp-add-http")
        .with_field(
            InlineFormField::new("name", "Name")
                .required()
                .with_placeholder("server-name"),
        )
        .with_field(
            InlineFormField::new("url", "URL")
                .required()
                .with_placeholder("https://api.example.com/mcp"),
        )
        .with_field(InlineFormField::new("api_key", "API Key").with_placeholder("optional"))
}

/// Build a selector for browsing MCP registry (placeholder).
pub fn build_mcp_registry_browser() -> InteractiveState {
    // Placeholder data - real registry integration planned
    let items = vec![
        InteractiveItem::new("__coming_soon__", "Registry Coming Soon")
            .with_description("MCP registry integration is under development")
            .with_disabled(true),
    ];

    InteractiveState::new(
        "MCP Registry",
        items,
        InteractiveAction::Custom("mcp-registry".to_string()),
    )
    .with_hints(vec![("Esc".to_string(), "back".to_string())])
}

/// Build an inline form for adding a new MCP server (legacy - kept for compatibility).
/// This form is displayed within the MCP panel, not as a separate modal.
pub fn build_mcp_add_server_form() -> InlineFormState {
    build_mcp_stdio_form()
}

/// Build an interactive state for MCP server actions (for a specific server).
pub fn build_mcp_server_actions(server: &McpServerInfo) -> InteractiveState {
    let mut items = Vec::new();

    match server.status {
        McpStatus::Running | McpStatus::Starting => {
            items.push(InteractiveItem::new("stop", "Stop Server").with_shortcut('s'));
            items.push(InteractiveItem::new("restart", "Restart Server").with_shortcut('r'));
        }
        McpStatus::Stopped | McpStatus::Error => {
            items.push(InteractiveItem::new("start", "Start Server").with_shortcut('s'));
        }
    }

    if server.requires_auth {
        items.push(InteractiveItem::new("auth", "Configure Authentication").with_shortcut('a'));
    }

    items.push(InteractiveItem::new("logs", "View Logs").with_shortcut('l'));

    items.push(
        InteractiveItem::new("remove", "Remove Server")
            .with_shortcut('d')
            .with_description("Remove this server from configuration"),
    );

    InteractiveState::new(
        format!("Actions: {}", server.name),
        items,
        InteractiveAction::Custom(format!("mcp:{}", server.name)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_server(name: &str, status: McpStatus) -> McpServerInfo {
        McpServerInfo {
            name: name.to_string(),
            status,
            tool_count: 5,
            error: None,
            requires_auth: false,
        }
    }

    #[test]
    fn test_build_mcp_selector_empty() {
        let state = build_mcp_selector(&[]);
        assert!(state.items.is_empty());
        assert_eq!(
            state.banner.as_deref(),
            Some("MCP servers · 0 of 0 connected")
        );
        assert!(!state.searchable);
    }

    #[test]
    fn test_build_mcp_selector_with_servers() {
        let servers = vec![
            create_test_server("test1", McpStatus::Running),
            create_test_server("test2", McpStatus::Stopped),
        ];
        let state = build_mcp_selector(&servers);
        assert_eq!(state.items.len(), 2);
        assert_eq!(state.items[0].id, "test1");
        assert_eq!(state.items[0].icon, Some('✓'));
        assert!(
            state.items[0]
                .description
                .as_deref()
                .unwrap_or("")
                .contains("connected")
        );
        assert_eq!(state.items[1].icon, Some('○'));
        assert_eq!(
            state.banner.as_deref(),
            Some("MCP servers · 1 of 2 connected")
        );
    }

    #[test]
    fn error_row_offers_reconnect() {
        let mut server = create_test_server("sentry", McpStatus::Error);
        server.error = Some("token expired".into());
        let state = build_mcp_selector(&[server]);
        assert_eq!(state.items[0].icon, Some('×'));
        assert!(
            state.items[0]
                .description
                .as_deref()
                .unwrap_or("")
                .contains("failed — token expired · r to reconnect")
        );
    }

    #[test]
    fn test_build_mcp_server_actions() {
        let server = create_test_server("test", McpStatus::Running);
        let state = build_mcp_server_actions(&server);
        // Stop, Restart, Logs, Remove
        assert!(state.items.len() >= 4);
    }
}
