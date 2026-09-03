# CI secrets

This repository does not invent cloud accounts. Configure these GitHub Actions secrets and variables on `CortexLM/cli` only if the matching Cortex Foundation account already exists.

None of these values belong in git. Do not add AWS access keys or an IAM user for CLI binary publishing.

## Required for PR CI (`.github/workflows/ci.yml`)

No secrets. `fmt`, `clippy`, `test`, `audit`, and TUI jobs use the public crates.io index and `GITHUB_TOKEN`.

## Version bump (`.github/workflows/version-bump.yml`)

| Secret | Used for |
|--------|----------|
| `GITHUB_TOKEN` | Commit the bump on `main` and push tag `vX.Y.Z` (default Actions token is enough if repo settings allow) |

## Release artifacts (`.github/workflows/release.yml`)

Fires on tag `v*` / `cli-v*` and on `workflow_dispatch` with a version. Builds `Cortex` binaries, creates a GitHub Release, then **automatically** calls `.github/workflows/publish-r2.yml` (`workflow_call`) so the same artifacts are published to `https://software.cortex.foundation`.

The GitHub Release job uses `GITHUB_TOKEN` only. Optional Linux deps (`libasound2-dev`) are installed in the build job; no extra secrets.

## Publish to software.cortex.foundation (`.github/workflows/publish-r2.yml`)

Production publisher. Uploads to Cloudflare R2 bucket `cortex-software` using **R2 API tokens** (not AWS IAM, not GitHub OIDC). Public URLs stay under `https://software.cortex.foundation`.

Called automatically after a successful `release` job. Can also be run by hand (`workflow_dispatch`) to republish, passing the release workflow's `release_run_id`.

| Secret | Used for |
|--------|----------|
| `R2_ACCESS_KEY_ID` | Cloudflare R2 access key for bucket `cortex-software` |
| `R2_SECRET_ACCESS_KEY` | Cloudflare R2 secret |
| `CLOUDFLARE_ACCOUNT_ID` | R2 S3 endpoint `https://$CLOUDFLARE_ACCOUNT_ID.r2.cloudflarestorage.com` |

No GitHub Actions variables are required for this path. Do not configure `github-production-deploy`, `PRODUCTION_SOFTWARE_BUCKET`, or AWS keys for CLI binaries.

### Public URL map

Bucket root is the host root (`software.cortex.foundation`):

| Object | Public URL |
|--------|------------|
| Unix installer | `https://software.cortex.foundation/install.sh` |
| Windows installer | `https://software.cortex.foundation/install.ps1` |
| Channel manifest | `https://software.cortex.foundation/releases/manifest.json` |
| Per-version JSON | `https://software.cortex.foundation/releases/<version>.json` |
| Platform archive | `https://software.cortex.foundation/v1/assets/<platform>/<version>/cortex.tar.gz` (`.zip` on Windows) |
| Archive checksum | same path + `.sha256` |

Aliases also written for `cortex upgrade`: `/v1/releases/manifest.json`, `/v1/releases/<version>.json`, `/v1/releases/latest.json`.

Platform keys: `linux-x86_64`, `linux-aarch64`, `darwin-x86_64`, `darwin-aarch64`, `windows-x86_64`.

`cortex upgrade` and the installers parse the same `ReleaseInfo` / `manifest.json` JSON. Tag versions that contain `-` (prerelease) publish to the `beta` channel; otherwise `stable`.

## Homebrew / WinGet (optional)

| Secret | Workflow | Used for |
|--------|----------|----------|
| `HOMEBREW_TAP_TOKEN` | `homebrew.yml` | Push formula updates to `CortexLM/homebrew-tap` |
| (WinGet) | `winget.yml` | Only if that workflow is enabled; see the file for the token name it already expects |

## Operator notes

- `CORTEX_API_KEY` / WorkOS credentials are **user** secrets for running the CLI, not CI secrets.
- Live API tests stay off in CI unless a job is explicitly gated on `CORTEX_LIVE_API=1` *and* a dedicated secret is added later. Do not put personal sessions in Actions.
