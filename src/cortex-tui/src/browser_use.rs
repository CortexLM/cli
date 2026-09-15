//! Browser / computer-use surface (COR-371).
//!
//! Cortex has **no built-in browser or desktop-automation tool**. Driving a
//! browser is done by attaching an MCP server that provides those tools, and
//! every one of its tool calls goes through the same authority boundary as any
//! other MCP tool: the sandbox, the approval prompt, and the deny list.
//!
//! This module is the honest surface for that: it names the catalog entries that
//! provide browser automation, reports whether one is actually connected, and
//! refuses to claim a capability the CLI does not have. It also keeps the
//! existing "Computer" runtime concept (Cloud / This PC / SSH) distinct from
//! computer *use*, which is a different thing with a confusingly similar name.
//!
//! Nothing here drives a browser. It reports what is configured.

use serde::{Deserialize, Serialize};

/// Catalog entries that provide browser or desktop automation tools.
///
/// These are MCP servers the user installs; the CLI ships none of them.
pub const BROWSER_MCP_SERVERS: &[&str] = &["puppeteer"];

/// What a browser-automation capability is provided by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserCapability {
    /// No browser-automation server is connected.
    NotConnected,
    /// One or more named MCP servers are connected and provide it.
    ViaMcpServer(Vec<String>),
}

impl BrowserCapability {
    /// True when a browser can actually be driven right now.
    pub fn is_available(&self) -> bool {
        matches!(self, BrowserCapability::ViaMcpServer(servers) if !servers.is_empty())
    }

    /// Names of the connected servers, if any.
    pub fn servers(&self) -> &[String] {
        match self {
            BrowserCapability::NotConnected => &[],
            BrowserCapability::ViaMcpServer(servers) => servers,
        }
    }

    /// One-line status for the transcript.
    pub fn status_line(&self) -> String {
        match self {
            BrowserCapability::NotConnected => {
                "No browser automation is connected. Cortex ships no browser tool; \
install an MCP server that provides one, then its calls still pass the sandbox and approvals."
                    .to_string()
            }
            BrowserCapability::ViaMcpServer(servers) => format!(
                "Browser automation via {} — tool calls pass the same sandbox and approvals.",
                servers.join(", ")
            ),
        }
    }
}

/// One MCP server as this surface needs it: name plus whether it is running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserServer {
    /// Server name, e.g. `puppeteer`.
    pub name: String,
    /// Whether the server is currently running.
    pub running: bool,
    /// Number of tools the server exposes.
    pub tool_count: usize,
}

/// Resolve the browser capability from the connected MCP servers.
///
/// A server that is not running does not provide anything, so it is not counted:
/// reporting a stopped server as available would be a capability claim the CLI
/// cannot honour.
pub fn resolve_capability(servers: &[BrowserServer]) -> BrowserCapability {
    let running: Vec<String> = servers
        .iter()
        .filter(|server| server.running)
        .filter(|server| is_browser_server(&server.name))
        .map(|server| server.name.clone())
        .collect();
    if running.is_empty() {
        BrowserCapability::NotConnected
    } else {
        BrowserCapability::ViaMcpServer(running)
    }
}

/// True when `name` is a catalog entry that provides browser automation.
pub fn is_browser_server(name: &str) -> bool {
    BROWSER_MCP_SERVERS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(name))
}

/// The runtime a Code session runs on. Distinct from browser automation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerRuntime {
    /// Cloud runtime (the shipped default).
    Cloud,
    /// The local machine, opt-in via `CORTEX_COMPUTER`.
    ThisPc,
    /// An SSH host, opt-in via `CORTEX_SSH_HOST` / `CORTEX_SSH_TARGET`.
    Ssh,
}

impl ComputerRuntime {
    /// Product label, matching `cortex_engine::client::ComputerKind::label`.
    pub fn label(self) -> &'static str {
        match self {
            ComputerRuntime::Cloud => "Cloud",
            ComputerRuntime::ThisPc => "This PC",
            ComputerRuntime::Ssh => "SSH",
        }
    }
}

