# Local operations and incident runbook

## Opt-in local diagnostics

Set `CORTEX_DIAGNOSTICS_DIR` to a private directory for either application.
If absent, no diagnostic journal is created. There is no OTLP, error-tracking,
analytics, or alert exporter in this implementation.

```bash
export CORTEX_DIAGNOSTICS_DIR="$HOME/.cortex/diagnostics"
./target/debug/Cortex debug doctor --json
python3 scripts/readiness/insights.py "$CORTEX_DIAGNOSTICS_DIR"
```

The application creates private directories/files (0700/0600 on Unix), rejects
symlink/public output directories, and accepts only a closed set of events:
command completion, server startup/requests, and session creation/deletion.
Fields are schema version, time, app version, operation, trace/span IDs, numeric
status, and duration. It never accepts prompts, file paths, headers, tokens,
email addresses, session IDs, response bodies, or arbitrary error text.

Each process journal is capped at 2 MiB. At startup, only generated
`run-<UUID>.jsonl` files older than seven days are deleted; other files are untouched.
At most 64 recent files are allowed. On quota/storage errors, diagnostics report
a failure rather than claiming successful recording. Existing app operations
are not turned into successes or failures by a later recording error.
Normal application/session logs are separate and are **not safe diagnostic
attachments**. See [privacy](../reference/privacy.md).

## Alerts and deployment comparison

The local insights command exits nonzero for either:

- at least 20 operations and an error rate of 5% or more;
- at least 20 server requests and p95 response-creation latency above 2,000 ms.

Empty input is an error, not a healthy deployment. Results group counts,
operations, failures, and latency by the existing application version. Compare
actual before/after deployment samples; low traffic cannot establish an SLO.
No background service or external paging is configured. Operators can invoke
the command from their own local scheduler and route its exit status locally.

These counters also show local feature use (commands and session lifecycle).
They are not a user-tracking system and cannot establish organization-wide
product adoption. Trace IDs correlate async tasks and incoming HTTP requests
locally. No new trace headers are sent to the remote coding API. Cross-service
distributed tracing remains out of scope under the local-only data policy.

## Triage a failure

1. Run `Cortex debug doctor --json`; fix missing Git/ripgrep, malformed TOML, or
   failed storage checks. Do not describe these as coding-service outages.
2. For the server, check `/api/v1/health` and authenticated `/metrics`. A 401 is
   an authentication problem; 429 means rate limiting. Do not disable auth.
3. Match local request/trace IDs and app version, then inspect aggregate
   `insights.py` output. Never copy raw request/session logs into an issue.
4. Reproduce with `python3 scripts/readiness/qa.py` and the narrow relevant
   unit/integration test. A real upstream failure must show
   **The coding service is temporarily unavailable**, not a fake response.
5. Draft a local issue summary containing version, sanitized reproduction,
   expected/actual result, test names, and aggregate counts. Include no secrets,
   prompts, customer code, home paths, or authentication details.
6. Only submit the issue after explicit approval. Assign priority and area
   according to [maintenance](maintenance.md). Add a regression test and a
   runbook update for a genuinely new failure mode.

## Profiling

`X-Response-Time`, journal durations, and nextest reports locate slow operations.
For deeper Linux CPU profiling, use the system `perf` tool locally:

```bash
cargo build --locked --profile profiling -p cortex-cli -p cortex-app-server
perf record --call-graph dwarf -- ./target/profiling/Cortex debug doctor
perf report
```

Do not lower system profiling protections or use elevated privileges just to
run this command. If host policy blocks perf, report that limitation. Profiles
can contain process/path information; keep them local and delete them after
investigation. This supplies a profiling procedure and timing instrumentation,
not an always-on CPU profiler. The `profiling` profile preserves debug information
and symbols without changing release artifacts.

## Release and rollback

Use the existing version-bump PR workflow on `main`. Keep `VERSION_CLI`,
workspace version, and `src/cortex-cli/VERSION` aligned. Review changelog, CI,
local QA, and versioned diagnostic samples before accepting a release.

If a deployed CLI regresses, reinstall the last known-good **existing** release
using the documented installer/version mechanism, then rerun local QA and
compare samples. For a managed server, restore the previous tested binary and
restart using the same protected configuration. Do not rewrite tags, delete
sessions, or publish a synthetic release to improve deployment statistics.
Actual release frequency requires genuine shipped changes over time.
