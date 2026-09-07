# Local JavaScript / TypeScript client

`packages/sdk` is a dependency-free, typed client for an **installed Cortex CLI
subprocess**, not a runtime plugin host or a client for invented remote endpoints.
The private `@cortexlm/sdk` package is Apache-2.0 and is not published.

## Supported contract

- Node.js 22; local Linux subprocess lifecycle is tested.
- SDK contract identifier: `cortex.exec.stream-json/0.1.8`. This is a client-side
  identifier, **not** a wire handshake or a promise about every 0.1.8 binary.
- Requires the integrated CLI's repaired `exec --output-format stream-json`
  contract: an initial `system/init` event, consistent UUID `session_id`, and a
  terminal `completion` with **`success: true`**, followed by process exit 0.
  Legacy binaries omitting `completion.success` are rejected, even if they
  report the same product version.
- `resume` uses the CLI's `--session-id` and requires the emitted ID to match.
  It depends on the runtime's `Session::resume` implementation; a new session
  masquerading as a resume is rejected.
- The client does not use JSON-RPC, ACP, REST storage, app-server streaming,
  remote attach, replay, or reconnect. Each turn owns one subprocess.

The runtime source of this contract is
`src/cortex-cli/src/exec_cmd/{cli.rs,runner.rs,runtime_contract_options.rs}`.
Runtime changes must be integrated before exercising the SDK with a real CLI.
Fixture tests validate local transport behavior, not authenticated generation.

## Use

From this repository (ES modules):

```javascript
import { CortexClient, CortexError } from "./packages/sdk/src/index.mjs";

const client = new CortexClient({
  executable: "/absolute/path/to/Cortex",
  cwd: "/absolute/path/to/workspace",
});
const controller = new AbortController();
const turn = client.start("Explain the test layout without modifying files.", {
  signal: controller.signal,
  timeoutMs: 60_000,
  onEvent(event) {
    if (event.type === "delta") {
      // Render event.content in your application; do not automatically log it.
    }
  },
});

try {
  const result = await turn.result;
  // Explicit continuation, only when the user requests another turn:
  // await client.resume(result.sessionId, "Explain the unit tests.").result;
} catch (error) {
  if (error instanceof CortexError) console.error(error.message);
}
// To cancel active work and await local cleanup: await turn.cancel();
```

The TypeScript declarations are `src/index.d.mts`; a typed example is
`examples/read-only.mts`. JavaScript runtime and TypeScript consumers use the
same implementation. Package metadata follows the repository's `VERSION_CLI`;
it is not an independent release version source.

### Authority and lifecycle

The executable and working directory must be absolute, operator-selected paths.
No shell concatenation is used. The prompt goes to stdin, never argv.
The only execution policy exposed is `--auto read-only`; this client cannot
approve a tool write, change autonomy, install a plugin, or bypass CLI policy.
The effective sandbox and service-side tool authority remain the CLI/runtime's
responsibility, not a JavaScript sandbox or a prompt-only permission guarantee.

Configure login with Cortex separately. The SDK does not read credential files.
By default the child inherits the calling process's environment; supply `env`
to replace it, especially in tests. `prefixArgs` is only for a trusted launcher
or controlled fixture and must never come from repository content.

`onEvent` is synchronous. It receives typed init, text, reasoning, tool
observation, and successful completion events; its exceptions stop the child.
A completion event alone is not success: `turn.result` also waits for clean
stdout closure and process exit 0. EOF, malformed UTF-8/JSON, mismatched IDs,
duplicate init/completion, late events, nonzero exits and service errors fail.
Observers must treat streamed content as provisional and untrusted.

Default bounds: prompt 256 KiB; line 1 MiB; stdout 16 MiB; stderr 1 MiB;
100,000 events; 600-second deadline; 500-ms signal grace.
Limits are configurable positive integers. Raw stderr and service error text
are drained/discarded rather than exposed through public exceptions.
Text/tool events and final results can still contain user/code content.

Cancellation requests the CLI's interrupt using SIGINT, then escalates to SIGKILL
after the grace period. Same-process-group descendants are also terminated on
Linux; processes that deliberately detach into another group are outside this
guarantee. Cancellation and timeout never claim confirmed service-side
cancellation. Windows process-tree cleanup and editor-host interoperability have
not been verified.

## Offline validation

```bash
node --test packages/sdk/test/*.test.mjs
cd packages/sdk && npm run check
```

No package installation is needed. Tests launch a controlled Node fixture,
not Cortex or a model, and replace its environment. They cover actual pipes,
UTF-8 framing, stdin/argv boundaries, resume, terminal failures, size limits,
abort, timeout, callback failure and process-group cleanup.
With an already-installed TypeScript compiler, declarations/example can also be
checked with `tsc --noEmit --strict --module NodeNext --target ES2022
packages/sdk/examples/read-only.mts`. Syntax checking is not type checking.

## GitHub automation boundary

`Cortex github run` uses a real engine session for read-only analysis. It accepts
`issue_comment`, `pull_request`, `pull_request_review`, and `issues` event files.
`--event` is required; `--event-path`, `--repository`, and `--token` have the
documented `GITHUB_EVENT_PATH`, `GITHUB_REPOSITORY`, and `GITHUB_TOKEN` fallbacks.
Prefer the token environment variable, not argv.

Only repository-associated human authors (`OWNER`, `MEMBER`, `COLLABORATOR`)
whose sender identity matches the triggering author are accepted. Fork PRs are
rejected, including PRs discovered from issue comments. This is an Actions
consumer, not a webhook authentication server: a local operator controls the
input file and is responsible for its provenance.

`--dry-run` validates/plans without needing a token or starting an agent.
Actual runs require `--output PATH` (new local file, mode 0600 on Unix) or
**`--publish`**, which explicitly approves one result comment.
No processing/success reactions or placeholder replies are posted.
`fix` means suggest a fix, not apply it; no branches are pushed or PRs approved.
Generated workflows use read-only GitHub permissions, the trusted default-branch
checkout without stored credentials, and a seven-day analysis artifact.
They do not automatically post comments. Review the official CLI installer and
configure the `cortex-analysis` environment before enabling a generated workflow.

Publication uses stable event markers to avoid repeat comments on sequential
reruns and generated workflows serialize work per issue/PR. GitHub has no atomic
comment idempotency operation here; independent concurrent CLI invocations can
still race. Missing/binary patches, stale PR heads, oversized context, agent
failure, and timeout return errors rather than a successful review.
This is bounded textual analysis, not a certified complete code review.
Backend turns/tool policy/cancellation and an authorized disposable-repository
live test remain separate integration requirements.
