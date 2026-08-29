# CI secrets

This repository does not invent cloud accounts. The workflows imported from `CortexLM/cortex-code` already point at `cortex.foundation` / `software.cortex.foundation`. Configure these GitHub Actions secrets on `CortexLM/cli` only if the matching account already exists.

None of these values belong in git.

## Required for PR CI (`.github/workflows/ci.yml`)

No secrets. `fmt`, `clippy`, `test`, `audit`, and TUI jobs use the public crates.io index and `GITHUB_TOKEN`.

## Version bump (`.github/workflows/version-bump.yml`)

| Secret | Used for |
|--------|----------|
| `GITHUB_TOKEN` | Commit the bump on `main` and push tag `vX.Y.Z` (default Actions token is enough if repo settings allow) |

## Release artifacts (`.github/workflows/release.yml`)

Builds `Cortex` binaries and attaches them to the GitHub Release. Uses `GITHUB_TOKEN` only.

Optional Linux deps (`libasound2-dev`) are installed in the job; no extra secrets.

## Publish to software.cortex.foundation (`.github/workflows/publish-r2.yml`)

The existing R2 publisher uploads to bucket `cortex-software` and public URLs under `https://software.cortex.foundation/v1/assets/...`.

| Secret | Used for |
|--------|----------|
| `R2_ACCESS_KEY_ID` | Cloudflare R2 access key (already used by cortex-code) |
| `R2_SECRET_ACCESS_KEY` | Cloudflare R2 secret |
| `CLOUDFLARE_ACCOUNT_ID` | R2 endpoint `https://$CLOUDFLARE_ACCOUNT_ID.r2.cloudflarestorage.com` |

Do not add AWS account IDs, IAM users, or a second bucket name unless Cortex Foundation already operates them.

## Homebrew / WinGet (optional)

| Secret | Workflow | Used for |
|--------|----------|----------|
| `HOMEBREW_TAP_TOKEN` | `homebrew.yml` | Push formula updates to `CortexLM/homebrew-tap` |
| (WinGet) | `winget.yml` | Only if that workflow is enabled; see the file for the token name it already expects |

## Operator notes

- `CORTEX_API_KEY` / WorkOS credentials are **user** secrets for running the CLI, not CI secrets.
- Live API tests stay off in CI unless a job is explicitly gated on `CORTEX_LIVE_API=1` *and* a dedicated secret is added later. Do not put personal sessions in Actions.
