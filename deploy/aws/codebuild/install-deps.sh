#!/usr/bin/env bash
# System + Rust toolchain for CodeBuild Linux CI. Fail closed; no mock-success.
set -euo pipefail

if [[ -z "${CORTEX_CLI_SRC:-}" ]]; then
  echo "CORTEX_CLI_SRC is required" >&2
  exit 1
fi
cd "$CORTEX_CLI_SRC"

export DEBIAN_FRONTEND=noninteractive
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
mkdir -p "$CARGO_HOME/bin"
export PATH="$CARGO_HOME/bin:$PATH"

install_apt() {
  apt-get update
  apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    curl \
    git \
    libasound2-dev \
    libssl-dev \
    pkg-config \
    python3 \
    python3-pip \
    python3-venv \
    ripgrep
}

install_dnf() {
  dnf install -y \
    alsa-lib-devel \
    ca-certificates \
    curl \
    gcc \
    gcc-c++ \
    git \
    make \
    openssl-devel \
    pkgconf-pkg-config \
    python3 \
    python3-pip \
    tar \
    gzip
  dnf install -y ripgrep || true
}

install_ripgrep_tarball() {
  local target url tmp
  case "$(uname -m)" in
    x86_64) target="x86_64-unknown-linux-musl" ;;
    aarch64 | arm64) target="aarch64-unknown-linux-gnu" ;;
    *)
      echo "Unsupported architecture for ripgrep fallback: $(uname -m)" >&2
      return 1
      ;;
  esac
  url="https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-${target}.tar.gz"
  tmp="$(mktemp -d)"
  curl -LsSf "$url" | tar zxf - -C "$tmp"
  install -m 0755 "$tmp"/ripgrep-*/rg /usr/local/bin/rg
  rm -rf "$tmp"
}

if command -v apt-get >/dev/null 2>&1; then
  install_apt
elif command -v dnf >/dev/null 2>&1; then
  install_dnf
else
  echo "Unsupported CodeBuild image: need apt-get or dnf" >&2
  exit 1
fi

if ! command -v rg >/dev/null 2>&1; then
  install_ripgrep_tarball
fi
if ! command -v rg >/dev/null 2>&1; then
  echo "ripgrep (rg) is required for readiness tests" >&2
  exit 1
fi

channel="1.98.0"
if [[ -f rust-toolchain.toml ]]; then
  channel="$(sed -n 's/^channel = "\([^"]*\)"/\1/p' rust-toolchain.toml | head -n1)"
fi
if [[ -z "$channel" ]]; then
  echo "Could not determine Rust toolchain channel" >&2
  exit 1
fi

if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain "$channel" --profile minimal \
    --component rustfmt --component clippy --component llvm-tools-preview
fi
# shellcheck disable=SC1091
. "$CARGO_HOME/env"
rustup toolchain install "$channel" --profile minimal --component rustfmt,clippy,llvm-tools-preview
rustup default "$channel"

arch="$(uname -m)"
case "$arch" in
  x86_64)
    nextest_dist="linux"
    llvm_cov_target="x86_64-unknown-linux-musl"
    ;;
  aarch64 | arm64)
    nextest_dist="linux-arm"
    llvm_cov_target="aarch64-unknown-linux-musl"
    ;;
  *)
    echo "Unsupported architecture: $arch" >&2
    exit 1
    ;;
esac

if ! cargo nextest --version 2>/dev/null | grep -q '0.9.102'; then
  curl -LsSf "https://get.nexte.st/0.9.102/${nextest_dist}" | tar zxf - -C "$CARGO_HOME/bin"
fi
if ! cargo llvm-cov --version 2>/dev/null | grep -q '0.6.21'; then
  tmp="$(mktemp -d)"
  curl -LsSf \
    "https://github.com/taiki-e/cargo-llvm-cov/releases/download/v0.6.21/cargo-llvm-cov-${llvm_cov_target}.tar.gz" \
    | tar zxf - -C "$tmp"
  install -m 0755 "$tmp/cargo-llvm-cov" "$CARGO_HOME/bin/cargo-llvm-cov"
  rm -rf "$tmp"
fi

python3 -m pip install --disable-pip-version-check -r scripts/readiness/requirements.txt

command -v rustc >/dev/null
command -v cargo >/dev/null
command -v python3 >/dev/null
command -v node >/dev/null
command -v git >/dev/null
command -v rg >/dev/null
cargo nextest --version
cargo llvm-cov --version
node --version
echo "CodeBuild CI dependencies ready (toolchain $channel, arch $arch)"
