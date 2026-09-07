#!/usr/bin/env python3
"""Run with the renderer's Pillow environment: python3 scripts/check-macos-demo.py."""
import importlib.util
import hashlib
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
x = macos.DESKTOP_MARGIN + macos.CONTENT_INSET
y = macos.WINDOW_TOP + macos.TITLEBAR_H + macos.CONTENT_INSET
box = (x, y, x+raw.width, y+raw.height)
previous = None
for frame in range(98):
    image = macos.animate_mouse(desktop, frame, 98)
    assert image.crop(box).tobytes() == raw.tobytes(), f"CLI pixels changed at {frame}"
    if previous is not None:
        assert ImageChops.difference(previous, image).getbbox(), f"Pointer stalled at {frame}"
    previous = image
assert macos.animate_mouse(desktop, 0, 98).tobytes() == macos.animate_mouse(desktop, 98, 98).tobytes()

gif = Path(__file__).resolve().parents[1] / "docs/media/intro.gif"
assert gif.stat().st_size < 5 * 1024 * 1024
with Image.open(gif) as image:
    assert image.size == desktop.size
    assert image.n_frames == 98, image.n_frames
    assert image.info["loop"] == 0
    pixels = set()
    duration = 0
    for frame in range(image.n_frames):
        image.seek(frame)
        pixels.add(hashlib.sha256(image.convert("RGB").tobytes()).digest())
        duration += image.info["duration"]
    assert len(pixels) == 98, len(pixels)
    assert 8000 <= duration <= 15000, duration
print("PASS: desktop dimensions, exact CLI pixels, moving looping pointer, 98 unique GIF frames, duration and <5 MiB")
