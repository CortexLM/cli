#!/usr/bin/env python3
"""Render the Cortex CLI launch stills for Twitter/X.

Nothing in the terminal is drawn by hand. Every window shows a frame recorded
from the signed lock TUI by `generate_tui_demo` (the same recorder behind
`docs/media/intro.gif`), rasterised with JetBrains Mono and composited onto
the green macOS wallpaper with Terminal.app chrome. Output is 1600×900 at @2x
(3200×1800 px), the Twitter/X landscape card size.

Usage:
    python3 design/cli-twitter-promo/render.py            # all candidates
    python3 design/cli-twitter-promo/render.py --only a   # one candidate

Requires: cargo (to record the frames), Google Chrome / Chromium, Pillow, and
the JetBrains Mono + Inter fonts installed system-wide.
"""

from __future__ import annotations

import argparse
import base64
import html
import io
import json
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

try:
    from PIL import Image
except ImportError:  # pragma: no cover - dependency guard
    sys.exit("Pillow is required: python3 -m pip install pillow")

ROOT = Path(__file__).resolve().parents[2]
OUT_DIR = Path(__file__).resolve().parent
WALLPAPER = ROOT / "docs/media/macos-wallpaper-green.jpg"
BANNER = ROOT / "assets/banner.jpg"

# Twitter/X landscape card, @2x.
CANVAS_W, CANVAS_H, SCALE = 1600, 900, 2

# Brand.
INK = "#211F1C"
CREAM = "#F3EFE6"
GREEN = "#1F4945"

# Terminal defaults: the lock paints `Color::Reset`, which the host renders as
# white on black (see scripts/ansi-frames-to-gif.py).
DEFAULT_FG = (255, 255, 255)
DEFAULT_BG = (0, 0, 0)

SGR = re.compile(r"\x1b\[([0-9;]*)m")

CHROME_CANDIDATES = [
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "google-chrome",
]


# ---------------------------------------------------------------------------
# Recording: real frames from the lock TUI
# ---------------------------------------------------------------------------


def record_frames(cols: int, rows: int, cache: Path) -> Path:
    """Record the README hero storyboard at `cols`×`rows` and return the dir."""
    out = cache / f"frames-{cols}x{rows}"
    if (out / "manifest.json").is_file():
        return out
    binary = ROOT / "target/debug/generate_tui_demo"
    if binary.is_file():
        cmd = [str(binary)]
    else:
        cmd = ["cargo", "run", "--quiet", "-p", "cortex-tui", "--bin", "generate_tui_demo", "--"]
    subprocess.run(
        [*cmd, "--output", str(out), "--width", str(cols), "--height", str(rows)],
        check=True,
        cwd=ROOT,
    )
    return out


def frame_for(frames: Path, label: str) -> str:
    manifest = json.loads((frames / "manifest.json").read_text())
    for entry in manifest["frames"]:
        if entry["label"] == label:
            return (frames / entry["file"]).read_text()
    raise SystemExit(f"no {label!r} beat in {frames}")


# ---------------------------------------------------------------------------
# ANSI → cell grid → HTML
# ---------------------------------------------------------------------------


@dataclass
class Style:
    fg: tuple[int, int, int] = DEFAULT_FG
    bg: tuple[int, int, int] = DEFAULT_BG
    bold: bool = False
    dim: bool = False

    def apply(self, params: list[int]) -> None:
        i = 0
        while i < len(params):
            code = params[i]
            if code == 0:
                self.fg, self.bg, self.bold, self.dim = DEFAULT_FG, DEFAULT_BG, False, False
            elif code == 1:
                self.bold = True
            elif code == 2:
                self.dim = True
            elif code in (38, 48) and params[i + 1 : i + 2] == [2]:
                rgb = tuple(params[i + 2 : i + 5])
                if len(rgb) == 3:
                    if code == 38:
                        self.fg = rgb
                    else:
                        self.bg = rgb
                i += 4
            i += 1

    def key(self) -> tuple:
        return (self.fg, self.bg, self.bold, self.dim)


