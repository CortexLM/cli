# Cortex CLI / Cortex Code

The Cortex coding-agent harness: a CLI and TUI that talks to [api.cortex.foundation](https://api.cortex.foundation) for Chat/Code turns, tools, computers, plugins, and snapshots.

Auth is WorkOS session / existing `/v1` auth against `auth.cortex.foundation` and `api.cortex.foundation`. Credentials are stored in the OS keyring.

License: [Apache-2.0](./LICENSE).

## Build

Requires current stable Rust (see `rust-toolchain.toml`).

```bash
# Linux deps used by optional audio / desktop crates
sudo apt-get install -y libasound2-dev pkg-config

cargo build -p cortex-cli --release
# binary: target/release/Cortex
```

A debug build is enough for local work:

```bash
cargo build -p cortex-cli
./target/debug/Cortex --help
```

## Login

```bash
# Device-code / WorkOS browser login against the live Cortex API
./target/debug/Cortex login

# Or set a key (still stored via the keyring helpers when you login)
export CORTEX_API_KEY=...
# optional override; default is https://api.cortex.foundation
export CORTEX_API_URL=https://api.cortex.foundation
```

`cortex login` opens the Cortex auth flow (`auth.cortex.foundation`). On success the session is written to the OS keyring (`cortex-cli` / `auth`).

## Run the TUI

```bash
./target/debug/Cortex
# or a one-shot turn
./target/debug/Cortex "explain this repository"
```

The TUI is the default when stdin is a TTY. If the coding API is unreachable you will see **The coding service is temporarily unavailable** — not a raw provider or HTTP-client name.

## Tests

```bash
cargo fmt --all -- --check
./scripts/clippy.sh
cargo test --workspace
cargo audit
./scripts/check-cli-version.sh
```

Headless TUI / snapshot tests (required when you change a TUI surface):

```bash
cargo test -p cortex-tui -p cortex-tui-capture -p cortex-tui-components \
  -p cortex-tui-framework -p cortex-tui-core -p cortex-tui-buffer \
  -p cortex-tui-widgets -p cortex-tui-layout -p cortex-tui-text \
  -p cortex-tui-input -p cortex-tui-terminal -p cortex-tui-syntax
```

## Install from software.cortex.foundation

Published artifacts (when a release is cut) are at [software.cortex.foundation](https://software.cortex.foundation):

```bash
curl -fsSL https://software.cortex.foundation/install.sh | sh
```

## Release and CI secrets

Merges to `main` run `.github/workflows/version-bump.yml` (patch semver + tag). Tags run `.github/workflows/release.yml`, which can publish to R2 / `software.cortex.foundation` via `.github/workflows/publish-r2.yml`.

This repo does **not** invent cloud accounts. Secret *names* CI already expects are listed in [docs/CI_SECRETS.md](docs/CI_SECRETS.md). Do not put secret values in git.

## Contributing

See [AGENTS.md](AGENTS.md), [`.rules/`](.rules/), and [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md). Every PR must fill the attestation list in `.github/PULL_REQUEST_TEMPLATE.md`.
