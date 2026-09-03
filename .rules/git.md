# Git and pull requests

- Default branch is `main`. Feature branches; no force-push to `main`.
- Commit subjects: `type(scope): summary` (lowercase, ≤72 chars).
- Every PR uses `.github/PULL_REQUEST_TEMPLATE.md` and must fill the attestation list:
  - security reviewed
  - product-facing errors
  - TUI verified for the surfaces touched
  - tests added
  - no secrets
- CI on PRs to `main` is required: fmt, clippy `-D warnings`, test, audit, TUI checks.
- Versioning: bump `VERSION_CLI` / `Cargo.toml` / `src/cortex-cli/VERSION` in a PR; merge tags `v*.*.*` via `.github/workflows/version-bump.yml`. Do not add another version bot or push directly to `main`.
- Do not commit `Cargo.lock` deletions. This is a binary workspace; the lockfile is source of truth.
- PR titles and bodies: Cortex CLI / Cortex Code. Never Grok.
