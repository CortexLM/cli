# Cortex CLI staging soak, Designer cli — CLI_SMOKE_v019

**Result: PARTIAL. Production: HOLD, untouched.**

Observation window: 2026-09-07, 21:41–21:48 UTC. Post-#41 smoke of Cortex CLI
v0.1.9 from this agent host. No staging mutations, member login, model turns,
deployments, or production writes ran. Only this document changes tracked
files.

Supersedes the v0.1.8 hop-blocked soaks (#35, #37, #38) for this version
lane. Hop rows remain unobservable from this VM; offline binary + lock v2
rows below are new evidence against tip `fe94a48`.

## Provenance

- Tip under test: `fe94a4896006d41bfb9f7205e7e1cbf05c9bf157`
  (`chore: bump version to 0.1.9`, [PR #41](https://github.com/CortexLM/cli/pull/41),
  merged `2026-09-07T21:30:21Z`).
- Annotated tag `v0.1.9` points at that commit
  (`refs/tags/v0.1.9` → `e093cf47…`, peeled `fe94a48`).
- `CortexLM/cli` checkout: `fe94a48` (`main` at smoke start). Version sources
  all report `0.1.9` (`VERSION_CLI`, `[workspace.package].version`,
  `src/cortex-cli/VERSION`). `./scripts/check-cli-version.sh` exit **0**.
- Binary: `target/debug/Cortex` (`cargo build --locked -p cortex-cli`,
  first attempt failed on missing `alsa.pc`; after
  `libasound2-dev` / `libssl-dev` from `docs/guides/development.md`, retry
  exit **0**, `dev` profile). Reports `cortex 0.1.9 (fe94a48 2026-09-07)`.
- Toolchain: rustc 1.98.0 (`rust-toolchain.toml`).

No sealed env, AWS session, or IdC/VPN on this host. No credentials were
created, read, or written. Public production API DNS resolves; it was not
called.

## Results

| Check | Status | Evidence |
| --- | --- | --- |
| Staging hop `127.0.0.1:18081/readyz` | BLOCKED | GET at 21:42Z and 21:48Z, HTTP **000**, curl **7** (connection refused). Same for `:18080` and `:18090`. No listener. |
| Real-PTY trust / login with `CORTEX_API_URL` | BLOCKED | Hop unreachable. Trust/login against the staging base was not started. No device-code request, no production fallback attempt. |
| `/model` vs `GET /v1/models` emptiness | BLOCKED | GET `http://127.0.0.1:18081/v1/models` at 21:48Z, HTTP **000**, curl **7**. TUI `/model` empty-state (`No models available…`) not exercised against a live list. Source still maps `/model` to `GET {CORTEX_API_URL}/v1/models` (`src/cortex-tui/src/providers/manager.rs`). |
| CLI build + version 0.1.9 | PASS | Locked build exit **0**. `--version` contains `0.1.9` and short SHA `fe94a48`. Version-consistency script exit **0**. |
| Chrome lock v2, headless | PASS | `cargo test --locked -p cortex-tui lock_v2`: **11 passed**, 0 failed (wide/narrow uniqueness, welcome, first-run `/model` tips, user bars, settings modal, effort High→Medium→Low, slash hover). Finished 21:48Z. |
| `debug doctor` (local only) | PASS | Isolated `HOME`, `CORTEX_API_URL=http://127.0.0.1:18081`. Exit **0**, `ready: true`, `scope: local`, `coding_service: not_checked`, checks `configuration` / `git` / `ripgrep` / `storage` all `true`. Doctor is local-only by contract. |
| IAM staging hop / sealed env | BLOCKED | No `cursor-staging-soak.env` (or equivalent). `aws` CLI absent. No port-forwards started. |
| Public staging DNS | PASS (expected limitation) | `staging.cortex.foundation` / `api.staging.cortex.foundation` do not resolve from this host. |
| TUI chrome, full member VPN | BLOCKED | No IdC / Client VPN / WorkOS login on this host. Remains with Mathis. |
| Production | HOLD | Untouched. No go decision. |

`BLOCKED` means prerequisites were unavailable; no observed service failure is
claimed and none of those rows is a pass of the staging service.

## Unblock and repeat

1. Provide a short-lived staging hop (listeners on `127.0.0.1:18080/18081/18090`)
   or the sealed soak session, then require API `/readyz` **200**.
2. Real PTY: `CORTEX_API_URL=http://127.0.0.1:18081` in an isolated `HOME`;
   accept trust, complete login against the staging base (no production
   fallback).
3. Emptiness check: `GET /v1/models` body vs TUI `/model` list. If the API
   list is empty, `/model` must show the product empty copy rather than a
   silent or stale picker. If the API list is non-empty, `/model` must show
   the same product names.
4. Member IdC/VPN, WorkOS login and authenticated chrome remain with Mathis.

No production go decision follows from this report.

SOAK_RESULT: PARTIAL tip=fe94a48 v=0.1.9 (hop BLOCKED 127.0.0.1:18081/readyz curl 7; build PASS; version PASS; lock_v2 11/11 PASS; real-PTY login BLOCKED; /model vs /v1/models BLOCKED; PROD HOLD)
CLI_SMOKE_v019
