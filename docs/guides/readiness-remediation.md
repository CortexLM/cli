# Readiness remediation snapshot

Local changes on `chore/agent-readiness`, based on
`2e24cba2ed05d2833edaf06e9e1a34d0f65d6cc5`. This is a validation snapshot,
not a new readiness score or a claim that all historical criteria now pass.
It records validation before publication; subsequent commits and GitHub CI
results belong to the pull request. No release was made.

## Implemented

- Source complexity, large-file, duplication, feature-consumer, dependency
  inheritance, unused-dependency, and minimum-release-age controls.
- Nextest timing/JUnit reports, independent stability repetitions, and an
  enforced 80% floor on changed production lines in CLI/server/common.
- Locked local setup, development container, local doctor, and real-process
  CLI/server functional and dynamic security QA.
- Actual server authentication/middleware, JWT/admin checks, request limits,
  workspace file boundaries, WebSocket authentication, and safe error handling.
- Opt-in local diagnostics, request correlation, live metrics, local aggregate
  alerts, version comparisons, privacy guidance, and a symbol-preserving profile.
- Generated CLI/OpenAPI contracts, documentation checks, repository skills,
  ownership, triage guidance, and dependency-update configuration.
- Corrected the CLI plugin-version/verbosity collision, placeholder feature
  initialization, stale doctest imports, and an asynchronous append visibility
  race exposed by the full test run. No sleeps, retries, or ignored tests were
  added to conceal failures.

The authorized GitHub configuration changes were applied: main requires one
approval, and four priority plus six area labels exist. CODEOWNERS and new CI
controls require the branch to be reviewed and published.

## Verified locally

| Check | Result |
| --- | --- |
| `cargo test --locked --offline --workspace --no-fail-fast` | Passed, including doctests and headless TUI tests |
| Nextest workspace, three repetitions | 6,039 passed per run; no observed flaky tests; 19 pre-existing skips per run |
| Local functional/security QA | 14 cases passed against real processes |
| Changed-line production coverage | 750/879 executable lines, 85.32%; no missing production files |
| Python policy tests | 25 passed |
| Source/dependency policy | 18,535 functions analyzed; zero regressions or policy failures |
| Formatting, Clippy, version and whitespace checks | Passed |
| Cargo audit | Passed against 1,239 loaded advisories, without new exceptions |
| Unused dependencies / release-age policy | Passed; two newly locked releases checked |
| Generated contracts | Fresh |
| Development image | Built; unprivileged, network-disabled prerequisite smoke passed |
| Profiling profile | `cargo check --profile profiling -p cortex-common` passed |

Reproducible commands are in [development](development.md) and
[source quality](quality.md). Detailed local evidence is under ignored
`target/readiness/`. Diagnostic journals and aggregates have no upload step.

## Remaining limits

- The scanner retains **178 inherited findings**, including unconsumed-feature
  candidates. A no-new-debt gate is not elimination of existing debt or proof
  that every dynamic feature consumer is covered.
- Interactive authenticated TUI/coding-service QA requires a dedicated approved
  test account and an installed interaction driver. Neither a successful model
  turn nor a complete Droid Control QA installation was verified. Local message
  storage tests do not stand in for model inference.
- Cross-service tracing, remote alerting, and organization-wide product
  analytics were not enabled. Diagnostics remain local-only as requested.
- Release frequency and backlog health need genuine activity over time.
  No releases or issues were fabricated to improve these metrics.
- The complete editor-driven Dev Containers post-create flow, CPU sampling
  permissions, semantic workflow lint, and non-Linux platforms were not verified.
  JSON/YAML parsing and the local container smoke are narrower checks.
- The original persisted readiness report has not been rescored. At the time of
  this snapshot, these CI controls had not yet run on GitHub.
