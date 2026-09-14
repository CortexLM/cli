# Cortex CLI TUI lock v2 — runtime captures

Headless `MockTerminal` renders of the live session chrome (inky background,
dual-hairline composer, model chip, slash palette, settings modal, effort
radios). Regenerated with `./scripts/render-tui-lock-v2.sh`. The signed accent
is banner green `#1F4945`; historical violet `#A78BFA` is not the lock.

Designer boards (pixel target) live in `docs/media/tui-lock-v2/{40x12,120x40}/`.
These runtime frames are what Designer cli signs off against.

SPEC §7 plus the COR-35 batch: **110** boards at 120×40 and **58** at 40×12. Each
filename is one distinct live state — no two PNGs share a sha256. Includes
`/goal` composer chips (`goal-chip-*`), distinct `offline` and `rate-limit`
diagnostics, Computer lock boards (`computer-disconnected`,
`computer-cloud-default`), local-tools consent, composer `@file` chip, `/undo`
`/redo` `/rewind` sheet, runtime-only COR-18/225/227/228 scenes
(`theme-picker`, `handoff-confirm`, `session-fork`, `init-agents`,
`custom-commands`, `hooks-lifecycle`), the `session-shared` status-line marker
while a read-only `/share` link is live, and the COR-35 batch (`bare-ci`,
`ci-cookbook`, `permission-rules`, `checkpoint-rewind`, `json-schema`,
`cloud-teleport`, `review-only`, `plugin-marketplace`, `sandbox-allowlist`,
`auto-approval`, `pr-apply-back`, `acp-editor`, `stdin-multiturn`).
