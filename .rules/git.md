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
- Versioning: `.github/workflows/version-bump.yml` patch-bumps and tags on merge to `main`. Do not add another version bot.
- Do not commit `Cargo.lock` deletions. This is a binary workspace; the lockfile is source of truth.
- PR titles and bodies: Cortex CLI / Cortex Code. Never Grok.
