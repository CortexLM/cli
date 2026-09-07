#!/usr/bin/env python3
"""Composite raw TUI captures into generated macOS-styled chrome.

The CLI below retains window-only lock captures. The README rasteriser uses
build_desktop and animate_mouse for wallpaper, menu bar, Dock and pointer.

Each REAL capture from `docs/media/tui-lock/{40x12,120x40}/` is pasted 1:1
under a Terminal.app title bar (traffic lights, `cortex-api — cortex — W×H`
proxy-icon title) and the macOS window corners are rounded. That is the whole
output: the window, cropped tight — no desktop wallpaper, no menu bar, no drop
shadow. The pixels outside the rounded corners are transparent, like a
`⌘⇧4` + space window capture with the shadow turned off.

Because the content is never resampled, a 40×12 capture yields a genuinely
small 40-column window and a 120×40 capture a wide 120-column one — the two
packs differ in canvas size, and every locked colour (the `#4ADE80` of a
`+58`, the green of a selected `>`) survives exactly. No terminal text is
ever invented; the rounded corners belong to the macOS window only, the TUI
itself stays frameless.

Usage:
    python3 scripts/compose-macos-terminal.py \
        --raw docs/media/tui-lock/40x12 \
        --output docs/media/tui-lock/macos/40x12 \
        --size 40x12
"""

from __future__ import annotations

import argparse
import math
import sys
from functools import lru_cache
from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageFilter, ImageFont
except ImportError:  # pragma: no cover - dependency guard
    sys.exit(
        "Pillow is required to composite the macOS screenshots.\n"
        "Install it with: python3 -m pip install pillow"
    )

# ---------------------------------------------------------------------------
# Window metrics (1x, non-retina screenshot look)
# ---------------------------------------------------------------------------

TITLEBAR_H = 28
CORNER_RADIUS = 10
TRAFFIC_LIGHT_R = 6
TRAFFIC_LIGHT_CX = 19
TRAFFIC_LIGHT_GAP = 20
SUPERSAMPLE = 4

# Terminal.app dark appearance
TITLEBAR_TOP = (0x3A, 0x3A, 0x3C)
TITLEBAR_BOTTOM = (0x2C, 0x2C, 0x2E)
TITLE_TEXT = (0xB8, 0xB8, 0xBD)

TRAFFIC_LIGHTS = [
    ((0xFF, 0x5F, 0x57), (0xE0, 0x44, 0x3E)),  # close
    ((0xFE, 0xBC, 0x2E), (0xDE, 0xA1, 0x23)),  # minimise
    ((0x28, 0xC8, 0x40), (0x1D, 0xAD, 0x33)),  # zoom
]

SANS_REGULAR = [
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
]


def font(candidates: list[str], size: int) -> ImageFont.FreeTypeFont:
    for path in candidates:
        if Path(path).is_file():
            return ImageFont.truetype(path, size)
    raise SystemExit("No sans font found for the window title.")


def draw_folder_icon(draw: ImageDraw.ImageDraw, x: int, cy: int) -> int:
    """Small blue folder proxy icon; returns its width."""
    w, h = 14, 11
    y0 = cy - h // 2
    tab_w = round(w * 0.42)
    draw.rounded_rectangle([x, y0, x + tab_w, y0 + 4], radius=1, fill=(0x4C, 0x9E, 0xE8))
    draw.rounded_rectangle([x, y0 + 2, x + w, y0 + h], radius=2, fill=(0x55, 0xA9, 0xF0))
    draw.rounded_rectangle(
        [x, y0 + 3, x + w, y0 + h], radius=2, outline=(0x3E, 0x86, 0xC8), width=1
    )
    return w


def rounded_mask(size: tuple[int, int], radius: int) -> Image.Image:
    """Anti-aliased rounded-rectangle mask (drawn supersampled)."""
    ss = SUPERSAMPLE
    big = Image.new("L", (size[0] * ss, size[1] * ss), 0)
    ImageDraw.Draw(big).rounded_rectangle(
        [(0, 0), (size[0] * ss - 1, size[1] * ss - 1)], radius=radius * ss, fill=255
    )
    return big.resize(size, Image.LANCZOS)


