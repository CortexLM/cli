#!/usr/bin/env python3
"""Composite raw TUI captures into generated macOS-styled chrome.

The CLI below retains window-only lock captures. The README rasteriser uses
build_desktop and animate_mouse for a photographed desktop, menu bar, Dock
and a pointer that walks the TUI storyboard.

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

# Photographed desktop for the README GIF. Lock captures stay window-only
# (see `compose`); this chrome is only composited onto `intro.gif`.
DESKTOP_MARGIN = 80
WINDOW_TOP = 76
CONTENT_INSET = 12
MENUBAR_H = 27
# Green forest photo, free license (Unsplash sunlight-through-trees still).
WALLPAPER_PATH = Path(__file__).resolve().parents[1] / "docs/media/macos-wallpaper-green.jpg"
CLICK_LABELS = frozenset({"prompt-ready", "palette", "model", "composer"})


def desktop_metrics(content_size: tuple[int, int]) -> tuple[int, int, int, int]:
    """Return (desktop_w, desktop_h, window_w, window_h) for a 1:1 capture."""
    cw, ch = content_size
    ww, wh = cw + 2 * CONTENT_INSET, ch + 2 * CONTENT_INSET + TITLEBAR_H
    return ww + 2 * DESKTOP_MARGIN, wh + WINDOW_TOP + 100, ww, wh


def cover_crop(image: Image.Image, size: tuple[int, int]) -> Image.Image:
    """Scale-to-cover and center-crop `image` to `size`."""
    target_w, target_h = size
    src_w, src_h = image.size
    scale = max(target_w / src_w, target_h / src_h)
    new_w = max(target_w, round(src_w * scale))
    new_h = max(target_h, round(src_h * scale))
    resized = image.resize((new_w, new_h), Image.LANCZOS)
    left = (new_w - target_w) // 2
    top = (new_h - target_h) // 2
    return resized.crop((left, top, left + target_w, top + target_h))


def _frosted(photo: Image.Image, box: tuple[int, int, int, int], darken: float) -> Image.Image:
    x0, y0, x1, y1 = box
    strip = photo.crop((x0, y0, x1, y1))
    width, height = strip.size
    small = strip.resize((max(1, width // 12), max(1, height // 8)), Image.BILINEAR)
    blur = small.resize((width, height), Image.BILINEAR)
    overlay = Image.new("RGB", (width, height), (20, 22, 24))
    return Image.blend(blur, overlay, darken)


def _paint_menubar(desktop: Image.Image, photo: Image.Image) -> None:
    width, _ = desktop.size
    frost = _frosted(photo, (0, 0, width, MENUBAR_H), 0.48)
    desktop.paste(frost, (0, 0))
    draw = ImageDraw.Draw(desktop)
    draw.line((0, MENUBAR_H - 1, width, MENUBAR_H - 1), fill=(255, 255, 255, 28))
    ui_font = font(SANS_REGULAR, 13)
    # Simple apple mark —  is missing from the Linux sans faces we ship.
    draw.ellipse((16, 7, 28, 20), fill=(244, 246, 244))
    draw.polygon([(22, 5), (26, 9), (18, 9)], fill=(244, 246, 244))
    draw.text(
        (36, 6),
        "Terminal    Shell    Edit    View    Window    Help",
        font=ui_font,
        fill=(244, 246, 244),
    )
    draw.text((width - 188, 6), "Wi-Fi    100%    Mon 9:41", font=ui_font, fill=(244, 246, 244))


def _paint_dock(desktop: Image.Image, photo: Image.Image) -> None:
    width, height = desktop.size
    dock_w, dock_h = 292, 64
    dock_x = (width - dock_w) // 2
    dock_y = height - dock_h - 10
    frost = _frosted(photo, (dock_x, dock_y, dock_x + dock_w, dock_y + dock_h), 0.36)
    plate = Image.new("RGBA", (dock_w, dock_h), (0, 0, 0, 0))
    ImageDraw.Draw(plate).rounded_rectangle(
        (0, 0, dock_w - 1, dock_h - 1),
        radius=18,
        fill=(32, 34, 36, 210),
        outline=(255, 255, 255, 48),
    )
    desktop.paste(frost, (dock_x, dock_y), plate.split()[-1])
    desktop.alpha_composite(plate, (dock_x, dock_y))
    draw = ImageDraw.Draw(desktop)
    icons = (
        (69, 154, 226),
        (232, 88, 80),
        (27, 29, 32),
        (88, 168, 92),
        (90, 96, 108),
    )
    for i, colour in enumerate(icons):
        x = dock_x + 14 + i * 56
        y = dock_y + 8
        draw.rounded_rectangle((x, y, x + 46, y + 40), radius=10, fill=colour)
        if i == 2:
            draw.text((x + 23, y + 20), ">_", font=font(SANS_REGULAR, 16), fill="white", anchor="mm")
            draw.ellipse((x + 20, dock_y + dock_h - 9, x + 26, dock_y + dock_h - 4), fill=(236, 240, 236))


def _paint_shadow(desktop: Image.Image, window_w: int, window_h: int) -> Image.Image:
    shadow = Image.new("RGBA", desktop.size, (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle(
        (DESKTOP_MARGIN, WINDOW_TOP + 14, DESKTOP_MARGIN + window_w, WINDOW_TOP + window_h + 14),
        radius=CORNER_RADIUS,
        fill=(0, 0, 0, 155),
    )
    return Image.alpha_composite(desktop, shadow.filter(ImageFilter.GaussianBlur(18)))


@lru_cache(maxsize=2)
def desktop_backdrop(content_size: tuple[int, int]) -> Image.Image:
    """Photographed desktop, menu bar, Dock and window shadow — never the TUI."""
    if not WALLPAPER_PATH.is_file():
        raise SystemExit(f"Missing wallpaper photo: {WALLPAPER_PATH}")
    width, height, window_w, window_h = desktop_metrics(content_size)
    photo = cover_crop(Image.open(WALLPAPER_PATH).convert("RGB"), (width, height))
    desktop = photo.convert("RGBA")
    _paint_menubar(desktop, photo)
    _paint_dock(desktop, photo)
    return _paint_shadow(desktop, window_w, window_h)


def build_desktop(content: Image.Image, title: str) -> Image.Image:
    """Inset preserves every capture pixel, including the rounded corners."""
    padded = Image.new("RGB", (content.width + 2 * CONTENT_INSET, content.height + 2 * CONTENT_INSET))
    padded.paste(content, (CONTENT_INSET, CONTENT_INSET))
    desktop = desktop_backdrop(content.size).copy()
    desktop.alpha_composite(build_window(padded, title), (DESKTOP_MARGIN, WINDOW_TOP))
    return desktop.convert("RGB")


def _lerp(a: tuple[int, int], b: tuple[int, int], t: float) -> tuple[int, int]:
    t = max(0.0, min(1.0, t))
    return round(a[0] + (b[0] - a[0]) * t), round(a[1] + (b[1] - a[1]) * t)


def pointer_anchors(
    content_size: tuple[int, int], desktop_size: tuple[int, int]
) -> dict[str, tuple[int, int]]:
    """Named hits for the README pointer: titlebar, composer, slash, tools."""
    cw, ch = content_size
    dw, _dh = desktop_size
    content_x = DESKTOP_MARGIN + CONTENT_INSET
    content_y = WINDOW_TOP + TITLEBAR_H + CONTENT_INSET
    window_w = cw + 2 * CONTENT_INSET
    return {
        "rest": (dw - 51, 118),
        "titlebar": (DESKTOP_MARGIN + window_w // 2, WINDOW_TOP + TITLEBAR_H // 2),
        "composer": (content_x + 88, content_y + max(48, int(ch * 0.12))),
        "palette": (content_x + 76, content_y + int(ch * 0.30)),
        "model": (content_x + 104, content_y + int(ch * 0.22)),
        "working": (content_x + 148, content_y + int(ch * 0.16)),
        "shell": (content_x + 128, content_y + int(ch * 0.24)),
    }


def _label_anchor(label: str) -> str:
    if label in {"typing", "prompt-ready", "composer"}:
        return "composer"
    if label in {"working", "palette", "model", "shell", "titlebar", "rest"}:
        return label
    if label == "splash":
        return "titlebar"
    return "rest"


def _ease_label_hold(
    label: str, index_in_label: int, label_count: int, anchors: dict[str, tuple[int, int]]
) -> tuple[int, int]:
    t = index_in_label / max(1, label_count - 1)
    if label == "splash":
        if t < 0.4:
            return _lerp(anchors["rest"], anchors["titlebar"], t / 0.4)
        return _lerp(anchors["titlebar"], anchors["composer"], (t - 0.4) / 0.6)
    if label == "composer":
        return _lerp(anchors["composer"], anchors["rest"], t)
    if label == "palette":
        return _lerp(anchors["composer"], anchors["palette"], min(1.0, t * 1.4))
    return anchors[_label_anchor(label)]


@lru_cache(maxsize=8)
def pointer_track(
    labels: tuple[str, ...],
    desktop_size: tuple[int, int],
    content_size: tuple[int, int],
) -> tuple[tuple[int, int, bool], ...]:
    """One (x, y, pressed) per playback frame; last pose matches the first."""
    anchors = pointer_anchors(content_size, desktop_size)
    n = len(labels)
    if n == 0:
        return ()
    first_of: dict[str, int] = {}
    counts: dict[str, int] = {}
    for i, label in enumerate(labels):
        first_of.setdefault(label, i)
        counts[label] = counts.get(label, 0) + 1
    seen: dict[str, int] = {}
    poses: list[tuple[int, int, bool]] = []
    for i, label in enumerate(labels):
        seen[label] = seen.get(label, 0)
        x, y = _ease_label_hold(label, seen[label], counts[label], anchors)
        seen[label] += 1
        pressed = label in CLICK_LABELS and first_of[label] <= i < first_of[label] + 2
        poses.append((x, y, pressed))

    wrap = min(8, max(3, n // 10))
    start = poses[0][:2]
    for k in range(wrap):
        t = (k + 1) / wrap
        i = n - wrap + k
        x, y = _lerp(poses[i][:2], start, t)
        poses[i] = (x, y, False)
    return tuple(poses)


def pointer_pose(
    frame: int,
    total: int,
    *,
    desktop_size: tuple[int, int],
    content_size: tuple[int, int] = (1232, 912),
    label: str = "",
    labels: tuple[str, ...] | None = None,
) -> tuple[int, int, bool]:
    """Looping pointer pose. Optional storyboard labels drive the proud path."""
    if labels:
        track = pointer_track(labels, desktop_size, content_size)
        x, y, pressed = track[frame % len(track)]
    else:
        anchors = pointer_anchors(content_size, desktop_size)
        tour = (
            anchors["rest"],
            anchors["titlebar"],
            anchors["composer"],
            anchors["palette"],
            anchors["working"],
            anchors["shell"],
            anchors["rest"],
        )
        phase = (frame % max(1, total)) / max(1, total)
        scaled = phase * (len(tour) - 1)
        i = min(len(tour) - 2, int(scaled))
        x, y = _lerp(tour[i], tour[i + 1], scaled - i)
        pressed = i in {2, 3} and (scaled - i) < 0.18
    wobble = 2 * math.pi * frame / max(1, total)
    x += round(2 * math.sin(wobble * 3))
    y += round(2 * math.cos(wobble * 2))
    _ = label
    return x, y, pressed


def _draw_pointer(draw: ImageDraw.ImageDraw, x: int, y: int, pressed: bool) -> None:
    if pressed:
        draw.ellipse((x - 10, y - 10, x + 18, y + 18), outline=(255, 255, 255, 180), width=2)
    points = [
        (x, y),
        (x, y + 23),
        (x + 6, y + 17),
        (x + 11, y + 27),
        (x + 15, y + 25),
        (x + 10, y + 15),
        (x + 19, y + 15),
    ]
    draw.polygon(points, fill=(18, 20, 22))
    draw.line(points + [points[0]], fill="white", width=2)


def animate_mouse(
    desktop: Image.Image,
    frame: int,
    total: int,
    label: str = "",
    labels: tuple[str, ...] | None = None,
    content_size: tuple[int, int] = (1232, 912),
) -> Image.Image:
    """Pointer tours titlebar → composer / slash → tools, then arcs back."""
    x, y, pressed = pointer_pose(
        frame,
        total,
        desktop_size=desktop.size,
        content_size=content_size,
        label=label,
        labels=labels,
    )
    image = desktop.copy()
    _draw_pointer(ImageDraw.Draw(image), x, y, pressed)
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
