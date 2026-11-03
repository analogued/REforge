#!/usr/bin/env python3
# Copyright (c) 2026 analogued.
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate controller coverage/classification masks from retained CC0 SVGs.

Requires Python 3, Pillow and rsvg-convert. No network or original game data.
"""
from pathlib import Path
import argparse
import io
import subprocess
import xml.etree.ElementTree as ET
from PIL import Image

SVG = 'http://www.w3.org/2000/svg'
ET.register_namespace('', SVG)
MODELS = [
    ('xbox-series', {'stick': [7, 9, 10], 'dpad': [26], 'north': [6, 8], 'west': [11, 16], 'east': [12, 15], 'south': [22, 25]}),
    ('dualsense', {'stick': [34, 40, 42, 44, 46], 'dpad': [15, 19, 20, 25], 'north': [13, 14], 'east': [17, 21], 'west': [18, 22], 'south': [26, 27]}),
    ('switch-pro', {'stick': [7, 10, 11], 'dpad': [22], 'north': [5, 8], 'west': [12, 17], 'east': [13, 16], 'south': [20, 21]}),
]
CLASSES = ['neutral', 'stick', 'dpad', 'south', 'east', 'west', 'north']
GLYPHS = [
    ('xbox_button_', ['a', 'b', 'x', 'y']),
    ('playstation_button_', ['cross', 'circle', 'square', 'triangle']),
    ('switch_button_', ['b', 'a', 'y', 'x']),
]


def rasterize(root, width, renderer):
    raw = subprocess.run([renderer, '--width', str(width)], input=ET.tostring(root), stdout=subprocess.PIPE, check=True).stdout
    return Image.open(io.BytesIO(raw)).convert('RGBA')


def main():
    here = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=here / 'source')
    parser.add_argument('--output', type=Path, default=here / 'generated')
    parser.add_argument('--rsvg-convert', default='rsvg-convert')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for name, groups in MODELS:
        root = ET.parse(args.source / 'gidedin' / f'{name}.svg').getroot()
        paths = list(root.iter(f'{{{SVG}}}path'))
        labels = {index: CLASSES.index(control) for control, indices in groups.items() for index in indices}
        for index, path in enumerate(paths):
            # Widely spaced grey values preserve classification through rasterizer
            # premultiplication rounding. Coverage remains the exact rendered alpha.
            grey = 32 * (labels.get(index, 0) + 1)
            path.set('fill', f'rgb({grey},{grey},{grey})')
        image = rasterize(root, 2048, args.rsvg_convert)
        red, _, _, alpha = image.split()
        classes = red.point(lambda value: max(0, min(6, (value + 16) // 32 - 1)))
        Image.merge('LA', (classes, alpha)).save(args.output / f'{name}.png', optimize=True)
    atlas = Image.new('L', (512, 384))
    for row, (prefix, symbols) in enumerate(GLYPHS):
        for column, symbol in enumerate(symbols):
            root = ET.parse(args.source / 'kenney' / f'{prefix}{symbol}_outline.svg').getroot()
            root.set('viewBox', '0 0 64 64')
            root.attrib.pop('width', None)
            root.attrib.pop('height', None)
            atlas.paste(rasterize(root, 128, args.rsvg_convert).getchannel('A'), (column * 128, row * 128))
    atlas.save(args.output / 'glyphs.png', optimize=True)


if __name__ == '__main__':
    main()
