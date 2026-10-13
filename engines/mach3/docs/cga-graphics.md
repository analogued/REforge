# CGA graphics

## Splash and intro asset format

`data1` and `data6` are headerless 16,384-byte (`0x4000`) CGA framebuffer
images. Their files contain no dimensions or palette metadata; video mode and
palette come from the executable. Both screens use the same layout below.

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

| File offsets       | Contents                               |
| ------------------ | -------------------------------------- |
| `0000`–`1f3f`      | 100 even scanlines, 80 bytes each      |
| `1f40`–`1fff`      | 192 bytes outside the visible image    |
| `2000`–`3f3f`      | 100 odd scanlines, 80 bytes each       |
| `3f40`–`3fff`      | 192 bytes outside the visible image    |

Ranges are inclusive. The original copy reads the first `0x3f40` bytes,
including the gap between banks. The decoder needs that prefix and ignores the
final unused region; a file does not need to supply all `0x4000` bytes to decode
the visible frame. This acceptance rule differs from the full stored file size.

The verified initial color-select value is `0x30`, giving these RGB values:

| Pixel index | RGB hexadecimal |
| ----------- | --------------- |
| 0           | `000000`        |
| 1           | `55ffff`        |
| 2           | `ff55ff`        |
| 3           | `ffffff`        |

The asset bytes, copied framebuffer, and decoder output have been compared with
the emulator's rendered surface. All 64,000 RGB pixels match. Captures and
original-derived images remain outside the source repository.

## Intro and gameplay transition

The first splash is displayed while the remaining assets load. Startup then
performs a DOS-clock wait, selects palette family 1, and calls the intro routine
at `076a`. That routine chooses another screen from `data6` or `data1`; the
verified initial path selects `data6`. Both use the same direct CGA layout.

The wait at `5df0` programs PIT channel 0 in mode 3 with reload `0x2a30`
(10,800 clocks), then repeatedly reads DOS time through `INT 21h/AH=2Ch`.
It returns when the seconds field has advanced by at least 12, adjusting for
minute rollover. IRQ0 still runs the BIOS clock handler here; the game's timer
handler is installed afterward. Consequently those 12 DOS seconds pass much
faster than real seconds: the BIOS tick counter advances once per accelerated
IRQ, but DOS converts each tick as though the divisor were still 65,536.
This is an automatic wait; Space does not shorten it in the original.

Normal playback reconstructs this check on the shared PIT timeline. DOS seconds
are derived as `floor(bios_ticks * 65536 / 1193182)`, modulo 60, with the BIOS
counter's daily wrap at `0x1800b0`. The initial BIOS tick count is fixed at zero
for deterministic playback: the wait takes 219 interrupts, approximately
1.9823 seconds. Other starting clock phases can shorten the original wait;
away from midnight it takes 201–219 interrupts, about 1.82–1.98 seconds.
PIT programming and the subsequent game-handler installation remain distinct,
so waiting on the splash does not advance game software counters. Incidental
asset-loading and instruction-execution delays are excluded. Host pause freezes
the wait along with simulation time. A fresh Space press provides an optional
modern skip, consumed until release so holding it cannot also advance the intro.

The intro sequence at `537a` repeats while its input result is zero. Its playback
calls are synchronous; completion alone does not provide a nonzero return.
Space advanced the observed intro into gameplay. The return boundary is `03d1`.
Initial scene rendering completes at `044a`, before another playback call and
entry into the gameplay loop. No intro duration is asserted here: timer changes
and debugger pauses make wall-clock measurements unsuitable for that purpose.
See [DOS sound assets](sound-assets.md) for the stored samples and verified
digitized playback path and the normal viewer's PC speaker synthesis.

## Gameplay graphics resources

Gameplay backgrounds use row-major packed pixels rather than CGA's alternating
scanline banks. Their stored bytes interleave two four-bit planes during loading:
output bits 7 through 0 take input bits `3,7,2,6,1,5,0,4`, respectively. The
converted byte contains four two-bit pixels in the same left-to-right order as
video memory.

| Resource | Converted contents                                                         |
| -------- | -------------------------------------------------------------------------- |
| `data2`  | 320-by-200 cockpit/background, followed by twelve 288-by-15 terrain strips |
| `data3`  | Alternate 320-by-200 cockpit/background with a starfield                   |
| `data4`  | Color pixels and descriptors for 190 sprites                               |
| `data5`  | Paired sprite masks with matching dimensions                               |

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
decoded; gameplay selects both bases and draws procedural gates and goal panels.

`StartupFrames` also decodes `data1` and `data6` using the CGA bank layout.
The optional `mach3-viewer` presents those frames and the initial scene through
SDL3. Normal animated playback uses the timed splash and reconstructed
audio-dependent intro control flow described above, then runs the
[gameplay simulation](dos-gameplay.md#rust-gameplay-simulation) through game over
and restart. Silent step/static/custom-period diagnostic previews retain manual
Space advancement of their startup screens, with held/repeated presses ignored.
