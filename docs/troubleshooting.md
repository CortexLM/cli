# Troubleshooting

## Start here

```bash
cortex --version
cortex whoami
cortex debug paths       # where config, sessions, logs and caches are
cortex debug config      # the configuration actually in effect
cortex debug system      # platform and environment details
cortex logs -n 200       # recent log output
```

For a single run, `-v` gives more detail, `--trace` gives a lot more, and
`--debug` writes every trace log to `./debug.txt` in the working directory.

## "The coding service is temporarily unavailable"

Cortex could not reach the coding API. This message is deliberately the whole
story — the CLI does not surface provider, SDK or transport names.

Work through:

1. Can you reach `api.cortex.foundation` from this machine?
2. Is a proxy in the way? `HTTPS_PROXY` and `HTTP_PROXY` are honoured for
   `cortex scrape`, but corporate TLS interception can still break other calls.
3. Is `CORTEX_API_URL` set to something unexpected? `cortex debug config --env`
   will tell you.
4. Are you signed in? `cortex whoami`.

## The TUI will not start

Cortex needs a terminal on both stdin and stdout. In a pipeline, a CI job, or
under a wrapper that redirects either, it refuses to start and points you at
[`cortex run` or `cortex exec`](guides/exec.md).

## Sign-in problems

See [Signing in — troubleshooting](reference/login.md#troubleshooting).

## The agent will not change anything

Check, in order:

1. **Operation mode.** `PLAN` is read-only. Cycle to `BUILD`.
2. **Sandbox.** `--sandbox read-only` blocks all writes. `workspace-write`
   confines them to the workspace.
3. **Approval policy.** With `--ask-for-approval never` and no autonomy, the
   agent may be silently declining rather than prompting.
4. **The `permission` table** in `config.toml` can deny `edit` or specific bash
   patterns outright.
5. **Agent tool access.** An agent with `tools: read-only` cannot write, whatever
   the session policy says.
6. **Specification mode.** Mutating tools stay locked until the plan is accepted.

`cortex debug config --diff` shows what differs from the defaults.

## A tool is not available

```bash
cortex exec --list-tools
```

If a tool is missing, something narrowed the set: `--enabled-tools` or
`--disabled-tools`, the agent's `tools` field, the current mode, or the
`permission` table. See [Tools — what is available when](reference/tools.md#what-is-available-when).

## An MCP server is not connecting

```bash
cortex mcp list --all          # is it there, and is it enabled?
cortex mcp debug <name>        # try the connection
cortex mcp debug <name> --test-auth --no-cache
```

Common causes:

- The stdio command is not on `PATH` in the environment Cortex launches it from.
- A `--` was missing from `cortex mcp add`, so the server's own flags were parsed
  as Cortex flags.
- The URL points at localhost or a private range and needs `--allow-local`.
- The bearer token environment variable named in the config is not set in the
  environment Cortex is running in.

## A run times out

`cortex exec` defaults to a 600-second timeout and 100 turns.

```bash
cortex exec --timeout 1800 --max-turns 40 "large task"
```

If a single shell command is the problem rather than the whole run, raise
`--command-timeout` or `execution.command_timeout_seconds` instead.

Breaking a large task into several smaller runs is usually better than raising
the ceiling.

## Output is being truncated or mangled

- `--color never` for logs and pipelines, or set `NO_COLOR`.
- `-o json` or `-o stream-json` when something downstream parses the output.
  The shape of `text` output is not a contract.

## Disk usage keeps growing

```bash
cortex compact status
cortex compact run
cortex cache size
cortex cache clear
cortex logs --clear --keep-days 7
```

Sessions you want to keep should be locked first, so cleanup skips them:

```bash
cortex lock add <SESSION_ID> -r "keep for the audit"
```

## A build from source fails

On Linux, the optional audio and desktop crates need ALSA headers:

```bash
sudo apt-get install -y libasound2-dev pkg-config
```

Use the toolchain pinned in [`rust-toolchain.toml`](../rust-toolchain.toml).

## Reporting a problem

```bash
cortex feedback bug "describe what happened" --include-logs
```

Or open an issue — see [Contributing](CONTRIBUTING.md) for what to include.

## See also

- [Configuration files](configuration/config.md)
- [Environment variables](configuration/env.md)
- [Data locations](configuration/data-locations.md)
