#!/usr/bin/env python3
"""Draws the PLACEHOLDER AfroBIM icon and writes afrobim.ico, afrobim.icns and afrobim-512.png.

The repository has no icon asset, so this draws a neutral shape (a dark rounded square with a
light triangular outline, an abstract warning-sign shape). It is NOT a brand mark: replace the
generated files, or this script, once the owner has a logo. Python standard library only.

    python3 make_icons.py [output-directory]      (default: the folder holding this script)
"""
import os
import struct
import sys
import zlib

BG = (31, 58, 95)        # dark blue
FG = (240, 242, 245)     # near white


def clamp01(v):
    return 0.0 if v < 0 else 1.0 if v > 1 else v


def rounded_box_sdf(px, py, half, radius):
    qx = abs(px) - (half - radius)
    qy = abs(py) - (half - radius)
    ox, oy = max(qx, 0.0), max(qy, 0.0)
    return (ox * ox + oy * oy) ** 0.5 + min(max(qx, qy), 0.0) - radius


def tri_sdf(px, py, verts):
    """Signed distance (negative inside) to a convex counter-clockwise polygon (y up)."""
    best = -1e9
    n = len(verts)
    for i in range(n):
        ax, ay = verts[i]
        bx, by = verts[(i + 1) % n]
        ex, ey = bx - ax, by - ay
        length = (ex * ex + ey * ey) ** 0.5
        d = ((px - ax) * ey - (py - ay) * ex) / length   # outward normal of a CCW edge
        best = max(best, d)
    return best


# Counter-clockwise in a y-up space: apex, bottom left, bottom right.
OUTER = [(0.0, 0.62), (-0.60, -0.46), (0.60, -0.46)]
INNER = [(0.0, 0.34), (-0.34, -0.28), (0.34, -0.28)]


def render(size):
    """Rows of RGBA bytes for one square image, anti-aliased by signed distance."""
    px_size = 2.0 / size            # one pixel in the -1..1 coordinate space
    rows = []
    for j in range(size):
        y = 1.0 - (j + 0.5) / size * 2.0          # image rows grow downwards, the shape is y up
        row = bytearray()
        for i in range(size):
            x = (i + 0.5) / size * 2.0 - 1.0
            a_bg = clamp01(0.5 - rounded_box_sdf(x, y, 0.94, 0.30) / px_size)
            d_out = tri_sdf(x, y, OUTER)
            d_in = tri_sdf(x, y, INNER)
            cover = clamp01(0.5 - d_out / px_size) * (1.0 - clamp01(0.5 - d_in / px_size))
            row += bytes(int(BG[k] * (1 - cover) + FG[k] * cover + 0.5) for k in range(3))
            row.append(int(a_bg * 255 + 0.5))
        rows.append(bytes(row))
    return rows


def png(size, rows):
    raw = b"".join(b"\x00" + r for r in rows)

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def ico(images):
    """images: list of (size, png bytes). PNG payloads in .ico are supported from Windows Vista on."""
    head = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, blobs = b"", b""
    for size, data in images:
        w = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    return head + entries + blobs


def icns(images):
    """images: dict of OSType -> png bytes. Modern .icns files accept PNG payloads for these types."""
    body = b"".join(t + struct.pack(">I", 8 + len(d)) + d for t, d in images.items())
    return b"icns" + struct.pack(">I", 8 + len(body)) + body


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.abspath(__file__))
    os.makedirs(out, exist_ok=True)
    cache = {}

    def get(size):
        if size not in cache:
            cache[size] = png(size, render(size))
        return cache[size]

    with open(os.path.join(out, "afrobim.ico"), "wb") as f:
        f.write(ico([(s, get(s)) for s in (16, 32, 48, 64, 128, 256)]))
    with open(os.path.join(out, "afrobim.icns"), "wb") as f:
        # ic07 128 px, ic08 256 px, ic09 512 px, ic10 1024 px (the 512 px @2x image)
        f.write(icns({b"ic07": get(128), b"ic08": get(256), b"ic09": get(512), b"ic10": get(1024)}))
    with open(os.path.join(out, "afrobim-512.png"), "wb") as f:
        f.write(get(512))
    print("wrote afrobim.ico, afrobim.icns, afrobim-512.png in", out)


if __name__ == "__main__":
    main()
