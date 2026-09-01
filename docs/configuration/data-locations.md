# Data locations

Where Cortex keeps config, sessions, logs and caches.

The quickest way to see the real answer on your machine:

```bash
cortex debug paths
```

## Cortex home

Everything hangs off a single root, resolved as:

1. `CORTEX_CONFIG_DIR` (config only)
2. `CORTEX_HOME`
3. `~/.cortex`

```
~/.cortex/
├── config.toml        Global configuration
├── sessions/          Session transcripts
├── code-sessions.json Workspace → Code session id cache
├── agents/            Personal agents
├── skills/            Personal skills
├── plugins/           Installed plugins
├── mcp/               MCP server state
├── snapshots/         Workspace snapshots
├── cache/             Model, response and update caches
│   └── logs/          Application logs
├── auth/              Credential material not held in the keyring
├── feedback/          Queued feedback submissions
├── aliases.toml       Command aliases
└── session_locks.json Sessions protected from cleanup
```

Individual directories can be moved with `CORTEX_DATA_DIR` and
`CORTEX_CACHE_DIR`. See [Environment variables](env.md#locations).

## Project directory

Cortex also reads and writes inside the project you are working in:

| Path | Contents |
|------|----------|
| `AGENTS.md` | Project instructions, created by `cortex init` |
| `.cortex/config.toml` | Project configuration |
| `.cortex/agents/` | Project agents |
| `.cortex/skills/` | Project skills |
| `.cortex/plugins/` | Project plugins |
| `.cortex/commands/` | Project slash commands |
| `.agents/`, `.agent/` | Alternative agent and skill locations that are also scanned |
| `./debug.txt` | Written by `cortex --debug` |

Project files take priority over personal ones with the same name.

## Platform data directories

Some subsystems use the conventional per-platform application directory rather
than `~/.cortex`:

| Platform | Path |
|----------|------|
| Linux | `~/.local/share/Cortex/` |
| macOS | `~/Library/Application Support/Cortex/` |
| Windows | `%APPDATA%\Cortex\` |

The maintenance commands also read the platform cache directory —
`~/.cache/cortex/` on Linux — for logs and caches. Both honour
`CORTEX_DATA_DIR` and `CORTEX_CACHE_DIR`.

A legacy `~/.config/cortex` location is still read for configuration, agents and
skills so that older installs keep working.

## Credentials

Sign-in material lives in the OS keyring, under the service `cortex-cli` with
the account `auth` — Keychain on macOS, Secret Service on Linux, Credential
Manager on Windows. It is deliberately not a file in your home directory. See
[Signing in](../reference/login.md).

## Managing what accumulates

```bash
cortex compact status        # what could be reclaimed
cortex compact run           # compact logs, sessions and history
cortex cache size
cortex cache clear
cortex logs --paths          # where the logs are
cortex logs --clear
```

`cortex uninstall` removes the binary and, unless you pass `--keep-config` or
`--keep-data`, the directories above. `--dry-run` shows what it would delete and
`--backup` archives the data first.

## See also

- [Configuration files](config.md)
- [Environment variables](env.md)
- [Sessions](../guides/sessions.md)
