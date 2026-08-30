# CLI reference

Every command Cortex CLI exposes. The generated help is always authoritative —
`cortex --help` and `cortex <command> --help` come from the same definitions
this page describes.

```
cortex [OPTIONS] [PROMPT]
cortex [OPTIONS] <COMMAND> [ARGS]
```

With no subcommand and a terminal attached, Cortex starts the [TUI](../guides/tui.md).
A positional prompt seeds that session. Without a terminal, use
[`run` or `exec`](../guides/exec.md).

## Global options

| Flag | Description |
|------|-------------|
| `-c`, `--config <KEY=VALUE>` | Configuration override; repeatable |
| `-v`, `--verbose` | Verbose output, equivalent to `--log-level debug` |
| `--trace` | Trace-level logging |
| `--color <auto\|always\|never>` | Colour output. Default `auto`. |
| `-m`, `--model <MODEL>` | Model to use |
| `--oss` | Use local or open-source providers instead of the hosted API |
| `-p`, `--profile <NAME>` | Profile from `config.toml` |
| `-s`, `--sandbox <MODE>` | Sandbox policy: `read-only`, `workspace-write`, `danger-full-access` |
| `-a`, `--ask-for-approval <POLICY>` | Approval policy: `untrusted`, `on-failure`, `on-request`, `never` |
| `--full-auto` | Automatic execution inside the sandbox |
| `--dangerously-bypass-approvals-and-sandbox` | No prompts, no sandbox. Aliased `yolo`. |
| `-C`, `--cd <DIR>` | Working root for the agent |
| `--add-dir <DIR>` | Extra writable directory; repeatable |
| `-i`, `--image <PATH>` | Attach an image to the initial prompt |
| `--search` | Enable web search |
| `--max-agent-threads <N>` | Concurrent agent threads |
| `--max-tool-threads <N>` | Concurrent tool executions |
| `--command-timeout <SECONDS>` | Shell command timeout |
| `--http-timeout <SECONDS>` | HTTP request timeout |
| `--no-streaming` | Disable streaming responses |
| `-L`, `--log-level <LEVEL>` | `error`, `warn`, `info`, `debug`, `trace`. Default `info`. |
| `--debug` | Write all trace logs to `./debug.txt` |
| `-h`, `--help` | Help |
| `-V`, `--version` | Version |

## Running the agent

### `cortex run`

Non-interactive, streaming. Alias `r`.

```bash
cortex run [OPTIONS] [MESSAGE]...
```

| Flag | Description |
|------|-------------|
| `--command <COMMAND>` | Run a predefined command instead of a prompt |
| `-c`, `--continue` | Continue the most recent session |
| `-s`, `--session <ID>` | Continue a specific session |
| `--share` | Share the session and print the URL |
| `-m`, `--model <MODEL>` | Model, in `provider/model` form |
| `--agent <AGENT>` | Agent to use |
| `--format <default\|json\|jsonl>` | Output format. `--output` is an alias. |
| `-f`, `--file <PATH>` | Attach a file; repeatable |
| `--title <TITLE>` | Session title |
| `--attach <URL>` | Attach to a running server |
| `--port <PORT>` | Local server port |
| `-t`, `--temperature <N>` | 0.0–2.0 |
| `--top-p`, `--top-k`, `--seed` | Sampling controls |
| `-n`, `--notification` | Desktop notification when finished |
| `--stream` / `--no-stream` | Stream or buffer the response |
| `-C`, `--copy` | Copy the final response to the clipboard |
| `-o`, `--output-file <PATH>` | Write the final response to a file |
| `--cwd <PATH>` | Working directory |
| `--add-dir <DIR>` | Extra writable directory |
| `--timeout <SECONDS>` | 0 means no timeout |
| `--dry-run` | Preview without executing |
| `--max-tokens <N>` | Response cap |
| `--system <PROMPT>` | Custom system prompt |
| `--schema <PATH>` | JSON schema for structured output |
| `-q`, `--quiet` | Quiet output |
| `--no-progress`, `--no-cache` | |
| `--retry <N>` | Retry count |
| `--frequency-penalty`, `--presence-penalty`, `--stop`, `--logprobs`, `--n`, `--best-of` | Sampling controls |

### `cortex exec`

Headless execution for CI and scripts. Alias `e`. See
[Headless and one-shot runs](../guides/exec.md).

```bash
cortex exec [OPTIONS] [PROMPT]...
```

