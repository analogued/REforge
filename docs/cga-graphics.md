# CGA graphics

The initial screen is a direct copy of `data1` into CGA video memory at
`b800:0000`. The copy routine is `6004`, called at `0808` and returning at
`080b`; offsets are relative to the expanded program's code segment as described
in [DOS startup](dos-startup.md).

The copy length is `0x3f40` bytes. A visible mode-4 frame is 320 by 200 pixels,
with four two-bit pixels per byte and 80 bytes per scanline. Even scanlines
occupy the bank at offset zero; odd scanlines occupy the bank at `0x2000`.
These framebuffer coordinates fill a 4:3 display, with a pixel width-to-height
ratio of 5:6. The viewer applies that correction when presenting frames;
framebuffer exports retain the original 320-by-200 grid.
For pixel `(x, y)`:

```text
byte offset = (y % 2) * 0x2000 + (y / 2) * 80 + x / 4
pixel index = (byte >> (6 - 2 * (x % 4))) & 3
```

Divisions are integer divisions. Each bank contains 100 visible rows, totaling
8,000 bytes. The 192-byte gap before the second bank is not displayed. The last
visible byte ends at `0x3f40`; bytes beyond it are not needed to decode the frame.

The verified initial color-select value is `0x30`, giving these RGB values:

| Pixel index | RGB hexadecimal |
| --- | --- |
| 0 | `000000` |
| 1 | `55ffff` |
| 2 | `ff55ff` |
| 3 | `ffffff` |

The asset bytes, copied framebuffer, and decoder output have been compared with
the emulator's rendered surface. All 64,000 RGB pixels match. Captures and
original-derived images remain outside the source repository.

## Intro and gameplay transition

The first splash is displayed while the remaining assets load. Startup then
performs a DOS-clock wait, selects palette family 1, and calls the intro routine
at `076a`. That routine chooses another screen from `data6` or `data1`; the
verified initial path selects `data6`. Both use the same direct CGA layout.

The intro sequence at `537a` repeats while its input result is zero. Its playback
calls are synchronous; completion alone does not provide a nonzero return.
Space advanced the observed intro into gameplay. The return boundary is `03d1`.
Initial scene rendering completes at `044a`, before another playback call and
entry into the gameplay loop. No intro duration is asserted here: timer changes
and debugger pauses make wall-clock measurements unsuitable for that purpose.
Audio decoding and synthesis remain deferred.

## Gameplay graphics resources

Gameplay backgrounds use row-major packed pixels rather than CGA's alternating
scanline banks. Their stored bytes interleave two four-bit planes during loading:
output bits 7 through 0 take input bits `3,7,2,6,1,5,0,4`, respectively. The
converted byte contains four two-bit pixels in the same left-to-right order as
video memory.

| Resource | Converted contents |
| --- | --- |
| `data2` | 320-by-200 cockpit/background, followed by twelve 288-by-15 terrain strips |
| `data3` | Alternate 320-by-200 cockpit/background with a starfield |
| `data4` | Color pixels and descriptors for 190 sprites |
| `data5` | Paired sprite masks with matching dimensions |

The first background occupies 16,000 bytes. Each terrain strip occupies 1,080
bytes, with 72 bytes per row. The first strip starts at `0x3e80`; successive
strips are `0x438` bytes apart. The presence of the alternate background does not
establish which gameplay transition selects it.

Sprite rows are padded to an even byte width in memory. The loader supplies
zero color padding and `0xff` mask padding. A far-pointer table indexes the
color pixels; each pixel pointer has a descriptor before it containing dimensions
and positioning fields. The corresponding mask begins exactly `0x7d00` bytes
after the color source. The ordinary blitter at `6ffd` applies:

```text
destination = (destination & mask) | color
```

## Initial scene composition

The initial gameplay frame is composed in scratch memory, then copied into
the CGA framebuffer. Routine `6147` is called at `0429` for rows 0 through 99
and at `0447` for rows 100 through 199. Each pass covers all 80 bytes of its
scanlines. Within each pass, the drawing order is:

