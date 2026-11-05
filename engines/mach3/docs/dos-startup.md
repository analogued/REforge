# DOS startup and address conventions

The supported DOS programs contain the same initialized game image in two
executable packagings: a 43,008-byte reverse-RLE executable and a 23,929-byte
PKLITE-wrapped executable. Both use the same 17 external DATA files. The PKLITE
wrapper expands to the same reverse-RLE load image, excluding a reference-only
final padding byte; their complete initialized and relocated images match.
Both catalog distributions share compatibility ID `mach-3-dos`.

The DOS program starts through a self-unpacking loader. Its initialized expanded
image is `0x12b30` bytes (76,592 bytes), with 132 internal segment relocations.
The loader restores the stack and transfers control to relative `0000:0000`.

Use the expanded image before relocation as the address-independent analysis
input. It is a raw memory image, not a runnable DOS executable. The copied
unpacker, uninitialized allocation, and stack storage are outside its bounds.
For a runtime image loaded at segment `L`, relocation adds `L` modulo 65,536
to each inventoried segment word. Runtime addresses must record `L`; a raw dump
must not be interpreted as if its segment words were still relative.

Addresses below are hexadecimal offsets in the expanded program's code segment,
relative to `L`. The initial runtime data segment is `L + 095a`, and the restored
stack is `L + 14fd:0080`.

## PKLITE wrapper loading

The Rust loader recognizes the inspected PKLITE 1.15 large-model wrapper
(`210f` version/flags, 128-byte outer MZ header). It decodes the interleaved
16-bit control words, literal bytes and variable-length back-references without
executing the decompressor. Control bits are consumed least-significant first;
the next control word is fetched immediately after the sixteenth bit, before
any following literal or reference byte. Overlapping references repeat their
newly written bytes.

An extended-length byte of `fe` normalizes segmented addresses without producing
bytes or resetting the control stream; `ff` terminates expansion. The loader
checks the resulting length against the saved original MZ header, requires the
original's zero DOS relocations, and verifies the trailer's SS:SP and CS:IP.
Output allocation, input accesses and reference ranges are bounded. Unknown
wrapper layouts and malformed/truncated inputs are rejected. The ordinary
reverse-RLE decoder then produces the same address-independent program image.
This is support for the verified wrapper, not a general PKLITE implementation.

## Initialization path

| Offset | Role |
| --- | --- |
| `0000` | Runtime entry: sets data/stack segments, clears uninitialized data, and obtains DOS environment and command-line information |
| `02f6` | Calls the runtime wrapper at `8010` |
| `8010` | Prepares runtime state and standard file handles |
| `809d` | Calls application entry `0339` |
| `0339` | Begins application initialization |
| `037f` | Calls buffer/graphics initialization at `5cad` |
| `0392` | Calls the asset loader at `07b5` |

## Initial graphics and file access

Graphics initialization allocates buffers through the DOS allocation helper at
`5c2b`, then calls `5c17`. The BIOS video interrupt at `5c1d` receives `AX=0004`.
This is the verified initial CGA mode-setting path; it does not establish all
hardware modes the program may support.

The asset loader uses DOS file wrappers. Its first file is `data1`, opened
read-only. The first read requests `0x3f40` bytes through DOS function `3f`.
The open wrapper at `81fb` loads the filename pointer at `8202`, sets function
`3d` at `8205`, and uses the shared interrupt site at `81e6`.

The expanded image, relocation handling, entry transfer, initial mode call,
and first open/read request have been checked against an isolated DOSBox-X
debugger execution. The broader startup map is based on static control flow.
See [CGA graphics](cga-graphics.md) for verified asset layouts and composition,
and [DOS gameplay](dos-gameplay.md) for the subsequent update loop.
See [DOS sound assets](sound-assets.md) for the headerless sample resources,
speaker conversion, executable music sequencing and live playback.

## Original sample-disable argument

At `036a`–`0379`, startup reads the first byte of the first argument and compares
it with lowercase `s` (`73h`) and uppercase `S` (`53h`). Either clears the word
at `DS:b666`, initially enabled. This flag bypasses digitized sample calls in
the intro, pre-game, completion and ending paths; procedural sound is separate.
It is retained through game-over returns and new games within the process.
REforge accepts bare `S` and `s` as hidden aliases for
`--digitized-samples off` when Mach 3 is selected. Only those complete tokens are
aliases; option values and data paths are not reinterpreted. Other games ignore
the Mach 3 option. The argument is temporary and does not modify saved settings.
