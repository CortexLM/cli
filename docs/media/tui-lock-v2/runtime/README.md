# Cortex CLI TUI lock v2 — runtime captures

Headless `MockTerminal` renders of the live session chrome (inky background,
dual-hairline composer, model chip, slash palette, settings modal, effort
radios). Regenerated with `./scripts/render-tui-lock-v2.sh`.

Designer boards (pixel target) live in `docs/media/tui-lock-v2/{40x12,120x40}/`.
These runtime frames are what Designer cli signs off against.

SPEC §7: **77** boards at 120×40 and **31** at 40×12. Each filename is one
distinct live state — no two PNGs share a sha256.
