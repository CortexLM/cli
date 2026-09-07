# Cortex CLI staging soak, Designer cli

**Result: BLOCKED. Production: HOLD, untouched.**

Observation window: 2026-09-07, 14:46–15:03 UTC. Bounded readiness attempt,
not a completed soak or release acceptance. No staging mutations, member
login, model turns, deployments, or production writes ran. Only this document
changes tracked files.

Supersedes the 11:46 UTC attempt (PR #35) for tip `sha-45d44b02`.

## Provenance

- Tip under test: backend `45d44b02fbd58f2db47427f1cf35cfd017f783e2`
  (backend PR #201, "Redirect /chat to the Chat home", merged
  `2026-09-07T13:45:04Z`).
- [Deploy staging 34131499106](https://github.com/CortexLM/backend/actions/runs/34131499106):
  `completed / success`, updated `2026-09-07T14:12:10Z`, `headSha` equals the
  tip above. Historical deployment record, not a live readback.
- Backend checkout used for the hop script: `ef169f7ce41e472fec28a0ea8b012348897eb546`.
- `CortexLM/cli` checkout: `99a94ed` (`main` + this document). Version
  sources all report `0.1.8`.

## Results

| Check | Status | Evidence |
| --- | --- | --- |
| Tip image / API `/readyz` | BLOCKED | Deployment record for `sha-45d44b02` is green (above). Live `/readyz` unobservable: GET `http://127.0.0.1:18081/readyz` at 14:47Z, HTTP **000**, curl **7** (connection refused). |
| API path CLI uses (guest auth + `/v1/me`) | BLOCKED | Source: `src/cortex-engine/src/client/code_agent.rs` and `src/cortex-tui/src/runner/login_screen.rs` call `POST /v1/auth/guest`. GET `http://127.0.0.1:18081/v1/me` at 15:02Z, HTTP **000**, curl **7**. No request reached staging. |
| `cortex` CLI binary against staging API base | BLOCKED (build PASS) | `cargo build --locked -p cortex-cli` exit **0** (online fetch; the offline `phc` blocker from #35 is gone). `CORTEX_API_URL=http://127.0.0.1:18081 Cortex debug doctor --json` in an isolated `HOME`: exit **0**, `checks` all `true`, `ready: true`, `scope: local`, `coding_service: not_checked`. Doctor is local-only by contract; no staging turn was exercised. |
| TUI chrome, full member VPN | BLOCKED | No IdC / Client VPN on this host. Remains with Mathis. |
| `/chat` redirect on web hop (expect 307→`/`) | BLOCKED | GET `http://127.0.0.1:18080/chat` with `Host: staging.cortex.foundation` at 15:02Z, HTTP **000**, curl **7**. Not observed. |
| IAM staging hop | BLOCKED | `/home/box/agent-data/shared-secrets/cursor-staging-soak.env` absent; no Factory shared-secrets equivalent found. `SKIP_ASSUME=1 scripts/cursor-staging-access.sh --once` exit **253**: `NoCredentials`. `aws sts get-caller-identity` fails the same way, so opening own port-forwards with the soak role is not possible either. No PIDs started. |
| Public staging DNS | PASS (expected limitation) | `staging.cortex.foundation` / `api.staging.cortex.foundation` do not resolve from this host. |
| CLI version consistency | PASS | `./scripts/check-cli-version.sh` exit **0**, all sources `0.1.8`. |

`BLOCKED` means prerequisites were unavailable; no observed service failure is
claimed and none of these rows is a pass of the staging service.

## Delta vs #35

- CLI build now succeeds and `debug doctor` runs green locally.
- Tip advanced to `sha-45d44b02`; its staging deploy run is green.
- Access blocker unchanged: no AWS credentials for `cursor-staging-soak`.

## Unblock and repeat

1. Provide the sealed `cursor-staging-soak.env` (or equivalent short-lived
   session) to the agent host, then rerun `scripts/cursor-staging-access.sh --once`.
2. With listeners up: require API `/readyz` 200, web `/chat` → 307 `Location: /`,
   `POST /v1/auth/guest` then `GET /v1/me` 200 through the CLI's client path.
3. Run `target/debug/Cortex` with `CORTEX_API_URL="$STAGING_API_URL"` in an
   isolated home for one guest turn. Doctor stays a local prerequisite only.
4. Member IdC/VPN, WorkOS login and TUI chrome remain with Mathis.

No production go decision follows from this report.

SOAK_RESULT: BLOCKED tip=sha-45d44b02 (agent AWS credentials unavailable; /readyz, /v1/me, /chat redirect and CLI staging smoke unverified; CLI build + local doctor PASS; member IdC/VPN remains Mathis; PROD HOLD)
