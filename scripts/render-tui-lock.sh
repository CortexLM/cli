#!/usr/bin/env bash
# Render visual-lock TUI frames through MockTerminal and rasterise PNGs.
#
# Produces two sets per size:
#   - raw captures           docs/media/tui-lock/{40x12,120x40}/*.png
#   - macOS Terminal windows docs/media/tui-lock/macos/{40x12,120x40}/*.png
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
  echo "==> Compositing macOS Terminal windows to $output_dir/macos/$spec"
  python3 scripts/compose-macos-terminal.py \
    --raw "$pngs" --output "$output_dir/macos/$spec" --size "$spec"
done

python3 - <<'PY'
from pathlib import Path
from PIL import Image

ACCENT = (167, 139, 250)
root = Path("docs/media/tui-lock/120x40")

def exact_accent(path: Path) -> int:
    im = Image.open(path).convert("RGB")
    return sum(1 for p in im.getdata() if p == ACCENT)

splash = exact_accent(root / "splash.png")
login = exact_accent(root / "login.png")
empty = exact_accent(root / "session_empty.png")
print(f"accent pixels 120×40: splash={splash} session_empty={empty} login={login}")
if splash < 40:
    raise SystemExit(f"splash composer `>` is not lock violet (#A78BFA): {splash} exact pixels")
if empty < 40:
    raise SystemExit(f"session_empty composer `>` is not lock violet: {empty} exact pixels")
splash_bytes = (root / "splash.png").read_bytes()
empty_bytes = (root / "session_empty.png").read_bytes()
if splash_bytes == empty_bytes:
    raise SystemExit("splash.png and session_empty.png are identical")
print("lock colour check ok")
PY

cat > "$output_dir/README.md" <<'EOF'
# Cortex CLI visual lock captures

Headless `MockTerminal` renders of the session, login, slash palette, and
settings hub on the gray chrome. Regenerated with
`./scripts/render-tui-lock.sh`.

Each scene is captured at **40×12** (narrow) and **120×40** (wide), twice:

- `40x12/`, `120x40/` — raw terminal pixels. The background is the host
  terminal's (`Color::Reset`, black by default); nothing paints a wash and no
  rounded frame is drawn — the TUI bleeds to the terminal edges.
- `macos/40x12/`, `macos/120x40/` — the same captures as a macOS Terminal.app
  *window*, cropped to the window: a title bar (traffic lights, `cortex-api —
  cortex — W×H` proxy-icon title) over the capture pasted 1:1, rounded window
  corners on a transparent background — no desktop wallpaper, no menu bar, no
  shadow. TUI pixels are never resampled, so the 40×12 pack is a genuinely
  small 40-column window (432×324) and the 120×40 pack a wide 120-column one
  (1232×940), and every locked colour survives pixel-exact. Nothing is
  re-typed, and the rounded corners belong to the macOS window chrome only.

Chrome rules: structure is gray — `#3A3A3A` hairlines above and below the
`> ` composer and around search fields, `#141414` charcoal panels for tips,
`#1C1C1C` bars behind past user turns, `#6B7280` secondary copy, white
primary copy. The one accent is the Cortex violet `#A78BFA`, on the focused
selection only (the `>` caret and the selected label on the `#262626` gray bar,
never inverted, never a `#221A38` wash); unselected rows lead with a dim
middot. Green `#4ADE80` appears only on `✓`
and `+N` diff additions, red / amber only on diagnostics, and the Thinking
status is the muted gold `#C9A95C`. The footer is the model on the left and
one shortcut hint on the right, all gray.

Replies auto-format through the real `MarkdownRenderer`: markdown tables are
the gray plus-ASCII grid (`+---+`, `|` — never Unicode box drawing), fenced
code sits between two hairlines with its language tag, a dim line-number
gutter and monochrome (bold-keyword) highlighting, nested bullets indent
(`•` / `◦`), and task items render as `✓` / `○`. Edit tiles show unified
hunks with a dim old/new gutter, white context, red `-`, green `+`, and
word-level colour on a changed line.

Splash copy in these frames is the lock line `Cortex CLI v1.0.0`. The shipped
binary still reports the crate version.

| File | Surface |
|------|---------|
| `splash.png` | Launch splash: cwd, `> cortex`, `Cortex CLI v1.0.0`, `/ commands` legend, hairline composer |
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
| `interrupt.png` | Interrupt — tiles stay on screen, `× Stopped` in error red (same board as `stopped.png`: the UI after ctrl+c) |
| `quota.png` | Agent quota exhausted (title in error red) |
| `clear.png` | `/clear` confirm (same board as `clear_confirm.png`) |
| `session_empty.png` | Open session without splash title/legend: cwd, dual-bar composer, footer |
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
| `mcp_call.png` | MCP tool call + issue list as a `+---+` plus-ASCII table |
| `task.png` | Task running |
| `edit.png` | Edit tile +diff |
| `md_table.png` | Reply with a markdown table: gray `+---+` plus-ASCII grid, header + 3 rows |
| `md_fence.png` | Reply with a fenced TypeScript block: `─ ts ─` hairline, line numbers, bold keywords, closing hairline |
| `md_list.png` | Reply with a nested bullet list (`•` / `◦`) and a `✓` / `○` task list |
| `md_mixed.png` | Heading + bullets + table + fence in one reply — the auto-format proof |
| `diff_hunk.png` | Edit tile + unified hunk: dim gutter, white context, red `-`, green `+` |
| `diff_word.png` | Edit tile, one changed line: only the mutated token is coloured |
| `sandbox_deny.png` | Sandbox blocked a command (`× Sandbox denied` in error red) |
| `mcp_drop.png` | MCP server dropped (`x github dropped` in error red) |
EOF

echo "Wrote $output_dir"
