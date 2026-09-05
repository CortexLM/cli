#!/usr/bin/env python3
"""Cortex CLI — TUI lock v2 board renderer.

Design-only tool. It paints a fixed character grid (a headless "terminal")
from the board recipes in ``boards.py`` and rasterises each grid to PNG with
IBM Plex Mono, so that every hex value in the chrome contract survives
pixel-exact. Box-drawing glyphs are drawn as vectors so hairlines are crisp
at any cell size. A plain-text dump of every grid is written next to the
PNGs so implementers can diff a MockTerminal capture against the lock.

Usage:
    python3 render_lock_v2.py --out ../ --sizes 120x40,40x12
    python3 render_lock_v2.py --only welcome-cortex,settings-appearance

Fonts: IBM Plex Mono is fetched on first run into ``~/.cache/cortex-lock-v2``
(override with ``--font-dir``). Glyphs missing from Plex (``♦ ▸ ▼ ● ○ ⠇``)
fall back to DejaVu Sans Mono from the system.
"""

from __future__ import annotations

import argparse
import os
import sys
import urllib.request
from dataclasses import dataclass, field, replace
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# --------------------------------------------------------------------------- #
# Chrome contract (the only colours a board may use)
# --------------------------------------------------------------------------- #

TOKENS = {
    "bg": "#000000",
    "text": "#F5F5F5",
    "dim": "#6B7280",
    "muted": "#4B5563",
    "hairline": "#3A3A3A",
    "hairline_hover": "#525252",
    "panel": "#141414",
    "bar_user": "#1C1C1C",
    "bar_hover": "#1A1A1A",
    "bar_selected": "#262626",
    "accent": "#A78BFA",
    "success": "#4ADE80",
    "warning": "#FFC857",
    "error": "#F87171",
}

BG = TOKENS["bg"]
TEXT = TOKENS["text"]
DIM = TOKENS["dim"]
MUTED = TOKENS["muted"]
HAIR = TOKENS["hairline"]
HAIR_HI = TOKENS["hairline_hover"]
PANEL = TOKENS["panel"]
BAR_USER = TOKENS["bar_user"]
BAR_HOV = TOKENS["bar_hover"]
BAR_SEL = TOKENS["bar_selected"]
VIOLET = TOKENS["accent"]
GREEN = TOKENS["success"]
AMBER = TOKENS["warning"]
RED = TOKENS["error"]

# Cell geometry: IBM Plex Mono advance is 0.6 em, so 20 px → 12 px per column;
# its ascent+descent (1025+275) is 1.3 em → 26 px per row.
FONT_PX = 20
CELL_W = 12
CELL_H = 26
BASELINE = 20
PAD = 12

BOX_CHARS = set("─│╭╮╰╯┌┐└┘├┤┬┴┼")


@dataclass(frozen=True)
class St:
    fg: str = TEXT
    bg: str | None = None
    b: bool = False
    i: bool = False
    u: bool = False

    def __call__(self, **kw) -> "St":
        return replace(self, **kw)


S = St()
S_DIM = St(fg=DIM)
S_MUTED = St(fg=MUTED)
S_HAIR = St(fg=HAIR)
S_ACC = St(fg=VIOLET)
S_BOLD = St(b=True)
S_OK = St(fg=GREEN)
S_WARN = St(fg=AMBER)
S_ERR = St(fg=RED)


@dataclass
class Cell:
    ch: str = " "
    fg: str = TEXT
    bg: str = BG
    b: bool = False
    i: bool = False
    u: bool = False