def parse_ansi(text: str, cols: int, rows: int) -> list[list[tuple[str, Style]]]:
    grid: list[list[tuple[str, Style]]] = []
    style = Style()
    for line in text.split("\n")[:rows]:
        cells: list[tuple[str, Style]] = []
        pos = 0
        for match in SGR.finditer(line):
            cells.extend((ch, Style(*style.key())) for ch in line[pos : match.start()])
            raw = match.group(1)
            style.apply([int(p) if p else 0 for p in raw.split(";")] if raw else [0])
            pos = match.end()
        cells.extend((ch, Style(*style.key())) for ch in line[pos:])
        cells = cells[:cols]
        while len(cells) < cols:
            cells.append((" ", Style(*style.key())))
        grid.append(cells)
    while len(grid) < rows:
        grid.append([(" ", Style()) for _ in range(cols)])
    return grid


def css_rgb(rgb: tuple[int, int, int], dim: bool = False) -> str:
    if dim:
        rgb = tuple(int(c * 0.65) for c in rgb)
    return "#%02x%02x%02x" % rgb


def grid_html(grid: list[list[tuple[str, Style]]], cell_w: float, cell_h: float) -> str:
    """One `<div class=row>` per line; same-style runs become fixed-width spans.

    Fixed widths pin every run to the character grid, so a fallback glyph
    (braille spinner, check mark) can never push the rest of the line.
    """
    rows_html = []
    for row in grid:
        parts = []
        run_start = 0
        while run_start < len(row):
            key = row[run_start][1].key()
            run_end = run_start + 1
            while run_end < len(row) and row[run_end][1].key() == key:
                run_end += 1
            text = "".join(ch for ch, _ in row[run_start:run_end])
            style = row[run_start][1]
            decl = [f"width:{(run_end - run_start) * cell_w:g}px"]
            if text.strip() or style.bg != DEFAULT_BG:
                decl.append(f"color:{css_rgb(style.fg, style.dim)}")
                if style.bg != DEFAULT_BG:
                    decl.append(f"background:{css_rgb(style.bg)}")
                if style.bold:
                    decl.append("font-weight:700")
            parts.append(f'<span style="{";".join(decl)}">{html.escape(text)}</span>')
            run_start = run_end
        rows_html.append(f'<div class="row" style="height:{cell_h:g}px">{"".join(parts)}</div>')
    return "\n".join(rows_html)


# ---------------------------------------------------------------------------
# Assets
# ---------------------------------------------------------------------------


def data_uri(image: Image.Image, fmt: str = "PNG") -> str:
    buf = io.BytesIO()
    image.save(buf, fmt)
    return f"data:image/{fmt.lower()};base64,{base64.b64encode(buf.getvalue()).decode()}"


def brand_lockup(mark_only: bool) -> Image.Image:
    """Lift the `CX cortex` lockup off assets/banner.jpg as cream-on-alpha."""
    banner = Image.open(BANNER).convert("RGB")
    box = (425, 180, 630, 325) if mark_only else (425, 180, 1110, 325)
    crop = banner.crop(box)
    out = Image.new("RGBA", crop.size, (0, 0, 0, 0))
    src, dst = crop.load(), out.load()
    cream = tuple(int(CREAM[i : i + 2], 16) for i in (1, 3, 5))
    lo, hi = 110, 235
    for y in range(crop.height):
        for x in range(crop.width):
            white = min(src[x, y])
            alpha = 0 if white <= lo else 255 if white >= hi else int(255 * (white - lo) / (hi - lo))
            dst[x, y] = (*cream, alpha)
    return out.crop(out.getbbox())


# ---------------------------------------------------------------------------
# Scenes
# ---------------------------------------------------------------------------

TITLEBAR_H = 28
INSET = 14  # Terminal.app content inset
RADIUS = 11


