# Original DOS engine reference

This reference preserves recovered format and hardware knowledge from the
55,067-byte DOS executable identified by BLAKE3 in
[Original game required](../README.md#original-game-required). Its commercial
release designation remains unverified. The scope is that executable's PC/CGA
and PCjr software paths; these findings do not establish other distributions or
platform versions.

The descriptions combine static instruction review, checks against the original
file and bounded execution of original routines under DOSBox-X. Emulator
observations are identified separately from the program's requested operations.
Useful partial contracts are retained even where REforge uses different host
facilities. Original files, raw instruction listings and captures are external
research material and are not included here.

For scheduling, sound programs and gameplay see [timing](timing.md). Approved
replacement behavior is recorded in [deviations](deviations.md). This document
does not change either the implementation or its support claims.

## Executable layout and address conventions

The selected file has a DOS MZ header of `0x0200` bytes and nine relocation
entries. Its header specifies initial `CS=0723h`, `IP=0000h`, placing the entry
instruction at file offset `0x7430` after accounting for the header. The reviewed
game-data block starts at file offset `0x0300` and occupies `0x7130` bytes, ending
immediately before that code block.

| Address used in these references | File location in the selected executable |
| --- | --- |
| `CS:xxxx`, an original code-segment offset | `0x7430 + xxxx` |
| `DS:xxxx`, an offset in the reviewed game-data block | `0x0300 + xxxx` |

Offsets and register values in code notation are hexadecimal unless stated
otherwise. Pixel coordinates, dimensions, counts and the palette table's color
codes below are decimal. DOS load segments depend on the loader; the notation
above does not prescribe a runtime segment address. Analysis tools may relocate
their working views without changing these file-relative mappings.

This is a partial map of the examined program, not a universal MZ layout or an
inventory of every table. The replacement's external-data reader is in
[`assets.rs`](../src/assets.rs).

## Packed display memory

The original draws into the `B800h` video segment using a 16-KiB, two-bank layout
for 320 by 200 logical pixels. Each byte contains four two-bit pixel indices,
most significant pixel first. Even scanlines start in the bank at `0000h`; odd
scanlines start at `2000h`. Each visible scanline occupies 80 bytes.

The coordinate helper at `CS:2cb0` computes:

```text
offset = (y & 1) * 0x2000 + (y >> 1) * 80 + (x >> 2)
phase  = 2 * (x & 3)
pixel  = (screen[offset] >> (6 - phase)) & 3
```

`phase` is the helper's returned bit phase; `pixel` shows how to interpret the
stored byte. Using `phase` directly as the extraction shift reverses pixel order
within a byte. The helper takes X in CX and Y in DL, returns the byte offset in
AX and the phase in CL, and overwrites DX. It performs no clipping.

The initializer at `CS:2a00` fills 4,000 words with `AAAAh` at each of offsets
`0000h` and `2000h`. These are the 8,000 visible bytes in each bank, filled with
logical color index two. The remaining 192 bytes per bank are outside the
visible image. Initial clears and helper comparisons must distinguish the full
16-KiB storage from its visible pixels.

### Drawing helpers

| Original code offset | Operation |
| --- | --- |
| `2b24` | Copy a list of byte-width rectangles from DS:BX, adding AX to destination offsets in ES. |
| `2cb0` | Convert pixel coordinates to a banked byte offset and bit phase. |
| `2ccc` | Composite source pixels over the screen: zero preserves the old pixel; indices one through three replace it. Save the old screen rectangle. |
| `2d35` | AND source words with screen words, saving the old screen rectangle. |
| `2d70` | Copy pitched source rows into a contiguous buffer; AL supplies source pitch in words. |
| `2d9d` | Copy a contiguous word rectangle to banked display memory. |
| `2dca` | Read a banked display rectangle into a contiguous buffer. |
| `1124` | Save the player's background into the buffer at data offset `05fa`. |
| `1145` | Draw the player through the AND compositor and save the previous background. |
| `11e3` | Restore the saved player background. |

For the word-rectangle operations, CL counts words per row and CH counts rows.
SI and DI identify the source and destination; the compositors also use DS:BP
for saved screen words. After a row, the banked side rewinds the copied width,
toggles bit `2000h`, and adds 80 bytes if the resulting bank bit is zero. The
other side remains contiguous. Source-color transparency and bitwise AND are
different operations; the player's `1145` path specifically uses the latter.

The byte-rectangle list at data offset `28a0` uses bytes rather than words for
its width. Its `0802h` header specifies two bytes by eight rows. It has 24
source/destination word pairs and an `FFFFh` source terminator at `2902`.
The initializer uses destination displacement AX=0 for this list.

Several helpers share mutable scratch bytes. Width is stored as a byte at
`2ae0`, while some operations read a word there and rely on the following byte,
`2ae1`, being zero. Both bytes are initially zero in this input; a complete
indirect-write analysis has not established that invariant for every path.
In `2dca`, scratch accesses use ES: caller `1124` temporarily sets DS to `B800h`
while ES still selects game data. Treating all scratch accesses as DS-relative
would misdescribe that call.

The original helpers do not safely define zero-size or out-of-range rectangles
as empty or clipped operations. REforge's checked framebuffer operations and
complete-scene composition are separate host policies; see
[`framebuffer.rs`](../src/framebuffer.rs) and
[scene and sprite composition](deviations.md#scene-and-sprite-composition).

## Keyboard and joystick interfaces

Startup reads the BIOS model byte through `CS:13aa` and records it at data offset
`0697`. Value `FDh` selects the PCjr branches; the sampled CGA configuration uses
`FCh`. Keyboard initialization is at `13e8`, handler installation at `1419`, and
restoration of saved vectors at `147f`.

| Path | Original interface |
| --- | --- |
| PC keyboard | `INT 09h` handler `14b3` consumes the raw byte from port `60h`. |
| PCjr keyboard | Handler `14fb` consumes the callback's raw key byte in AL. The installed `INT 48h` entry at `1554` invokes `INT 09h`, then returns from the interrupt. |

The game tracks a 22-byte scan-code table at data offset `06a1`, with
corresponding state bytes starting at `06b7`. A tracked make stores `00h`; a
break stores `80h`. The word at `0693` counts make events. Repeated makes and
untracked makes also increment that counter; breaks do not. A new make event
therefore differs from a change in the tracked held-key state. Title and menu
waits use this distinction, as described in the
[keyboard menu reference](timing.md#keyboard-menu).

Static decoding and bounded emulator checks agree on that state/counter
contract. PC-side checks covered several tracked keys, repeated makes and
untracked codes. PCjr execution checked a right-key make/break pair through
the emulated NMI/BIOS callback chain. That establishes the observed original
handler behavior without proving physical PCjr serial-keyboard fidelity,
typematic timing or every simultaneous-key combination.

### Decoding gameplay requests

The main loop calls controls at `1200` before player update `08e5`. Controls
normally polls when unsigned low-BIOS-tick subtraction from `069f` reaches two.
When byte `069b` is zero, it calls keyboard decoder `12c1`.

| Data offset | Decoded request |
| --- | --- |
| `0698` | Horizontal: `00h` neutral, `01h` right, `FFh` left. |
| `0699` | Vertical: `00h` neutral, `01h` down, `FFh` up. |
| `069a` | Active-low action: `00h` held, `10h` released. Keyboard input derives it from the Alt state shifted right three bits. |

The decoder processes down/right first, then lets up/left overwrite them.
Opposing keys consequently favor up and left. The non-PCjr path additionally
checks four diagonal keypad scan codes; the `FDh` path skips those extra tests.
Game logic can also overwrite request bytes, so they are mutable gameplay
requests rather than an immutable copy of hardware state.

The keyboard path includes a wait based on the latched PIT channel-zero
counter, read through helper `13b7`. The [input ordering reference](timing.md#input-and-outer-loop-ordering)
describes its scheduling consequences. The counter predicate should not be
interpreted as an arbitrary fixed delay in milliseconds.

### Original joystick path

Nonzero `069b` selects the joystick path instead of keyboard decoding. It reads
port `201h`, including button mask `10h`, and uses timed PIT reads for axis
measurements. The original menu asks about joystick use before skill selection.
The title also has a button release-before-press path when joystick mode is set.

These are partial static findings. The recorded keyboard/menu comparisons do
not validate the complete joystick setup, calibration, axis thresholds or
physical-device timing. REforge's SDL controller mapping does not implement
the original gameport interface; its omission of the setup question is described
in [startup and keyboard menu](deviations.md#startup-and-keyboard-menu).

## Video startup and palette programming

The original requests BIOS video mode 4 at `CS:0032` in both sampled machine
configurations. Data byte `0690` becomes six on the CGA path and four on PCjr;
this does not represent a second video-mode request. The CGA request observed
as `AX=0B06h` at `0047` has function `AH=0Bh`, not mode-setting function zero.
That startup palette request uses `BH=1`, `BL=1` on both machines. Later CGA
startup writes `20h` directly to port `3D9h` at `0064`; the sampled PCjr startup
does not execute that instruction.

Scene palette helper `1d31` branches on model byte `0697`:

- PC/CGA calls BIOS `INT 10h` at `1d44` with `AH=0Bh`, `BH=1` and
  `BL=data[1853 + scene]`, selecting a CGA palette.
- PCjr makes three calls through helper `1d6e`, whose BIOS instruction is at
  `1d72`. Each uses `AX=1000h`, with BL selecting entry one, two or three and
  BH supplying its scene-specific hardware color code.
- Both finish at `1d6b` with `AH=0Bh`, `BX=0000h`, requesting background/border
  color zero. The residual low AX byte is not necessarily zero on CGA.

### Scene palette tables

The following are the executable's numeric requests, checked against original
file bytes and execution of the original helper in CGA and PCjr emulation.
Hardware color codes are decimal; they are not measured RGB values.

| Scene | CGA selection | PCjr entry 1 | PCjr entry 2 | PCjr entry 3 |
| --- | --- | --- | --- | --- |
| 0 — alley | 1 | 10 | 9 | 15 |
| 1 — fishbowl room | 1 | 9 | 14 | 15 |
| 2 — underwater | 1 | 11 | 4 | 15 |
| 3 — bookcase | 1 | 11 | 6 | 15 |
| 4 — cheese | 0 | 14 | 12 | 15 |
| 5 — birdcage | 0 | 9 | 10 | 15 |
| 6 — milk | 1 | 6 | 13 | 15 |
| 7 — hearts | 0 | 10 | 4 | 15 |

The eight-byte tables start at data offsets `183b`, `1843` and `184b` for
PCjr entries one, two and three, and `1853` for CGA selection. Entry zero is
reset to black by the shared final background request. The transition routine
calls the scene helper at `1c24`, returning at `1c27`.

### What the observations establish

Palette-only calls left the complete 16-KiB framebuffer unchanged. The BIOS
data-area byte at `0040:0066` became `20h` or `00h` for CGA selections one or
zero. In the PCjr captures it stayed `20h` despite different palette-entry
requests, so that byte alone cannot describe the PCjr mapping.

Read-only emulator-state queries additionally showed PCjr's first four palette
entries becoming `[0, table1[scene], table2[scene], 15]`. The inspected CGA
attribute-controller palette field instead remained zero, so that field was
not its logical-index mapping. The emulator's unchanged DAC table was also
distinct from either scene's logical-index selection. A framebuffer dump, BIOS
request, BIOS bookkeeping byte and emulator palette table describe different
parts of presentation.

DOSBox-X reported its internal `M_TANDY4` renderer name for both configurations;
that name does not classify the CGA run as Tandy hardware. Its reported 320 by
200 logical dimensions and 4:3 ratio describe emulator presentation metadata.
These observations do not establish physical monitor RGB, composite output or
the complete dynamic palette timeline. Other gameplay effects change background
colors outside `1d31`; the [timing reference](timing.md) records reconstructed
effects separately from this scene-palette contract.
