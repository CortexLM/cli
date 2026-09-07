# Cortex executable plugin host

This is the **plugin runtime**, not the separate session/client TypeScript SDK.
The private package is embedded in Cortex CLI; its release version follows
`VERSION_CLI`. The independent wire/ABI protocol is **1**.

## Supported authoring path

Node **22.13+ in the Node 22 LTS line** is required. Cortex strips *erasable*
TypeScript from `src/index.ts` into the manifest's `.mjs` entrypoint:

```sh
Cortex plugin new example --typescript
Cortex plugin build --path example
Cortex plugin validate --path example --json
Cortex plugin install ./example --trust-code
Cortex plugin run example hello Ada --json
Cortex plugin run example --tool greet --input '{"name":"  Ada  "}' --json
Cortex plugin run example --tool greet --input '{"name":"blocked"}' --json
Cortex plugin disable example
```

The greeting tool trims the input through a real before-hook. `blocked` is a
veto and exits nonzero. Session-start produces a validated notification in the
command result. The in-tree example lives at
[`examples/plugins/hello-typescript`](../../examples/plugins/hello-typescript).

The build calls Node's `module.stripTypeScriptTypes` in `strip` mode, **not**
`tsc`, `wasm-pack`, npm, npx or a plugin-provided build script. This single-file
workflow does **not type-check**, resolve dependencies, transform enums,
namespaces or parameter properties, rewrite imports, or produce WASM.
Precompiled `.mjs` packages may also be installed. Validation checks artifact
existence, manifest, syntax and supported contracts without executing Node
modules; initialization/export correctness is checked by `plugin run`.
`dev --watch` and arbitrary TUI widgets are explicitly unsupported.

## Trust and permissions

A Node process is **not a sandbox**. Only explicitly trusted native code may run.
Trust is pinned to a SHA-256 fingerprint of packaged files in
`~/.cortex/plugins.json`; changes invalidate that trust. `plugin trust ID --yes`
renews it. Trust does not transfer automatically to a changed update.

Trusted Node code can use filesystem, network and subprocess APIs directly.
The host clears inherited environment variables, but that is **not** isolation
from files, credentials stored on disk, other processes, or the network.
Dependencies outside the packaged files are not integrity-pinned by Cortex.
No repository-local package automatically becomes trusted. Default discovery is
only `~/.cortex/plugins`, sorted by path. Additional roots require explicit host
configuration; first-discovered IDs take precedence. Enablement and trust are
stored together; `enabled` inside `plugin.toml` is not an activation switch.

There are no filesystem, shell, network, credentials or approval-grant RPCs.
Only bounded notification effects are supported and require manifest
`permissions = ["notifications"]`. Other permission declarations do not grant
host effects. Hooks cannot return `allow` or replace a failed tool with success.
For automated tool execution, the Rust caller must still apply its existing
schema/path validation and approval/execpolicy/sandbox gate **after** mutation.

## Protocol 1

One JSON object per line, at most **1 MiB**, one outstanding request per process:

```json
{"protocol":1,"id":2,"method":"command","name":"hello","input":["Ada"],"context":{"session_id":"s","cwd":"/workspace","extra":{"call_id":"c"}}}
```

Replies echo protocol and ID and contain either `result` or a product-facing
`error`. Handshake binds plugin ID/version, CLI version, entrypoint SHA-256, and
exact manifest command/tool/hook exports. A default module exports:

```js
export default {
  protocol: 1,
  init: context => ({ data: null }),
  shutdown: context => ({ data: null }),
  commands: { hello: (args, context) => ({ data: args }) },
  tools: {},
  hooks: {}
};
```

Invocation results are `{data, notifications?: [{level, message}]}`.
Notifications are limited to 16 per invocation, 4 KiB each, four known levels,
and no terminal-control characters. Hooks return their decision inside `data`:
`{decision:"continue"|"deny", reason?, input?}`.

