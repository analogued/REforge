#!/usr/bin/env python3
# Copyright (c) 2026 analogued.
# Project license not yet selected; see the repository README.
"""Decode a Mach 3 mode-4 framebuffer into a binary PPM image."""

import argparse
from pathlib import Path

WIDTH = 320
HEIGHT = 200
ROW_BYTES = 80
BANK_OFFSET = 0x2000
MINIMUM_SIZE = 0x3F40
DEFAULT_COLORS = ("000000", "55ffff", "ff55ff", "ffffff")


def parse_colors(value):
    """Parse four comma-separated RGB hexadecimal colors in pixel-index order."""
    fields = value.split(",")
    if len(fields) != 4 or any(len(field) != 6 for field in fields):
        raise argparse.ArgumentTypeError("provide four six-digit RGB hex colors")
    try:
        return tuple(bytes.fromhex(field) for field in fields)
    except ValueError as error:
        raise argparse.ArgumentTypeError("colors must be hexadecimal RGB") from error


def decode_mode4(data, colors):
    """Decode the visible 320-by-200 pixels, skipping the gaps between banks."""
    if len(data) < MINIMUM_SIZE:
        raise ValueError(f"framebuffer needs at least {MINIMUM_SIZE} bytes")
    if len(colors) != 4 or any(len(color) != 3 for color in colors):
        raise ValueError("palette needs four RGB triples")
    pixels = bytearray()
    for row in range(HEIGHT):
        start = (row & 1) * BANK_OFFSET + (row // 2) * ROW_BYTES
        for value in data[start : start + ROW_BYTES]:
            for shift in (6, 4, 2, 0):
                pixels.extend(colors[(value >> shift) & 3])
    return bytes(pixels)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="external framebuffer/data file")
    parser.add_argument("output", type=Path, help="new PPM file outside source inputs")
    parser.add_argument(
        "--colors",
        type=parse_colors,
        default=parse_colors(",".join(DEFAULT_COLORS)),
        help="four RGB hex colors; default is the verified bright palette",
    )
    args = parser.parse_args()
    if args.input.resolve() == args.output.resolve():
        parser.error("input and output must be different files")
    try:
        pixels = decode_mode4(args.input.read_bytes(), args.colors)
        with args.output.open("xb") as output:
            output.write(f"P6\n{WIDTH} {HEIGHT}\n255\n".encode("ascii"))
            output.write(pixels)
    except (OSError, ValueError) as error:
        parser.exit(1, f"{parser.prog}: {error}\n")


if __name__ == "__main__":
    main()
