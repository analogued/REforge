# CGA splash graphics

The initial screen is a direct copy of `data1` into CGA video memory at
`b800:0000`. The copy routine is `6004`, called at `0808` and returning at
`080b`; offsets are relative to the expanded program's code segment as described
in [DOS startup](dos-startup.md).

The copy length is `0x3f40` bytes. A visible mode-4 frame is 320 by 200 pixels,
with four two-bit pixels per byte and 80 bytes per scanline. Even scanlines
occupy the bank at offset zero; odd scanlines occupy the bank at `0x2000`.
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

## Decoder

From the repository root, using external input and output paths:

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
