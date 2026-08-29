//! MCP (Model Context Protocol) command handlers.

use super::CommandExecutor;
use crate::commands::types::{CommandResult, ModalType, ParsedCommand};

impl CommandExecutor {
    pub(super) fn cmd_mcp(&self, _cmd: &ParsedCommand) -> CommandResult {
        // Always open the interactive MCP panel - all management is centralized there
        CommandResult::OpenModal(ModalType::McpManager)
    }
}