| Flag | Description |
|------|-------------|
| `-f`, `--file <PATH>` | Read the prompt from a file |
| `-o`, `--output-format <FORMAT>` | `text` (default), `json`, `stream-json`, `debug`, `stream-jsonrpc` |
| `--input-format <FORMAT>` | `text` (default) or `stream-jsonrpc` |
| `--auto <LEVEL>` | `read-only` (default), `low`, `medium`, `high` |
| `--skip-permissions-unsafe` | Bypass all permission checks. Conflicts with `--auto`. |
| `-m`, `--model <MODEL>` | Model |
| `--spec-model <MODEL>` | Model for specification mode |
| `--use-spec` | Start in specification mode |
| `-r`, `--reasoning-effort <LEVEL>` | Reasoning effort |
| `-s`, `--session-id <ID>` | Continue a session |
| `--enabled-tools <LIST>` | Comma-separated allow list |
| `--disabled-tools <LIST>` | Comma-separated deny list |
| `--list-tools` | Print the available tools and exit |
| `--cwd <PATH>` | Working directory |
| `--max-turns <N>` | Default 100 |
| `--timeout <SECONDS>` | Default 600 |
| `-i`, `--image <PATH>` | Attach an image; repeatable |
| `--system <PROMPT>` | Custom system prompt |
| `--max-tokens <N>` | Response cap |
| `--echo` | Include the prompt in the output |
| `--user <ID>` | User identifier for tracking |
| `--response-format <FORMAT>` | `text`, `json`, `json_object` |
| `--output-schema <SCHEMA>` | Inline JSON or a file path |
| `--url <URL>` | Fetch a URL into the context; repeatable |
| `--clipboard` | Read the clipboard into the context |
| `--git-diff` | Include the git diff |
| `--include <GLOB>` / `--exclude <GLOB>` | Filter files in the context; repeatable |
| `-v`, `--verbose` | Verbose output |
| `--frequency-penalty`, `--presence-penalty`, `--stop`, `--logprobs`, `-n`, `--best-of` | Sampling controls |

## Sessions

See [Sessions](../guides/sessions.md).

| Command | Description |
|---------|-------------|
| `cortex resume [SESSION_ID]` | Resume a session. `--last`, `--pick`, `--all`, `--no-session`. |
| `cortex sessions` | List sessions. `--all`, `--days`, `--since`, `--until`, `--favorites`, `-s/--search`, `-l/--limit`, `--json`. |
| `cortex export [SESSION_ID]` | Export. `-o/--output`, `-f/--format json\|yaml\|csv`, `--pretty`. |
| `cortex import <FILE_OR_URL>` | Import. `-f/--force`, `--resume`. `-` reads stdin. |
| `cortex delete <SESSION_ID>` | Delete. `-y/--yes`, `-f/--force`. |
| `cortex lock [SESSION_ID]` | Protect sessions from cleanup. Alias `protect`. Subcommands `add`, `remove`, `list`, `check`. |

## Authentication

See [Signing in](login.md).

| Command | Description |
|---------|-------------|
| `cortex login` | Sign in. `--with-api-key`, `--token <TOKEN>`, `--device-auth`, `--sso`. Subcommand `status`. |
| `cortex logout` | Sign out. `-y/--yes`, `--all`. |
| `cortex whoami` | Show the signed-in account. |

## Extensibility

### `cortex agent`

Manage agents. See [Agents](../customization/agents.md).

| Subcommand | Arguments |
|------------|-----------|
| `list` | `--json`, `--primary`, `--subagents`, `--all`, `--remote`, `--filter` |
| `show <name>` | `--json`, `--model` |
| `create` | `--name`, `-d/--description`, `--mode`, `--non-interactive`, `--generate <DESCRIPTION>`, `--model` |
| `edit <name>` | `-e/--editor` |
| `remove <name>` | `-f/--force` |
| `install <name>` | `-f/--force`, `--registry` |
| `copy <source> <destination>` | `-f/--force`. Alias `clone`. |
| `export <name>` | `-o/--output`, `--json` |

### `cortex mcp`

Manage MCP servers. See [MCP servers](../customization/mcp.md).

| Subcommand | Arguments |
|------------|-----------|
| `list` | `--json`, `--all`. Alias `ls`. |
| `get <name>` | `--json` |
| `add <name>` | `-f/--force`, `--allow-local`, `--env KEY=VALUE`, `--url <URL>`, `--bearer-token-env-var <ENV_VAR>`, `--sse <URL>`, `--sse-bearer-token-env-var <ENV_VAR>`, or `-- <command>...` for stdio |
| `remove <name>` | `-y/--yes`. Alias `rm`. |
| `enable <name>` / `disable <name>` | |
| `rename <old> <new>` | |
| `auth [name]` | `--client-id`, `--client-secret`. Subcommand `list`. |
| `logout [name]` | `--all` |
| `debug <name>` | `--json`, `--test-auth`, `--timeout`, `--no-cache`, `--show-cache-info` |

### `cortex plugin`

Manage plugins. Alias `plugins`. See [Plugins](../customization/plugins.md).

