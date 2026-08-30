# Environment variables

Every variable Cortex reads, by name. Values are never documented here — put
secrets in your OS keyring or your CI secret store, not in a shell profile that
gets committed or shared.

Environment variables sit between config files and command-line flags in
[precedence](config.md#precedence): a flag beats a variable, a variable beats a
config file.

## Locations

| Variable | Effect |
|----------|--------|
| `CORTEX_HOME` | Root for config and data. Default `~/.cortex`. Checked for writability at startup. |
| `CORTEX_CONFIG_DIR` | Config directory, taking priority over `CORTEX_HOME` |
| `CORTEX_CONFIG` | Path to a specific config file |
| `CORTEX_DATA_DIR` | Data directory (sessions, history) |
| `CORTEX_CACHE_DIR` | Cache directory |

See [Data locations](data-locations.md) for the defaults these override.

## Authentication

| Variable | Effect |
|----------|--------|
| `CORTEX_API_KEY` | API key, for headless and CI use |
| `CORTEX_AUTH_TOKEN` | Session or bearer token |
| `CORTEX_API_URL` | Base URL of the Cortex API. Defaults to `https://api.cortex.foundation`. |

Interactive use should prefer `cortex login`, which stores the session in the OS
keyring. See [Signing in](../reference/login.md).

## Model selection

| Variable | Effect |
|----------|--------|
| `CORTEX_MODEL` | Default model |
| `CORTEX_DEFAULT_MODEL` | Fallback default model |
| `CORTEX_PROVIDER` | Provider id. Defaults to `cortex`. |
| `CORTEX_MAX_TOKENS` | Default response length cap |
| `CORTEX_TEMPERATURE` | Default sampling temperature |
| `CORTEX_PRICING_<MODEL>` | Per-model price used by `cortex stats` |

## Logging and diagnostics

| Variable | Effect |
|----------|--------|
| `CORTEX_LOG_LEVEL` | Log verbosity when running a subcommand: `error`, `warn`, `info`, `debug`, `trace` |
| `RUST_LOG` | Overrides the tracing filter entirely when set |
| `RUST_BACKTRACE` | Standard Rust backtrace control |
| `NO_COLOR` | Disable colour. `--color never` sets this for child processes. |

## Server and sandbox

| Variable | Effect |
|----------|--------|
| `CORTEX_LISTEN_ADDR` | Listen address for the HTTP API server |
| `CORTEX_JWT_SECRET` | Signing secret for the HTTP API server |
| `CORTEX_MDNS_*` | mDNS advertisement settings for the HTTP API server |
| `CORTEX_SANDBOX`, `CORTEX_SANDBOX_*` | Injected into sandboxed child processes so they can tell they are sandboxed |
| `CORTEX_GIT_TIMEOUT_SECS` | Timeout for git operations |

## Set for you, not by you

These are populated by Cortex for the processes it spawns. Hook scripts and
plugins can read them; you should not set them yourself.

| Variable | Available to |
|----------|--------------|
| `CORTEX_FILE`, `CORTEX_SESSION_ID`, `CORTEX_MESSAGE_ID` | Hook commands |
| `CORTEX_PLUGIN_ARGS` | Plugin invocations |
| `CORTEX_CHILD_TASK`, `CORTEX_SPEC_MODE`, `CORTEX_SURFACE`, `CORTEX_OPERATION_MODE` | Internal task routing |

## Development and testing

| Variable | Effect |
|----------|--------|
| `CORTEX_LIVE_API` | Set to `1` to enable tests that hit the live API |
| `CORTEX_TUI_CAPTURE`, `CORTEX_TUI_CAPTURE_DIR`, `CORTEX_TUI_CAPTURE_ALL` | Capture TUI frames while the app runs |
| `CORTEX_DUMP_SNAPSHOTS` | Directory for dumped test snapshots |
| `CORTEX_CURSOR_BLINK` | Override cursor blink behaviour |
| `CORTEX_DEBUG` | Reported by `cortex debug config --env` |
| `CORTEX_GIT_HASH`, `CORTEX_BUILD_DATE` | Read at **compile** time and baked into `cortex --version` |

## Standard variables Cortex respects

| Variable | Used for |
|----------|----------|
| `EDITOR`, `VISUAL` | Opening an editor for `cortex agent edit`, `cortex workspace edit` and similar |
| `SHELL` | Shell completion setup and diagnostics |
| `HTTPS_PROXY`, `HTTP_PROXY` (and lowercase) | Outbound HTTP from `cortex scrape` |
| `SUDO_USER`, `SUDO_UID` | Resolving the real home directory when running under `sudo` |
| `PATH`, `TERM`, `LANG`, `LC_ALL`, `USER` | Diagnostics reported by `cortex debug system` |
| `XDG_*` | Legacy path resolution |

## Checking what is in effect

```bash
cortex debug config --env
cortex debug paths
```

## See also

- [Configuration files](config.md)
- [Data locations](data-locations.md)
- [CI secrets](../CI_SECRETS.md) — the secret names the release workflows expect
