# AWS CodeBuild CI for CortexLM/cli

Public-repo CodeBuild integration for Linux **x64** and **arm64**. GitHub
Actions assumes a dedicated IAM role with **OIDC** (no long-lived AWS keys)
and starts the projects. Each project posts a commit status comparable to
the CortexLM/backend checks `cortex-gha-x64` / `cortex-gha-arm64`.

Marker: `CLI_CODEBUILD_CI_READY`

Existing workflows stay in place: `ci.yml`, `release.yml`, `publish-r2.yml`,
`homebrew.yml`, `winget.yml`, `version-bump.yml`, `test-stability.yml`.
CodeBuild **extends** them. It does not replace R2 publishing, version
bumps, or macOS/Windows release jobs. Staging/prod app deploy remains
unchanged (prod HOLD).

Windows CodeBuild is **out of scope**. Compliance treats Windows CI as
outside the production gate; keep `windows-latest` on GitHub-hosted runners
in `ci.yml` / `release.yml` until a separate follow-up.

Do not commit AWS account IDs, access keys, PATs, or internal hostnames.

## Status checks (branch protection)

After the one-time AWS setup below, add these **required** checks on `main`:

| Context | Push/`main` project (S3 cache) | Pull-request project (no cache) | Arch |
|---------|--------------------------------|----------------------------------|------|
| `cortex-cli-gha-x64` | `cortex-cli-gha-x64` | `cortex-cli-gha-x64-pr` | Linux x86_64 |
| `cortex-cli-gha-arm64` | `cortex-cli-gha-arm64` | `cortex-cli-gha-arm64-pr` | Linux aarch64 |

Unapproved same-repository pull requests start the `*-pr` projects only.
Those projects use a **logs-only** service role and `NO_CACHE`. They cannot
read, write, or delete the shared cargo cache. The workflow also loads
`buildspec-ci.yml` from the PR **base** (or the pushed `main` SHA), not
from the unapproved head.

Keep the existing `ci.yml` checks (`Format`, `Clippy`, `Test`, `TUI checks`,
`Security Audit`, `Source and dependency policy`, `Changed-line coverage`,
`CLI Version and Distribution`, `CI Success`). Do not remove them in this
change. After CodeBuild is required and stable, a later PR can slim the
duplicate GitHub-hosted Linux cargo jobs.

Same-repo PRs start the `*-pr` projects. Pushes to `main` start the
cached projects. Fork PRs keep using GitHub-hosted `ci.yml` only (OIDC
is not granted to forks).

## Prefer existing org projects?

If this AWS account already hosts backend projects `cortex-gha-x64` /
`cortex-gha-arm64`, **reuse the GitHub OIDC provider** and the account, not
the projects. A CodeBuild project has one source/buildspec; do not point
backend projects at this public CLI repo. Create dedicated
`cortex-cli-gha-*` projects. Override names only via GitHub **variables**
if an admin already created equivalent CLI projects.

## One-time admin setup

### 1. Reuse or create the GitHub OIDC provider

In the AWS account that already runs CortexLM/backend CodeBuild (or a new
account dedicated to public CLI CI):

1. IAM → Identity providers → `token.actions.githubusercontent.com`.
2. If it exists, **do not recreate it**. Continue to the role.
3. If it does not exist, create it:
   - Provider URL: `https://token.actions.githubusercontent.com`
   - Audience: `sts.amazonaws.com`
   - Or pass `CreateGithubOidcProvider=true` to the stack below.

### 2. Deploy the stack (recommended)

From a workstation that can assume an admin role (never from this repo's
CI, and never with keys committed here):

```bash
aws cloudformation deploy \
  --stack-name cortex-cli-codebuild \
  --template-file deploy/aws/codebuild/cloudformation.yaml \
  --capabilities CAPABILITY_NAMED_IAM \
  --parameter-overrides \
    GitHubOrgRepo=CortexLM/cli \
    ProjectNameX64=cortex-cli-gha-x64 \
    ProjectNameArm64=cortex-cli-gha-arm64 \
    GhaRoleName=cortex-cli-codebuild-gha \
    CreateGithubOidcProvider=false
```

Copy the `GithubActionsRoleArn` output. It contains the account ID; store
it as a GitHub **variable**, not in git.

### 3. Manual IAM if you do not use CloudFormation

1. Create role `cortex-cli-codebuild-gha`.
2. Trust policy: `iam-trust-policy.json` with `ACCOUNT_ID` replaced at
   deploy time. Subjects must be only
   `repo:CortexLM/cli:ref:refs/heads/main` and
   `repo:CortexLM/cli:pull_request`.
3. Permissions: `iam-gha-permissions.json` with `ACCOUNT_ID` and `REGION`
   replaced. Actions are only `codebuild:StartBuild`,
   `codebuild:BatchGetBuilds`, and `logs:GetLogEvents` on the two CLI
   projects.
4. Create cached projects `cortex-cli-gha-x64` / `cortex-cli-gha-arm64`
   (`BUILD_GENERAL1_LARGE`, source **NO_SOURCE**, S3 cache, 90 minute
   timeout) and matching `*-pr` projects with **NO_CACHE**.
5. Cached-project service role: CloudWatch Logs plus read/write on the
   cache bucket. PR-project service role: CloudWatch Logs **only**. No
   deploy, no R2, no production secrets.

### 4. GitHub repository variables (not secrets)

On `CortexLM/cli` → Settings → Secrets and variables → Actions → Variables:

| Variable | Value |
|----------|--------|
| `AWS_CODEBUILD_ROLE_ARN` | `GithubActionsRoleArn` stack output |
| `AWS_REGION` | Region of the stack (default in the workflow is `us-east-1`) |
| `AWS_CODEBUILD_PROJECT_X64` | Optional override; default `cortex-cli-gha-x64` |
| `AWS_CODEBUILD_PROJECT_ARM64` | Optional override; default `cortex-cli-gha-arm64` |

Do **not** add `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`. Do not put
staging or production app secrets on these projects.

Until `AWS_CODEBUILD_ROLE_ARN` is set, `.github/workflows/codebuild.yml`
validates the in-repo assets and **skips** StartBuild. It does not post a
green `cortex-cli-gha-*` status for that skip (no mock-success).

### 5. Require the checks

Branch protection / ruleset on `main`: require
`cortex-cli-gha-x64` and `cortex-cli-gha-arm64` in addition to the
existing `ci.yml` jobs. Require these only after a successful StartBuild
has been observed on a test PR.

## What CodeBuild runs

`buildspec-ci.yml` clones the public `CortexLM/cli` commit over HTTPS
(no PAT) and runs `run-ci.sh`:

- `cargo fmt --all -- --check`
- `./scripts/clippy.sh`
- `./scripts/check-cli-version.sh`
- `python3 scripts/readiness/tests.py`
- `cargo test --locked --workspace --doc`
- `python3 scripts/readiness/schema.py`
- `cargo build --locked -p cortex-cli -p cortex-app-server`
- `python3 scripts/readiness/qa.py`
- headless TUI / snapshot packages (same set as `ci.yml`)
- changed-line coverage against the real PR base SHA

Cargo registry, git, rustup, and `target/` are cached in S3.

## Follow-up (Windows)

Not in this change. If Windows CodeBuild is added later, use a separate
project and a non-required check. Do not block production on it.
