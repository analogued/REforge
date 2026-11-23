<!-- Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later -->

# REforge HNM reader

`reforge_hnm` decodes the observed early HNM1 movie profile into indexed bitmap
updates, palette state and ordered typed chunks. It also decodes standard HNM4
video into indexed display rasters and palette state. The crate has no runtime
dependencies and contains no original game data.
Authored code/documentation use GPL-3.0-or-later; see [LICENSE](../../LICENSE).

## Inputs

Supply an isolated movie stream implementing `Read + Seek`. For CD resources,
`reforge_disc::Region::open()` provides such a bounded stream without extraction.
The application selects/verifies physical originals; this crate only validates
the selected format. Choose `Hnm1` or `Hnm4` explicitly.

## HNM1 output

```rust,no_run
use std::{fs::File, io};
use reforge_hnm::{Canvas, Hnm1, Palette};

fn decode(selected_movie: &std::path::Path) -> io::Result<()> {
    // Real callers supply the current game palette/background when inherited.
    let mut movie = Hnm1::new(File::open(selected_movie)?, Palette::default())?;
    let mut canvas = Canvas::default();
    while let Some(frame) = movie.next_frame()? {
        if let Some(update) = &frame.video {
            canvas.apply(update)?;
        }
        // Present canvas.pixels() with frame.palette at the caller's next tick.
        // Consume frame.chunks in order; the engine interprets raw sd sound payloads.
    }
    Ok(())
}
```

The decoder carries palette state across superchunks. Its `Palette` retains raw
encoded RGB values plus a per-entry `defined` mask: missing entries inherit
caller state, rather than being silently assigned a movie palette. In the
examined DOS profile, each channel is a six-bit VGA DAC value. Scaling those
values to a host display and applying game fades remain presentation decisions.

`Video` contains tightly packed eight-bit palette indices, dimensions, position,
drawing mode and original outer flags. `Canvas` implements ordinary 320×200
composition: FE copies every pixel, FF treats zero as transparent, and direct
updates write from framebuffer offset zero without clearing the untouched tail.
Out-of-viewport ordinary updates are clipped. Hardware out-of-bounds memory
side effects and game-specific scaling/destination transforms are not emulated.
An engine may consume bitmap updates without using this compositor.

`pl` updates are applied to the palette and retained among the typed chunks.
`sd` retains raw sound payloads. A VOC header does not establish that subsequent
payloads form a valid standalone VOC file: an original player can use actual
packet lengths instead of the header's declared sound-block length. Transport
interpretation, sample decoding and clocks belong above the movie container.
Other bounded typed chunks, including `pt` and `kl`, are preserved without
inventing their game meaning. Nothing here selects tracks, drives lip movement,
chooses movie order, loops a clip or supplies an assumed frame rate.

`expand_hsq()` also exposes the same bounded checksum-171 wrapper expansion
for standalone resources. Its result is bytes, without assuming a bitmap.
`expand_rle8()` exposes the row-bounded eight-bit RLE stage for sprite banks
using that same encoding; geometry is supplied explicitly.

### HNM1 profile

The implemented profile has:

- A little-endian 16-bit header size, palette ranges, FF padding and a required
  32-bit relative superchunk-offset table ending at the movie's logical EOF.
- Bounded 16-bit superchunks and typed chunks, with an optional final bitmap.
  An exact six-byte `RM` placeholder is exposed as a chunk with no bitmap.
- Raw bitmap data, checksum-171 HSQ expansion, and checksum-173 split-stream
  raster expansion. The split format keeps coordinates outside the compressed
  stream and expands samples before consuming the run controls.
- Outer width flags select compression (0200), direct framebuffer output
  (0400), and signed row-local raster RLE (8000) for ordinary updates. Direct
  output bypasses the row-local RLE stage. Rows have no alignment padding.
- Palette range updates, including count zero meaning 256 entries, and unchanged
  frames with a zero bitmap dimension.

Checksum 172, other bitmap flags and modes, and geometry above 320×200 remain
unsupported. HNM1 validation covers the selected Dune CD HNM/LOP resources and
all 111 actor and portrait archive entries across both discs of the English
Dragon Lore Gold 2.0 GOG DOS release. Dune-specific strides, scaling, palette
entry zero and LOP looping belong to the caller.

Indexing reads only the at-most-65,535-byte header. `next_frame()` reads one
bounded superchunk and validates output sizes/backreferences/RLE row boundaries.
Each decompressed intermediate fits the format's 16-bit size field; a canvas
holds 64,000 indices. Successful frames commit palette/index state atomically.
After an error, decoding stays stopped until `rewind()`; reset the separate
canvas/background too when replaying dependent updates.

`indexed_frame(index)` reads a selected superchunk using the header palette
and preserves sequential state. Use it for resources which select packets
independently. The caller still owns any inherited raster or other game state.

## HNM4 output

`Hnm4::new(reader, inherited_palette)` accepts the standard double-buffered
HNM4 profile. `next_frame()` borrows a complete
indexed raster in display row order and the current palette. Copy the bytes
before advancing if they must outlive that call. `rewind()` resets both raster
buffers and restores the inherited palette.

The reader uses the header's frame count and frame-area boundary, and checks
each 24-bit superchunk and chunk length against that boundary. Frame data ends
before the TAB trailer. A stream's nominal file length may include the trailer
or end at its offset. The frame allocation field can exceed the visible extent
and the decoded buffers are sized to the visible geometry.

`PL` updates the VGA palette. An `IZ` keyframe expands an HSQ-style raster using
a 32-bit high-first bit queue and refreshes both buffers. A keyframe may
describe a shorter, full-width raster covering complete row pairs. The
target buffer retains the rows below it before being copied to the other
buffer. An `IU` interframe writes into the older buffer. Skips preserve its
pixels, and the previous-frame
flag selects the latest buffer as the source. Pixel pair references and swaps
operate on the packed raster before deinterleaving its rows for display.

A byte skip or fill just before the interframe terminator can end one pixel
pair past the raster. Only the visible prefix contributes to the result. All
other commands and reference sources are bounded by the visible extent.

The decoder skips bounded `SD` chunks. It supports raster dimensions up to
320×200 with an even height and an initial keyframe. HNM4A, `IB` bitmap chunks,
eight-bit palettes, HNM0 and HNM6 are outside the supported profiles.

## Format references and validation

The implementation is authored from format descriptions and original-program
observations. The [HNM1 description](https://wiki.multimedia.cx/index.php/HNM_(1))
and [HNM4 description](https://wiki.multimedia.cx/index.php/HNM4) document
the container and codecs. The Dragon Lore Gold 2.0 DOS decoder establishes
the terminal IU fill and skip behavior, the buffer rotation and the order of
backline pair writes. No third-party decoder source is used.

Public tests use authored synthetic streams covering decoding, composition,
palette inheritance, rewind and malformed inputs. Local validation decoded all
223 HNM4 archive entries in the English GOG DOS payload. All 5,144 frames of
the Cryo and Mindscape logos, English title, castle opening and greeting
matched indexed pixels and raw palette bytes from the original decoder.
The Lost Eden DOS corpus adds 152 streams and 28,395 decoded pictures. It
exercises padded allocations, shorter keyframes and terminal skips on
320×160 rasters. Original-data comparisons are kept outside the repository.

From the workspace, run:

```sh
cargo test --locked -p reforge_hnm
cargo clippy --locked -p reforge_hnm --all-targets -- -D warnings
```
