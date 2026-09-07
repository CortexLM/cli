# Cortex CLI staging soak, Designer cli

**Result: BLOCKED. Production: HOLD, untouched.**

Observation window: 2026-09-07, 11:46–11:50 UTC. This is a bounded readiness
attempt, not a completed soak or release acceptance. No staging application
mutations, member login, model turns, deployments, or production requests ran.
Only this staging evidence document changes tracked files.

## Provenance

- Backend checkout and deployment workflow revision:
  `2c1c575051002524386cf3a27613f952c8afd44c`.
- [Deploy staging 34117265669](https://github.com/CortexLM/backend/actions/runs/34117265669):
  `completed / success`, updated `2026-09-07T11:34:57Z`. The Client VPN check,
  both Helm upgrades, and rollout verification completed successfully.
- Allowlisted deployment-log evidence confirmed image
  `sha-db24a64032cc73c893ba4095e64e68b20d8662ab` and `vpn_ready=true`.
  These are historical deployment observations, not a fresh live readback.
- Existing `CortexLM/cli` checkout used:
  `79f40090ef4874d6415c4d240606694f42df16af`.
  Tag `v0.1.8` resolves to `3c06750cf3c5baa90952c655053c22ffad1888cd`.
  The checkout includes subsequent changes; it is not the exact release tree.
  The inspected client-routing, doctor, CLI entrypoint, and version-marker
  files match the tag. No released binary was executed.

## Results

| Check | Status | Evidence |
| --- | --- | --- |
| Deployment record | PASS | GitHub reports the staging deployment successful; image and VPN flag match the supplied context. This does not establish current service readiness. |
| IAM staging hop | BLOCKED | From the backend repository root, `scripts/cursor-staging-access.sh --once` exited **253**: `NoCredentials: Unable to locate credentials`. It stopped at caller identity, before role assumption, kubeconfig generation, or port-forward startup. |
| API `/readyz` | BLOCKED | At `11:48:03Z`, GET `http://127.0.0.1:18081/readyz`: curl **7**, HTTP **000**, connection refused. No HTTP response or staging service observation. |
| Web `/api/readyz` | BLOCKED | At `11:48:03Z`, GET `http://127.0.0.1:18080/api/readyz`: curl **7**, HTTP **000**, connection refused. No HTTP response or staging service observation. |
| Public staging DNS | PASS (expected limitation) | At `11:48:37Z`, both `staging.cortex.foundation` and `api.staging.cortex.foundation` returned resolver `EAI_NONAME` (-2). Consistent with the expected public DNS limitation, not a service-health failure. |
| CLI version consistency | PASS | `./scripts/check-cli-version.sh` exited **0**; all three version sources are `0.1.8`. |
| CLI staging routing | PASS (source inspection only) | The CLI client accepts `CORTEX_API_URL`; `STAGING_API_URL` must be explicitly mapped to it. No implicit production fallback was exercised. |
| CLI build / local doctor | BLOCKED | `cargo build --locked --offline -p cortex-cli` exited **101**: `no matching package named phc found`. No CLI binary was produced; `Cortex debug doctor --json` did not run. |
| CLI smoke against staging | BLOCKED | No established hop, populated `STAGING_API_URL`, or built CLI. No API compatibility or coding-turn success is claimed. |
| Member IdC/VPN and WorkOS flow | BLOCKED | Remains with **Mathis**, as supplied in the task. No member identity, VPN, or authenticated end-to-end flow was verified here. |
| Formatting | PASS | `cargo fmt --all -- --check` exited **0**. This is source hygiene, not runtime evidence. |

`FAIL` would indicate an observed failure of the service or behavior under
test. The failed hop, connection, and offline-build commands above are recorded
as `BLOCKED` because their prerequisites were unavailable; they are not passes.

## Execution boundaries and evidence details

- The unmodified hop script was run from the backend checkout with explicit
  staging role, cluster, namespace, service, and port settings. `SKIP_ASSUME=0`;
  no IAM, VPN, ingress, or deployment configuration was changed.
- Task-specific temporary kubeconfig and PID paths were used to avoid touching
  another soak's files. Both remained absent after the failed hop. No
  port-forward process was started or left behind by this attempt.
- `STAGING_API_URL` and `STAGING_WEB_URL` were unset. The loopback addresses
  above are the hop's documented candidates, **not successfully established
  staging endpoints**.
- Readiness probes used unauthenticated GET, no proxy, no redirects, a two-second
  connect timeout and five-second total timeout, and discarded response bodies.
  No `/healthz` fallback was counted as a `/readyz` pass.
- Rust was available outside the initial PATH. With `/root/.cargo/bin` added,
  Cargo and rustc both reported `1.98.0`. The build stayed locked and offline;
  dependencies were not installed or updated to work around the blocker.
- Doctor's contract is `scope: local`, `coding_service: not_checked`, even when
  an API URL is set. A future successful doctor run must not be reported as a
  staging API or coding-turn pass.
- Full Rust tests, Clippy, audit, local functional/security QA, coverage, and
  member browser/TUI flows were not run as part of this blocked staging attempt.
  The local QA script exercises an isolated local app server, not staging.
- No credential values, tokens, cookies, raw deployment logs, or user content
  are included. Only image hashes, status fields, and redacted errors were
  retained from deployment evidence.

## Unblock and repeat

1. Provision an approved short-lived AWS session in the agent environment that
   can use `cursor-staging-soak`, then rerun the backend hop with `--once`.
   Agent API/process checks do **not** require member Client VPN; missing AWS
   credentials are the immediate access blocker. Do not use SSO device codes
   here or widen staging ingress.
2. After successful hop startup, set the emitted staging URLs in the shell
   used for probes. Require both API `/readyz` and web `/api/readyz` to return
   successful HTTP responses, and verify the live image before claiming
   same-build evidence.
3. Make the locked CLI dependencies or a verified `v0.1.8` binary available.
   Explicitly use `CORTEX_API_URL="$STAGING_API_URL"` in an isolated CLI home
   for any subsequent staging-targeted command. Local doctor is a separate
   prerequisite check, not a network smoke. Recheck command egress before
   running commands that can start an automatic update check.
4. Mathis retains member IdC/VPN and real-host WorkOS validation. Login,
   session creation, and model turns were outside this read-only attempt;
   obtain separately authorized member evidence rather than treating the hop
   or deployment flag as proof.

No production go decision follows from this report.

SOAK_RESULT: BLOCKED, agent AWS credentials unavailable; staging API/web readiness and CLI smoke unverified; member IdC/VPN remains Mathis; PROD HOLD.