@dataclass
class Window:
    grid_html: str
    cols: int
    rows: int
    cell_w: float
    cell_h: float
    title: str

    @property
    def width(self) -> float:
        return self.cols * self.cell_w + 2 * INSET

    @property
    def height(self) -> float:
        return self.rows * self.cell_h + 2 * INSET + TITLEBAR_H

    def html(self, extra_class: str = "", style: str = "") -> str:
        font_px = self.cell_w / 0.6  # JetBrains Mono advance is 600/1000 em
        return f"""
<div class="window {extra_class}" style="width:{self.width:g}px;height:{self.height:g}px;{style}">
  <div class="titlebar">
    <div class="lights"><i class="close"></i><i class="min"></i><i class="zoom"></i></div>
    <div class="title"><i class="folder"></i><span>{html.escape(self.title)}</span></div>
  </div>
  <div class="screen" style="font-size:{font_px:g}px;line-height:{self.cell_h:g}px">
{self.grid_html}
  </div>
  <div class="edge"></div>
</div>"""


def make_window(frames: Path, label: str, cols: int, rows: int, cell_w: float) -> Window:
    grid = parse_ansi(frame_for(frames, label), cols, rows)
    cell_h = round(cell_w / 0.6 * 1.32 * 2) / 2  # half-pixel rows at 1x → whole pixels at @2x
    return Window(
        grid_html=grid_html(grid, cell_w, cell_h),
        cols=cols,
        rows=rows,
        cell_w=cell_w,
        cell_h=cell_h,
        title=f"Cortex CLI — cortex — {cols}×{rows}",
    )


BASE_CSS = f"""
* {{ box-sizing: border-box; }}
html, body {{ margin: 0; width: {CANVAS_W}px; height: {CANVAS_H}px; overflow: hidden; background: {INK}; }}
body {{ position: relative; font-family: Inter, "Helvetica Neue", Arial, sans-serif; }}
.wallpaper {{
  position: absolute; inset: -12px;
  background: url("WALLPAPER_URI") center 62% / cover no-repeat;
  filter: saturate(1.04) brightness(0.94);
}}
.vignette {{
  position: absolute; inset: 0;
  background:
    radial-gradient(120% 90% at 50% 45%, rgba(0,0,0,0) 35%, rgba(0,0,0,0.42) 100%),
    linear-gradient(180deg, rgba(0,0,0,0.10) 0%, rgba(0,0,0,0) 30%, rgba(0,0,0,0) 70%, rgba(0,0,0,0.28) 100%);
}}
.window {{
  position: absolute; border-radius: {RADIUS}px; overflow: hidden; background: #000;
  box-shadow: 0 34px 90px rgba(0,0,0,0.58), 0 10px 26px rgba(0,0,0,0.38), 0 0 0 1px rgba(0,0,0,0.65);
}}
.window .edge {{
  position: absolute; inset: 0; border-radius: {RADIUS}px; pointer-events: none;
  box-shadow: inset 0 0 0 1px rgba(255,255,255,0.10), inset 0 1px 0 rgba(255,255,255,0.06);
}}
.titlebar {{
  position: relative; height: {TITLEBAR_H}px;
  background: linear-gradient(#3a3a3c, #2c2c2e);
  border-bottom: 1px solid #0c0c0c;
}}
.lights {{ position: absolute; left: 13px; top: 8px; display: flex; gap: 8px; }}
.lights i {{ display: block; width: 12px; height: 12px; border-radius: 50%; }}
.lights .close {{ background: #ff5f57; box-shadow: inset 0 0 0 1px rgba(0,0,0,0.18); }}
.lights .min   {{ background: #febc2e; box-shadow: inset 0 0 0 1px rgba(0,0,0,0.18); }}
.lights .zoom  {{ background: #28c840; box-shadow: inset 0 0 0 1px rgba(0,0,0,0.18); }}
.title {{
  position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; gap: 6px;
  font-size: 13px; font-weight: 600; color: #c9c9cf; letter-spacing: 0.1px;
}}
.title .folder {{
  position: relative; display: block; width: 15px; height: 11px; border-radius: 2px;
  background: linear-gradient(#5fb0f2, #4a97dc);
}}
.title .folder::before {{
  content: ""; position: absolute; left: 0; top: -3px; width: 7px; height: 4px;
  border-radius: 1.5px 1.5px 0 0; background: #4e9ee5;
}}
.screen {{
  position: absolute; left: 0; right: 0; top: {TITLEBAR_H}px; bottom: 0; padding: {INSET}px;
  background: #000; color: #fff;
  font-family: "JetBrains Mono", "DejaVu Sans Mono", "Noto Sans Mono", "Noto Sans Symbols2", monospace;
  font-variant-ligatures: none; font-feature-settings: "liga" 0, "calt" 0;
}}
.row {{ white-space: pre; }}
.row span {{ display: inline-block; height: 100%; overflow: hidden; vertical-align: top; }}
.corner-lockup {{
  position: absolute; right: 44px; bottom: 36px; width: 128px; height: auto; opacity: 0.82;
  filter: drop-shadow(0 2px 10px rgba(0,0,0,0.55));
}}
"""


