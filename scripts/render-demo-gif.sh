#!/usr/bin/env bash
# Regenerate the README banner at docs/media/intro.gif.
#
# Two stages:
#   1. cortex-tui-capture renders the session view headlessly to ANSI frames.
#   2. scripts/ansi-frames-to-gif.py rasterises those frames and calls ffmpeg.
#
# Requires: cargo, ffmpeg, python3 with Pillow.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

frames_dir="${FRAMES_DIR:-target/tui-demo}"
output="${OUTPUT:-docs/media/intro.gif}"

echo "==> Recording the session view to $frames_dir"
cargo run --quiet -p cortex-tui-capture --bin generate_tui_demo -- --output "$frames_dir"

echo "==> Rasterising frames into $output"
python3 scripts/ansi-frames-to-gif.py --frames "$frames_dir" --output "$output"
