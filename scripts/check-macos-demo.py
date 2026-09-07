#!/usr/bin/env python3
"""Run with the renderer's Pillow environment: python3 scripts/check-macos-demo.py."""
import hashlib
import importlib.util
from pathlib import Path

from PIL import Image, ImageChops

spec = importlib.util.spec_from_file_location(
    "macos", Path(__file__).with_name("compose-macos-terminal.py")
)
macos = importlib.util.module_from_spec(spec)
spec.loader.exec_module(macos)

# Deliberately non-black edges catch clipping, shadows and resampling.
raw = Image.new("RGB", (1232, 912), (74, 222, 128))
raw.putpixel((0, 0), (255, 255, 255))
desktop = macos.build_desktop(raw, "Cortex CLI — cortex — 120×40")
assert desktop.size == (1416, 1140), desktop.size
assert macos.WALLPAPER_PATH.is_file(), macos.WALLPAPER_PATH

x = macos.DESKTOP_MARGIN + macos.CONTENT_INSET
y = macos.WINDOW_TOP + macos.TITLEBAR_H + macos.CONTENT_INSET
box = (x, y, x + raw.width, y + raw.height)
assert desktop.crop(box).tobytes() == raw.tobytes(), "CLI pixels must stay 1:1 before the pointer"

# A photograph has far more unique colours than the retired teal ellipses.
forest = desktop.crop((8, macos.MENUBAR_H + 8, 72, 220))
assert len(set(forest.getdata())) > 80, "desktop backdrop is not a forest photograph"

labels = (
    ("splash",) * 12
    + ("typing",) * 36
    + ("prompt-ready",) * 6
    + ("working",) * 12
    + ("palette",) * 12
    + ("model",) * 10
    + ("shell",) * 12
    + ("composer",) * 10
)
total = len(labels)
titlebar_hits = 0
terminal_hits = 0
previous = None
for frame in range(total):
    image = macos.animate_mouse(
        desktop, frame, total, label=labels[frame], labels=labels, content_size=raw.size
    )
    px, py, _pressed = macos.pointer_pose(
        frame,
        total,
        desktop_size=desktop.size,
        content_size=raw.size,
        label=labels[frame],
        labels=labels,
    )
    if macos.WINDOW_TOP <= py <= macos.WINDOW_TOP + macos.TITLEBAR_H:
        titlebar_hits += 1
    if box[0] <= px <= box[2] and box[1] <= py <= box[3]:
        terminal_hits += 1
    if previous is not None:
        assert ImageChops.difference(previous, image).getbbox(), f"Pointer stalled at {frame}"
    previous = image

assert titlebar_hits >= 2, f"pointer never visited the titlebar ({titlebar_hits})"
assert terminal_hits >= 8, f"pointer never entered the terminal ({terminal_hits})"
assert macos.animate_mouse(desktop, 0, total, labels=labels).tobytes() == macos.animate_mouse(
    desktop, total, total, labels=labels
).tobytes()

gif = Path(__file__).resolve().parents[1] / "docs/media/intro.gif"
assert gif.stat().st_size < 5 * 1024 * 1024, gif.stat().st_size
with Image.open(gif) as image:
    assert image.size == desktop.size, image.size
    n_frames = image.n_frames
    assert 100 <= n_frames <= 160, n_frames
    assert image.info["loop"] == 0
    pixels = set()
    duration = 0
    for frame in range(n_frames):
        image.seek(frame)
        pixels.add(hashlib.sha256(image.convert("RGB").tobytes()).digest())
        duration += image.info["duration"]
    assert len(pixels) >= int(n_frames * 0.85), len(pixels)
    assert 8000 <= duration <= 15000, duration
print(
    "PASS: photo wallpaper, exact CLI pixels, titlebar+terminal pointer, "
    f"{n_frames} GIF frames, duration and <5 MiB"
)