1. Copy the stored cockpit/background and selected terrain strip (`63c6`).
2. Draw the scene-object list at `DS:a3f1`.
3. Draw the HUD-object list at `DS:984f`.
4. Copy the composed rows into CGA banks (`60a0`).

The first terrain selection is index 2, placed at pixel x=16, y=92. Its width
is 288 pixels and height is 15 scanlines. HUD artwork surrounding the playfield
is part of the stored background; counters and other changing elements overlay it.

Object lists contain zero-terminated far pointers to 41-byte records. Ordinary
records supply byte-x and scanline-y coordinates, dimensions, a resolved sprite
pointer, and an active flag. `7281` resolves sprite descriptors; `54c2` projects
scene positions; `5901` sorts scene objects by depth before composition.
Ordinary sprite dimensions come from their descriptors; projection determines
placement rather than rescaling their pixels. Rendering classification 1 uses
separate procedural handlers, outside the ordinary masked-sprite path.

The player update selects sprite index 13 from its initial world position before
the first draw. Its visible craft image starts at pixel `(148,139)` and is
52 by 18 pixels. A companion record selects shadow sprite 18 and is drawn before
the craft; its visible pixels extend to row 157, below the craft's last row 156.
The six initial counter zeros use glyph sprite 70, each 8 by 7 pixels,
starting at `(136,33)` with an 8-pixel horizontal spacing.

Drawing follows the resolved source pointer, which can differ from the record's
current sprite index. The initial HUD resolves two indicators to sprites 85 and
86, then changes their index fields to 87 before drawing without resolving again.
A renderer that looks up those current indices instead of using the resolved
sources would produce a different first frame.

In the verified opening updates, the two indicators resolve sprite 87 while
their initial artwork remains visible. Rebuilding a complete frame from the
current HUD records alone loses those retained pixels. Gameplay composition
must account for framebuffer history as well as current object state.

An independent composition of the background, terrain, scene objects, and HUD
matches all 16,000 visible packed bytes, or 64,000 pixels, of the captured first
gameplay frame. Runtime checkpoints also verify the two framebuffer passes:
after the first, the top 100 rows match the completed scene while the bottom
100 rows still contain the intro image. This verifies the initial scene, not
later gameplay, procedural rendering, or other graphics adapters.

## Decoder

The Rust library implements gameplay asset decoding in `assets`, packed frame
storage and clipped masked drawing in `frame`, and the fixed initial resolved
draw state in `scene`. `scene::initial_frame` composes into a row-major frame;
`Frame::to_cga` exports it into the two scanline banks. This produces the same
visible initial frame as the original two scratch-buffer passes without
reproducing their allocator or previous-frame bookkeeping. The source stride
for ordinary drawing comes from the object's width, including the initial
13-byte left fuel bar, rather than always using the descriptor's padded width.

`mach3-frame` uses the Rust implementation directly; it needs no emulator state
or captured pixel input. See the [README](../README.md#rust-renderer) for commands.
It implements the verified initial scene only. The alternate background is
decoded, but later scene selection and procedural drawing remain unimplemented.

`StartupFrames` also decodes `data1` and `data6` using the CGA bank layout.
The optional `mach3-viewer` presents those frames and the initial scene through
SDL3. Space manually advances the splash and intro, with held/repeated presses
ignored. The viewer then runs the bounded [opening simulation](dos-gameplay.md#rust-opening-simulation),
with optional manual body stepping or static presentation. It does not reproduce
the original startup waits or audio-dependent intro control flow.

For the separate Python video-memory decoder, run from the repository root
using external input and output paths:

```sh
python3 tools/decode_cga.py /path/to/screen.bin /path/to/screen.ppm
```

The input may be a splash asset or a video-memory dump starting at `b800:0000`.
It must contain at least `0x3f40` bytes. The tool writes a 320-by-200 binary PPM
image and refuses to replace an existing output or overwrite its input.
Its default palette is the verified bright palette above. To decode a frame
whose palette differs, pass four comma-separated six-digit RGB values with
`--colors`. Mode 6, composite-color output, and other adapters are outside this
decoder's scope.
