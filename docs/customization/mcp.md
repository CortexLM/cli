# MCP servers

Cortex is a Model Context Protocol client. Connecting an MCP server adds its
tools to the set the agent can call, alongside the [built-in tools](../reference/tools.md).

## Adding a server

### A local process (stdio)

Everything after `--` is the command Cortex launches:

```bash
cortex mcp add myserver -- npx @example/mcp-server
cortex mcp add myserver -- python -m my_server
cortex mcp add myserver --env API_HOST=example.com -- node server.js -v
```

The `--` matters. Without it, flags meant for the server (`-v`, `-m`) are parsed
as Cortex flags.

### A remote server (streamable HTTP)

```bash
cortex mcp add myapi --url https://mcp.example.com/mcp
cortex mcp add myapi --url https://mcp.example.com/mcp \
  --bearer-token-env-var MY_API_TOKEN
```

`--bearer-token-env-var` takes the **name** of an environment variable. Cortex
reads the token from it at runtime, so the token itself never lands in a config
file.

### A remote server (SSE)

```bash
cortex mcp add myevents --sse https://mcp.example.com/sse
cortex mcp add myevents --sse https://mcp.example.com/sse \
  --sse-bearer-token-env-var MY_API_TOKEN
```

### Local and private addresses

URLs pointing at `localhost`, `127.0.0.1` or private network ranges are rejected
by default. Pass `--allow-local` when you are deliberately talking to a
development server.

## Managing servers

```bash
cortex mcp list                  # alias: ls
cortex mcp list --all            # include disabled servers
cortex mcp list --json
cortex mcp get <name>
cortex mcp enable <name>
cortex mcp disable <name>
cortex mcp rename <old> <new>
cortex mcp remove <name>         # alias: rm
```

In the TUI, `/mcp` opens the manager, `/mcp-tools` lists the tools each server
exposes, and `/mcp-reload` re-reads the configuration. `Ctrl+E` opens the manager
directly.

## Authentication

For servers that speak OAuth:

```bash
cortex mcp auth <name>
cortex mcp auth <name> --client-id <id> --client-secret <secret>
cortex mcp auth list
cortex mcp logout <name>
cortex mcp logout --all
```

`/mcp-auth` covers the same ground inside the TUI.

## Diagnosing a server

```bash
cortex mcp debug <name>
cortex mcp debug <name> --test-auth
cortex mcp debug <name> --timeout 60 --no-cache
cortex mcp debug <name> --json
```

## Configuration format

`cortex mcp add` writes into the global `config.toml` under `mcp_servers`.

```toml
[mcp_servers.myserver]
enabled = true

[mcp_servers.myserver.transport]
type = "stdio"
command = "npx"
args = ["@example/mcp-server"]

[mcp_servers.myserver.transport.env]
API_HOST = "example.com"
```

```toml
[mcp_servers.myapi]
enabled = true

[mcp_servers.myapi.transport]
type = "http"
url = "https://mcp.example.com/mcp"
bearer_token_env_var = "MY_API_TOKEN"
```

```toml
[mcp_servers.myevents]
enabled = true

[mcp_servers.myevents.transport]
type = "sse"
url = "https://mcp.example.com/sse"
```

`type` is one of `stdio`, `http`, `sse` or `web_socket`. `enabled` defaults to
`true`.

Servers you add through the TUI are stored as one JSON file per server under the
platform data directory (`<data>/mcps/<name>.json`), with fields `name`,
`enabled`, `transport`, `command`, `args`, `env`, `url`, `api_key_env_var`,
`cwd` and `auto_start`.

## How MCP tools appear to the agent

A tool from server `myserver` called `search` is presented as
`mcp__myserver__search`. That prefix is what you use when filtering with
`--enabled-tools` or `--disabled-tools`, and what appears in the
`permission.mcp` table:

```toml
[permission.mcp]
"myserver" = "allow"
"risky-server" = "ask"
```

## Running Cortex as an MCP-adjacent server

`cortex acp` starts an Agent Client Protocol server for IDE integration:

```bash
cortex acp --stdio
cortex acp --port 8123 --host 127.0.0.1
cortex acp --allow-tool Read --allow-tool Grep --deny-tool Execute
```

## See also

- [Tools](../reference/tools.md)
- [Configuration files](../configuration/config.md) — the `permission` table
- [Plugins](plugins.md) — the other way to add tools
