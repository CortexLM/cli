# Reproducible local development

## Prerequisites and setup

Use the Rust version in `rust-toolchain.toml` (1.98.0). On Debian/Ubuntu:

```bash
sudo apt-get update
sudo apt-get install -y build-essential libasound2-dev libssl-dev pkg-config python3-venv ripgrep git
bash scripts/dev-setup.sh
export PATH="$PWD/target/readiness-tools/bin:$PATH"
```

Setup fetches the **locked** graph, including the Git-sourced terminal dependency,
and installs pinned analysis/test tools under ignored `target/`. It does not
change Git configuration, install hooks, run a remote installer, or log in.
An offline build needs a successful `cargo fetch --locked` first.

The development container uses the same prerequisites and setup command.
Open this repository with a Dev Containers-compatible editor and choose
**Reopen in Container**. It runs as the unprivileged `cortex` user and does not
mount credentials or Docker sockets. No ports are automatically published.
The workspace must be writable by that user (the editor maps the host UID on Linux).

```bash
cargo build --locked -p cortex-cli -p cortex-app-server
./target/debug/Cortex debug doctor --json
python3 scripts/readiness/qa.py
```

No database or fake model service is required. Legacy REST sessions are in memory;
other session implementations use JSON/JSONL files. Local QA uses a fresh temporary
home/workspace, an ephemeral loopback port, and an in-memory generated server key.
It deletes only its own temporary data and stops its own server.

## Real local QA

`qa.py` exercises both built binaries, not mocks:

- CLI: successful configuration/storage/tool checks, then invalid configuration
  must fail with a nonzero exit status.
- Server: authenticated session creation, message storage/readback, deletion,
  missing resources, request correlation, and metrics.
- Dynamic security: missing/invalid credentials, WebSocket upgrade protection,
  endpoint-prefix bypass, oversized bodies, browser origins, workspace traversal,
  and symlinks.

Evidence is in `target/readiness/qa/`. These tests **do not generate a model response**.
Message storage is not evidence of a successful coding turn.

`Cortex debug doctor` checks local prerequisites only. `coding_service: not_checked`
is deliberate. A failed tool, storage round-trip, or TOML parse fails the command.
Debug commands and `serve` do not start the automatic update check.

For actual interactive TUI/chat QA, use the existing login procedure in
[Getting started](getting-started.md), with a dedicated user-approved test account.
Verify `/help`, a real turn, approvals, and disconnect recovery. Without such an
account, report that flow as **blocked**, not passed. Never substitute a success
response for the coding service. Keep live tests out of default CI.

## Test reports and coverage

```bash
python3 scripts/readiness/tests.py
python3 scripts/readiness/tests.py --repeat 3
cargo test --locked --workspace --doc
mkdir -p target/readiness
cargo llvm-cov nextest --locked -p cortex-cli -p cortex-app-server -p cortex-common \
  --profile ci --lcov --output-path target/readiness/lcov.info
python3 scripts/readiness/coverage.py --base origin/main
```

Nextest 0.9.102 runs each test in an isolated process. Retries are disabled.
Each repetition has a separate JUnit report; any failure fails the command,
including a failure followed by a pass. Timings, slowest tests, and flaky test
names are in `target/readiness/tests/`. Weekly CI runs three repetitions.
Doctests still use Cargo because nextest does not run them.

Coverage uses cargo-llvm-cov 0.6.21 and LLVM tools from the pinned Rust toolchain.
The enforced floor is **80% of changed executable lines** in the CLI, app server,
and shared common crate, under their `src/` directories. Test harnesses and
schema-export examples are validated separately, not included in that floor.
Missing coverage for a changed production file containing functions fails closed.
Existing unmodified uncovered lines are not counted as fixed.
Review the full LCOV artifact as well as the changed-line gate.

Test names use `test_<behavior>`; integration files live under the owning
crate's `tests/`. Assert observable outcomes and negative cases. Never ignore a
failure to make CI green. See [testing rules](../../.rules/testing.md).

The append regression test checks immediate visibility after Tokio 1.53.1 file
writes. An awaited `flush` finishes the pending write; it is not an `fsync`
durability guarantee. Do not replace this check with sleeps or retries.