Supported events: `session_start`, `session_end`, `tool_execute_before`,
`tool_execute_after`, `chat_message`, `error_handle`. The manifest maps each to
an export in `hooks` via `function`. Order is ascending priority, then plugin ID,
then manifest order. Later hooks see earlier transformations. After-tool,
session-end and error hooks are observers and cannot mutate results or veto
completed work. Tool schemas support closed objects, required properties, and
string/number/integer/boolean/null values; unknown schema keywords fail.

Instances retain state until shutdown/reload. Startup follows dependency order;
shutdown reverses successful startup. Runtime timeout is 1–30,000 ms (default
5,000). Timeout, cancellation, EOF, malformed frames and protocol errors stop
the worker; restart is explicit, not automatic, and resets state. V8 old-space
is capped at 128 MiB; this is **not a total native-process memory limit**.
On Unix the supervisor kills the process group, including ordinary descendants;
trusted code can deliberately escape a process group. Windows descendant
cleanup and other platforms have not been verified.

## WASM

WASM uses the same result envelope and manifest protocol. The runtime retains
one store/instance per plugin and applies 10M fuel per call, up to 16 MiB memory,
table limits, and a 10 ms epoch ticker with the configured deadline. WASI is
not linked; use `wasm32-unknown-unknown`. Rust source builds require
`plugin build --trust-code`, because Cargo build scripts/macros execute native
code. The Rust compilation target was not installed in the verification environment;
WASM runtime evidence comes from the known-source module fixtures. Required exports: `init`, `shutdown`,
`cmd_NAME`, `tool_NAME` and declared hook functions, each `() -> i32`.
Zero succeeds; nonzero/traps fail. Commands, tools and hooks must set a result.

Imports in module `cortex`:

* `input_len() -> i32`: request byte length.
* `read_input(ptr: i32, capacity: i32) -> i32`: copies bounded UTF-8 JSON
  `{protocol:1,input,context}` into exported guest memory, or returns a negative
  status.
* `set_result(ptr: i32, length: i32) -> i32`: copies and parses a bounded result,
  or returns a negative status.
* `log(level, ptr, len)`: legacy bounded logging, not a UI effect.

Legacy `get_context` length-only and UI/event imports are rejected at load,
not silently accepted. Use returned notifications instead. This is an explicit
ABI change, not backward compatibility with old illustrative WASM modules.
Known-source executable WASM tests are in
`src/cortex-plugins/tests/fixtures/protocol_v1.wat`.

## Installation and application integration

The installer stages bounded directories or `.tar.gz` packages, rejects
links/traversal/special entries/duplicates, validates identity and the executable
artifact, then replaces the old directory with rollback. Replacement is a pair of renames
with error rollback, not a single atomic exchange or a power-loss recovery journal.
A stale `.install.lock` after a crash requires operator inspection. Limits: 1,024 entries,
64 MiB expanded/archive bytes, 16 MiB artifact, 256 KiB manifest. Packages can
be flat or have one directory named after the manifest ID. Publishing is local
archive preparation only. `update ID --source PATH` exercises the same atomic
installer. A malformed update leaves the previous install unchanged.

The registry uses the shared `PluginIndex` schema and only
`https://software.cortex.foundation`; redirects and missing checksums fail.
Signed remote artifacts without an implemented configured-key flow fail rather
than skipping verification. Registry availability was not live-tested.

The CLI command path is executable. The **TUI/session startup integration is a
separate caller change**: use
`cortex_engine::plugin::executable::ExecutableSession`, retain it for the session,
wire before/after/message/error events and commands/tools, render/drain its
notifications, and call `finish`. Do not use the old metadata-only engine
manager as evidence of executable activation. Shell hooks without a host-owned
`ScriptHookBroker` fail closed; the broker must enforce project trust, policy,
sandbox, exit status, timeouts and output limits.

Tests: `node --test packages/plugin-host/test/host.test.mjs` and
`cargo test --locked --offline -p cortex-plugins --test executable_runtime`.
This is neither an npm resolver nor an OpenCode compatibility adapter.

Product-path self-check: `python3 src/cortex-cli/src/plugin_cmd/cli_check.py /absolute/path/to/Cortex`.
