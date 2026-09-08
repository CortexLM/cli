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

unique_pngs() {
  local dir="$1"
  local expected="$2"
  local count
  count="$(find "$dir" -maxdepth 1 -name '*.png' | wc -l | tr -d ' ')"
  if [[ "$count" != "$expected" ]]; then
    echo "expected $expected PNGs in $dir, got $count" >&2
    return 1
  fi
  python3 - "$dir" <<'PY'
import hashlib, sys
from collections import defaultdict
from pathlib import Path
d = Path(sys.argv[1])
by_hash = defaultdict(list)
for p in sorted(d.glob("*.png")):
    h = hashlib.sha256(p.read_bytes()).hexdigest()
    by_hash[h].append(p.name)
dups = {h: names for h, names in by_hash.items() if len(names) > 1}
if dups:
    print("duplicate PNG hashes in", d, file=sys.stderr)
    for h, names in dups.items():
        print(f"  {h[:16]}  {names}", file=sys.stderr)
    sys.exit(1)
print(f"{d}: {len(by_hash)} unique hashes")
PY
}

for spec in 40x12 120x40; do
  width="${spec%x*}"
  height="${spec#*x}"
  frames="$frames_root/$spec"
  pngs="$output_dir/$spec"
  rm -rf "$pngs"
  mkdir -p "$pngs"
  echo "==> Rendering lock v2 frames at ${width}x${height}"
  cargo run --quiet -p cortex-tui --bin generate_tui_lock_screenshots -- \
    --v2 --width "$width" --height "$height" --output "$frames"
  echo "==> Rasterising PNGs to $pngs"
  python3 scripts/ansi-frames-to-gif.py --frames "$frames" --png-only "$pngs"
done

unique_pngs "$output_dir/40x12" 31
unique_pngs "$output_dir/120x40" 77

python3 - "$output_dir" <<'PY'
from pathlib import Path
import sys
from PIL import Image

ACCENT = (31, 73, 69)
VIOLET = (167, 139, 250)
root = Path(sys.argv[1]) / "120x40"

def exact(path: Path, colour: tuple[int, int, int]) -> int:
    im = Image.open(path).convert("RGB")
    return sum(1 for p in im.getdata() if p == colour)

welcome = exact(root / "welcome-cortex.png", ACCENT)
empty = exact(root / "session-empty.png", ACCENT)
print(f"v2 accent pixels 120×40: welcome-cortex={welcome} session-empty={empty}")
if welcome < 40:
    raise SystemExit(f"welcome-cortex composer `>` is not lock green (#1F4945): {welcome} exact pixels")
if empty < 40:
    raise SystemExit(f"session-empty composer `>` is not lock green: {empty} exact pixels")
for png in sorted(root.glob("*.png")):
    violet = exact(png, VIOLET)
    if violet:
        raise SystemExit(f"{png.name} still has {violet} historical violet (#A78BFA) pixels")
print("v2 lock colour check ok")
PY

cat > "$output_dir/README.md" <<'EOF'
# Cortex CLI TUI lock v2 — runtime captures

Headless `MockTerminal` renders of the live session chrome (inky background,
dual-hairline composer, model chip, slash palette, settings modal, effort
radios). Regenerated with `./scripts/render-tui-lock-v2.sh`. The signed accent
is banner green `#1F4945`; historical violet `#A78BFA` is not the lock.

Designer boards (pixel target) live in `docs/media/tui-lock-v2/{40x12,120x40}/`.
These runtime frames are what Designer cli signs off against.

SPEC §7: **77** boards at 120×40 and **31** at 40×12. Each filename is one
distinct live state — no two PNGs share a sha256.
EOF

echo "Wrote runtime captures to $output_dir"