| Subcommand | Arguments |
|------------|-----------|
| `list` | `--json`, `--enabled`, `--disabled`. Alias `ls`. |
| `install <name>` | `-v/--version`, `-f/--force`. Alias `add`. |
| `remove <name>` | `-y`. Aliases `rm`, `uninstall`. |
| `enable <name>` / `disable <name>` | |
| `show <name>` | `--json`. Alias `info`. |
| `new <name>` | `-d`, `-a/--author`, `-o/--output`, `--advanced`, `--typescript`. Alias `create`. |
| `dev` | `-p/--path`, `-w/--watch`, `--debounce-ms` |
| `build` | `-p/--path`, `--debug`, `-o/--output` |
| `validate` | `-p/--path`, `--json`, `-v`. Alias `check`. |
| `publish` | `-p/--path`, `--dry-run`, `-o` |

### `cortex acp`

Start an Agent Client Protocol server for IDE integration.

`-C/--cwd`, `-p/--port` (0 means stdio), `--host`, `--stdio`, `-v/--verbose`,
`-m/--model`, `--agent`, `--allow-tool`, `--deny-tool`.

## Configuration

| Command | Description |
|---------|-------------|
| `cortex config` | Show configuration. `--json`, `--edit`. Subcommands `get <key>`, `set <key> <value>`, `unset <key>`. |
| `cortex models [PROVIDER]` | List models. `--json`. Subcommand `list` with `--limit`, `--offset`, `--sort`, `--full`. |
| `cortex features list` | Inspect feature flags. |
| `cortex init` | Write `AGENTS.md` in the current directory. `-f/--force`, `-y/--yes`. |

See [Configuration files](../configuration/config.md).

## Utilities

| Command | Description |
|---------|-------------|
| `cortex github` | GitHub integration. Alias `gh`. Subcommands `install`, `run`, `status`, `uninstall`, `update`. |
| `cortex pr <NUMBER>` | Check out a pull request. `-p/--path`, `-b/--branch`, `-F/--force`, `--info`, `--diff`, `--comments`, `--apply`, `--token`. |
| `cortex scrape <URL>` | Fetch a page as markdown, text or HTML. `-o/--output`, `-f/--format`, `--method`, `-t/--timeout`, `--retries`, `--user-agent`, `-H/--header`, `--cookie`, `--no-follow-redirects`, `--no-images`, `--no-links`, `--selector`, `--xpath`, `--pretty`. |
| `cortex stats` | Usage statistics. `-d/--days`, `-p/--provider`, `-m/--model`, `--json`, `-v`. |
| `cortex completion [SHELL]` | Shell completions for `bash`, `elvish`, `fish`, `powershell`, `zsh`. `--install`. |

## Maintenance

| Command | Description |
|---------|-------------|
| `cortex upgrade [VERSION]` | Update Cortex. `-c/--check`, `--changelog`, `-f/--force`, `-y/--yes`, `--channel`, `--pre`. |
| `cortex uninstall` | Remove Cortex. `-c/--keep-config`, `-d/--keep-data`, `--dry-run`, `-f/--force`, `-y/--yes`, `--backup`, `-p/--purge`. |
| `cortex compact` | Compaction and cleanup. Aliases `gc`, `cleanup`. Subcommands `run`, `logs`, `vacuum`, `status`, `config`. |
| `cortex cache` | Cache management. Subcommands `show`, `clear`, `size`, `list`. |
| `cortex logs` | Read logs. `-n`, `-f/--follow`, `-l/--level`, `-s/--session`, `--json`, `--paths`, `--clear`, `--keep-days`. |
| `cortex feedback [MESSAGE]` | Send feedback. Alias `report`. Subcommands `bug`, `good`, `bad`, `submit`, `history`. |
| `cortex alias` | Command aliases. Alias `aliases`. Subcommands `set`, `list`, `remove`, `show`. |

## Diagnostics

`cortex debug` groups the diagnostic commands. They are hidden from the main
help but supported.

| Subcommand | Reports |
|------------|---------|
| `config` | Resolved configuration. `--json`, `--env`, `--diff`. |
| `paths` | Where everything is on disk |
| `system` | Platform and environment details |
| `file <path>` | How a file resolves |
| `skill <name>` | How a skill resolves |
| `lsp` | Language server status |
| `ripgrep` | Search backend status |
| `snapshot` | Workspace snapshots. `--create`, `--restore`, `--snapshot-id`, `--description`, `--json`. |

Also hidden, and supported: `shell` (aliases `interactive`, `repl`), `dag`
(alias `tasks`), `servers`, `history`, `workspace` (alias `project`), `sandbox`
(alias `sb`), `serve`, and `mcp-server`.

## See also

- [Slash commands](slash-commands.md) — the in-TUI equivalents
- [Tools](tools.md)
- [Configuration files](../configuration/config.md)
- [Environment variables](../configuration/env.md)
