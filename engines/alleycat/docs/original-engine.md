# Original DOS engine reference

This reference preserves recovered format and hardware knowledge from the
55,067-byte DOS executable identified by BLAKE3 in
[Original game required](../README.md#original-game-required). Its commercial
release designation remains unverified. The scope is that executable's PC/CGA
and PCjr software paths, with the separately identified restored-input section
below. These findings do not establish unrelated distributions or platforms.

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

## Examined DOS disk container

The supported 3,145,728-byte `alley_cat.img` has BLAKE3
`2b8cf7b87eb8975f9ad502428d39c8fbaa57cb2aacd16f0f2e79d4b3f2aee4b8`.
It is a partitioned hard-disk image despite some archives describing it as a
floppy image. Its first MBR partition is type `06h`, starts at LBA 8 and contains
6,136 sectors. The FAT16 volume uses 512-byte sectors, one sector per cluster,
one reserved sector, two 24-sector FATs and a 512-entry root directory.

The root directory at image offset `0x7200` contains only `CAT.EXE`, starting
at cluster 2. Its 108-cluster chain supplies exactly 55,067 file bytes, beginning
at image offset `0xb200`; final-cluster padding is outside the executable.
Those file bytes are identical to the standalone DOS conversion described above.
Container authorship and creation date are not established by that equality.
This is distinct from the original PC Booter program described below.

REforge reads the selected partition, root entry and bounded FAT16 cluster chain
in memory, then uses the existing MZ asset parser. It does not execute the boot
records or write extracted files. The reader supports this verified container's
root-file use; it is not general disk emulation or discovery of other releases.

## Restored DOS input layouts

The NewRisingSun restoration also supports the 55,067-byte conversion through
its external `791CE265.DIF`. Applying that BSDIFF40 patch yields a 56,624-byte
full-build layout. Applying the package's separate `TANDYCAT.DIF` adds a DOS
hardware adapter and yields the supported 56,881-byte `TANDYCAT.EXE` exactly.
The latter has a 512-byte MZ header, no relocations and initial `CS:IP=0000:db30`;
its game code starts at file offset `0x0200`, and its full `0x73f0`-byte data
segment starts at `0x6940`. Addresses in this section refer to that full layout;
other sections retain the conversion's address conventions unless stated.

The restored data contains an additional three-voice title score and sequencer
state, shifting later asset locations. REforge reads it into a separate checked
segment, translates documented data regions and asset-pointer fields into its
established conversion layout, and retains the full segment for music parsing.
This is a structural translation, not another release-fingerprint check.
The DOS adapter's interrupt hooks are unnecessary in the native application.

The restoration package is discussed in
[NewRisingSun's VOGONS posts](https://www.vogons.org/viewtopic.php?t=45896).
The [bjt42 Tandy patch](https://github.com/bjt42/tandycat) instead targets an
executable that already has the full music resources; it cannot restore those
resources in the selected 55,067-byte conversion by itself.

### Palette and text differences

The restored Tandy adapter selects the PCjr software branches. Its alley and
room RGBI table values match the conversion's PCjr tables: the alley uses
entries 10, 9 and 15. A bounded DOSBox-X Tandy run confirmed those palette-entry
writes. That evidence does not support globally dimming the Tandy palette.

The restored BIOS teletype helper at `CS:61ad` instead selects text color 3;
the conversion's `CS:5e2b` selects color 2. This changes skill/instruction and
pause captions, including with CGA or PCjr selected. Completion scoring already
uses color 3 in both layouts. REforge preserves this input-specific text color,
rather than treating it as a different global Tandy palette.

### Three-voice title score

Full-build `CS:16a0` initializes the PSG title and programs PIT channel zero to
`0xcfff` input clocks per BIOS tick (about 22.4 Hz). The changed-tick sequencer
at `CS:173e`/`178d` writes three tone channels through port `c0h`; the noise
channel stays muted. It advances once per observed changed BIOS tick, without
catch-up for skipped ticks. The score uses byte offsets into a word divider
table, per-note durations, attenuation envelopes and staged channel enables.

| Full-build data | Purpose |
| --- | --- |
| `DS:1014` | PSG divider words |
| `DS:109a`, `10f2`, `113a` | Three note streams, ending with marker `86h` |
| `DS:10c6`, `1116`, `11b1` | Corresponding duration streams |
| `DS:124c` | Pointers to three nine-byte channel states |
| `DS:1256` | Staged channel-enable limits |

The initial title timeout therefore occurs sooner in wall time than the
conversion's speaker title. The later PSG title keeps running instead of taking
the speaker version's periodic presentation reset. Turning native sound off
mutes the PSG without restoring the BIOS divisor; enabling it reinitializes the
sequence. Title exit restores channel zero to divisor 65,536. Gameplay effects
retain the speaker path. These are inspected program contracts, not physical
PCjr/Tandy audio or timing measurements.

## Examined PC booter image

A separately examined 184,320-byte disk image has BLAKE3
`f6f0fe51a58b2c32995427fd8f3af2b1df27f3f4b8033a4a2b55946e3a90c900`.
REforge supports this specific image through a raw-sector loader. Its loader
and disk-check bypasses prevent treating it as proven unmodified commercial media;
its historical release identity and the authors of those changes remain unknown.

The image uses 40 cylinders, one head and nine 512-byte sectors per track. Its
boot sector bypasses the filesystem, loading sectors 1–8 from cylinders 2–17
into `0800:0000`, one 4-KiB block per track. The ninth sector of each track is
skipped, so the resulting 64-KiB program is not a contiguous slice of the image.

A decoding pass transforms every fourth loaded byte at offsets `0000` through
`fff8`, for `3fff` iterations. Its 16-bit state starts at `fa59`. Each iteration
computes feedback from bit 1 of the low byte XOR the high byte, shifts the state
right with that bit entering bit 15, and XORs the selected byte with the new low
state byte. The final four-byte group is untouched. Bounded DOSBox-X CGA and
PCjr boots reproduced all 65,536 decoded bytes from the static extraction.
Further runs reached the CGA title loop and PCjr external-speaker question,
without injecting a keyboard response or validating a complete game.

The first `db30` decoded bytes match the full restored DOS program at
`TANDYCAT.EXE` file offsets `0200`–`dd2f`, except four bytes in startup disk-check
handling. At code offset `0048`, the booter calls the disk-check path while the
DOS program calls a return stub. The booter's disk routine instead bypasses its
BIOS verify operation and forces the success branch. Its requested sector IDs
would otherwise lie outside the ordinary nine-sector geometry. The earlier
loader also contains an XOR pass that does not store its result. These are
observed modifications, not an attribution of the original protection scheme.

The entire `73f0`-byte data segment at decoded offset `6740` matches the restored
DOS segment at file offset `6940`. This includes the three-voice title score and
its tables; the compared PSG code is also identical. The same structural data
translation reproduces all 28,976 bytes of the supported conversion's data
segment. This establishes resource identity, not full gameplay or physical
hardware equivalence. The Tandy DOS adapter is absent from the matching program.

The final 9,424 loaded bytes contain duplicate code and data fragments rather
than zero padding. Their origin and reachability remain unresolved. REforge
retains the complete 64-KiB payload through decoding, then reads the verified
full-build data segment; it does not execute any of the loaded code. No generic
filesystem, controller emulation or additional image-library dependency is needed.

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

### Walking sprite origins

The examined data's pointer table at `0f7a` contains six walking images per
direction, with right-facing entries first. Each image is 24 pixels wide by
11 rows (`0b03h` in the word-rectangle format) and uses the player's AND blend.
Walking advances the six-frame cycle independently of the logical displacement.

Visible features shift within that fixed rectangle as the pose changes. In
zero-based right-facing phases four and five, the rightmost black pixel moves
from local X 22 to X 20. The packed drawing origin also rounds logical X down
to a four-pixel boundary. Consequently, a feature can move backward for one
animation step even while logical X advances; a larger drawing-origin step can
instead hide that pose shift. These are properties of the examined artwork and
packed rendering, distinct from a reversal of the player's logical movement.

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
