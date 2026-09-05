#!/usr/bin/env bash
# Recapture lock v2 runtime PNGs from the real TUI (MockTerminal).
#
# Writes:
#   docs/media/tui-lock-v2/runtime/{40x12,120x40}/*.png
#
# Usage:
#   ./scripts/render-tui-lock-v2.sh

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

output_dir="${OUTPUT_DIR:-docs/media/tui-lock-v2/runtime}"
frames_root="${FRAMES_DIR:-target/tui-lock-v2}"

mkdir -p "$output_dir"

for spec in 40x12 120x40; do
  width="${spec%x*}"
  height="${spec#*x}"
  frames="$frames_root/$spec"
  pngs="$output_dir/$spec"
  echo "==> Rendering lock v2 frames at ${width}x${height}"
  cargo run --quiet -p cortex-tui --bin generate_tui_lock_screenshots -- \
    --v2 --width "$width" --height "$height" --output "$frames"
  echo "==> Rasterising PNGs to $pngs"
  python3 scripts/ansi-frames-to-gif.py --frames "$frames" --png-only "$pngs"
done

cat > "$output_dir/README.md" <<'EOF'
# Cortex CLI TUI lock v2 — runtime captures

Headless `MockTerminal` renders of the live session chrome (inky background,
dual-hairline composer, model chip, slash palette, settings modal, effort
radios). Regenerated with `./scripts/render-tui-lock-v2.sh`.

Designer boards (pixel target) live in `docs/media/tui-lock-v2/{40x12,120x40}/`.
These runtime frames are what Designer cli signs off against.

Each scene is captured at **40×12** (narrow) and **120×40** (wide).
EOF

echo "Wrote runtime captures to $output_dir"
