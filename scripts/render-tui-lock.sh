#!/usr/bin/env bash
# Render visual-lock TUI frames through MockTerminal and rasterise PNGs.
#
# Usage:
#   ./scripts/render-tui-lock.sh
#   OUTPUT_DIR=docs/media/tui-lock ./scripts/render-tui-lock.sh

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

output_dir="${OUTPUT_DIR:-docs/media/tui-lock}"
frames_root="${FRAMES_DIR:-target/tui-lock}"

mkdir -p "$output_dir"

for spec in 40x12 120x40; do
  width="${spec%x*}"
  height="${spec#*x}"
  frames="$frames_root/$spec"
  pngs="$output_dir/$spec"
  echo "==> Rendering lock frames at ${width}x${height}"
  cargo run --quiet -p cortex-tui --bin generate_tui_lock_screenshots -- \
    --width "$width" --height "$height" --output "$frames"
  echo "==> Rasterising PNGs to $pngs"
  python3 scripts/ansi-frames-to-gif.py --frames "$frames" --png-only "$pngs"
done

cat > "$output_dir/README.md" <<'EOF'
# Cortex CLI visual lock captures

Headless `MockTerminal` renders of the session, login, slash palette, and
settings hub. Regenerated with `./scripts/render-tui-lock.sh`.

Each scene is captured at **40×12** (narrow) and **120×40** (wide).

Splash copy in these frames is the lock line `Cortex CLI v1.0.0`. The shipped
binary still reports the crate version.

| File | Surface |
|------|---------|
| `splash.png` | Empty session chrome: cwd, `> cortex`, version, composer |
| `typing.png` | Prompt typed in the composer, block cursor |
| `model_compact.png` | `/model` compact picker |
| `model_full.png` | `/model` full picker with effort + billing note |
| `mode.png` | `/mode` Agent / Plan / Ask radios |
| `permissions.png` | `/permissions` approval policy picker |
| `working.png` | Working spinner + elapsed / tokens |
| `read.png` | Read tile — numbered excerpt |
| `login_select.png` | Aligned radios, mint on the selected row |
| `login_waiting.png` | Loading / waiting for browser auth |
| `login_success.png` | `Signed in.` |
| `login_error.png` | Product-facing error |
| `palette.png` | `/` home: twenty commands |
| `palette_empty.png` | `/` filter with no matches |
| `settings_hub.png` | `/settings` seven-row hub |
| `settings_empty.png` | Settings filter with no matches |
| `tool_tiles.png` | Grep tile (one card at a time) |
| `diagnostics.png` | Diagnostics tile |
| `multi_diff.png` | `/diff` files changed this turn |
| `compact.png` | Compact mode |
| `interrupt.png` | Interrupt / Esc |
| `clear.png` | Clear (empty session again) |
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
| `first_run.png` | First-run tips |
| `bash.png` | Bash mode |
| `config.png` | `/config` tree |
| `footer_max.png` | MAX footer after push |
| `login.png` | Sign in — two radios, hint under browser |
| `thinking.png` | Thinking spinner + reasoning |
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
EOF

echo "Wrote $output_dir"
