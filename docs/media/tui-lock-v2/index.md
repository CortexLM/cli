# Cortex CLI — TUI lock v2 · board index

Design-only pack for the full TUI redesign (reference layout language, Cortex chrome).
Every board exists at **120x40**; boards where density matters also at **40x12**.
Text dumps of each grid live in `txt/<size>/<board>.txt`. Spec: [`SPEC.md`](SPEC.md).

Regenerate: `python3 tools/render_lock_v2.py --index` (fetches IBM Plex Mono on first run).

## A. Entry / welcome

| Board | State | Wide | Narrow |
|---|---|---|---|
| `welcome-cortex` | Cold start `cortex` — clean welcome, alternate screen, no shell echoes | [120x40](120x40/welcome-cortex.png) | [40x12](40x12/welcome-cortex.png) |
| `welcome-agent` | Cold start `agent` — same chrome, agent wording + placeholder | [120x40](120x40/welcome-agent.png) | [40x12](40x12/welcome-agent.png) |
| `first-run-tips` | First launch — charcoal tips panel under the welcome | [120x40](120x40/first-run-tips.png) | [40x12](40x12/first-run-tips.png) |
| `session-empty` | Resumed / after first turn — header, composer, footer only | [120x40](120x40/session-empty.png) | [40x12](40x12/session-empty.png) |

## B. Session

| Board | State | Wide | Narrow |
|---|---|---|---|
| `session-user-bars` | Two user prompt bars + timestamps, Thought, reply, Worked, ▼ more, opt-in banner | [120x40](120x40/session-user-bars.png) | [40x12](40x12/session-user-bars.png) |
| `session-thought` | `♦ Thought for Xs` collapsed | [120x40](120x40/session-thought.png) | — |
| `session-thought-expanded` | Thought expanded (Show thinking blocks = on) | [120x40](120x40/session-thought-expanded.png) | — |
| `session-thinking-live` | Live `⠇ Thinking · 3s` while the turn runs | [120x40](120x40/session-thinking-live.png) | [40x12](40x12/session-thinking-live.png) |
| `session-assistant` | Plain reply with bold + bullets | [120x40](120x40/session-assistant.png) | [40x12](40x12/session-assistant.png) |
| `session-worked` | `Worked for Xs` after a reply | [120x40](120x40/session-worked.png) | — |
| `session-optin` | `Help improve Cortex` banner — Opt out | Opt in | [120x40](120x40/session-optin.png) | [40x12](40x12/session-optin.png) |
| `session-optin-hover` | Banner with the mouse over `[Opt in]` | [120x40](120x40/session-optin-hover.png) | — |
| `composer-empty` | Empty composer — caret before the placeholder, violet `>` | [120x40](120x40/composer-empty.png) | [40x12](40x12/composer-empty.png) |
| `composer-typing` | Mid-type, caret on | [120x40](120x40/composer-typing.png) | [40x12](40x12/composer-typing.png) |
| `composer-typing-blink` | Mid-type, caret off (blink phase) | [120x40](120x40/composer-typing-blink.png) | — |
| `composer-hover` | Mouse over the composer — hairline lifts to #525252 | [120x40](120x40/composer-hover.png) | [40x12](40x12/composer-hover.png) |
| `composer-multiline` | Alt+Enter newlines — box grows upward | [120x40](120x40/composer-multiline.png) | — |
| `footer-shortcuts` | Footer strip with text typed (4 hints) | [120x40](120x40/footer-shortcuts.png) | — |
| `footer-hover` | Mouse over `Ctrl+x:shortcuts` | [120x40](120x40/footer-hover.png) | — |
| `tokens-topright` | Token counter `142K / 500K` top-right | [120x40](120x40/tokens-topright.png) | [40x12](40x12/tokens-topright.png) |
| `tokens-topright-warn` | Counter ≥ 90 % — amber + /compact hint | [120x40](120x40/tokens-topright-warn.png) | — |
| `compact-chat` | Compact mode — edge-to-edge bars, no timestamps | [120x40](120x40/compact-chat.png) | [40x12](40x12/compact-chat.png) |

