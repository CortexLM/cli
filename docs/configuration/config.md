# Configuration files

Cortex reads a global config file, an optional project config file, and any
overrides you pass on the command line — in that order, with later sources
winning.

## Where the files live

### Global

The config directory is resolved as:

1. `CORTEX_CONFIG_DIR`, if set
2. `CORTEX_HOME`, if set
3. `~/.cortex`

The file is `config.toml` inside that directory. `CORTEX_CONFIG` overrides the
file path outright. TOML, JSON and JSONC are all accepted, picked by extension:
`config.toml`, `config.json`, `config.jsonc`.

### Project

Cortex walks up from the working directory to the project or git root looking for
the first of:

1. `.cortex/config.toml`
2. `.cortex/config.json`
3. `.cortex/config.jsonc`
4. `cortex.toml`
5. `cortex.json`
6. `cortex.jsonc`

Project settings are merged over global ones. Commit the project file when the
settings belong to the repository rather than to you.

### Command line

`-c` / `--config` takes `key=value` pairs and beats both files:

```bash
cortex -c model=<model-id> -c approval_policy=never
```

Values are parsed as TOML scalars, so booleans and numbers are not strings.

## Inspecting and editing

```bash
cortex config              # show the resolved configuration
cortex config --json       # ... as JSON
cortex config --edit       # edit interactively
cortex config get <key>
cortex config set <key> <value>
cortex config unset <key>
```

`cortex debug config --env` additionally reports which environment variables are
influencing the result, and `--diff` shows what differs from the defaults.

## Keys

### Model

| Key | Type | Notes |
|-----|------|-------|
| `model` | string | Default model |
| `model_provider` | string | Provider id |
| `model_context_window` | integer | Override the assumed context window |
| `model_auto_compact_token_limit` | integer | When to auto-compact the conversation |
| `model_reasoning_effort` | `low` \| `medium` \| `high` | |
| `model_reasoning_summary` | `none` \| `brief` \| `detailed` \| `auto` | |
| `model_aliases` | table | Map your own short names to model ids |
| `small_model` | string | Model used for cheap background work such as titles and summaries |
| `providers` | table | Custom provider definitions |

```toml
model = "your-model-id"

[model_aliases]
fast = "another-model-id"
```

### Permissions and sandboxing

| Key | Values | Meaning |
|-----|--------|---------|
| `approval_policy` | `untrusted`, `on-failure`, `on-request`, `never` | When to ask before running a tool. `on-request` is the default. |
| `sandbox_mode` | `read-only`, `workspace-write`, `danger-full-access` | What the sandbox permits. `workspace-write` is the default. |
| `trusted_directories` | array of paths | Directories that do not prompt on entry |

`sandbox_workspace_write` refines `workspace-write`:

```toml
sandbox_mode = "workspace-write"

[sandbox_workspace_write]
writable_roots = ["/tmp/scratch"]
network_access = false
exclude_tmpdir_env_var = false
exclude_slash_tmp = false
```

The `permission` table sets per-capability policy. Each value is `allow`, `ask`
or `deny`:

```toml
[permission]
edit = "ask"
webfetch = "allow"
doom_loop = "ask"
external_directory = "deny"

[permission.bash]
"git *" = "allow"
"rm *" = "deny"

[permission.skill]
"deploy" = "ask"

[permission.mcp]
"my-server" = "allow"
```

The equivalent command-line flags are `--ask-for-approval`, `--sandbox`,
`--full-auto` and `--dangerously-bypass-approvals-and-sandbox`. See
[CLI reference](../reference/cli.md#global-options).

### Behaviour

| Key | Type | Meaning |
|-----|------|---------|
| `instructions` | string | Extra instructions prepended to every session |
| `current_agent` | string | Agent used by default |
| `hide_agent_reasoning` | bool | Hide reasoning output |
| `show_raw_agent_reasoning` | bool | Show reasoning verbatim |
| `check_for_update_on_startup` | bool | Look for a new release on launch |
| `disable_paste_burst` | bool | Turn off paste-burst detection in the composer |

### History

```toml
[history]
persistence = "save-all"   # or "none"
max_bytes = 10000000
```

### TUI

```toml
[tui]
animations = true
notifications = true
# Enter the alternate screen (default). Set false to stay inline in the host
# terminal. Equivalent CLI flag: --alternate-screen
alternate_screen = true

[tui.theme]
name = "dark"              # dark, light, ocean_dark, monokai
```

See [Themes](../customization/themes.md).

### Execution

```toml
[execution]
max_agent_threads = 4
max_tool_threads = 8
command_timeout_seconds = 120
http_timeout_seconds = 60
max_retries = 3
retry_delay_ms = 500
streaming = true
max_file_size_bytes = 1048576
max_batch_files = 50
verbose = false
```

Each of these has a command-line equivalent: `--max-agent-threads`,
`--max-tool-threads`, `--command-timeout`, `--http-timeout`, `--no-streaming`.

### Extensions

| Key | Type | Points at |
|-----|------|-----------|
| `mcp_servers` | table | [MCP servers](../customization/mcp.md) |
| `plugins`, `plugin_dirs`, `plugin_settings` | table / array | [Plugins](../customization/plugins.md) |
| `commands` | table | Custom slash commands |

### Profiles

A profile is a named bundle of settings you can select with `-p` / `--profile`:

```toml
[profiles.review]
model = "your-model-id"
approval_policy = "never"
sandbox_mode = "read-only"
```

```bash
cortex --profile review "review the last three commits"
```

## Precedence

From strongest to weakest:

1. `-c` / `--config` overrides and other command-line flags
2. Environment variables
3. Project config file
4. Global config file
5. Built-in defaults

## Other files Cortex writes

| File | Purpose |
|------|---------|
| `~/.cortex/aliases.toml` | Command aliases created by `cortex alias` |
| `~/.cortex/session_locks.json` | Sessions protected by `cortex lock` |
| `.cortex/config.toml` | Project settings, also written by `cortex workspace set` |
| `AGENTS.md` | Project instructions, created by `cortex init` |

## See also

- [Environment variables](env.md)
- [Data locations](data-locations.md)
- [CLI reference](../reference/cli.md)
