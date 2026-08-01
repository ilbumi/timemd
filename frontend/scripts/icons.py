"""Regenerates the PWA icons: the running timer's dial, drawn straight to PNG.

Run with `python3 frontend/scripts/icons.py`. Only needed if the palette or the
dial changes; the PNGs it writes are committed.

No image library on purpose — the repo should not gain a build-time dependency
for five small files, and a hand-rolled PNG writer is about thirty lines.
"""

import math
import struct
import zlib
from pathlib import Path

PAPER = (0xF2, 0xEF, 0xE6)
INK = (0x11, 0x11, 0x11)
RED = (0xD1, 0x33, 0x2E)

SUPERSAMPLE = 4
# Fractions of the drawn circle's radius.
OUTER = 0.44
INNER = 0.26
STROKE = 0.020
# How far the red sweep runs, clockwise from twelve o'clock.
SWEEP_DEGREES = 100.0


def sample(x: float, y: float, scale: float) -> tuple[int, int, int]:
    """Colour at a point in a -0.5..0.5 square, `scale` shrinking the drawing."""
    x /= scale
    y /= scale
    radius = math.hypot(x, y)

    if radius > OUTER + STROKE:
        return PAPER
    if abs(radius - OUTER) <= STROKE or abs(radius - INNER) <= STROKE:
        return INK
    if radius < INNER:
        return PAPER

    # Clockwise from twelve o'clock, which is where the dial's tick sits.
    angle = math.degrees(math.atan2(x, -y)) % 360.0
    return RED if angle < SWEEP_DEGREES else PAPER


def render(size: int, scale: float) -> bytes:
    rows = bytearray()
    step = 1.0 / (size * SUPERSAMPLE)
    for row in range(size):
        rows.append(0)  # PNG filter type: none
        for column in range(size):
            totals = [0, 0, 0]
            for sub_y in range(SUPERSAMPLE):
                for sub_x in range(SUPERSAMPLE):
                    x = (column * SUPERSAMPLE + sub_x + 0.5) * step - 0.5
                    y = (row * SUPERSAMPLE + sub_y + 0.5) * step - 0.5
                    pixel = sample(x, y, scale)
                    for channel in range(3):
                        totals[channel] += pixel[channel]
            rows.extend(total // (SUPERSAMPLE * SUPERSAMPLE) for total in totals)
    return bytes(rows)


def chunk(kind: bytes, payload: bytes) -> bytes:
    return (
        struct.pack(">I", len(payload))
        + kind
        + payload
        + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
    )


def write_png(path: Path, size: int, scale: float) -> None:
    header = struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(render(size, scale), 9))
        + chunk(b"IEND", b"")
    )
    print(f"{path.name}: {path.stat().st_size} bytes")


static = Path(__file__).resolve().parent.parent / "static"
write_png(static / "icon-192.png", 192, 0.92)
write_png(static / "icon-512.png", 512, 0.92)
# Maskable icons get cropped to a circle inscribed in the middle 80%, so the
# drawing shrinks to survive it.
write_png(static / "icon-maskable.png", 512, 0.62)
write_png(static / "apple-touch-icon.png", 180, 0.92)
