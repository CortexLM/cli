#!/usr/bin/env python3
"""Composite raw TUI lock captures into a photorealistic macOS Terminal.app.

The raw captures under `docs/media/tui-lock/{40x12,120x40}/` are the source
of truth for every TUI pixel — this script never invents terminal text. Each
capture is placed 1:1 inside a Terminal.app window (traffic lights, centered
`cortex-api — cortex — WxH` title, dark title bar, rounded window chrome,
native drop shadow) on a macOS desktop wallpaper. The rounded corners belong
to the macOS window only; the TUI content itself stays frameless.

Usage:
    python3 scripts/compose-macos-terminal.py \
        --raw docs/media/tui-lock/40x12 \
        --output docs/media/tui-lock/macos/40x12 \
        --size 40x12
"""

from __future__ import annotations

import argparse
import sys
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

TITLEBAR_HEIGHT = 28
CORNER_RADIUS = 12
TRAFFIC_LIGHT_RADIUS = 6
TRAFFIC_LIGHT_GAP = 20
TRAFFIC_LIGHT_X = 19
SHADOW_BLUR = 34
SHADOW_OFFSET_Y = 18
SHADOW_ALPHA = 130
MARGIN_X_FRACTION = 0.16
MARGIN_TOP_FRACTION = 0.14
MARGIN_BOTTOM_FRACTION = 0.20

# Terminal.app dark appearance
TITLEBAR_TOP = (0x30, 0x30, 0x32)
TITLEBAR_BOTTOM = (0x28, 0x28, 0x2A)
TITLEBAR_DIVIDER = (0x00, 0x00, 0x00)
TITLE_TEXT = (0x9E, 0x9E, 0xA3)
CONTENT_BG = (0, 0, 0)

TRAFFIC_LIGHTS = [
    ((0xFF, 0x5F, 0x57), (0xE0, 0x44, 0x3E)),  # close
    ((0xFE, 0xBC, 0x2E), (0xDE, 0xA1, 0x23)),  # minimise
    ((0x28, 0xC8, 0x40), (0x1D, 0xAD, 0x33)),  # zoom
]

TITLE_FONT_CANDIDATES = [
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
]


def title_font(size: int = 13) -> ImageFont.FreeTypeFont:
    for path in TITLE_FONT_CANDIDATES:
        if Path(path).is_file():
            return ImageFont.truetype(path, size)
    raise SystemExit("No sans font found for the window title.")


def make_wallpaper(width: int, height: int) -> Image.Image:
    """Deterministic macOS-style abstract gradient wallpaper (dark)."""
    base = Image.new("RGB", (width, height))
    draw = ImageDraw.Draw(base)

    top = (24, 16, 48)
    bottom = (58, 32, 88)
    for y in range(height):
        t = y / max(1, height - 1)
        draw.line(
            [(0, y), (width, y)],
            fill=(
                round(top[0] + (bottom[0] - top[0]) * t),
                round(top[1] + (bottom[1] - top[1]) * t),
                round(top[2] + (bottom[2] - top[2]) * t),
            ),
        )

    # Soft colour blobs, Sequoia-style. Fixed positions keep output stable.
    blobs = [
        ((-0.20, -0.25, 0.55, 0.60), (86, 48, 160)),
        ((0.55, -0.15, 1.25, 0.45), (140, 70, 190)),
        ((0.30, 0.55, 1.10, 1.30), (60, 40, 130)),
        ((-0.15, 0.60, 0.40, 1.25), (170, 90, 150)),
    ]
    overlay = Image.new("RGB", (width, height), (0, 0, 0))
    odraw = ImageDraw.Draw(overlay)
    for (x0, y0, x1, y1), colour in blobs:
        odraw.ellipse(
            [x0 * width, y0 * height, x1 * width, y1 * height],
            fill=colour,
        )
    overlay = overlay.filter(ImageFilter.GaussianBlur(radius=min(width, height) / 6))
    return Image.blend(base, overlay, 0.5)


SUPERSAMPLE = 4


def rounded_mask(size: tuple[int, int], radius: int) -> Image.Image:
    """Anti-aliased rounded-rectangle mask (drawn supersampled)."""
    ss = SUPERSAMPLE
    big = Image.new("L", (size[0] * ss, size[1] * ss), 0)
    ImageDraw.Draw(big).rounded_rectangle(
        [(0, 0), (size[0] * ss - 1, size[1] * ss - 1)], radius=radius * ss, fill=255
    )
    return big.resize(size, Image.LANCZOS)


