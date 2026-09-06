---
name: local-qa
description: Exercise the Cortex CLI and real loopback app server with isolated data and negative security cases, without contacting the coding service.
---

# Local functional QA

Read `docs/guides/development.md` and `docs/reference/app-server.md`.

Build the affected binaries with the locked graph, then run
`python3 scripts/readiness/qa.py`. This interacts with actual application
processes and local HTTP handlers; it does not fake model responses.

Review `target/readiness/qa/report.json` and the sanitized aggregate insights.
Report each completed flow and any failing assertion. A missing binary or
startup failure is blocked/failed evidence, never success.

For changes to the interactive TUI, this script is insufficient. Use the
existing headless tests and an available terminal interaction skill with a
dedicated approved test account. Do not invent credentials or bypass login.
If interactive or live API QA cannot run, say which flow is blocked and why.
Keep captures local. Do not publish evidence or create issues automatically.
