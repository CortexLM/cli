#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

for tool in cargo rustc git python3 pkg-config; do
  command -v "$tool" >/dev/null || { echo "Missing prerequisite: $tool. See docs/guides/development.md" >&2; exit 1; }
done
if [[ "$(uname -s)" == Linux ]]; then
  pkg-config --exists alsa openssl || {
    echo "Install libasound2-dev libssl-dev pkg-config (Debian/Ubuntu). See docs/guides/development.md" >&2
    exit 1
  }
fi
tools="$PWD/target/readiness-tools"
python3 -m venv "$tools"
"$tools/bin/python" -m pip install --disable-pip-version-check -r scripts/readiness/requirements.txt
cargo fetch --locked
cargo install --locked --version 0.9.1 cargo-machete --root "$tools"
cargo install --locked --version 0.9.102 cargo-nextest --root "$tools"
cargo install --locked --version 0.6.21 cargo-llvm-cov --root "$tools"
cargo install --locked --version 0.22.2 cargo-audit --root "$tools"
rustup component add llvm-tools-preview
echo "Setup complete. Use: export PATH=\"$tools/bin:\$PATH\""