def build_window(content: Image.Image, title: str) -> Image.Image:
    """Terminal.app window: title bar + content, rounded, on transparent."""
    width = content.width
    height = TITLEBAR_HEIGHT + content.height
    window = Image.new("RGBA", (width, height))
    draw = ImageDraw.Draw(window)

    # Title bar gradient
    for y in range(TITLEBAR_HEIGHT):
        t = y / max(1, TITLEBAR_HEIGHT - 1)
        draw.line(
            [(0, y), (width, y)],
            fill=(
                round(TITLEBAR_TOP[0] + (TITLEBAR_BOTTOM[0] - TITLEBAR_TOP[0]) * t),
                round(TITLEBAR_TOP[1] + (TITLEBAR_BOTTOM[1] - TITLEBAR_TOP[1]) * t),
                round(TITLEBAR_TOP[2] + (TITLEBAR_BOTTOM[2] - TITLEBAR_TOP[2]) * t),
                255,
            ),
        )
    # Hairline highlight along the very top, divider above the content.
    draw.line([(0, 0), (width, 0)], fill=(255, 255, 255, 34))
    draw.line(
        [(0, TITLEBAR_HEIGHT - 1), (width, TITLEBAR_HEIGHT - 1)],
        fill=(*TITLEBAR_DIVIDER, 255),
    )

    # Traffic lights — supersampled circles for smooth anti-aliased rims.
    cy = TITLEBAR_HEIGHT // 2
    ss = SUPERSAMPLE
    lights = Image.new("RGBA", (width * ss, TITLEBAR_HEIGHT * ss), (0, 0, 0, 0))
    ldraw = ImageDraw.Draw(lights)
    for i, (fill, ring) in enumerate(TRAFFIC_LIGHTS):
        cx = (TRAFFIC_LIGHT_X + i * TRAFFIC_LIGHT_GAP) * ss
        r = TRAFFIC_LIGHT_RADIUS * ss
        ldraw.ellipse(
            [cx - r, cy * ss - r, cx + r, cy * ss + r],
            fill=(*fill, 255),
            outline=(*ring, 255),
            width=ss,
        )
    lights = lights.resize((width, TITLEBAR_HEIGHT), Image.LANCZOS)
    window.alpha_composite(lights, (0, 0))

    # Centered window title
    font = title_font()
    bbox = draw.textbbox((0, 0), title, font=font)
    tx = (width - (bbox[2] - bbox[0])) // 2
    ty = cy - (bbox[3] - bbox[1]) // 2 - bbox[1]
    draw.text((tx, ty), title, font=font, fill=(*TITLE_TEXT, 255))

    # Terminal content — the real capture, 1:1, on its black area.
    window.paste(content.convert("RGBA"), (0, TITLEBAR_HEIGHT))

    # Round the macOS window chrome only.
    window.putalpha(rounded_mask((width, height), CORNER_RADIUS))

    # Subtle 1px window outline, drawn after masking so it hugs the shape.
    ss = SUPERSAMPLE
    outline = Image.new("RGBA", (width * ss, height * ss), (0, 0, 0, 0))
    ImageDraw.Draw(outline).rounded_rectangle(
        [(0, 0), (width * ss - 1, height * ss - 1)],
        radius=CORNER_RADIUS * ss,
        outline=(0, 0, 0, 140),
        width=ss,
    )
    window = Image.alpha_composite(window, outline.resize((width, height), Image.LANCZOS))
    return window


def compose(raw_png: Path, out_png: Path, wallpaper: Image.Image, title: str) -> None:
    content = Image.open(raw_png).convert("RGB")

    window = build_window(content, title)

    margin_x = round(window.width * MARGIN_X_FRACTION)
    margin_top = round(window.height * MARGIN_TOP_FRACTION) + 8
    margin_bottom = round(window.height * MARGIN_BOTTOM_FRACTION) + 8
    canvas_w = window.width + margin_x * 2
    canvas_h = window.height + margin_top + margin_bottom

    canvas = wallpaper.resize((canvas_w, canvas_h)).convert("RGBA")

    # Native window shadow: blurred rounded rect behind the window.
    shadow = Image.new("RGBA", (canvas_w, canvas_h), (0, 0, 0, 0))
    sdraw = ImageDraw.Draw(shadow)
    sdraw.rounded_rectangle(
        [
            (margin_x, margin_top + SHADOW_OFFSET_Y // 2),
            (margin_x + window.width - 1, margin_top + window.height - 1 + SHADOW_OFFSET_Y),
        ],
        radius=CORNER_RADIUS + 4,
        fill=(0, 0, 0, SHADOW_ALPHA),
    )
    shadow = shadow.filter(ImageFilter.GaussianBlur(SHADOW_BLUR))
    canvas = Image.alpha_composite(canvas, shadow)

    canvas.alpha_composite(window, (margin_x, margin_top))

    out_png.parent.mkdir(parents=True, exist_ok=True)
    canvas.convert("RGB").save(out_png)


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

    # One deterministic wallpaper per size; sized generously and rescaled to
    # each canvas (all canvases in a set share dimensions anyway).
    probe = Image.open(pngs[0])
    wallpaper = make_wallpaper(probe.width * 2, probe.height * 2 + 200)

    for raw_png in pngs:
        compose(raw_png, args.output / raw_png.name, wallpaper, title)
    print(f"Wrote {len(pngs)} macOS composites to {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
