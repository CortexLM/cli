# Slash commands

Type `/` in the [TUI](../guides/tui.md) composer to open the command list.
Everything below is built in; you can add your own as markdown files under
`.cortex/commands/`, and `/commands` lists those.

A line beginning with `/` that is not a known command is sent to the agent as an
ordinary message.

## General

| Command | Aliases | Usage |
|---------|---------|-------|
| `/help` | `h`, `?` | `/help [topic]` |
| `/quit` | `q`, `exit` | Quit |
| `/version` | `v` | Show version information |
| `/upgrade` | `update` | Check for and install updates |
| `/settings` | `config`, `prefs` | Open the settings panel |
| `/reload-config` | `reload` | Re-read configuration from disk |
| `/copy` | `cp` | How to copy text out |
| `/theme` | | `/theme [name]` |
| `/compact` | | Toggle compact display |
| `/palette` | `cmd` | Open the command palette |
| `/init` | | `/init [--force]` — write `AGENTS.md` |
| `/commands` | `cmds` | List custom commands |
| `/agents` | `subagents` | List and manage agents |
| `/delegates` | | `/delegates [action]` — manage subagents |
| `/tasks` | `bg`, `background` | Background tasks and agents |
| `/skills` | `sk` | List and manage skills |
| `/skill` | `invoke` | `/skill <name> [args...]` |
| `/skill-reload` | `sr` | Reload skills from disk |
| `/plugins` | `plugin` | `/plugins [action] [plugin-id]` |
| `/hooks` | | `/hooks [action]` |
| `/custom-commands` | `cc` | `/custom-commands [action]` |
| `/cost` | | Token usage and cost |
| `/ratelimits` | `limits`, `quota` | API rate limits and usage |
| `/experimental` | `exp`, `features` | `/experimental [feature] [--enable\|--disable]` |
| `/review` | | `/review [target] [--base=branch]` |
| `/multiedit` | `sed`, `replace` | `/multiedit <pattern> <replacement> [--glob=pattern]` |
| `/ghost` | | `/ghost [action]` — ghost commits for undo |
| `/spec` | | `/spec [off]` — toggle specification mode |
| `/bg-process` | | `/bg-process [action] [target]` |
| `/ide` | | Manage IDE integration |
| `/install-github-app` | | Install the Cortex GitHub App |
| `/bug` | | `/bug [description]` |

## Authentication and billing

| Command | Aliases | Usage |
|---------|---------|-------|
| `/login` | `signin` | Sign in |
| `/logout` | `signout` | Clear stored credentials |
| `/account` | `whoami`, `me` | Account information |
| `/billing` | `plan`, `subscription` | Billing status and credits |
| `/usage` | `stats`, `credits` | `/usage [--from YYYY-MM-DD] [--to YYYY-MM-DD]` |
| `/refresh` | `retry` | Refresh billing after adding payment |

## Session

| Command | Aliases | Usage |
|---------|---------|-------|
| `/session` | `info` | Current session details |
| `/clear` | `cls` | Clear the conversation |
| `/new` | `n` | Start a new session |
| `/resume` | `r`, `load` | `/resume [session-id]` |
| `/sessions` | `list`, `ls-sessions` | List sessions |
| `/fork` | `branch` | `/fork [name]` |
| `/rename` | `mv` | `/rename <name>` |
| `/favorite` | `fav`, `star` | Mark as favourite |
| `/unfavorite` | `unfav`, `unstar` | Remove the favourite mark |
| `/export` | `save` | `/export [format]` — Markdown, JSON or text |
| `/share` | | `/share [duration]` — for example `30d`, `24h`, `60m`, `never` |
| `/timeline` | `tl` | View the session timeline |
| `/rewind` | `rw` | `/rewind [steps]` |
| `/undo` | `u` | Undo the last action |
| `/redo` | | Redo |
| `/delete` | `rm` | `/delete [session-id]` |

## Navigation

| Command | Aliases | Usage |
|---------|---------|-------|
| `/diff` | `d` | `/diff [file]` |
| `/transcript` | `tr` | View the transcript |
| `/history` | `hist` | Command history |
| `/scroll` | | `/scroll <top\|bottom\|n>` |
| `/goto` | `g` | `/goto <n>` |

## Files and context

| Command | Aliases | Usage |
|---------|---------|-------|
| `/add` | `a`, `include` | `/add <file>...` |
| `/remove` | `rm-file`, `exclude` | `/remove <file>...` |
| `/search` | `find`, `grep` | `/search <pattern>` |
| `/ls` | `dir`, `files` | `/ls [path]` |
| `/tree` | | `/tree [path]` |
| `/mention` | `@`, `ref` | `/mention <file\|symbol>` |
| `/images` | `img`, `pics` | `/images <file>...` |
| `/context` | `ctx` | Show the current context files |

## Model and policy

| Command | Aliases | Usage |
|---------|---------|-------|
| `/models` | `m`, `lm`, `list-models` | `/models [name]` |
| `/approval` | `approve` | `/approval <ask\|session\|always\|never>` |
| `/sandbox` | `sb` | `/sandbox [on\|off]` |
| `/auto` | `autopilot` | `/auto [on\|off]` |
| `/temperature` | `temp` | `/temperature <0.0-2.0>` |
| `/tokens` | `max-tokens` | `/tokens <n>` |

## MCP

| Command | Aliases | Usage |
|---------|---------|-------|
| `/mcp` | | Interactive server management |
| `/mcp-tools` | `tools`, `lt` | List and manage MCP tools |
| `/mcp-auth` | `auth` | MCP authentication |
| `/mcp-reload` | | Reload MCP server configuration |

## Diagnostics

| Command | Aliases | Usage |
|---------|---------|-------|
| `/debug` | `dbg` | `/debug [on\|off]` |
| `/status` | `stat` | Application status |
| `/config` | `cfg` | `/config [key]` |
| `/logs` | `log` | `/logs [level]` |
| `/dump` | | `/dump [file]` |
| `/metrics` | `perf` | Performance metrics |
| `/diagnostics` | `diag`, `lint` | `/diagnostics [file]` |

## See also

- [The TUI](../guides/tui.md)
- [Keyboard shortcuts](keyboard.md)
- [CLI reference](cli.md) — the equivalents outside the TUI
