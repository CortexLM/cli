#!/usr/bin/env bash
# Heavy Linux CI suite for CodeBuild. Mirrors .github/workflows/ci.yml
# clippy / test / tui / coverage / schema / QA. Fail closed.
set -euo pipefail

if [[ -z "${CORTEX_CLI_SRC:-}" ]]; then
  echo "CORTEX_CLI_SRC is required" >&2
  exit 1
fi
cd "$CORTEX_CLI_SRC"

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/cortex-cli-target}"
mkdir -p "$CARGO_TARGET_DIR"
# shellcheck disable=SC1091
. "$CARGO_HOME/env"
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_REGISTRIES_CRATES_IO_PROTOCOL="${CARGO_REGISTRIES_CRATES_IO_PROTOCOL:-sparse}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

QUALITY_BASE="${QUALITY_BASE:-}"
if [[ -z "$QUALITY_BASE" || "$QUALITY_BASE" == "0000000000000000000000000000000000000000" ]]; then
  git fetch --no-tags origin main
  QUALITY_BASE="$(git rev-parse --verify origin/main)"
fi
git fetch --no-tags origin "$QUALITY_BASE"
QUALITY_BASE="$(git rev-parse --verify "${QUALITY_BASE}^{commit}")"

echo "==> format"
cargo fmt --all -- --check

echo "==> clippy"
./scripts/clippy.sh

echo "==> CLI version"
./scripts/check-cli-version.sh

echo "==> tests"
python3 scripts/readiness/tests.py

echo "==> doctests"
cargo test --locked --workspace --doc

echo "==> API contracts"
python3 scripts/readiness/schema.py

echo "==> local QA binaries"
cargo build --locked -p cortex-cli -p cortex-app-server

echo "==> local functional QA"
python3 scripts/readiness/qa.py

echo "==> TUI / snapshot tests"
cargo test -p cortex-tui -p cortex-tui-capture -p cortex-tui-components \
  -p cortex-tui-framework -p cortex-tui-core -p cortex-tui-buffer \
  -p cortex-tui-widgets -p cortex-tui-layout -p cortex-tui-text \
  -p cortex-tui-input -p cortex-tui-terminal -p cortex-tui-syntax

echo "==> changed-line coverage"
mkdir -p target/readiness
cargo llvm-cov nextest --locked -p cortex-cli -p cortex-app-server -p cortex-common \
  --profile ci --lcov --output-path target/readiness/lcov.info
python3 scripts/readiness/coverage.py --base "$QUALITY_BASE"

echo "CodeBuild Linux CI passed"
