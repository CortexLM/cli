# Changelog

All notable changes to Cortex CLI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## Unreleased

### Changed
- Gray chrome with one accent, replacing the violet wash: the background is still never painted (`Color::Reset`), structure comes from gray — hairlines `#3A3A3A`, filled charcoal panels `#141414`, dim `#6B7280` secondary copy, white primary copy — and the Cortex violet `#A78BFA` appears only on the focused selection (the `>` caret and the selected label on the dark gray `#262626` bar; the bar is never a violet wash). Green `#4ADE80` covers `✓` success and `+N` diff additions; red and amber stay on diagnostics; the Thinking status is a muted gold. The `#221A38` wash and the interim cyan `#7DD3FC` highlight are banned everywhere
- The composer is the Devin-style bar in every session, working and queue state: a full-width thin gray hairline above the `> ` prompt and another below it, dim placeholder, white block cursor; it follows the transcript until the transcript fills the screen
- Past user turns sit on a full-width, slightly lighter gray bar behind `> prompt text`
- Login, trust, `/mode`, `/permissions`, permission prompts, plan / clear / delete confirms and questions are numbered pickers: `> 1 …` violet on the selected row, `· 2 …` white on the others, dim descriptions under the titles, `↑↓ select · ↵ confirm · esc …` hints; the sign-in screen reads `Welcome to Cortex CLI!` / `How would you like to log in?`
- `/model`, `/resume`, `/skills` and the settings hub frame their `/ Type to search` field with two hairlines; no pricing bar
- The footer is model left, shortcut hint right, all gray: `Cortex Mini 1 · Agent · 92% context` … `shift+tab to cycle modes` (the palette shows its own keys there); the first-run tips sit on a filled charcoal panel
- All 50 lock states recaptured at 40×12 and 120×40 on the gray chrome (raw, plus window-only macOS Terminal composites)
- Models show as English product names everywhere in the TUI — `Cortex Mini 1`, `Cortex 1`, `Cortex Max 1` — in the footer, `/model`, `/settings`, `/config`, session lists and the splash; served ids stay hyphenated internally
- Every 40×12 and 120×40 lock fixture carries whole copy: bodies wrap at word boundaries instead of stopping at a fragment, code spans keep a trailing space (`estimateTokens(prompt) counts`, `rateLimit() checks`), code excerpts keep their indentation, list rows keep their column gaps and end in an ellipsis when shortened
- Live session chrome stays complete in the empty, loading, error and no-match states (version, keystroke hints, composer, cwd + model footer); a live run says `Working · 0s · esc to interrupt` and the composer invites `Add a follow-up ↵ to queue`; the settings panel shows a real empty state; *The coding service is temporarily unavailable* is followed by what to do next
- New violet chrome lock, replacing the mint/`#1A3330` chrome: the background is never painted (`Color::Reset` — the host terminal shows through, black by default), the accent is `#A78BFA` violet on the `>` prompt, selection carets, `●` tile dots and `✓` checks, selection bars are light text on `#221A38` (never inverted), and green (`#4ADE80`) appears only on `+` diff additions — `#00F5D4` and `#00FFA3` are banned everywhere
- Zero rounded frames: the wide slash popup, inline forms and overlay widgets drop their `╭╮` / rounded borders; the TUI bleeds to the terminal edges
- All 50 lock states recaptured at 40×12 and 120×40 on the violet chrome (layout, copy and wrap rules unchanged), plus a second committed set compositing each capture into a photorealistic macOS Terminal.app window under `docs/media/tui-lock/macos/`

### Added
- Lock boards for typing, `/model` (compact + full), `/mode`, `/permissions`, working, and Read (states 02, 04–09) with captures at 40×12 and 120×40
- Code session turns against `POST /v1/code/sessions/{id}/turns` (streaming tokens, tool rows, Plan/Spec harness locks)
- Device login against `POST /v1/auth/device` on `api.cortex.foundation` (no silent guest substitution)
- This PC / Cloud / SSH computer label on the welcome card
- Workspace Code session id cache (`~/.cortex/code-sessions.json`)
- Guest session as an explicit TUI login choice

