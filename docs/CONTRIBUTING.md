# Contributing

Thanks for wanting to work on Cortex CLI.

The short version: branch off `main`, keep commits small and conventionally
named, make the gates pass, and fill in the attestation checklist on your PR.

The engineering rules this repository holds itself to live in
[`.rules/`](../.rules/), and [`AGENTS.md`](../AGENTS.md) is the one-page summary.

## Reporting a bug

The fastest route is from the CLI, which attaches the context we would otherwise
have to ask for:

```bash
cortex feedback bug "describe what happened" --include-logs
```

If you open a GitHub issue instead, include:

1. **What you ran** — the exact command, including flags.
2. **What happened** — the complete error message, verbatim. `cortex --debug`
   writes full trace logs to `./debug.txt`.
3. **What you expected** instead.
4. **Your environment** — `cortex --version`, your OS and version, and how you
   installed Cortex. `cortex debug system` prints most of this.
5. **How to reproduce it**, as a numbered list.

Screenshots of the TUI help a lot for anything visual.

## Setting up

```bash
sudo apt-get install -y libasound2-dev pkg-config   # Linux only
cargo build -p cortex-cli
./target/debug/Cortex --help
```

Use the toolchain pinned in [`rust-toolchain.toml`](../rust-toolchain.toml).

## Making a change

1. Branch off `main`. Never force-push to `main`.
2. Write the change and the tests together.
3. Run the gates below.
4. Open a PR against `main` and complete
   [`.github/PULL_REQUEST_TEMPLATE.md`](../.github/PULL_REQUEST_TEMPLATE.md).

### Commit messages

`type(scope): summary` — lowercase, 72 characters or fewer.

```
feat(tui): show elapsed time on the status line
fix(mcp): reject private-network URLs without --allow-local
docs(reference): document the exec output formats
```

Common types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`.

## The gates

These are exactly what CI runs, so run them before you push:

```bash
cargo fmt --all -- --check
./scripts/clippy.sh
cargo test --workspace
cargo audit
./scripts/check-cli-version.sh
```

Plus the TUI suite, for any change that touches a TUI surface:

```bash
cargo test -p cortex-tui -p cortex-tui-capture -p cortex-tui-components \
  -p cortex-tui-framework -p cortex-tui-core -p cortex-tui-buffer \
  -p cortex-tui-widgets -p cortex-tui-layout -p cortex-tui-text \
  -p cortex-tui-input -p cortex-tui-terminal -p cortex-tui-syntax
```

## Things reviewers will hold you to

- **Tests that can fail.** A test that reports success when the thing under test
  did not work is worse than no test. If a test cannot reach a backend, it should
  assert the product-facing failure instead of stubbing a green path.
- **A TUI change needs two tests.** A unit test for the logic, and a headless
  snapshot or buffer assertion via `cortex-tui-capture` or `cortex-tui-buffer`.
- **Product-facing error copy.** User-visible failures use Cortex wording. When
  the coding service is unreachable the message is *The coding service is
  temporarily unavailable* — no provider, SDK or transport names anywhere a user
  can see.
- **No secrets, ever.** Credentials belong in the OS keyring or a CI secret
  store. Document secret *names* in [CI secrets](CI_SECRETS.md), never values.
- **Layouts that reflow.** Test a narrow viewport (40×12) and a wide one (120×40)
  when you change a TUI surface.
- **`cargo audit` stays clean.** Ignore an advisory only in `.cargo/audit.toml`,
  with a comment naming the crate and the reason.

## Documentation

User-facing documentation lives in [`docs/`](README.md) as plain markdown. There
is no second docs site in this tree, and English is the source language. If your
change adds or alters a command, flag, slash command or configuration key,
update the matching reference page in the same PR.

## Releases

Bump the version in a PR (`./scripts/bump-version.sh patch|minor|major`, commit
`chore: bump version to X.Y.Z`). Merging that PR runs
`.github/workflows/version-bump.yml`, which tags `vX.Y.Z` and triggers
`release.yml` (GitHub Release plus automatic R2 publish). Use
`workflow_dispatch` on the same workflow to open an automated bump PR or tag
the current `VERSION_CLI`. The version lives in `VERSION_CLI`,
`[workspace.package].version` and `src/cortex-cli/VERSION`, and
`./scripts/check-cli-version.sh` verifies the three agree — do not introduce a
second scheme.

## Licence

Cortex CLI is Apache-2.0. Contributions are accepted under the same licence.