/// Copy that keeps "where tools run" separate from "driving a browser".
pub const RUNTIME_NOTE: &str =
    "Computer · where tools run (Cloud, This PC, SSH) — not browser or desktop automation.";

/// Copy that states the boundary plainly.
pub const NO_BUILTIN_TOOL_NOTE: &str = "Cortex ships no built-in browser or desktop tool. Browser automation comes from an MCP server you install.";

/// Narrow variant that still fits 40 columns.
pub const NO_BUILTIN_TOOL_NOTE_NARROW: &str =
    "Cortex ships no browser tool. Connect an MCP server.";

#[cfg(test)]
mod tests {
    use super::*;

    fn server(name: &str, running: bool) -> BrowserServer {
        BrowserServer {
            name: name.into(),
            running,
            tool_count: 12,
        }
    }

    #[test]
    fn no_connected_server_means_no_capability() {
        let capability = resolve_capability(&[]);
        assert!(!capability.is_available());
        assert_eq!(capability, BrowserCapability::NotConnected);
        assert!(capability.servers().is_empty());
        let status = capability.status_line();
        assert!(status.contains("No browser automation"), "{status}");
        assert!(
            status.contains("Cortex ships no browser tool"),
            "the status must not imply a built-in tool: {status}"
        );
    }

    #[test]
    fn a_running_browser_server_provides_the_capability() {
        let capability = resolve_capability(&[server("puppeteer", true)]);
        assert!(capability.is_available());
        assert_eq!(capability.servers(), ["puppeteer".to_string()]);
        let status = capability.status_line();
        assert!(status.contains("puppeteer"), "{status}");
        assert!(
            status.contains("sandbox") && status.contains("approvals"),
            "the status must state that calls stay governed: {status}"
        );
    }

    #[test]
    fn a_stopped_server_does_not_claim_a_capability() {
        let capability = resolve_capability(&[server("puppeteer", false)]);
        assert!(
            !capability.is_available(),
            "a stopped server provides nothing; claiming otherwise would be a false capability"
        );
        assert_eq!(capability, BrowserCapability::NotConnected);
    }

    #[test]
    fn unrelated_servers_are_not_browser_automation() {
        let capability = resolve_capability(&[
            server("github", true),
            server("filesystem", true),
            server("postgres", true),
        ]);
        assert!(!capability.is_available());
        assert_eq!(capability, BrowserCapability::NotConnected);
    }

    #[test]
    fn several_running_browser_servers_are_all_named() {
        let capability = resolve_capability(&[server("puppeteer", true), server("github", true)]);
        assert_eq!(capability.servers(), ["puppeteer".to_string()]);
    }

    #[test]
    fn browser_server_matching_is_case_insensitive() {
        assert!(is_browser_server("puppeteer"));
        assert!(is_browser_server("Puppeteer"));
        assert!(is_browser_server("PUPPETEER"));
        assert!(!is_browser_server("github"));
        assert!(!is_browser_server(""));
    }

    #[test]
    fn the_runtime_concept_stays_separate_from_computer_use() {
        assert_eq!(ComputerRuntime::Cloud.label(), "Cloud");
        assert_eq!(ComputerRuntime::ThisPc.label(), "This PC");
        assert_eq!(ComputerRuntime::Ssh.label(), "SSH");
        assert!(
            RUNTIME_NOTE.contains("not browser"),
            "the runtime note must disambiguate the two meanings: {RUNTIME_NOTE}"
        );
        assert!(
            NO_BUILTIN_TOOL_NOTE.contains("no built-in browser"),
            "{NO_BUILTIN_TOOL_NOTE}"
        );
        assert!(
            NO_BUILTIN_TOOL_NOTE_NARROW.contains("ships no browser tool"),
            "the narrow note must still deny a built-in tool: {NO_BUILTIN_TOOL_NOTE_NARROW}"
        );
    }

    #[test]
    fn the_shipped_server_list_names_only_catalog_entries() {
        // `puppeteer` is the one browser-automation entry in the local MCP
        // catalog; nothing else here may be invented.
        assert_eq!(BROWSER_MCP_SERVERS, ["puppeteer"]);
    }
}
