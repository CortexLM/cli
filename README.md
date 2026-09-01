<h1 align="center">Cortex CLI</h1>

<p align="center">
  <strong>Cortex Code</strong> — a coding agent that runs in your terminal.
</p>

<p align="center">
  <a href="https://github.com/CortexLM/cli/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/CortexLM/cli/actions/workflows/ci.yml/badge.svg"></a>
  <a href="./LICENSE"><img alt="License: Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-blue.svg"></a>
  <a href="https://github.com/CortexLM/cli/releases"><img alt="Latest release" src="https://img.shields.io/github/v/tag/CortexLM/cli?label=release&sort=semver"></a>
  <img alt="Built with Rust" src="https://img.shields.io/badge/built%20with-Rust-dea584.svg">
</p>

<p align="center">
  <a href="./docs/README.md">Documentation</a> ·
  <a href="./docs/guides/getting-started.md">Getting started</a> ·
  <a href="./docs/reference/cli.md">CLI reference</a> ·
  <a href="https://cortex.foundation">cortex.foundation</a>
</p>

![Demo of Cortex CLI](./docs/media/intro.gif)

## What is Cortex CLI

Cortex CLI — also called **Cortex Code** — is a coding agent you run from a
terminal in your project. Describe the change you want and it works through it:
searching the codebase, reading the files that matter, editing them, running your
build and tests, and reporting what it did. Every step shows up in the timeline
as it happens, and you decide how much it can do without asking.

One binary gives you:

- **An interactive TUI** with a live timeline, tool approvals, plan and build
  modes, session history, rewind and fork.
- **Headless one-shot runs** for scripts and CI, with autonomy levels, structured
  JSON output, and turn and time limits.
- **The tools a coding agent needs** — search, read, edit, patch, shell
  execution, language-server queries and web fetch — under an approval policy and
  a sandbox you control.
- **Extension points**: MCP servers, skills, custom agents and subagents, shell
  hooks and WebAssembly plugins.

Cortex CLI talks to the Cortex API at
[api.cortex.foundation](https://api.cortex.foundation) and signs in there with
device login (`POST /v1/auth/device`). Credentials are stored
in your OS keyring.

## Install

### Linux and macOS

```bash
curl -fsSL https://software.cortex.foundation/install.sh | sh
```

### Windows

```powershell
irm https://software.cortex.foundation/install.ps1 | iex
```

### From source

Requires the toolchain pinned in [`rust-toolchain.toml`](./rust-toolchain.toml).

```bash
cargo build -p cortex-cli --release
# binary: target/release/Cortex
```

On Linux, the optional audio and desktop crates need ALSA headers:
`sudo apt-get install -y libasound2-dev pkg-config`.

Full instructions, including Homebrew and WinGet, are in
[Getting started](./docs/guides/getting-started.md).

## Quick start

```bash
cd ~/code/my-project
cortex
```

That opens the session view from the recording above. Type what you want changed
and press `Enter`:

```
> add a /healthz endpoint and cover it with a test
```

Press `Esc` to interrupt a turn, `Shift+Tab` to change how much autonomy the
agent has, and `?` for help.

Prefer one-shot? Both of these work without a terminal:

```bash
cortex run "explain the release process"
cortex exec --auto read-only --git-diff "review my uncommitted changes"
```

## Login

```bash
cortex login
```

This opens the Cortex sign-in flow in your browser and stores the session in
your OS keyring. For machines without a browser:

```bash
cortex login --device-auth      # device-code flow
cortex login --sso              # enterprise SSO
cortex login --with-api-key     # read an API key from stdin
```

Check it worked with `cortex whoami`. See
[Signing in](./docs/reference/login.md) for the full picture.

## Documentation

| | |
|---|---|
| [Getting started](./docs/guides/getting-started.md) | Install, sign in, first session |
| [The TUI](./docs/guides/tui.md) | Timeline, composer, modes, approvals |
| [Sessions](./docs/guides/sessions.md) | Resume, export, import, share |
| [Headless / exec mode](./docs/guides/exec.md) | Scripts and CI |
| [Plan and Spec modes](./docs/guides/plan.md) | Approve a plan before anything changes |
| [Configuration](./docs/configuration/config.md) | Files, keys, profiles, permissions |
| [Agents](./docs/customization/agents.md) · [Skills](./docs/customization/skills.md) · [MCP](./docs/customization/mcp.md) · [Hooks](./docs/customization/hooks.md) · [Plugins](./docs/customization/plugins.md) | Extending Cortex |
| [CLI reference](./docs/reference/cli.md) · [Tools](./docs/reference/tools.md) · [Slash commands](./docs/reference/slash-commands.md) · [Keyboard](./docs/reference/keyboard.md) | Reference |
| [Troubleshooting](./docs/troubleshooting.md) | When something does not work |

The index is at [docs/README.md](./docs/README.md).

## Building and testing

```bash
cargo build -p cortex-cli
./target/debug/Cortex --help
```

The gates CI enforces:

```bash
cargo fmt --all -- --check
./scripts/clippy.sh
cargo test --workspace
cargo audit
./scripts/check-cli-version.sh
```

Headless TUI and snapshot tests, required whenever a TUI surface changes:

```bash
cargo test -p cortex-tui -p cortex-tui-capture -p cortex-tui-components \
  -p cortex-tui-framework -p cortex-tui-core -p cortex-tui-buffer \
  -p cortex-tui-widgets -p cortex-tui-layout -p cortex-tui-text \
  -p cortex-tui-input -p cortex-tui-terminal -p cortex-tui-syntax
```

The banner above is generated from this repository, not captured by hand:

```bash
./scripts/render-demo-gif.sh
```

That records the session view headlessly through `cortex-tui-capture` and
rasterises the frames into `docs/media/intro.gif`.

## Release and CI secrets

Merges to `main` run [`.github/workflows/version-bump.yml`](.github/workflows/version-bump.yml),
which patch-bumps the version and tags it. Tags run
[`release.yml`](.github/workflows/release.yml), which can publish to
[software.cortex.foundation](https://software.cortex.foundation) via
[`publish-r2.yml`](.github/workflows/publish-r2.yml).

This repository does not invent cloud accounts. The secret *names* CI expects are
listed in [docs/CI_SECRETS.md](./docs/CI_SECRETS.md). Values never go in git.

## Contributing

Start with [docs/CONTRIBUTING.md](./docs/CONTRIBUTING.md), then
[`AGENTS.md`](./AGENTS.md) and [`.rules/`](./.rules/). Every PR fills the
attestation list in
[`.github/PULL_REQUEST_TEMPLATE.md`](./.github/PULL_REQUEST_TEMPLATE.md).

## Licence

[Apache-2.0](./LICENSE).