def page(body: str, extra_css: str = "") -> str:
    wallpaper = Image.open(WALLPAPER).convert("RGB")
    css = BASE_CSS.replace("WALLPAPER_URI", data_uri(wallpaper, "JPEG")) + extra_css
    return f"""<!doctype html>
<html><head><meta charset="utf-8"><style>{css}</style></head>
<body>
<div class="wallpaper"></div>
<div class="vignette"></div>
{body}
</body></html>"""


def lockup_html() -> str:
    """Discreet `CX cortex` lockup in the lower-right corner of the desktop."""
    return f'<img class="corner-lockup" src="{data_uri(brand_lockup(mark_only=False))}" alt="Cortex">'


def scene_a(cache: Path) -> str:
    """A — one Terminal window, mid-session: Shell tool running the tests."""
    cols, rows, cell_w = 116, 22, 10.5
    win = make_window(record_frames(cols, rows, cache), "shell", cols, rows, cell_w)
    left = (CANVAS_W - win.width) / 2
    top = (CANVAS_H - win.height) / 2 - 14  # a touch above centre; the path shows below
    return page(win.html(style=f"left:{left:g}px;top:{top:g}px") + lockup_html())


def scene_b(cache: Path, front_label: str = "working") -> str:
    """B — stacked: the splash card peeks above the live session in front.

    The back card is scaled down a touch and unfocused (gray traffic lights),
    the way macOS draws an inactive window behind the active one.
    """
    cols, rows = 114, 18
    frames = record_frames(cols, rows, cache)
    # The back card is set a half-pixel-per-cell smaller instead of CSS-scaled,
    # so its text stays crisp.
    back = make_window(frames, "splash", cols, rows, 9.5)
    front = make_window(frames, front_label, cols, rows, 10)
    # Peek: title bar, welcome lines and the composer with its dual hairlines
    # (six rows of the splash board).
    dy = TITLEBAR_H + INSET + 6 * back.cell_h + 12
    stack_h = front.height + dy
    y0 = (CANVAS_H - stack_h) / 2 + 2
    body = (
        back.html("back", f"left:{(CANVAS_W - back.width) / 2:g}px;top:{y0:g}px")
        + front.html("front", f"left:{(CANVAS_W - front.width) / 2:g}px;top:{y0 + dy:g}px")
        + lockup_html()
    )
    css = """
.window.back { filter: brightness(0.82); box-shadow: 0 18px 50px rgba(0,0,0,0.45), 0 0 0 1px rgba(0,0,0,0.6); }
.window.back .titlebar { background: linear-gradient(#333335, #29292b); }
.window.back .title { color: #8e8e93; }
.window.back .lights i { background: #4b4b4e; box-shadow: inset 0 0 0 1px rgba(0,0,0,0.25); }
.window.front { box-shadow: 0 44px 110px rgba(0,0,0,0.66), 0 12px 30px rgba(0,0,0,0.45), 0 0 0 1px rgba(0,0,0,0.7); }
"""
    return page(body, css)


