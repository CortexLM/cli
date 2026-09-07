#!/usr/bin/env bash
# Regenerate the README banner at docs/media/intro.gif.
#
# Two stages:
#   1. generate_tui_demo paints the signed lock TUI (splash → typing → working)
#      at 120×40 through cortex-tui.
#   2. scripts/ansi-frames-to-gif.py rasterises those frames, composites a
#      macOS-styled desktop and moving pointer, and calls ffmpeg.
#
# Requires: cargo, ffmpeg, python3 with Pillow, and a supported sans font
# (Liberation Sans, DejaVu Sans, Noto Sans, or macOS Helvetica/Arial).
# Raw raster: 1232×912 at the default 16px font and padding.
# Composed GIF: 1416×1140. Validate with python3 scripts/check-macos-demo.py.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

frames_dir="${FRAMES_DIR:-target/tui-demo}"
output="${OUTPUT:-docs/media/intro.gif}"

echo "==> Recording the lock TUI to $frames_dir"
cargo run --quiet -p cortex-tui --bin generate_tui_demo -- --output "$frames_dir" --width 120 --height 40

echo "==> Rasterising frames into $output"
python3 scripts/ansi-frames-to-gif.py --frames "$frames_dir" --output "$output"
