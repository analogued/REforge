# Shared bitmap graphics

`reforge_graphics` provides the common 8×8 printable ASCII font used by Alley Cat
and BTZ. It has no dependencies, uses `no_std` and is independent of SDL.

[`font8x8::glyph`](src/font8x8.rs) returns eight bitmap rows for character codes
32–126, or `None` for other bytes. Bit zero is the leftmost pixel in each row.
Each engine supplies its pixel packing, colours and screen composition. BTZ's
border, block and shade glyphs remain in its display module.

The font's upstream notices are retained in the source; provenance and license
details are in [THIRD_PARTY.md](../../THIRD_PARTY.md#dos-bitmap-text-fonts).
Authored code and documentation use [GPL-3.0-or-later](../../LICENSE).

Run the workspace validation in [CONTRIBUTING.md](../../CONTRIBUTING.md).
