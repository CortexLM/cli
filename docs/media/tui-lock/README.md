# Cortex CLI visual lock captures

Headless `MockTerminal` renders of the session, login, slash palette, and
settings hub on the gray chrome. Regenerated with
`./scripts/render-tui-lock.sh`.

Each scene is captured at **40×12** (narrow) and **120×40** (wide), twice:

- `40x12/`, `120x40/` — raw terminal pixels. The background is the host
  terminal's (`Color::Reset`, black by default); nothing paints a wash and no
  rounded frame is drawn — the TUI bleeds to the terminal edges.
- `macos/40x12/`, `macos/120x40/` — the same captures composited into the
  designer macOS chrome template: Sequoia ray wallpaper, menu bar, and a
  Terminal.app window (traffic lights, `cortex-api — cortex — W×H` proxy
  title, native shadow). TUI pixels are never resampled: 120×40 captures are
  pasted 1:1 into the window's black content rect (the chrome renders at
  2.5×), 40×12 captures keep the same chrome with a smaller content rect at an
  integer 2× nearest-neighbour blow-up — so every locked colour survives
  pixel-exact. Nothing is re-typed, and the rounded corners belong to the
  macOS window chrome only.

Chrome rules: structure is gray — `#3A3A3A` hairlines above and below the
`> ` composer and around search fields, `#141414` charcoal panels for tips,
`#1C1C1C` bars behind past user turns, `#6B7280` secondary copy, white
primary copy. The one accent is cyan `#7DD3FC`, on the focused selection only
(the `>` caret and the selected label on the `#262626` bar, never inverted);
unselected rows lead with a dim middot. Green `#4ADE80` appears only on `✓`
and `+N` diff additions, red / amber only on diagnostics, and the Thinking
status is the muted gold `#C9A95C`. The footer is the model on the left and
one shortcut hint on the right, all gray.

Splash copy in these frames is the lock line `Cortex CLI v1.0.0`. The shipped
binary still reports the crate version.

| File | Surface |
|------|---------|
| `splash.png` | Empty session chrome: cwd, `> cortex`, version, hairline composer |
| `typing.png` | Prompt typed in the composer, block cursor |
| `model_compact.png` | `/model` compact picker, hairline search field |
| `model_full.png` | `/model` full picker: search field, descriptions, effort, billing note |
| `mode.png` | `/mode` Agent / Plan / Ask radios |
| `permissions.png` | `/permissions` approval policy picker |
| `working.png` | Working spinner + elapsed / tokens |
| `read.png` | Read tile — numbered excerpt |
| `login_select.png` | Sign-in picker with the selection moved to option 2 (`> 2 Paste an API key`) |
| `login_waiting.png` | Loading / waiting for browser auth |
| `login_success.png` | `✓ Signed in.` |
| `login_error.png` | Product-facing error |
| `palette.png` | `/` typed in the composer, twenty aligned commands |
| `palette_empty.png` | `/` filter with no matches |
| `settings_hub.png` | `/settings` seven-row hub |
| `settings_empty.png` | Settings filter with no matches |
| `tool_tiles.png` | Grep tile (one card at a time) |
| `diagnostics.png` | Diagnostics tile |
| `multi_diff.png` | `/diff` files changed this turn |
| `compact.png` | `/compact` — thread compacted (same board as `compacted.png`) |
| `interrupt.png` | Interrupt — tiles stay on screen, `✗ Stopped` |
| `clear.png` | `/clear` confirm (same board as `clear_confirm.png`) |
| `session_empty.png` | Empty session |
| `session_loading.png` | Loading / streaming |
| `session_error.png` | Error |
| `session_success.png` | Success |
| `shell.png` | Live Shell tile |
| `permission.png` | Permission prompt |
| `plan.png` | Plan confirmation |
| `streaming.png` | Assistant stream + fence |
| `resume.png` | `/resume` session list |
| `mcp.png` | `/mcp` servers |
| `usage.png` | `/usage` bars |
| `quota.png` | Agent quota exhausted |
| `sandbox.png` | `/sandbox` |
| `cloud.png` | Cloud handoff |
| `sudo.png` | Elevated Shell password |
| `ask.png` | Ask read-only mode |
| `files.png` | `@` file picker |
| `queue.png` | Follow-up queue |
| `jobs.png` | `/jobs` agents |
| `help.png` | `/help` two-column commands |
| `first_run.png` | First-run tips on the charcoal panel |
| `bash.png` | Bash mode |
| `config.png` | `/config` tree |
| `footer_max.png` | MAX footer after push |
| `login.png` | Sign in — numbered options, option 1 (`> 1 Continue with browser`) focused |
| `thinking.png` | Thinking (muted gold) + reasoning |
| `todos.png` | Working 1/5 checklist |
| `question.png` | Clarifying question |
| `skills.png` | `/skills` picker |
| `btw.png` | Side thread |
| `stopped.png` | Interrupt / Stopped |
| `compacted.png` | `/compact` |
| `write.png` | Write new file excerpt |
| `clear_confirm.png` | `/clear` confirm |
| `grep.png` | Grep tile — one card, numbered hits |
| `glob.png` | Glob tile — matching paths |
| `delete.png` | Delete confirm radios |
| `list.png` | List directory entries |
| `fetch.png` | Fetch URL excerpt |
| `mcp_call.png` | MCP tool call + table |
| `task.png` | Task running |
| `edit.png` | Edit tile +diff |
