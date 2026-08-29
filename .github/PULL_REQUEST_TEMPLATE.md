## Summary

<!-- What does this PR change, and why? Product name is Cortex CLI / Cortex Code. -->

## Test plan

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace` (or note the subset and why)
- [ ] TUI / snapshot tests for every surface this PR touches
- [ ] `cargo audit` (or note a documented `.cargo/audit.toml` exception)

## Attestation (required)

I attest that:

- [ ] **Security reviewed** — auth, exec/sandbox, path traversal, network egress, and secret handling in this diff were reviewed. No secrets, tokens, or keyring dumps are in the change.
- [ ] **Product-facing errors** — user-visible failures use Cortex product copy. API-down paths say *The coding service is temporarily unavailable*. No raw provider, SDK, or transport names.
- [ ] **TUI verified** — every TUI surface touched in this PR was exercised (build + headless snapshot / ratatui-style test, and a real run when a TTY was available).
- [ ] **Tests added** — unit tests cover the new logic; TUI changes include a snapshot or buffer assertion. No mocks that report success.
- [ ] **No secrets** — no API keys, WorkOS secrets, R2/AWS credentials, or `.env` files are included.

## Risk

<!-- Auth, exec policy, sandbox, release, or API-contract impact. -->
