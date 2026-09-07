---
name: verify-readiness
description: Verify Cortex source policy, dependency drift, generated contracts, and local test evidence before proposing a merge.
---

# Verify readiness

Read `AGENTS.md`, `.rules/testing.md`, `docs/guides/development.md`, and
`docs/guides/quality.md`. Work only in this repository.

1. Inspect the diff and preserve unrelated work.
2. Resolve the real target branch/base commit. Never use HEAD as a regression
   baseline just to pass the gate.
3. Run the narrow relevant Rust tests, policy unit tests, source/dependency
   checks, schema freshness, and coverage as documented.
4. Retain reports under `target/readiness/`. Report exit statuses, failed tests,
   existing debt, and unverified platforms. Do not replace failures with skips.
5. Do not commit, push, change GitHub settings, release, or post reports without
   an explicit request. Never attach raw sessions, debug logs, or credentials.