## C. Slash + model

| Board | State | Wide | Narrow |
|---|---|---|---|
| `slash-palette` | `/` palette — focused row + hover row + `… more` trailer | [120x40](120x40/slash-palette.png) | [40x12](40x12/slash-palette.png) |
| `slash-model-typed` | `/mod` typed — violet matched chars, ghost completion | [120x40](120x40/slash-model-typed.png) | [40x12](40x12/slash-model-typed.png) |
| `model-list` | `/model` — Cortex Mini 1 · Cortex 1 · Cortex Max 1 | [120x40](120x40/model-list.png) | [40x12](40x12/model-list.png) |
| `model-list-hover` | Model list with mouse over row 3 | [120x40](120x40/model-list-hover.png) | — |
| `model-effort-high` | Effort radios — High focused | [120x40](120x40/model-effort-high.png) | [40x12](40x12/model-effort-high.png) |
| `model-effort-medium` | Effort radios — Medium focused | [120x40](120x40/model-effort-medium.png) | — |
| `model-effort-low` | Effort radios — Low focused | [120x40](120x40/model-effort-low.png) | — |
| `model-effort-hover` | Effort radios — Medium focused, mouse over Low | [120x40](120x40/model-effort-hover.png) | — |

## D. Settings

| Board | State | Wide | Narrow |
|---|---|---|---|
| `settings-appearance` | Settings modal — Appearance, Compact mode focused | [120x40](120x40/settings-appearance.png) | [40x12](40x12/settings-appearance.png) |
| `settings-mouse` | Settings scrolled to Mouse / Behavior | [120x40](120x40/settings-mouse.png) | [40x12](40x12/settings-mouse.png) |
| `settings-row-hover` | Keyboard focus on Compact mode, mouse over Show timestamps | [120x40](120x40/settings-row-hover.png) | [40x12](40x12/settings-row-hover.png) |
| `settings-search` | `/ scro` search — filtered rows, violet match | [120x40](120x40/settings-search.png) | — |
| `settings-theme-submenu` | Theme submenu — Cortex Night / Cortex Day / Ocean Dark / Monokai | [120x40](120x40/settings-theme-submenu.png) | [40x12](40x12/settings-theme-submenu.png) |

## E. Modes / tools / errors

