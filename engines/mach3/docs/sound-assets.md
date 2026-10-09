# DOS sound assets

This reference describes the verified sample format and digitized playback path
in the DOS program. Audio playback is not yet implemented in the Rust engine.
Addresses are hexadecimal offsets in the expanded code segment; see
[DOS startup](dos-startup.md) for relocation and data-segment conventions.

## Stored samples

`data7` through `datah` contain raw mono unsigned 8-bit PCM: one amplitude byte
per source sample, starting at file offset zero. They have no sample header,
compression, embedded sample rate, channel count, or loop metadata. Silence is
near the unsigned midpoint; converting to a signed representation uses
`signed_sample = sample_byte - 128` with a sufficiently wide signed type.

The asset loader at `07b5` reads these files directly into separately allocated
buffers. Their initial-data-segment far pointers and file lengths are:

| Resource | Bytes  | Pointer offset word | Pointer segment word |
| -------- | ------ | ------------------- | -------------------- |
| `data7`  | 11,872 | `0f46`              | `0f48`               |
| `data8`  | 9,723  | `0f4a`              | `0f4c`               |
| `data9`  | 10,874 | `0f4e`              | `0f50`               |
| `dataa`  | 19,690 | `0f52`              | `0f54`               |
| `datab`  | 15,500 | `0f56`              | `0f58`               |
| `datac`  | 8,700  | `0f5a`              | `0f5c`               |
| `datad`  | 34,458 | `0f5e`              | `0f60`               |
| `datae`  | 8,700  | `0f62`              | `0f64`               |
| `dataf`  | 8,664  | `0f66`              | `0f68`               |
| `datag`  | 8,802  | `0f6a`              | `0f6c`               |
| `datah`  | 8,927  | `0f6e`              | `0f70`               |

Playback length is a call argument rather than a value in the file. It can
differ from the stored length: the intro uses 8,660-byte portions of several
music samples, and the pre-game call uses 6,638 bytes from `dataa`. A file's full
length therefore does not establish a particular playback duration.

## PC Speaker conversion and pacing

The synchronous sample routine at `55f9` receives a far buffer pointer, a source
byte count, and a direction flag. It builds a 256-entry lookup table at
`5637`–`5657`, equivalent to:

```text
timer_count = 64 - (sample_byte >> 2)
```

This maps the eight-bit stored amplitudes to 64 timer counts, from 1 through
64. It discards the lowest two amplitude bits during playback, without
rewriting the source buffer. A decoder should preserve the original bytes;
six-bit playback precision is not the stored asset format.

The routine programs PIT channel 0 with divisor 60 and channel 2 for mode 0,
low-byte-only writes. The forward handler at `57a4` and reverse handler at
`57b8` translate a source byte and write its count to port `42h` on each timer
interrupt. The source pointer changes every second interrupt, so each byte
normally produces two count writes. With nominal PIT input frequency
1,193,182 Hz:

```text
interrupt rate = 1193182 / 60     ≈ 19886.37 Hz
source rate    = 1193182 / 120    ≈  9943.18 samples/second
```

These are timer-derived nominal rates, not metadata from the asset or a measured
analog output rate. Interrupt phase, boundary behavior, speaker gating, and
timer restoration also affect the resulting signal. In particular, reverse
entry sets the source offset to the requested count, rather than `count - 1`;
reconstruction must account for buffer contents at that boundary instead of
assuming ordinary reversal of a sample slice.

## Music sequencing

The inspected intro sequence is encoded in executable calls and loops, starting
at `503f`, with a follow-on routine at `5308`. Those calls select sample buffers,
counts, repetitions, and direction. This path does not load a separate song or
pattern file. Samples are played serially by the synchronous routine.

An uninterrupted observed intro traversal made 29 sample calls: 28 forward and one
reverse call using 15,500 source bytes from `datab`. It includes partial sample
lengths as well as full resources. The same routine also handles the pre-game
sample. Sequencing, interruption, and effect priority belong to executable
behavior; the PCM bytes alone do not specify them. Procedural gameplay sounds
also program timers and speaker control directly, so these sample files are
not a complete description of the game's audio.

## Corroboration and scope

The separate Ultra Force Mach 3 Sound Blaster player corroborates the sample
interpretation: ten of its eleven sample payloads exactly reproduce the DOS
game's resources; the remaining one matches `datac` apart from tail edits.
Its driver sends the payload unchanged through DMA using Sound Blaster DSP
command `14h`, which interprets it as unsigned 8-bit PCM. Its type-1 records use
time constant 156, selecting nominal 10,000 Hz playback. These command semantics
are specified in Creative's
[Sound Blaster Series Hardware Programming Guide](https://www.ardent-tool.com/sound/Sound_Blaster_HW_Programming_Guide_1st.pdf),
pages 6-6 and 6-14.

The player's Creative Voice-style container, textual song file, and 10,000 Hz
rate describe that player. Mach 3's DOS assets are headerless samples, and its
speaker sample rate follows the PIT programming above. This reference does not
establish Tandy/PCjr playback, every sound trigger, or complete audio fidelity.