### Fixed
- Cancel aborts the local SSE stream and best-effort POSTs cancel (API route is still 404)
- Task without a live ModelClient reports failure instead of a fake success
- Remote tool rows keep their label instead of dropping arguments

### Changed
- Remaining TUI lock states recaptured on the locked gray/white chrome: mint stays on the `>` prompt, `●` success dots, `✓` checks and `+` diff additions; selection bars keep light text on `#1A3330`
- Session view accent aligned to the locked mint; user text, cursors, spinners and hint rows are gray/white
- Sign-in docs point at `api.cortex.foundation` device login
- Stream timeouts use the product-facing coding-service error

---

## 0.0.5

### Added
- Added interactive /agents command for agent management with full TUI support
- Added interactive /mcp panel for centralized MCP server management
- Added multi-step wizard for adding MCP servers with guided configuration
- Added --debug flag to write all trace logs to ./debug.txt for troubleshooting
- Added async loading panels for /billing and /account commands
- Added artifact system for large tool outputs to prevent payload errors
- Added real-time todos display for subagents during task execution
- Added mouse hover and click support for interactive mode
- Added scrollbar click-drag scrolling and hover interaction in TUI
- Added scrollbar support to dropdown menus
- Added TUI capture and snapshot testing framework for debugging
- Added TUI backtracking, file mentions, and external editor support (Ctrl+G)
- Added custom slash commands and agents system
- Added Plan Mode & Spec Mode system for structured task execution
- Added session favorites, tags, sharing, and search functionality
- Added multi-agent collaboration system with orchestrator support
- Added mandatory planning phase and final summary for orchestrator subagents
- Added hooks system with async events and LLM prompts
- Added advanced network proxy SSRF protection and sandbox modes
- Added session filtering and date filtering options
- Added styled output with theme-aware colors
- Added colorful styled help output with categorized options
- Added support for agent.md format (.agents/, .agent/) for agents and skills
- Added fault tolerance and robust defaults for corrupted config files

### Fixed
- Fixed agent creation to be fully automated when using --generate flag
- Fixed subagent max iterations increased from 10 to 500 for complex tasks
- Fixed subagent silent errors with improved logging
- Fixed subagent crash error messages now properly propagated
- Fixed false 'task crashed' errors due to race condition
- Fixed subagent fallback response when no text output is produced
- Fixed subagent display format with proper timer tracking
- Fixed 'Starting...' status changed to 'Processing request...' for clarity
- Fixed HTTP 413 payload errors by displaying todos separately
- Fixed frame capture rate limited to max 1 frame per second
- Fixed paste functionality in MCP server configuration modal
- Fixed MCP panel navigation to return after adding server
- Fixed MCP servers enabled by default after creation
- Fixed content ordering and streaming cursor display
- Fixed tool handler names aligned with registry expectations
- Fixed false session expiration during the last hour
- Fixed auth token reload into provider_manager after login
- Fixed expired token removal to prevent 'Already Logged In' dialog
- Fixed auto-redirect to login on session expiration
- Fixed scrollbar position to reflect viewport, not selection
- Fixed missing slash commands registration (init, commands, agents, share)
- Fixed full Unicode symbols preservation in TUI frame capture
- Fixed Windows Credential Manager 2560-byte BLOB size limit
- Fixed logout to clear all storage locations including fallback
- Fixed Windows keyring issues with fallback storage
- Fixed stale credentials cleared before saving new auth tokens
- Fixed context window configuration options exposed in default config
- Fixed model alias resolution in ACP server command
- Fixed dark green colors replaced with standard theme colors
- Fixed bold text color changed from blue to green in markdown
- Fixed AccessibilityHelp dialog no longer interferes with typing '?'
- Fixed terminal stream closure handling and memory leak prevention
- Fixed terminal duration metadata handling with defensive utilities

### Changed
- Changed MCP management from modal popup to inline card UI
- Changed CLI output to remove emojis for cleaner terminal display
- Changed error messages and logs to remove emojis
- Changed agents TUI with improved navigation
- Changed subagent display to Factory-style todos format
- Changed tool handlers to remove deprecated handlers and fix naming
- Changed 'droid' renamed to 'agent' throughout codebase
- Changed settings to persist in ~/.cortex on Linux/macOS
- Changed MCP server configurations to persist to local data directory