def build_window(content: Image.Image, title: str) -> Image.Image:
    """Terminal.app window — title bar over the capture, 1:1 — on transparent."""
    width = content.width
    height = TITLEBAR_H + content.height
    window = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    draw = ImageDraw.Draw(window)

    # Title bar gradient, a highlight along the very top and a divider above
    # the content.
    for y in range(TITLEBAR_H):
        t = y / max(1, TITLEBAR_H - 1)
        draw.line(
            [(0, y), (width, y)],
            fill=(
                round(TITLEBAR_TOP[0] + (TITLEBAR_BOTTOM[0] - TITLEBAR_TOP[0]) * t),
                round(TITLEBAR_TOP[1] + (TITLEBAR_BOTTOM[1] - TITLEBAR_TOP[1]) * t),
                round(TITLEBAR_TOP[2] + (TITLEBAR_BOTTOM[2] - TITLEBAR_TOP[2]) * t),
                255,
            ),
        )
    draw.line([(0, 0), (width, 0)], fill=(255, 255, 255, 30))
    draw.line([(0, TITLEBAR_H - 1), (width, TITLEBAR_H - 1)], fill=(12, 12, 12, 255))

    # Traffic lights — supersampled circles for smooth anti-aliased rims.
    cy = TITLEBAR_H // 2
    ss = SUPERSAMPLE
    lights = Image.new("RGBA", (width * ss, TITLEBAR_H * ss), (0, 0, 0, 0))
    ldraw = ImageDraw.Draw(lights)
    for i, (fill, ring) in enumerate(TRAFFIC_LIGHTS):
        cx = (TRAFFIC_LIGHT_CX + i * TRAFFIC_LIGHT_GAP) * ss
        r = TRAFFIC_LIGHT_R * ss
        ldraw.ellipse(
            [cx - r, cy * ss - r, cx + r, cy * ss + r],
            fill=(*fill, 255),
            outline=(*ring, 255),
            width=ss,
        )
    window.alpha_composite(lights.resize((width, TITLEBAR_H), Image.LANCZOS), (0, 0))

    # Proxy icon + centred title; the title yields to the traffic lights on
    # a narrow window and is dropped rather than overlapped.
    tfont = font(SANS_REGULAR, 12)
    bbox = draw.textbbox((0, 0), title, font=tfont)
    text_w = bbox[2] - bbox[0]
    icon_w, gap = 14, 5
    tx = (width - (icon_w + gap + text_w)) // 2
    lights_end = TRAFFIC_LIGHT_CX + 2 * TRAFFIC_LIGHT_GAP + TRAFFIC_LIGHT_R + 8
    if tx > lights_end:
        draw_folder_icon(draw, tx, cy)
        draw.text(
            (tx + icon_w + gap, cy - (bbox[3] - bbox[1]) // 2 - bbox[1]),
            title,
            font=tfont,
            fill=(*TITLE_TEXT, 255),
        )

    # The real capture, pasted 1:1 — never resampled.
    window.paste(content.convert("RGBA"), (0, TITLEBAR_H))

    # Round the macOS window chrome only (never the TUI content); the corners
    # stay transparent.
    window.putalpha(rounded_mask((width, height), CORNER_RADIUS))

    # Subtle 1px window outline that hugs the rounded shape.
    outline = Image.new("RGBA", (width * ss, height * ss), (0, 0, 0, 0))
    ImageDraw.Draw(outline).rounded_rectangle(
        [(0, 0), (width * ss - 1, height * ss - 1)],
        radius=CORNER_RADIUS * ss,
        outline=(0, 0, 0, 150),
        width=ss,
    )
    return Image.alpha_composite(window, outline.resize((width, height), Image.LANCZOS))


def compose(raw_png: Path, out_png: Path, title: str) -> tuple[int, int]:
    content = Image.open(raw_png).convert("RGB")
    window = build_window(content, title)
    out_png.parent.mkdir(parents=True, exist_ok=True)
    window.save(out_png)
    return window.size

# ponytail: procedural macOS-styled chrome, not an OS recording; use a real
# desktop capture only when demonstrating native OS interactions.
DESKTOP_MARGIN = 80
WINDOW_TOP = 76
CONTENT_INSET = 12

@lru_cache(maxsize=2)
def desktop_backdrop(content_size: tuple[int, int]) -> Image.Image:
    """Static desktop and shadow; never paint over the terminal capture."""
    cw, ch = content_size
    ww, wh = cw + 2 * CONTENT_INSET, ch + 2 * CONTENT_INSET + TITLEBAR_H
    width, height = ww + 2 * DESKTOP_MARGIN, wh + WINDOW_TOP + 100
    desktop = Image.new("RGB", (width, height))
    draw = ImageDraw.Draw(desktop)
    for y in range(height):
        t = y / height
        draw.line((0, y, width, y), fill=(round(18 + 22*t), round(43 + 40*t), round(58 + 27*t)))
    draw.ellipse((-width//2, height//3, width, height*2), fill=(38, 103, 103))
    draw.ellipse((width//3, height//2, width*2, height*2), fill=(62, 133, 119))
    draw.ellipse((width//2, height*3//4, width*2, height*2), fill=(96, 158, 130))
    draw.rectangle((0, 0, width, 27), fill=(22, 37, 43))
    ui_font = font(SANS_REGULAR, 13)
    draw.text((20, 6), "●    Terminal    Shell    Edit    View    Window    Help", font=ui_font, fill=(238, 243, 241))
    draw.text((width-207, 6), "Wi-Fi    100%    Mon 9:41", font=ui_font, fill=(238, 243, 241))

    dock_x, dock_y = width//2 - 133, height-76
    draw.rounded_rectangle((dock_x, dock_y, dock_x+266, height-12), radius=18,
                           fill=(88, 125, 123), outline=(147, 174, 166))
    for i, (label, colour) in enumerate((("Finder", (69, 154, 226)), ("Web", (62, 153, 177)),
                                       (">_", (27, 29, 32)), ("Files", (72, 151, 216)))):
        x = dock_x + 13 + i*61
        draw.rounded_rectangle((x, dock_y+8, x+48, dock_y+51), radius=10, fill=colour)
        draw.text((x+24, dock_y+29), label, font=font(SANS_REGULAR, 11 if i != 2 else 22),
                  fill="white", anchor="mm")
    draw.ellipse((dock_x+157, height-20, dock_x+161, height-16), fill=(229, 242, 236))
    shadow = Image.new("RGBA", desktop.size)
    ImageDraw.Draw(shadow).rounded_rectangle(
        (DESKTOP_MARGIN, WINDOW_TOP+14, DESKTOP_MARGIN+ww, WINDOW_TOP+wh+14),
        radius=CORNER_RADIUS, fill=(0, 0, 0, 155))
    return Image.alpha_composite(desktop.convert("RGBA"), shadow.filter(ImageFilter.GaussianBlur(18)))

def build_desktop(content: Image.Image, title: str) -> Image.Image:
    """Inset preserves every capture pixel, including the rounded corners."""
    padded = Image.new("RGB", (content.width+2*CONTENT_INSET, content.height+2*CONTENT_INSET))
    padded.paste(content, (CONTENT_INSET, CONTENT_INSET))
    desktop = desktop_backdrop(content.size).copy()
    desktop.alpha_composite(build_window(padded, title), (DESKTOP_MARGIN, WINDOW_TOP))
    return desktop.convert("RGB")

def animate_mouse(desktop: Image.Image, frame: int, total: int) -> Image.Image:
    """Smooth looping pointer in the desktop gutter; no obscured CLI pixels."""
    phase = 2 * math.pi * frame / max(1, total)
    x = round(desktop.width - 51 + 15 * math.sin(phase))
    y = round(118 + (desktop.height - 264) * (1 - math.cos(phase)) / 2)
    image = desktop.copy()
    draw = ImageDraw.Draw(image)
    points = [(x, y), (x, y+23), (x+6, y+17), (x+11, y+27),
              (x+15, y+25), (x+10, y+15), (x+19, y+15)]
    draw.polygon(points, fill=(18, 20, 22))
    draw.line(points + [points[0]], fill="white", width=2)
    return image


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw", type=Path, required=True, help="Directory of raw lock PNGs")
    parser.add_argument("--output", type=Path, required=True, help="Directory for macOS composites")
    parser.add_argument("--size", required=True, help="Terminal size label, e.g. 40x12")
    args = parser.parse_args()

    pngs = sorted(args.raw.glob("*.png"))
    if not pngs:
        raise SystemExit(f"No PNGs found under {args.raw}")

    cols, rows = args.size.split("x", 1)
    title = f"cortex-api — cortex — {cols}×{rows}"

    sizes = {compose(raw_png, args.output / raw_png.name, title) for raw_png in pngs}
    dims = ", ".join(f"{w}×{h}" for w, h in sorted(sizes))
    print(f"Wrote {len(pngs)} macOS window composites ({dims}) to {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