| Board | State | Wide | Narrow |
|---|---|---|---|
| `mode-agent` | Agent mode — dim chip in the composer border | [120x40](120x40/mode-agent.png) | — |
| `mode-plan` | Plan mode — `Plan · no edits` chip, plan reply | [120x40](120x40/mode-plan.png) | [40x12](40x12/mode-plan.png) |
| `mode-ask` | Ask mode — `Ask · read-only` chip | [120x40](120x40/mode-ask.png) | [40x12](40x12/mode-ask.png) |
| `mode-bash` | Bash mode (`!`) — chip + `!` sigil | [120x40](120x40/mode-bash.png) | — |
| `permission-prompt` | Exec approval — command on gray, numbered radios | [120x40](120x40/permission-prompt.png) | [40x12](40x12/permission-prompt.png) |
| `permission-prompt-hover` | Exec approval with mouse over option 2 | [120x40](120x40/permission-prompt-hover.png) | — |
| `permissions-picker` | `/permissions` — Smart / Read-only / Full access | [120x40](120x40/permissions-picker.png) | — |
| `mcp-servers` | `/mcp` — connected ✓, authenticating, failed × | [120x40](120x40/mcp-servers.png) | [40x12](40x12/mcp-servers.png) |
| `mcp-drop` | MCP server dropped mid-turn (error red) | [120x40](120x40/mcp-drop.png) | — |
| `plugins` | `/plugins` — enabled / disabled rows | [120x40](120x40/plugins.png) | — |
| `usage` | `/usage` — plan bars | [120x40](120x40/usage.png) | [40x12](40x12/usage.png) |
| `quota-exhausted` | Agent quota exhausted — red title, held composer | [120x40](120x40/quota-exhausted.png) | — |
| `sandbox` | `/sandbox` — filesystem / network / escalation | [120x40](120x40/sandbox.png) | — |
| `sandbox-deny` | Sandbox blocked a command — red title, radios | [120x40](120x40/sandbox-deny.png) | — |
| `cloud-handoff` | `&` handoff to Cortex Cloud | [120x40](120x40/cloud-handoff.png) | — |
| `diagnostics` | Diagnostics tile — error red, warn amber | [120x40](120x40/diagnostics.png) | [40x12](40x12/diagnostics.png) |
| `interrupt-stopped` | Esc / Ctrl+c — `× Stopped` | [120x40](120x40/interrupt-stopped.png) | [40x12](40x12/interrupt-stopped.png) |
| `error-unavailable` | API down — product-facing error | [120x40](120x40/error-unavailable.png) | — |
| `tool-tiles` | Grouped tool calls expanded — Read / Grep / Shell | [120x40](120x40/tool-tiles.png) | — |
| `tool-tiles-collapsed` | Grouped tool calls collapsed | [120x40](120x40/tool-tiles-collapsed.png) | — |
| `shell-running` | Live Shell tile with output | [120x40](120x40/shell-running.png) | — |
| `diff-hunk` | Edit tile + unified hunk — green +, red − | [120x40](120x40/diff-hunk.png) | [40x12](40x12/diff-hunk.png) |
| `edit-collapsed` | Collapsed edit blocks (setting on) | [120x40](120x40/edit-collapsed.png) | — |
| `md-table` | Markdown table — gray plus-ASCII grid | [120x40](120x40/md-table.png) | — |
| `code-fence` | Fenced code — language tag hairline, gutter, bold keywords | [120x40](120x40/code-fence.png) | — |
| `login` | `cortex login` — inline picker | [120x40](120x40/login.png) | [40x12](40x12/login.png) |
| `login-waiting` | Waiting for browser + device code | [120x40](120x40/login-waiting.png) | — |
| `login-success` | `✓ Signed in` | [120x40](120x40/login-success.png) | — |
| `login-error` | Sign-in failed — product copy | [120x40](120x40/login-error.png) | — |
| `shortcuts-overlay` | Ctrl+x shortcuts overlay | [120x40](120x40/shortcuts-overlay.png) | [40x12](40x12/shortcuts-overlay.png) |
| `resume-picker` | `/resume` — search field + session rows | [120x40](120x40/resume-picker.png) | — |
| `clear-confirm` | `/clear` confirm radios | [120x40](120x40/clear-confirm.png) | — |
| `plan-confirm` | `Implement this plan?` radios | [120x40](120x40/plan-confirm.png) | — |
| `queue` | Follow-up queue while a step runs | [120x40](120x40/queue.png) | — |
| `files-picker` | `@` file picker — violet matched chars, hover row | [120x40](120x40/files-picker.png) | — |
| `jobs` | `/jobs` — cloud agent, subagent, queued | [120x40](120x40/jobs.png) | — |
| `skills` | `/skills` — search field + skill rows | [120x40](120x40/skills.png) | — |
| `todos` | Working 2/5 checklist — ✓ done · › current · ○ pending | [120x40](120x40/todos.png) | — |
| `question` | Clarifying question radios | [120x40](120x40/question.png) | — |
| `sudo` | Elevated Shell — password row on gray | [120x40](120x40/sudo.png) | — |
| `config-tree` | `/config` read-only key tree | [120x40](120x40/config-tree.png) | — |
| `btw` | `/btw` side note during a running turn | [120x40](120x40/btw.png) | — |

**77** boards at 120x40 · **31** at 40x12 · 108 PNGs.
