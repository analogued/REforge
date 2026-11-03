# Controller diagrams and button prompts

The Settings Controls page combines Gidedin's **Gamepad Line Art Pack** controller
front views with Kenney's **Input Prompts 1.5A** button glyphs. These are independent
UI assets, unrelated to original game data. Both packs dedicate their drawings to
CC0 1.0 Universal. Original notices are retained alongside the unmodified SVGs;
see the root [third-party inventory](../../../../THIRD_PARTY.md) and
[CC0 text](../../../../LICENSES/CC0-1.0.txt).

- Gidedin: <https://gidedin.itch.io/gamepad-line-art-pack>
- Kenney: <https://kenney.nl/assets/input-prompts>

`sources.json` identifies the exact imported files, their upstream archive hashes
and each source file's SHA-256. The regular Xbox Series, DualSense and Switch Pro
views are retained. Controller trademarks remain their respective owners' and do
not imply affiliation or endorsement.

## Regeneration

Normal Cargo builds use the checked-in masks and need no SVG tools. To regenerate
from this directory's retained masters, install Python 3, Pillow and librsvg's
`rsvg-convert`, then run from the repository root:

```sh
python3 crates/reforge_sdl/assets/controller/generate.py
```

The generator also accepts `--source`, `--output` and `--rsvg-convert` paths.
Its temporary SVG modifications label existing paths by physical control and
normalize glyph viewports; it never changes the retained masters. The controller
masks are 2048 pixels wide, with one grayscale channel identifying the original
path's control group and the alpha channel retaining its antialiased coverage.
The 512×384 grayscale glyph atlas has one 128-pixel cell per prompt. Both use
lossless PNG. These raster masks are generated from vector masters, rather than
being a general-purpose runtime SVG implementation.

The UI tints one complete controller texture with the current theme and
highlights. It averages the premultiplied coverage into the required physical
pixel size, retaining thin lines when reducing the artwork. The glyph atlas is
reduced the same way and tinted at paint time. The two texture handles are reused
and updated only when size, theme or highlighted controls change; there is no
per-pixel geometry or per-frame SVG/PNG decoding. The controller drawings are
capped at 700 logical pixels wide and their textures at 2048 physical pixels.
The retained masks cover ordinary UI densities up to approximately three physical
pixels per point; higher densities upscale the cached 2048-pixel representation.