@dataclass
class Screen:
    cols: int
    rows: int
    cells: list = field(default_factory=list)
    caret: tuple | None = None  # (x, y) bar caret drawn by the renderer

    def __post_init__(self):
        self.cells = [[Cell() for _ in range(self.cols)] for _ in range(self.rows)]

    # -- primitives ------------------------------------------------------- #
    def cell(self, x: int, y: int) -> Cell | None:
        if 0 <= x < self.cols and 0 <= y < self.rows:
            return self.cells[y][x]
        return None

    def put(self, x: int, y: int, s: str, st: St = S, max_x: int | None = None) -> int:
        """Write ``s`` at (x, y); returns the next column. Clips at max_x/cols."""
        limit = self.cols if max_x is None else min(max_x, self.cols)
        for ch in s:
            if x >= limit:
                break
            c = self.cell(x, y)
            if c is not None:
                c.ch = ch
                c.fg = st.fg
                if st.bg is not None:
                    c.bg = st.bg
                c.b, c.i, c.u = st.b, st.i, st.u
            x += 1
        return x

    def spans(self, x: int, y: int, parts, max_x: int | None = None) -> int:
        for text, st in parts:
            x = self.put(x, y, text, st, max_x)
        return x

    def right(self, y: int, s: str, x_end: int, st: St = S) -> int:
        """Right-align ``s`` so its last glyph sits at column ``x_end - 1``."""
        return self.put(x_end - len(s), y, s, st)

    def right_spans(self, y: int, parts, x_end: int) -> int:
        total = sum(len(t) for t, _ in parts)
        return self.spans(x_end - total, y, parts)

    def center(self, y: int, s: str, x0: int, x1: int, st: St = S) -> int:
        w = x1 - x0
        return self.put(x0 + max(0, (w - len(s)) // 2), y, s, st)

    def center_spans(self, y: int, parts, x0: int, x1: int) -> int:
        total = sum(len(t) for t, _ in parts)
        return self.spans(x0 + max(0, (x1 - x0 - total) // 2), y, parts)

    def fill(self, x0: int, y: int, x1: int, bg: str):
        for x in range(max(0, x0), min(x1, self.cols)):
            c = self.cells[y][x]
            c.bg = bg

    def fill_rect(self, x0: int, y0: int, x1: int, y1: int, bg: str):
        for y in range(max(0, y0), min(y1, self.rows)):
            self.fill(x0, y, x1, bg)

    def clear_rect(self, x0: int, y0: int, x1: int, y1: int, bg: str = BG):
        for y in range(max(0, y0), min(y1, self.rows)):
            for x in range(max(0, x0), min(x1, self.cols)):
                self.cells[y][x] = Cell(bg=bg)

    def hline(self, y: int, x0: int, x1: int, fg: str = HAIR):
        for x in range(max(0, x0), min(x1, self.cols)):
            self.put(x, y, "─", St(fg=fg))

    def vline(self, x: int, y0: int, y1: int, fg: str = HAIR):
        for y in range(max(0, y0), min(y1, self.rows)):
            self.put(x, y, "│", St(fg=fg))

    def box(self, x: int, y: int, w: int, h: int, fg: str = HAIR, rounded: bool = True):
        tl, tr, bl, br = ("╭", "╮", "╰", "╯") if rounded else ("┌", "┐", "└", "┘")
        st = St(fg=fg)
        self.hline(y, x + 1, x + w - 1, fg)
        self.hline(y + h - 1, x + 1, x + w - 1, fg)
        self.vline(x, y + 1, y + h - 1, fg)
        self.vline(x + w - 1, y + 1, y + h - 1, fg)
        self.put(x, y, tl, st)
        self.put(x + w - 1, y, tr, st)
        self.put(x, y + h - 1, bl, st)
        self.put(x + w - 1, y + h - 1, br, st)

    def text_dump(self) -> str:
        lines = []
        for row in self.cells:
            lines.append("".join(c.ch for c in row).rstrip())
        return "\n".join(lines) + "\n"


def clip(s: str, width: int) -> str:
    if width <= 0:
        return ""
    if len(s) <= width:
        return s
    if width == 1:
        return "…"
    return s[: width - 1] + "…"


# --------------------------------------------------------------------------- #
# Fonts
# --------------------------------------------------------------------------- #

PLEX_FILES = {
    ("regular", False): "IBMPlexMono-Regular.ttf",
    ("regular", True): "IBMPlexMono-Italic.ttf",
    ("bold", False): "IBMPlexMono-Bold.ttf",
    ("bold", True): "IBMPlexMono-BoldItalic.ttf",
}
PLEX_URL = "https://raw.githubusercontent.com/google/fonts/main/ofl/ibmplexmono/{}"
SYSTEM_FONTS = Path("/usr/share/fonts/truetype")
# Symbol fallbacks, in order: `♦ ▸ ▼ ● ○` come from DejaVu Sans Mono, the
# braille spinner `⠇` from Cascadia Mono / DejaVu Sans. All monospace-ish at 12 px.
FALLBACKS = [
    ("dejavu/DejaVuSansMono.ttf", "dejavu/DejaVuSansMono-Bold.ttf"),
    ("cascadia/CascadiaMono.ttf", "cascadia/CascadiaMono.ttf"),
    ("dejavu/DejaVuSans.ttf", "dejavu/DejaVuSans-Bold.ttf"),
]


class Fonts:
    def __init__(self, font_dir: Path):
        font_dir.mkdir(parents=True, exist_ok=True)
        self.plex = {}
        for key, name in PLEX_FILES.items():
            path = font_dir / name
            if not path.exists():
                url = PLEX_URL.format(name)
                print(f"fetching {url}", file=sys.stderr)
                urllib.request.urlretrieve(url, path)
            self.plex[key] = ImageFont.truetype(str(path), FONT_PX)
        self._plex_cmap = self._cmap(font_dir / PLEX_FILES[("regular", False)])
        self.fallbacks = []  # (cmap, regular, bold)
        for regular, bold in FALLBACKS:
            rp, bp = SYSTEM_FONTS / regular, SYSTEM_FONTS / bold
            if rp.exists():
                self.fallbacks.append(
                    (self._cmap(rp), ImageFont.truetype(str(rp), FONT_PX), ImageFont.truetype(str(bp if bp.exists() else rp), FONT_PX))
                )

    @staticmethod
    def _cmap(path: Path) -> set:
        try:
            from fontTools.ttLib import TTFont

            return set(TTFont(str(path)).getBestCmap().keys())
        except Exception:  # pragma: no cover - fontTools optional
            return set(range(0x20, 0x7F))

    def pick(self, ch: str, bold: bool, italic: bool) -> ImageFont.FreeTypeFont:
        if ord(ch) in self._plex_cmap:
            return self.plex[("bold" if bold else "regular", italic)]
        for cmap, regular, bold_font in self.fallbacks:
            if ord(ch) in cmap:
                return bold_font if bold else regular
        return self.plex[("bold" if bold else "regular", italic)]


# --------------------------------------------------------------------------- #
# Rasteriser
# --------------------------------------------------------------------------- #


def _draw_box_char(draw: ImageDraw.ImageDraw, ch: str, x0: int, y0: int, color: str):
    w, h = CELL_W, CELL_H
    xm = x0 + w // 2 - 1  # 2 px stroke centred on the cell
    ym = y0 + h // 2 - 1
    r = w // 2

    def hseg(a, b):
        draw.rectangle([a, ym, b - 1, ym + 1], fill=color)

    def vseg(a, b):
        draw.rectangle([xm, a, xm + 1, b - 1], fill=color)

    left, right, top, bottom = x0, x0 + w, y0, y0 + h
    if ch == "─":
        hseg(left, right)
    elif ch == "│":
        vseg(top, bottom)
    elif ch == "┌":
        hseg(xm, right)
        vseg(ym, bottom)
    elif ch == "┐":
        hseg(left, xm + 2)
        vseg(ym, bottom)
    elif ch == "└":
        hseg(xm, right)
        vseg(top, ym + 2)
    elif ch == "┘":
        hseg(left, xm + 2)
        vseg(top, ym + 2)
    elif ch == "├":
        vseg(top, bottom)
        hseg(xm, right)
    elif ch == "┤":
        vseg(top, bottom)
        hseg(left, xm + 2)
    elif ch == "┬":
        hseg(left, right)
        vseg(ym, bottom)
    elif ch == "┴":
        hseg(left, right)
        vseg(top, ym + 2)
    elif ch == "┼":
        hseg(left, right)
        vseg(top, bottom)
    elif ch in "╭╮╰╯":
        # quarter arcs of radius r joining the cell mid-lines
        if ch == "╭":
            cx, cy = xm + r, ym + r
            draw.arc([cx - r, cy - r, cx + r + 1, cy + r + 1], 180, 270, fill=color, width=2)
            vseg(ym + r, bottom)
        elif ch == "╮":
            cx, cy = xm + 1 - r, ym + r
            draw.arc([cx - r, cy - r, cx + r + 1, cy + r + 1], 270, 360, fill=color, width=2)
            vseg(ym + r, bottom)
        elif ch == "╰":
            cx, cy = xm + r, ym + 1 - r
            draw.arc([cx - r, cy - r, cx + r + 1, cy + r + 1], 90, 180, fill=color, width=2)
            vseg(top, ym + 1 - r + 1)
        else:  # ╯
            cx, cy = xm + 1 - r, ym + 1 - r
            draw.arc([cx - r, cy - r, cx + r + 1, cy + r + 1], 0, 90, fill=color, width=2)
            vseg(top, ym + 1 - r + 1)


def rasterise(screen: Screen, fonts: Fonts) -> Image.Image:
    width = screen.cols * CELL_W + PAD * 2
    height = screen.rows * CELL_H + PAD * 2
    img = Image.new("RGB", (width, height), BG)
    draw = ImageDraw.Draw(img)

    # backgrounds first so glyph anti-aliasing blends onto the right bar colour
    for y, row in enumerate(screen.cells):
        for x, c in enumerate(row):
            if c.bg != BG:
                px, py = PAD + x * CELL_W, PAD + y * CELL_H
                draw.rectangle([px, py, px + CELL_W - 1, py + CELL_H - 1], fill=c.bg)

    for y, row in enumerate(screen.cells):
        for x, c in enumerate(row):
            px, py = PAD + x * CELL_W, PAD + y * CELL_H
            if c.ch == " ":
                pass
            elif c.ch in BOX_CHARS:
                _draw_box_char(draw, c.ch, px, py, c.fg)
            else:
                font = fonts.pick(c.ch, c.b, c.i)
                draw.text((px, py + BASELINE), c.ch, font=font, fill=c.fg, anchor="ls")
            if c.u:
                draw.rectangle([px, py + BASELINE + 3, px + CELL_W - 1, py + BASELINE + 3], fill=c.fg)

    if screen.caret is not None:
        cx, cy = screen.caret
        px, py = PAD + cx * CELL_W, PAD + cy * CELL_H
        draw.rectangle([px, py + 3, px + 1, py + CELL_H - 4], fill=TEXT)
    return img


# --------------------------------------------------------------------------- #
# index.md
# --------------------------------------------------------------------------- #


def render_index(boards, sizes) -> str:
    wide = [s for s in sizes if int(s.split("x")[0]) >= 80]
    narrow = [s for s in sizes if int(s.split("x")[0]) < 80]
    wide_dir = wide[0] if wide else "120x40"
    narrow_dir = narrow[0] if narrow else "40x12"
    lines = [
        "# Cortex CLI — TUI lock v2 · board index",
        "",
        "Design-only pack for the full TUI redesign (reference layout language, Cortex chrome).",
        f"Every board exists at **{wide_dir}**; boards where density matters also at **{narrow_dir}**.",
        "Text dumps of each grid live in `txt/<size>/<board>.txt`. Spec: [`SPEC.md`](SPEC.md).",
        "",
        "Regenerate: `python3 tools/render_lock_v2.py --index` (fetches IBM Plex Mono on first run).",
        "",
    ]
    total_w = total_n = 0
    for sec_key, sec_name in boards.SECTIONS.items():
        rows = [(bid, meta) for bid, _, meta in boards.BOARDS if meta["section"] == sec_key]
        lines += [f"## {sec_key}. {sec_name}", "", "| Board | State | Wide | Narrow |", "|---|---|---|---|"]
        for bid, meta in rows:
            total_w += 1
            w_link = f"[{wide_dir}]({wide_dir}/{bid}.png)"
            if meta.get("narrow"):
                total_n += 1
                n_link = f"[{narrow_dir}]({narrow_dir}/{bid}.png)"
            else:
                n_link = "—"
            lines.append(f"| `{bid}` | {meta['desc']} | {w_link} | {n_link} |")
        lines.append("")
    lines += [f"**{total_w}** boards at {wide_dir} · **{total_n}** at {narrow_dir} · {total_w + total_n} PNGs.", ""]
    return "\n".join(lines)


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #


def main(argv=None) -> int:
    here = Path(__file__).resolve().parent
    sys.path.insert(0, str(here))
    import boards  # noqa: E402  (local module)

    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default=str(here.parent), help="lock directory (default: parent of tools/)")
    ap.add_argument("--sizes", default="120x40,40x12")
    ap.add_argument("--only", default="", help="comma-separated board ids")
    ap.add_argument("--font-dir", default=os.environ.get("CORTEX_LOCK_FONT_DIR", str(Path.home() / ".cache/cortex-lock-v2")))
    ap.add_argument("--no-txt", action="store_true")
    ap.add_argument("--index", action="store_true", help="also (re)write index.md next to the size folders")
    args = ap.parse_args(argv)

    fonts = Fonts(Path(args.font_dir))
    out = Path(args.out)
    only = {s.strip() for s in args.only.split(",") if s.strip()}
    if args.index:
        (out / "index.md").write_text(render_index(boards, args.sizes.split(",")), encoding="utf-8")
    written = []
    for size in args.sizes.split(","):
        cols, rows = (int(v) for v in size.lower().split("x"))
        narrow = cols < 80
        png_dir = out / size
        txt_dir = out / "txt" / size
        png_dir.mkdir(parents=True, exist_ok=True)
        if not args.no_txt:
            txt_dir.mkdir(parents=True, exist_ok=True)
        for board_id, fn, meta in boards.BOARDS:
            if only and board_id not in only:
                continue
            if narrow and not meta.get("narrow", False):
                continue
            screen = Screen(cols, rows)
            fn(screen, boards.Ctx(cols, rows, narrow))
            img = rasterise(screen, fonts)
            path = png_dir / f"{board_id}.png"
            img.save(path, optimize=True)
            written.append(path)
            if not args.no_txt:
                (txt_dir / f"{board_id}.txt").write_text(screen.text_dump(), encoding="utf-8")
    print(f"wrote {len(written)} boards under {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
