#!/usr/bin/env python3
"""Measure the page frame of window screenshots (from dev/ui-shot.py): the empty space around and between blocks.

Usage: dev/ui-spacing.py DIR [--sidebar 176] [--footer 46]
For every PNG in DIR it prints the content area's top offset, left and right gutters, and the vertical gaps
between blocks of content (a run of rows with something drawn in it), so pages can be compared with the
page pattern in docs/UI.md ("Page frame"). A screenshot is the app's own window, so the sidebar is the
first `--sidebar` pixels and the footer bar the last `--footer`.
"""
import argparse
from pathlib import Path

from PIL import Image


def background(img, x, y):
    return img.getpixel((x, y))[:3]


def measure(path, sidebar, footer):
    img = Image.open(path).convert("RGB")
    width, height = img.size
    left_edge, right_edge, bottom = sidebar + 1, width, height - footer
    bg = background(img, left_edge + 3, bottom - 3) if bottom > 3 else (0, 0, 0)
    # The page background is what the top-left corner of the content area shows.
    bg = background(img, left_edge + 2, 2)

    def differs(pixel):
        return sum(abs(a - b) for a, b in zip(pixel, bg)) > 24

    rows = []
    for y in range(0, bottom):
        xs = [x for x in range(left_edge, right_edge, 1) if differs(img.getpixel((x, y)))]
        rows.append((xs[0], xs[-1]) if xs else None)
    blocks, start = [], None
    for y, row in enumerate(rows):
        if row and start is None:
            start = y
        if not row and start is not None:
            blocks.append((start, y - 1))
            start = None
    if start is not None:
        blocks.append((start, bottom - 1))
    # Merge runs split by one blank row (anti-aliasing inside text lines).
    merged = []
    for block in blocks:
        if merged and block[0] - merged[-1][1] <= 3:
            merged[-1] = (merged[-1][0], block[1])
        else:
            merged.append(block)
    lefts = [row[0] for row in rows if row]
    rights = [row[1] for row in rows if row]
    return {
        "size": f"{width}x{height}",
        "top": merged[0][0] if merged else None,
        "left": (min(lefts) - left_edge) if lefts else None,
        "right": (right_edge - 1 - max(rights)) if rights else None,
        "gaps": [merged[i + 1][0] - merged[i][1] - 1 for i in range(len(merged) - 1)],
        "heights": [b[1] - b[0] + 1 for b in merged],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("dir", type=Path)
    parser.add_argument("--sidebar", type=int, default=176)
    parser.add_argument("--footer", type=int, default=46)
    args = parser.parse_args()
    print(f"{'page':<28}{'top':>5}{'left':>6}{'right':>7}  gaps between blocks")
    for path in sorted(args.dir.glob("*.png")):
        m = measure(path, args.sidebar, args.footer)
        gaps = ",".join(str(g) for g in m["gaps"][:14])
        print(f"{path.stem:<28}{m['top']!s:>5}{m['left']!s:>6}{m['right']!s:>7}  {gaps}")


if __name__ == "__main__":
    main()
