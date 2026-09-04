# The TUI

`cortex` with a terminal attached starts the interactive UI. This page describes
what is on screen and how to drive it.

## Starting it

```bash
cortex                          # current directory
cortex "review the auth module" # start with a prompt already submitted
cortex --cd /path/to/project    # use another directory as the working root
cortex --model <model-id>       # pick a model for this session
cortex --profile work           # load a profile from config.toml
```

The TUI needs a terminal on both stdin and stdout. If either is redirected it
refuses to start and points you at [`cortex run` or `cortex exec`](exec.md).

The session runs **inline** in the host terminal (`alternate_screen` is
never on by default). Cortex does not enter the alternate screen buffer
on start, so your shell prompt and the typed command stay visible above
the app. The welcome splash is two lines:
`Welcome to **Cortex**, the coding agent CLI` then
`v{version} · / commands · @ files · ! shell · & cloud`. It does
not paint a fake shell prompt or working directory. After the first user
turn the splash is dropped; an empty session is composer and footer only.

To opt in to a full-screen alternate buffer:

```bash
cortex --alternate-screen
```

or in `~/.cortex/config.toml`:

```toml
[tui]
alternate_screen = true
```

## The session view

```
┌──────────────────────────────────────────────────────┐
│ timeline — welcome card, then the transcript         │
│                                                      │
│ status — spinner, what the agent is doing, elapsed   │
│ ╭──────────────────────────────────────────────────╮ │
│ │ > composer                                       │ │
│ ╰──────────────────────────────────────────────────╯ │
│ key hints                            mode · autonomy │
└──────────────────────────────────────────────────────┘
```

### Timeline

The timeline is the transcript. It renders several kinds of row:

| Row | Looks like | Meaning |
|-----|-----------|---------|
| Your prompt | `> …` in the accent colour | A message you submitted |
| Agent prose | Rendered markdown, with syntax-highlighted code | The model's reply, streamed |
| Tool call | `◐`/`●` then the tool name and a short argument summary | A tool the agent invoked |
| Tool result | `⎿ …` indented under the call | What the tool returned |
| Subagent task | `● Task <type>` with a todo list underneath | Work delegated to a subagent |
| Welcome | Two lines: `Welcome to Cortex, the coding agent CLI` then `v{version} · / commands · …`. No mascot, no boxes, no fake `> cortex` prompt. Dropped after the first user turn. | Shown only before the first user turn |

Tool rows collapse to a summary. Press `e` while the timeline has focus to
expand or collapse the details of the selected tool call.

### Status line

While a turn is in flight, a status line sits above the composer with a spinner,
a header, the elapsed time and `Esc to interrupt`. The header tells you what
stage the turn is in — `Thinking`, `Executing <tool>`, `Streaming..`,
`Delegation`, `Execute` or `Idle`.

### Composer

The empty composer keeps a white block cursor at input column 0 after `> `;
the dim placeholder (`Plan, search, build anything`) sits in the next cell.
The placeholder is paint-only and does not move the caret. While typing, the
placeholder is hidden, copy is `#F5F5F5`, and the block follows the caret.
Blink (~530ms) hides the block; the placeholder then starts at column 0.
`Enter` sends.
`Shift+Enter` inserts a newline. `Up` and `Down` walk your prompt
history. If you submit while a turn is running the message is queued, and the
composer shows how many are waiting.

Typing `/` opens the command list; typing `@` references a file or symbol. Both
autocomplete in a popup under the composer. A line starting with `/` that is not
a known command is sent to the agent as an ordinary message.

### Key hints and the mode indicator

The bottom line lists the most useful bindings and, on the right, the current
operation mode and autonomy level.

## Modes

### Operation mode

| Mode | Indicator | What it means |
|------|-----------|---------------|
| Build | `BUILD` / `[B]` | Full access. The agent reads, writes and runs commands. |
| Plan | `PLAN` / `[P]` | Read-only. The agent investigates and proposes, but does not change anything. |
| Spec | `SPEC` / `[S]` | Specification mode. Mutating tools stay locked until a plan is approved. |

Modes cycle Build → Plan → Spec → Build. `/spec` toggles specification mode
directly. See [Plan and Spec modes](plan.md).

### Autonomy

Autonomy controls how often you are asked to approve a tool call. `Shift+Tab`
cycles it; the current value shows on the right of the hints line.

| Shown as | Meaning |
|----------|---------|
| `yolo` | Allow everything |
| `low` | Low autonomy |
| `medium` | Medium autonomy |
| `high` | High autonomy |

`/approval <ask|session|always|never>` sets the approval behaviour explicitly,
and `/auto [on|off]` toggles auto-approval. `/sandbox [on|off]` toggles sandboxed
execution for tools.

## Approvals

When the agent wants to run something that needs your say-so, an approval
overlay replaces the composer:

| Key | Action |
|-----|--------|
| `y` or `Enter` | Approve this call |
| `n` or `Esc` | Reject this call |
| `s` | Approve for the rest of the session |
| `a` | Always allow |
| `Shift+A` | Approve all pending |
| `Shift+R` | Reject all pending |
| `d` | Show the diff |

## Modals and panels

Most of the surface area is reachable from the command palette (`Ctrl+K`) or a
slash command:

| Surface | How to open |
|---------|-------------|
| Command palette | `Ctrl+K` or `Ctrl+P`, or `/palette` |
| Sessions | `Ctrl+S` or `Ctrl+O`, or `/sessions` |
| Models | `Ctrl+M`, or `/models` |
| MCP servers | `Ctrl+E`, or `/mcp` |
| Transcript | `Ctrl+T`, or `/transcript` |
| Help | `?` or `F1`, or `/help` |
| Settings | `/settings` |
| Theme picker | `/theme` |
| Background tasks | `/tasks` |
| Rewind overlay | Press `Esc` twice quickly |

## Interrupting and quitting

- `Esc` interrupts the current turn. The timeline shows **Cancelled.** and the
  CLI aborts the local stream (and POSTs cancel when the API supports it).
- `Esc` twice in quick succession opens the rewind overlay, where you can step
  back to an earlier point in the conversation or fork from it.
- `Ctrl+C` force-quits. `Ctrl+Q` quits. `/quit` also works.

## When the service is unreachable

If the coding service cannot be reached, the TUI says **The coding service is
temporarily unavailable**. That is the whole message by design — Cortex does not
surface provider, SDK or transport names in the UI. See
[Troubleshooting](../troubleshooting.md).

## See also

- [Slash commands](../reference/slash-commands.md) — the full list
- [Keyboard shortcuts](../reference/keyboard.md) — every binding, by context
- [Themes](../customization/themes.md) — switching the colour scheme
- [Sessions](sessions.md) — what happens to a conversation after you close it
- [Visual lock captures](../media/tui-lock/README.md) — headless PNGs of splash, login, palette, and tiles
