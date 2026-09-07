# AGENTS.md — Cortex CLI / Cortex Code

Short contract for agents working in this repository. Prefer linking over restating runbooks.

**Product:** Cortex CLI / Cortex Code (`CortexLM/cli`) — a CLI and TUI coding-agent harness. It talks to the live Cortex API at [api.cortex.foundation](https://api.cortex.foundation) for Chat/Code turns, tools, computers, plugins, and snapshots. Auth is WorkOS session / existing `/v1` auth.

**Do not** write `Grok`, `Grok Bot`, or `Grok-core` in code, docs, PR titles, or UI copy.

Working branch: **`main`**. Version bumps land on `main` via PR; `.github/workflows/version-bump.yml` tags `v*.*.*` after a `chore: bump version to …` merge (or `workflow_dispatch` to tag `VERSION_CLI` / open a bump PR). Tags trigger `release.yml`, which publishes to R2 via `publish-r2.yml`.

## Workspace map

| Path | Role |
|------|------|
| `src/cortex-cli` | Binary (`Cortex`) and CLI commands |
| `src/cortex-engine` | Agent loop, API client, tools, MCP, skills, hooks |
| `src/cortex-protocol` | Shared protocol types |
| `src/cortex-tui` / `src/cortex-core` / `src/cortex-tui-*` | TUI application and capture/snapshot tests |
| `cortex-tui-framework/` | Headless-testable TUI primitive crates |
| `src/cortex-login` / `src/cortex-keyring-store` | WorkOS / `/v1` login and OS keyring |
| `src/cortex-exec*` / `src/cortex-sandbox*` | Exec policy and sandbox |
| `src/cortex-mcp-*` | MCP client/server/types |
| `docs/` | User and plugin docs |
| `.rules/` | Engineering rules (security, errors, TUI, tests, …) |
| `scripts/` | Version bump / consistency / release helpers |

## Non-negotiables

- **Product-facing errors only.** If the coding API is down, show *The coding service is temporarily unavailable*. Never leak raw provider, SDK, or transport names in UI, CLI, or logs that users see.
- **No mock-success.** Tests and health checks must fail when the thing under test failed. Do not stub a green path.
- **Secrets stay out of the repo.** Tokens live in the OS keyring or CI secrets. Document secret *names*, never values.
- **TUI surfaces you change must be tested** with a unit test plus at least one headless snapshot / ratatui-style test.
- **Keep the existing versioning scheme.** `VERSION_CLI` + `[workspace.package].version` + `src/cortex-cli/VERSION`. Do not invent a second scheme.
- **License is Apache-2.0.** Do not relicense crates to MIT.

## Commands (local)

```bash
cargo fmt --all -- --check
./scripts/clippy.sh
cargo test --workspace
cargo test -p cortex-tui -p cortex-tui-capture -p cortex-tui-components \
  -p cortex-tui-framework -p cortex-tui-core -p cortex-tui-buffer \
  -p cortex-tui-widgets -p cortex-tui-layout -p cortex-tui-text \
  -p cortex-tui-input -p cortex-tui-terminal -p cortex-tui-syntax
cargo audit
./scripts/check-cli-version.sh
```

Build and run the TUI:

```bash
cargo build -p cortex-cli
./target/debug/Cortex login    # WorkOS / /v1 against api.cortex.foundation
./target/debug/Cortex          # interactive TUI
```

Commit subjects: `type(scope): summary` (lowercase, ≤72 chars).

## Required gates (before merge)

Match CI (`.github/workflows/ci.yml`):

- `cargo fmt --all -- --check`
- `./scripts/clippy.sh`
- `cargo test --workspace`
- `cargo audit` (required; exceptions only in `.cargo/audit.toml` with rationale)
- TUI job for the framework + app surfaces
- `./scripts/check-cli-version.sh`
- PR attestation checklist in `.github/PULL_REQUEST_TEMPLATE.md`
- Source/dependency policy, API-contract freshness, local QA, and changed-line
  coverage in CI. Use the actual PR base SHA, never a self-baseline.

Local setup and the exact reporting/coverage commands:
[`docs/guides/development.md`](docs/guides/development.md).
Policy and existing-debt handling:
[`docs/guides/quality.md`](docs/guides/quality.md).
Diagnostics stay local, opt-in, and content-free:
[`docs/guides/operations.md`](docs/guides/operations.md).

## Where to read what

| Need | Start here |
|------|------------|
| User docs index | [`docs/README.md`](docs/README.md) |
| Security / exec / sandbox | [`.rules/security.md`](.rules/security.md) |
| Product-facing errors | [`.rules/errors.md`](.rules/errors.md) |
| TUI / responsive layout | [`.rules/tui.md`](.rules/tui.md) |
| Crate layout | [`.rules/structure.md`](.rules/structure.md) |
| Docs | [`.rules/docs.md`](.rules/docs.md) |
| API / product | [`.rules/api.md`](.rules/api.md) |
| Git / PRs | [`.rules/git.md`](.rules/git.md) |
| Testing | [`.rules/testing.md`](.rules/testing.md) |
| User install / login | [`README.md`](README.md) |
| Release / R2 secrets | [`docs/CI_SECRETS.md`](docs/CI_SECRETS.md) |

## Do not commit

- `.env`, API keys, WorkOS client secrets, keyring dumps
- `target/`, `.cargo-home/`, `.rustup/`
- Invented AWS account IDs, Cloudflare tokens, or Homebrew tap tokens
- Provider brand names (`Grok`, OpenAI, Anthropic, …) in user-visible copy
