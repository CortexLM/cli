#!/usr/bin/env python3
"""Rasterise ANSI frames from `generate_tui_demo` into a looping GIF.

The Rust recorder owns what the demo looks like; this script only turns its
ANSI frames into pixels and hands them to ffmpeg. Keeping the rasteriser out of
the Cargo workspace keeps image and font dependencies off the build.

Usage:
    python3 scripts/ansi-frames-to-gif.py \
        --frames target/tui-demo \
        --output docs/media/intro.gif
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:  # pragma: no cover - dependency guard
    sys.exit(
        "Pillow is required to rasterise the demo frames.\n"
        "Install it with: python3 -m pip install pillow"
    )

# Product palette, mirroring cortex-core::style. The chrome never paints its
# own background (`Color::Reset`), so unstyled cells rasterise as the host
# terminal default: black. No frame, border or canvas tint is drawn — the TUI
# bleeds to the terminal edges.
DEFAULT_FG = (255, 255, 255)
DEFAULT_BG = (0, 0, 0)
CANVAS_BG = (0, 0, 0)

# Primary face first, then faces that fill in box-drawing and symbol glyphs
# JetBrains Mono does not carry (the tool-result marker U+23BF, for instance).
FONT_CANDIDATES = {
    "regular": [
        os.path.expanduser("~/.local/share/fonts/IBMPlexMono-Regular.ttf"),
        "/usr/share/fonts/truetype/ibm-plex/IBMPlexMono-Regular.ttf",
        "/usr/share/fonts/opentype/ibm-plex/IBMPlexMono-Regular.otf",
        "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf",
        "/usr/share/fonts/truetype/macos/JetBrainsMono-Regular.ttf",
        "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf",
        "/Library/Fonts/JetBrainsMono-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    ],
    "bold": [
        os.path.expanduser("~/.local/share/fonts/IBMPlexMono-Bold.ttf"),
        "/usr/share/fonts/truetype/ibm-plex/IBMPlexMono-Bold.ttf",
        "/usr/share/fonts/opentype/ibm-plex/IBMPlexMono-Bold.otf",
        "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Bold.ttf",
        "/usr/share/fonts/truetype/macos/JetBrainsMono-Bold.ttf",
        "/usr/share/fonts/TTF/JetBrainsMono-Bold.ttf",
        "/Library/Fonts/JetBrainsMono-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Bold.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols-Bold.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    ],
}

# A codepoint no real face is expected to define, used to learn what this
# font's "missing glyph" bitmap looks like.
PROBE_MISSING = "\U0010fffd"

SGR_PATTERN = re.compile(r"\x1b\[([0-9;]*)m")


class CellStyle:
    """Foreground, background and weight for one terminal cell."""

    __slots__ = ("fg", "bg", "bold", "dim")

    def __init__(self) -> None:
        self.fg = DEFAULT_FG
        self.bg = DEFAULT_BG
        self.bold = False
        self.dim = False

    def copy(self) -> "CellStyle":
        clone = CellStyle()
        clone.fg = self.fg
        clone.bg = self.bg
        clone.bold = self.bold
        clone.dim = self.dim
        return clone

    def reset(self) -> None:
        self.fg = DEFAULT_FG
        self.bg = DEFAULT_BG
        self.bold = False
        self.dim = False

    def apply(self, params: list[int]) -> None:
        index = 0
        while index < len(params):
            code = params[index]
            if code == 0:
                self.reset()
            elif code == 1:
                self.bold = True
            elif code == 2:
                self.dim = True
            elif code in (38, 48) and params[index + 1 : index + 2] == [2]:
                rgb = tuple(params[index + 2 : index + 5])
                if len(rgb) == 3:
                    if code == 38:
                        self.fg = rgb
                    else:
                        self.bg = rgb
                index += 4
            index += 1


def parse_ansi(text: str, width: int, height: int) -> list[list[tuple[str, CellStyle]]]:
    """Turn one ANSI frame into a grid of (symbol, style) cells."""
    rows: list[list[tuple[str, CellStyle]]] = []
    style = CellStyle()

    for line in text.split("\n")[:height]:
        cells: list[tuple[str, CellStyle]] = []
        position = 0
        for match in SGR_PATTERN.finditer(line):
            for char in line[position : match.start()]:
                cells.append((char, style.copy()))
            raw = match.group(1)
            params = [int(part) if part else 0 for part in raw.split(";")] if raw else [0]
            style.apply(params)
            position = match.end()
        for char in line[position:]:
            cells.append((char, style.copy()))

        cells = cells[:width]
        while len(cells) < width:
            cells.append((" ", style.copy()))
        rows.append(cells)

    while len(rows) < height:
        rows.append([(" ", CellStyle()) for _ in range(width)])

    return rows


class FontSet:
    """A primary monospace face plus fallbacks for glyphs it does not carry."""

    def __init__(self, kind: str, size: int) -> None:
        self.size = size
        self.faces: list[ImageFont.FreeTypeFont] = []
        self.notdef: list[bytes] = []
        for path in FONT_CANDIDATES[kind]:
            if not Path(path).is_file():
                continue
            face = ImageFont.truetype(path, size)
            self.faces.append(face)
            self.notdef.append(self._signature(face, PROBE_MISSING))

        if not self.faces:
            raise SystemExit(
                f"No font found for '{kind}'. Install JetBrains Mono, "
                "DejaVu Sans Mono or Noto Sans Mono."
            )

        self.primary = self.faces[0]
        self._cache: dict[str, ImageFont.FreeTypeFont | None] = {}

    def _signature(self, face: ImageFont.FreeTypeFont, char: str) -> bytes:
        box = self.size * 2
        tile = Image.new("L", (box, box), 0)
        ImageDraw.Draw(tile).text((0, 0), char, font=face, fill=255)
        return tile.tobytes()

    def face_for(self, char: str) -> ImageFont.FreeTypeFont | None:
        """The first face that has a real glyph for `char`, or None."""
        if char in self._cache:
            return self._cache[char]

        chosen = None
        for face, notdef in zip(self.faces, self.notdef):
            if self._signature(face, char) != notdef:
                chosen = face
                break
        self._cache[char] = chosen
        return chosen


def dim_colour(rgb: tuple[int, int, int]) -> tuple[int, int, int]:
    return tuple(int(channel * 0.65) for channel in rgb)


def _dist2(a: tuple[int, int, int], b: tuple[int, int, int]) -> int:
    return sum((x - y) * (x - y) for x, y in zip(a, b))


def snap_cell_to_fg(
    image: Image.Image,
    x0: int,
    y0: int,
    x1: int,
    y1: int,
    fg: tuple[int, int, int],
    bg: tuple[int, int, int],
) -> None:
    """Replace anti-aliased fringe in one cell with the exact foreground."""
    pix = image.load()
    width, height = image.size
    x0 = max(0, x0)
    y0 = max(0, y0)
    x1 = min(width, x1)
    y1 = min(height, y1)
    for y in range(y0, y1):
        for x in range(x0, x1):
            pixel = pix[x, y]
            if pixel == bg or pixel == fg:
                continue
            if _dist2(pixel, fg) <= _dist2(pixel, bg):
                pix[x, y] = fg


def check_glyph_coverage(grid: list[list[tuple[str, CellStyle]]], fonts: dict[str, FontSet]) -> None:
    """Fail before rendering if any character would come out as a blank box."""
    missing = set()
    for row in grid:
        for symbol, style in row:
            if symbol.strip() and fonts["bold" if style.bold else "regular"].face_for(symbol) is None:
                missing.add(symbol)
    if missing:
        glyphs = ", ".join(f"U+{ord(char):04X} ({char!r})" for char in sorted(missing))
        raise SystemExit(f"No installed font has glyphs for: {glyphs}")


def render_frame(
    grid: list[list[tuple[str, CellStyle]]],
    fonts: dict[str, FontSet],
    cell_w: int,
    cell_h: int,
    pad: int,
    baseline: int,
) -> Image.Image:
    cols = len(grid[0])
    rows = len(grid)
    inner_w = cols * cell_w
    inner_h = rows * cell_h
    image = Image.new("RGB", (inner_w + pad * 2, inner_h + pad * 2), CANVAS_BG)
    draw = ImageDraw.Draw(image)

    for row_index, row in enumerate(grid):
        y = pad + row_index * cell_h

        # Background runs first, so glyphs are never clipped by a later fill.
        run_start = 0
        while run_start < cols:
            run_bg = row[run_start][1].bg
            run_end = run_start + 1
            while run_end < cols and row[run_end][1].bg == run_bg:
                run_end += 1
            if run_bg != DEFAULT_BG:
                draw.rectangle(
                    [
                        (pad + run_start * cell_w, y),
                        (pad + run_end * cell_w - 1, y + cell_h - 1),
                    ],
                    fill=run_bg,
                )
            run_start = run_end

        for col_index, (symbol, style) in enumerate(row):
            if not symbol.strip():
                continue
            font_set = fonts["bold" if style.bold else "regular"]
            face = font_set.face_for(symbol)
            if face is None:
                continue
            let colour = dim_colour(style.fg) if style.dim else style.fg
            # Fallback faces differ in advance width and ascent, so glyphs are
            # centred in the cell and pinned to the primary face's baseline.
            offset = max(0, round((cell_w - face.getlength(symbol)) / 2))
            gx = pad + col_index * cell_w + offset
            gy = y + baseline
            draw.text(
                (gx, gy),
                symbol,
                font=face,
                fill=colour,
                anchor="ls",
            )
            # FreeType anti-aliases onto the cell background, which turns a
            # lone `#A78BFA` `>` on black into a gray fringe. Snap coverage
            # back to the exact lock colour so composer and picker carets match.
            cell_bg = style.bg if style.bg != DEFAULT_BG else CANVAS_BG
            snap_cell_to_fg(
                image,
                pad + col_index * cell_w,
                y,
                pad + (col_index + 1) * cell_w,
                y + cell_h,
                colour,
                cell_bg,
            )

    return image


def build_gif(png_dir: Path, output: Path, fps: int, scale: int | None) -> None:
    if shutil.which("ffmpeg") is None:
        raise SystemExit("ffmpeg is required to assemble the GIF but was not found on PATH.")

    scale_filter = f"scale={scale}:-1:flags=lanczos," if scale else ""
    filters = (
        f"[0:v]{scale_filter}split[a][b];"
        "[a]palettegen=stats_mode=diff:max_colors=192[p];"
        "[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle"
    )

    output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "ffmpeg",
            "-y",
            "-loglevel",
            "error",
            "-framerate",
            str(fps),
            "-i",
            str(png_dir / "%05d.png"),
            "-filter_complex",
            filters,
            "-loop",
            "0",
            str(output),
        ],
        check=True,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--frames",
        type=Path,
        default=Path("target/tui-demo"),
        help="Directory holding manifest.json and the .ans frames",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("docs/media/intro.gif"),
        help="Path of the GIF to write",
    )
    parser.add_argument("--font-size", type=int, default=16, help="Glyph size in pixels")
    parser.add_argument("--padding", type=int, default=16, help="Canvas padding in pixels")
    parser.add_argument(
        "--scale",
        type=int,
        default=None,
        help="Optional output width in pixels (height follows the aspect ratio)",
    )
    parser.add_argument(
        "--keep-pngs",
        type=Path,
        default=None,
        help="Keep the intermediate PNG frames in this directory",
    )
    parser.add_argument(
        "--png-only",
        type=Path,
        default=None,
        help="Write named PNGs from manifest labels to this directory and skip the GIF",
    )
    args = parser.parse_args()

    manifest_path = args.frames / "manifest.json"
    if not manifest_path.is_file():
        raise SystemExit(
            f"No manifest at {manifest_path}. Run:\n"
            "  cargo run -p cortex-tui-capture --bin generate_tui_demo"
        )

    manifest = json.loads(manifest_path.read_text())
    width = manifest["width"]
    height = manifest["height"]
    fps = manifest["fps"]

    fonts = {
        "regular": FontSet("regular", args.font_size),
        "bold": FontSet("bold", args.font_size),
    }
    cell_w = round(fonts["regular"].primary.getlength("M"))
    cell_h = round(args.font_size * 1.38)
    ascent, _ = fonts["regular"].primary.getmetrics()
    baseline = min(ascent, cell_h - 2)

    png_root = args.keep_pngs
    temp_dir = None
    named_dir = args.png_only
    if named_dir is not None:
        named_dir.mkdir(parents=True, exist_ok=True)
        png_root = named_dir
    elif png_root is None:
        temp_dir = tempfile.TemporaryDirectory()
        png_root = Path(temp_dir.name)
    else:
        png_root.mkdir(parents=True, exist_ok=True)
    png_root.mkdir(parents=True, exist_ok=True)

    try:
        output_index = 0
        for entry in manifest["frames"]:
            ansi = (args.frames / entry["file"]).read_text()
            grid = parse_ansi(ansi, width, height)
            check_glyph_coverage(grid, fonts)
            image = render_frame(grid, fonts, cell_w, cell_h, args.padding, baseline)
            if named_dir is not None:
                label = entry.get("label") or Path(entry["file"]).stem
                safe = "".join(ch if ch.isalnum() or ch in "-_" else "_" for ch in label)
                image.save(named_dir / f"{safe}.png")
                output_index += 1
            else:
                for _ in range(entry["hold"]):
                    image.save(png_root / f"{output_index:05d}.png")
                    output_index += 1

        if named_dir is not None:
            print(f"Wrote {output_index} PNGs to {named_dir}")
            return 0

        build_gif(png_root, args.output, fps, args.scale)
    finally:
        if temp_dir is not None:
            temp_dir.cleanup()

    size_mb = args.output.stat().st_size / (1024 * 1024)
    print(
        f"Wrote {args.output} - {output_index} frames, "
        f"{output_index / fps:.1f}s at {fps} fps, {size_mb:.2f} MB"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
