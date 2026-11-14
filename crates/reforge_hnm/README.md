<!-- Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later -->

# REforge HNM reader

`reforge_hnm` decodes the observed early HNM1 movie profile into indexed bitmap
updates, palette state and ordered typed chunks. It has no runtime dependencies,
contains no original game data and does not mount images or install games.
Authored code/documentation use GPL-3.0-or-later; see [LICENSE](../../LICENSE).

## Input and output

Supply an isolated movie stream implementing `Read + Seek`. For CD resources,
`reforge_disc::Region::open()` provides such a bounded stream without extraction.
The application selects/verifies physical originals; this crate only validates
the selected format and never identifies a game or searches for substitutes.

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

## Supported profile and boundaries

Use `Hnm1` explicitly. It is not an automatic HNM revision detector. HNM0, HNM4,
HNM4 variants and HNM6 are different formats and are not implemented. A filename
ending in `.HNM` does not establish compatibility. Future revision decoders can
coexist without changing HNM1 parsing rules.

The implemented profile has:

- A little-endian 16-bit header size, palette ranges, FF padding and a required
  32-bit relative superchunk-offset table ending at the movie's logical EOF.
- Bounded 16-bit superchunks and typed chunks, with an optional final bitmap.
- Raw bitmap data or checksum-171 HSQ expansion. Outer width flags independently
  select compression (0200), direct framebuffer output (0400), and signed
  row-local raster RLE (8000) for ordinary updates. Direct output bypasses RLE
  composition. RLE follows optional HSQ expansion; it is not the
  separately described checksum-173 overlay codec. Rows have no alignment pad.
- Palette range updates, including count zero meaning 256 entries, and unchanged
  frames with a zero bitmap dimension.

Checksums 172/173, other bitmap flags/modes and geometry above 320×200 are
explicitly unsupported. Shared format support does not imply all games described
as using HNM1 have been tested. Validation currently covers the selected Dune CD
executable's profile and its HNM/LOP resources; looping LOP playback is caller
policy. Some Dune paths use other destination strides/scaling or reserve palette
entry zero. Those engine policies are deliberately separate from the codec.

Indexing reads only the at-most-65,535-byte header. `next_frame()` reads one
bounded superchunk and validates output sizes/backreferences/RLE row boundaries.
Each decompressed intermediate fits the format's 16-bit size field; a canvas
holds 64,000 indices. Successful frames commit palette/index state atomically.
After an error, decoding stays stopped until `rewind()`; reset the separate
canvas/background too when replaying dependent updates.

## Format references and validation

The implementation is authored from format descriptions and original-program
observations; no third-party decoder implementation or game data is included.
References: [HNM family overview](https://wiki.multimedia.cx/index.php/HNM) and
[HNM1 description](https://wiki.multimedia.cx/index.php/HNM_(1)). The latter's
codec-173 description is outside the implemented profile.

Public tests use authored synthetic streams, covering palette inheritance,
ordered side chunks, raw/HSQ/RLE bitmap paths, composition, rewind and malformed
inputs. Private original-data comparisons remain outside the repository.

From the workspace, run:

```sh
cargo test --locked -p reforge_hnm
cargo clippy --locked -p reforge_hnm --all-targets -- -D warnings
```
