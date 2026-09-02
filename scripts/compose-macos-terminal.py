#!/usr/bin/env python3
"""Composite raw TUI lock captures into a photorealistic macOS Terminal.app.

Rebuilds the designer chrome template — macOS Sequoia ray wallpaper, dark
menu bar (Apple mark, Terminal menus, status icons, `Fri May 10 14:32`), and
a Terminal.app window with traffic lights and a `cortex-api — cortex — W×H`
proxy-icon title over an empty black content rect — then places each REAL
capture from `docs/media/tui-lock/{40x12,120x40}/` into that rect. The TUI
pixels are only ever scaled uniformly; no terminal text is invented. The
rounded corners belong to the macOS window; the TUI itself stays frameless.

Placement — TUI pixels are never resampled, so every locked colour (the
`#4ADE80` of a `+58`, the `#7DD3FC` of a selected `>`) survives exactly:
- 120x40 captures are pasted 1:1 into the content rect (the chrome renders at
  2.5x so the 912px-tall capture fits), left-anchored; the remaining cells
  stay black, like any terminal background.
- 40x12 captures keep the same chrome and title format but occupy a smaller
  content rect at the top-left of the window, blown up by an integer factor
  with nearest-neighbour sampling (a 1x window on a retina screen).

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
from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageFilter, ImageFont
except ImportError:  # pragma: no cover - dependency guard
    sys.exit(
        "Pillow is required to composite the macOS screenshots.\n"
        "Install it with: python3 -m pip install pillow"
    )

# ---------------------------------------------------------------------------
# Geometry — the designer template is 1024x683; everything renders at 2.5x
# (2560x1707) from these 1x metrics, the smallest uniform scale at which a
# 120x40 capture fits the template's content rect without resampling.
# ---------------------------------------------------------------------------

SCALE = 2.5
CANVAS_W, CANVAS_H = 1024, 683


def px(value: float, s: float) -> int:
    """Scale a 1x metric to output pixels."""
    return int(round(value * s))


# Largest nearest-neighbour blow-up for a small capture: 2x reads as a 1x
# terminal window on a retina screen; more would dwarf the chrome.
MAX_BLOWUP = 2
MENUBAR_H = 24
WIN_X0, WIN_X1 = 147, 877
TITLEBAR_Y0 = 143
TITLEBAR_H = 26
CONTENT_Y0 = TITLEBAR_Y0 + TITLEBAR_H
CONTENT_Y1 = 538
CORNER_RADIUS = 10
TRAFFIC_LIGHT_R = 6
TRAFFIC_LIGHT_CX = 163
TRAFFIC_LIGHT_GAP = 18

TITLEBAR_TOP = (0x3A, 0x3A, 0x3C)
TITLEBAR_BOTTOM = (0x2C, 0x2C, 0x2E)
TITLE_TEXT = (0xB8, 0xB8, 0xBD)
MENUBAR_TINT = (0x2E, 0x24, 0x1E)
MENU_TEXT = (0xF2, 0xF2, 0xF2)

TRAFFIC_LIGHTS = [
    ((0xFF, 0x5F, 0x57), (0xE0, 0x44, 0x3E)),
    ((0xFE, 0xBC, 0x2E), (0xDE, 0xA1, 0x23)),
    ((0x28, 0xC8, 0x40), (0x1D, 0xAD, 0x33)),
]

SANS_REGULAR = [
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
]
SANS_BOLD = [
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
]


def font(candidates: list[str], size: int) -> ImageFont.FreeTypeFont:
    for path in candidates:
        if Path(path).is_file():
            return ImageFont.truetype(path, size)
    raise SystemExit("No sans font found for the chrome text.")


# ---------------------------------------------------------------------------
# Wallpaper — Sequoia-style rays: a warm burst near the top centre with
# blue/orange rays fanning out and widening towards the bottom.
# ---------------------------------------------------------------------------

# Ray stops over u = angle position (0 = right edge, 1 = left edge).
RAY_STOPS = [
    (0.00, (52, 70, 164)),
    (0.10, (232, 150, 88)),
    (0.17, (46, 62, 152)),
    (0.25, (238, 158, 92)),
    (0.33, (244, 182, 120)),
    (0.41, (248, 214, 164)),
    (0.475, (252, 238, 206)),
    (0.545, (250, 228, 190)),
    (0.60, (172, 182, 226)),
    (0.68, (118, 134, 212)),
    (0.78, (80, 100, 194)),
    (0.88, (58, 78, 172)),
    (1.00, (42, 58, 148)),
]

EDGE_SOFTNESS = 0.22


def _ray_colour(u: float) -> tuple[int, int, int]:
    """Colour for angular position `u`, with softened band edges."""
    u = min(1.0, max(0.0, u))
    for i in range(len(RAY_STOPS) - 1):
        u0, c0 = RAY_STOPS[i]
        u1, c1 = RAY_STOPS[i + 1]
        if u0 <= u <= u1:
            span = max(1e-6, u1 - u0)
            t = (u - u0) / span
            # keep the body of each ray flat; blend only near the edge
            if t < 1.0 - EDGE_SOFTNESS:
                return c0
            w = (t - (1.0 - EDGE_SOFTNESS)) / EDGE_SOFTNESS
            return (
                round(c0[0] + (c1[0] - c0[0]) * w),
                round(c0[1] + (c1[1] - c0[1]) * w),
                round(c0[2] + (c1[2] - c0[2]) * w),
            )
    return RAY_STOPS[-1][1]


def make_wallpaper(width: int, height: int) -> Image.Image:
    """Deterministic Sequoia-like ray-burst wallpaper."""
    # Rays converge just above the visible top edge, slightly right of centre.
    ox, oy = width * 0.53, -height * 0.08

    small_w, small_h = width // 4, height // 4
    img = Image.new("RGB", (small_w, small_h))
    px = img.load()
    sx, sy = ox / 4, oy / 4
    diag = math.hypot(small_w, small_h)
    for y in range(small_h):
        for x in range(small_w):
            # 0 = ray pointing right, 1 = ray pointing left
            u = math.atan2(y - sy, x - sx) / math.pi
            r, g, b = _ray_colour(u)
            # warm glow around the burst origin
            dist = math.hypot(x - sx, y - sy) / diag
            glow = max(0.0, 1.0 - dist * 2.1)
            glow *= glow
            r = round(r + (253 - r) * glow)
            g = round(g + (243 - g) * glow * 0.95)
            b = round(b + (216 - b) * glow * 0.9)
            # slight falloff towards the bottom corners
            fade = 1.0 - 0.16 * (y / small_h) * (abs(x - sx) / small_w + 0.35)
            px[x, y] = (round(r * fade), round(g * fade), round(b * fade))
    img = img.resize((width, height), Image.LANCZOS)
    return img.filter(ImageFilter.GaussianBlur(radius=width / 400))


# ---------------------------------------------------------------------------
# Menu bar
# ---------------------------------------------------------------------------


def draw_apple_mark(draw: ImageDraw.ImageDraw, cx: int, cy: int, h: int, colour, bar_colour) -> None:
    """Small solid Apple-ish silhouette (body, leaf, bitten right side)."""
    body_h = round(h * 0.78)
    body_w = round(body_h * 0.92)
    x0, y0 = cx - body_w // 2, cy - body_h // 2 + round(h * 0.10)
    draw.rounded_rectangle(
        [x0, y0, x0 + body_w, y0 + body_h], radius=body_w // 2 - 1, fill=colour
    )
    # bite on the right, punched with the bar colour
    bite_r = round(body_h * 0.26)
    draw.ellipse(
        [
            x0 + body_w - bite_r // 2,
            y0 + round(body_h * 0.30) - bite_r,
            x0 + body_w + bite_r + bite_r // 2,
            y0 + round(body_h * 0.30) + bite_r,
        ],
        fill=bar_colour,
    )
    # leaf
    leaf_w, leaf_h = round(body_w * 0.34), round(body_h * 0.30)
    lx, ly = cx + round(body_w * 0.02), y0 - leaf_h + 2
    draw.ellipse([lx, ly, lx + leaf_w, ly + leaf_h], fill=colour)


def draw_menu_bar(canvas: Image.Image, s: float) -> None:
    """Dark translucent menu bar over the wallpaper."""
    bar_h = px(MENUBAR_H, s)
    strip = canvas.crop((0, 0, canvas.width, bar_h)).filter(
        ImageFilter.GaussianBlur(radius=8 * s)
    )
    tint = Image.new("RGB", strip.size, MENUBAR_TINT)
    strip = Image.blend(strip, tint, 0.86)
    canvas.paste(strip, (0, 0))

    draw = ImageDraw.Draw(canvas)
    bold = font(SANS_BOLD, px(13, s))
    regular = font(SANS_REGULAR, px(13, s))
    cy = bar_h // 2
    stroke = max(1, px(1, s))

    bar_colour = canvas.getpixel((px(26, s), cy))
    draw_apple_mark(draw, px(20, s), cy, px(15, s), MENU_TEXT, bar_colour)

    x = px(38, s)
    for i, item in enumerate(["Terminal", "File", "Edit", "View", "Shell", "Window", "Help"]):
        f = bold if i == 0 else regular
        bbox = draw.textbbox((0, 0), item, font=f)
        draw.text((x, cy - (bbox[3] - bbox[1]) // 2 - bbox[1]), item, font=f, fill=MENU_TEXT)
        x += (bbox[2] - bbox[0]) + px(15, s)

    # Right side: clock, then status icons right-to-left.
    clock = "Fri May 10  14:32"
    bbox = draw.textbbox((0, 0), clock, font=regular)
    cx = canvas.width - px(12, s) - (bbox[2] - bbox[0])
    draw.text((cx, cy - (bbox[3] - bbox[1]) // 2 - bbox[1]), clock, font=regular, fill=MENU_TEXT)

    ix = cx - px(20, s)
    # control-centre toggle
    draw.rounded_rectangle(
        [ix - px(7, s), cy - px(4, s), ix + px(7, s), cy + px(4, s)],
        radius=px(4, s),
        outline=MENU_TEXT,
        width=stroke,
    )
    draw.ellipse([ix - px(5, s), cy - px(2, s), ix - stroke, cy + px(2, s)], fill=MENU_TEXT)
    ix -= px(26, s)
    # search
    draw.ellipse(
        [ix - px(5, s), cy - px(5, s), ix + px(2, s), cy + px(2, s)],
        outline=MENU_TEXT,
        width=stroke,
    )
    draw.line(
        [ix + px(2, s), cy + px(2, s), ix + px(5, s), cy + px(5, s)], fill=MENU_TEXT, width=stroke
    )
    ix -= px(26, s)
    # wifi arcs
    for r in (7, 4):
        draw.arc(
            [ix - px(r, s), cy - px(r, s) + px(2, s), ix + px(r, s), cy + px(r, s) + px(2, s)],
            start=215,
            end=325,
            fill=MENU_TEXT,
            width=stroke,
        )
    draw.ellipse([ix - stroke, cy + stroke, ix + stroke, cy + px(3, s)], fill=MENU_TEXT)
    ix -= px(30, s)
    # battery
    draw.rounded_rectangle(
        [ix - px(11, s), cy - px(5, s), ix + px(9, s), cy + px(5, s)],
        radius=px(2, s),
        outline=MENU_TEXT,
        width=stroke,
    )
    draw.rectangle([ix + px(10, s), cy - px(2, s), ix + px(11, s), cy + px(2, s)], fill=MENU_TEXT)
    draw.rectangle([ix - px(9, s), cy - px(3, s), ix + px(4, s), cy + px(3, s)], fill=MENU_TEXT)


# ---------------------------------------------------------------------------
# Terminal window
# ---------------------------------------------------------------------------


def draw_folder_icon(draw: ImageDraw.ImageDraw, x: int, cy: int, s: float) -> int:
    """Small blue folder proxy icon; returns its width."""
    w, h = px(14, s), px(11, s)
    y0 = cy - h // 2
    tab_w = round(w * 0.42)
    draw.rounded_rectangle(
        [x, y0, x + tab_w, y0 + px(4, s)], radius=px(1, s), fill=(0x4C, 0x9E, 0xE8)
    )
    draw.rounded_rectangle(
        [x, y0 + px(2, s), x + w, y0 + h], radius=px(2, s), fill=(0x55, 0xA9, 0xF0)
    )
    draw.rounded_rectangle(
        [x, y0 + px(3, s), x + w, y0 + h], radius=px(2, s), outline=(0x3E, 0x86, 0xC8), width=1
    )
    return w


def build_window(content_w: int, content_h: int, title: str, s: float) -> Image.Image:
    """Terminal.app window (title bar + black content) on transparent."""
    width = content_w
    bar_h = px(TITLEBAR_H, s)
    height = bar_h + content_h
    window = Image.new("RGBA", (width, height))
    draw = ImageDraw.Draw(window)

    for y in range(bar_h):
        t = y / max(1, bar_h - 1)
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
    draw.line([(0, bar_h - 1), (width, bar_h - 1)], fill=(12, 12, 12, 255))

    draw.rectangle([(0, bar_h), (width, height)], fill=(0, 0, 0, 255))

    # Traffic lights (supersampled for round rims).
    ss = 4
    cy = bar_h // 2
    lights = Image.new("RGBA", (width * ss, bar_h * ss), (0, 0, 0, 0))
    ldraw = ImageDraw.Draw(lights)
    for i, (fill, ring) in enumerate(TRAFFIC_LIGHTS):
        cx = round(((TRAFFIC_LIGHT_CX - WIN_X0) + i * TRAFFIC_LIGHT_GAP) * s * ss)
        r = round(TRAFFIC_LIGHT_R * s * ss)
        ldraw.ellipse(
            [cx - r, cy * ss - r, cx + r, cy * ss + r],
            fill=(*fill, 255),
            outline=(*ring, 255),
            width=ss,
        )
    window.alpha_composite(lights.resize((width, bar_h), Image.LANCZOS), (0, 0))

    # Proxy icon + centred title.
    tfont = font(SANS_REGULAR, px(12, s))
    bbox = draw.textbbox((0, 0), title, font=tfont)
    text_w = bbox[2] - bbox[0]
    icon_w = px(14, s)
    gap = px(5, s)
    tx = (width - (icon_w + gap + text_w)) // 2
    draw_folder_icon(draw, tx, cy, s)
    draw.text(
        (tx + icon_w + gap, cy - (bbox[3] - bbox[1]) // 2 - bbox[1]),
        title,
        font=tfont,
        fill=(*TITLE_TEXT, 255),
    )

    # Round the macOS window chrome only (never the TUI content).
    radius = round(CORNER_RADIUS * s * ss)
    mask = Image.new("L", (width * ss, height * ss), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [(0, 0), (width * ss - 1, height * ss - 1)], radius=radius, fill=255
    )
    window.putalpha(mask.resize((width, height), Image.LANCZOS))

    outline = Image.new("RGBA", (width * ss, height * ss), (0, 0, 0, 0))
    ImageDraw.Draw(outline).rounded_rectangle(
        [(0, 0), (width * ss - 1, height * ss - 1)],
        radius=radius,
        outline=(0, 0, 0, 150),
        width=ss,
    )
    window = Image.alpha_composite(window, outline.resize((width, height), Image.LANCZOS))
    return window


# ---------------------------------------------------------------------------
# Composition
# ---------------------------------------------------------------------------


def place_capture(content: Image.Image, rect_w: int, rect_h: int) -> Image.Image:
    """Place the REAL capture in the black content rect without resampling.

    The capture is blown up by the largest integer factor that fits (1 = pasted
    as-is) using nearest-neighbour sampling, so every locked colour survives
    pixel-exact — no blend ever turns a `#4ADE80` glyph grayish. Anchored
    top-left; the rest of the rect stays black (empty terminal cells). Nothing
    is ever cropped.
    """
    area = Image.new("RGB", (rect_w, rect_h), (0, 0, 0))
    factor = min(rect_w // content.width, rect_h // content.height, MAX_BLOWUP)
    if factor < 1:
        raise SystemExit(
            f"capture {content.width}x{content.height} does not fit the "
            f"{rect_w}x{rect_h} content rect without resampling; raise SCALE"
        )
    placed = content
    if factor > 1:
        placed = content.resize(
            (content.width * factor, content.height * factor), Image.NEAREST
        )
    area.paste(placed, (0, 0))
    return area


def compose(
    raw_png: Path,
    out_png: Path,
    wallpaper_canvas: Image.Image,
    title: str,
    s: float,
) -> None:
    canvas = wallpaper_canvas.copy()

    content_w = px(WIN_X1 - WIN_X0, s)
    content_h = px(CONTENT_Y1 - CONTENT_Y0, s)
    capture = Image.open(raw_png).convert("RGB")
    content = place_capture(capture, content_w, content_h)

    window = build_window(content_w, content_h, title, s)
    window.paste(content.convert("RGBA"), (0, px(TITLEBAR_H, s)), mask=None)
    # Re-apply the rounded alpha lost by the opaque paste.
    ss = 4
    mask = Image.new("L", (window.width * ss, window.height * ss), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [(0, 0), (window.width * ss - 1, window.height * ss - 1)],
        radius=round(CORNER_RADIUS * s * ss),
        fill=255,
    )
    window.putalpha(mask.resize(window.size, Image.LANCZOS))

    # Native window shadow.
    wx, wy = px(WIN_X0, s), px(TITLEBAR_Y0, s)
    shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle(
        [(wx, wy + px(10, s)), (wx + window.width - 1, wy + window.height - 1 + px(14, s))],
        radius=px(CORNER_RADIUS + 4, s),
        fill=(0, 0, 0, 120),
    )
    shadow = shadow.filter(ImageFilter.GaussianBlur(16 * s))
    canvas = Image.alpha_composite(canvas.convert("RGBA"), shadow)
    canvas.alpha_composite(window, (wx, wy))

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
    s = SCALE

    base = make_wallpaper(px(CANVAS_W, s), px(CANVAS_H, s))
    draw_menu_bar(base, s)

    for raw_png in pngs:
        compose(raw_png, args.output / raw_png.name, base, title, s)
    print(f"Wrote {len(pngs)} macOS composites to {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