def scene_c(cache: Path, caption: bool) -> str:
    """C — caption area left (ink gradient), Terminal right."""
    cols, rows, cell_w = 92, 22, 10
    win = make_window(record_frames(cols, rows, cache), "shell", cols, rows, cell_w)
    left = CANVAS_W - win.width - 64
    top = (CANVAS_H - win.height) / 2
    lockup = data_uri(brand_lockup(mark_only=False))
    text = ""
    if caption:
        # Product copy only: the splash tagline and the composer placeholder.
        text = f"""
<div class="caption">
  <img class="lockup" src="{lockup}" alt="Cortex">
  <div class="headline">The coding<br>agent CLI.</div>
  <div class="rule"></div>
  <div class="sub">Plan, search, build anything.</div>
</div>"""
    css = f"""
.shade {{
  position: absolute; inset: 0;
  background: linear-gradient(90deg, rgba(33,31,28,0.96) 0%, rgba(33,31,28,0.90) 26%, rgba(33,31,28,0.55) 40%, rgba(33,31,28,0) 58%);
}}
.caption {{ position: absolute; left: 96px; top: 0; bottom: 0; width: 470px; display: flex; flex-direction: column; justify-content: center; color: {CREAM}; }}
.caption .lockup {{ width: 188px; height: auto; opacity: 0.96; margin-bottom: 46px; }}
.caption .headline {{ font-size: 54px; line-height: 1.1; font-weight: 600; letter-spacing: -0.8px; }}
.caption .rule {{ width: 56px; height: 4px; background: {GREEN}; margin: 30px 0 24px; border-radius: 2px; }}
.caption .sub {{ font-size: 22px; line-height: 1.4; font-weight: 400; color: rgba(243,239,230,0.78); }}
"""
    return page('<div class="shade"></div>' + text + win.html(style=f"left:{left:g}px;top:{top:g}px"), css)


SCENES = {
    "a": ("a-single-window.png", lambda cache: scene_a(cache)),
    "b": ("b-layered-windows.png", lambda cache: scene_b(cache)),
    "c": ("c-caption-left.png", lambda cache: scene_c(cache, caption=True)),
    "c-blank": ("c-caption-left-blank.png", lambda cache: scene_c(cache, caption=False)),
}


# ---------------------------------------------------------------------------
# Chrome
# ---------------------------------------------------------------------------


def find_chrome() -> str:
    for name in CHROME_CANDIDATES:
        path = shutil.which(name)
        if path:
            return path
    raise SystemExit("Google Chrome or Chromium is required to rasterise the stills.")


def screenshot(chrome: str, html_path: Path, png_path: Path, profile: Path) -> None:
    subprocess.run(
        [
            chrome,
            "--headless=new",
            "--no-sandbox",
            "--disable-gpu",
            "--disable-dev-shm-usage",
            "--no-first-run",
            f"--user-data-dir={profile}",
            "--hide-scrollbars",
            f"--force-device-scale-factor={SCALE}",
            f"--window-size={CANVAS_W},{CANVAS_H}",
            "--virtual-time-budget=3000",
            f"--screenshot={png_path}",
            html_path.as_uri(),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=180,
    )


def finalise(png_path: Path) -> None:
    """Recompress and assert the Twitter/X card geometry."""
    image = Image.open(png_path).convert("RGB")
    expected = (CANVAS_W * SCALE, CANVAS_H * SCALE)
    if image.size != expected:
        raise SystemExit(f"{png_path.name}: got {image.size}, expected {expected}")
    image.save(png_path, "PNG", optimize=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--only", choices=sorted(SCENES), action="append", help="Render a subset")
    parser.add_argument("--out", type=Path, default=OUT_DIR, help="Output directory")
    parser.add_argument("--cache", type=Path, default=None, help="Keep recorded frames here")
    parser.add_argument("--keep-html", action="store_true", help="Write the scene HTML next to the PNGs")
    args = parser.parse_args()

    chrome = find_chrome()
    args.out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        cache = args.cache or Path(tmp) / "frames"
        cache.mkdir(parents=True, exist_ok=True)
        profile = Path(tmp) / "chrome-profile"
        for key in args.only or list(SCENES):
            filename, build = SCENES[key]
            html_text = build(cache)
            html_path = (args.out if args.keep_html else Path(tmp)) / f"{Path(filename).stem}.html"
            html_path.write_text(html_text)
            png_path = args.out / filename
            screenshot(chrome, html_path, png_path, profile)
            finalise(png_path)
            print(f"{png_path.relative_to(ROOT) if png_path.is_relative_to(ROOT) else png_path}: "
                  f"{png_path.stat().st_size / 1024 / 1024:.2f} MB")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
