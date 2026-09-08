//! Hidden `cortex mcp-server` flags and dispatch.
//!
//! Kept out of [`super::args`] / [`super::handlers`] so those modules stay at
//! their source-policy line-count baseline (same split as `lock_palette`).

use anyhow::{Result, bail};
use clap::Parser;

/// Hidden `cortex mcp-server` flags. `--verify` is the offline TUI+API verifier.
#[derive(Debug, Parser)]
pub struct McpServerCli {
    /// Run the Cortex verification MCP over stdio JSON-RPC (`cortex-verify/1`).
    #[arg(long)]
    pub verify: bool,
}

/// Run `cortex mcp-server`, including the hidden `--verify` verifier.
pub async fn run(args: McpServerCli) -> Result<()> {
    if args.verify {
        #[cfg(feature = "cortex-tui")]
        {
            return crate::verify_mcp::run().await;
        }
        #[cfg(not(feature = "cortex-tui"))]
        {
            bail!("Verification MCP requires the cortex-tui feature.");
        }
    }
    bail!("MCP server mode is not yet implemented. Use 'cortex mcp' for MCP server management.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args::{Cli, Commands};
    use clap::CommandFactory;

    #[test]
    fn test_mcp_server_verify_stays_hidden() {
        let command = Cli::command();
        let mcp = command
            .find_subcommand("mcp-server")
            .expect("mcp-server must exist");
        assert!(mcp.is_hide_set(), "keep hide=true until Designer sign-off");
        let cli = Cli::try_parse_from(["cortex", "mcp-server", "--verify"])
            .expect("should parse hidden mcp-server --verify");
        match cli.command {
            Some(Commands::McpServer(args)) => assert!(args.verify),
            _ => panic!("expected McpServer --verify"),
        }
    }

    #[tokio::test]
    async fn mcp_server_without_verify_fails_closed() {
        let err = run(McpServerCli { verify: false })
            .await
            .expect_err("default mcp-server is not implemented");
        assert!(err.to_string().contains("not yet implemented"));
    }

    #[tokio::test]
    async fn dispatch_mcp_server_without_verify_fails_closed() {
        let cli = Cli::try_parse_from(["cortex", "mcp-server"]).expect("parse mcp-server");
        let err = crate::cli::handlers::dispatch_command(cli)
            .await
            .expect_err("dispatch must fail closed");
        assert!(err.to_string().contains("not yet implemented"));
    }
}
