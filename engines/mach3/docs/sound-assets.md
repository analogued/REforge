# DOS sound assets

This reference describes the verified sample format and digitized playback path
in the DOS program. The Rust engine reconstructs the bounded intro and pre-game
sample routines without audible playback or integration into the live viewer.
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

For the 15,500-byte reverse call, the source sequence is the byte at offset
15,500, then asset offsets 15,499 through 1. Offset zero is omitted. The first
byte is outside `datab`: the allocator at `5c2b` truncates the requested byte
length to paragraphs, allocating 15,488 bytes for this resource, while the
loader still writes all 15,500 bytes. The reverse entry reads the following DOS
memory-control block. That byte is execution context, not asset padding; the
Rust memory interface requires it explicitly rather than inventing a value.

The handler inherits DI. Each interrupt increments it and masks it to one bit;
the pointer and completed-byte counter advance only when the result is zero.
An even initial DI produces two interrupts per byte. An odd initial DI advances
the first byte after one interrupt. Completion is tested by the foreground,
so the handler itself does not stop at the requested count.

Setup first selects channel 2 mode 1 and writes two zero bytes, enables both
speaker-control bits, then programs channel 0 divisor 60 and selects channel 2
mode 0. Return restores the saved interrupt handlers and explicitly programs
channel 0 mode 3 with divisor `4c68` (19,560); it does not restore a saved timer
phase. This routine does not update the game's separately stored divisor.

During playback, the temporary keyboard handler saves the latest raw make or
break code and acknowledges the keyboard. A key does not abort the current
sample. Return clears 65 ordinary keyboard-state words at `17c6` and returns the
raw saved scan code. Either F2 make (`3c`) or break (`bc`) disables sound at
return. When sound is already disabled, entry tail-jumps to the ordinary
keyboard processor rather than playing a sample or manufacturing a zero result.

## Music sequencing

The inspected intro sequence is encoded in unrolled executable calls, starting
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

The first wrapper checks accumulated sample results between groups. Its first
sample alone filters signed return values greater than 127. The four calls in
the follow-on wrapper always execute, including after an early first-wrapper
return. The outer intro routine can repeat this traversal after a timed input
wait; that surrounding loop is separate from the bounded sequencer.

## Reconstructed execution interface

`mach3::sound::IntroProgram` decodes resource operands, lengths and group
boundaries from the supplied expanded executable. `IntroSequence` advances
using actual sample return values. `SamplePlayer` reconstructs setup, IRQ0 and
IRQ1 processing, foreground completion and restoration. A caller supplies
memory bytes, device reads and interrupt deliveries through `SampleMemory`
and `SampleBus`.

`mach3::timer::SampleTask` runs that same sample algorithm as a suspended
foreground continuation on `timer::Clock`. A simulation budget advances PIT
events and dispatches the currently installed sample IRQ0/IRQ1 handlers;
foreground return executes the restoration writes and leaves any remaining
budget to the caller. No host sleep, audio renderer or second sample cursor
drives it. Original sound-disabled entry still requires the ordinary keyboard
continuation. Game counters and waits are documented in
[timer pacing](dos-gameplay.md#timer-pacing).

The clock orders operations and derived device edges and retains pending IRQ0/IRQ1
requests, interrupt-enable state, in-service priority and saved interrupt flags.
Repeated blocked requests coalesce; immediate OUT0 rises caused by writes also
request IRQ0. IRET restores saved flags separately from PIC end-of-interrupt.
The priority and non-specific EOI rules follow the fully nested mode described
in Intel's [8259A datasheet](https://www.pcjs.org/documents/datasheets/intel/INTEL_8259A_PIC.pdf),
page 15. Keyboard delivery uses an explicit single-byte latch and rejects overwriting
an unread byte. Observation is optional and cannot choose sample progression.

This is an unmasked IRQ0/IRQ1 execution subset. Initial pins and vectors are
supplied explicitly; boot hardware state, other PIC IRQs, electrical INTA timing
and spurious IRQ7, instruction gaps and
setup-internal interrupt windows are not reconstructed. Handler operations are
ordered at one whole-clock timestamp, and delivery occurs at caller execution
boundaries. The sample continuation drains deliverable pending IRQs before
foreground completion observes its counter and begins teardown. It does not
model instruction interrupt shadows or acknowledge pulses. A source-read or
handler error stops that clock permanently, rather than abandoning an entered
interrupt and advancing silently. This deterministic approximation preserves
meaningful ordering without claiming instruction-exact DOS timestamps. The live
viewer has not yet adopted this clock and still omits audible playback.

Every sample count write carries the original eight-bit byte, resource offset,
playback identity and source-visit identity. Two writes for one byte share a
visit; adjacent equal bytes remain separate visits. Out-of-asset bytes carry
external-memory provenance. Device actions retain repeated writes, handler
changes, interrupt-control effects and keyboard-state clearing. Rendering does
not select samples or advance another cursor.

The development exporter runs one intro traversal followed by the enabled
pre-game sample:

```sh
cargo run -p mach3 --features dev-tools --bin mach3-sound -- --context /path/to/device-reads.tsv --external-byte DATAB:15500:VALUE /path/to/assets /path/to/new-output-directory
```

Replace `VALUE` with the boundary byte established for the execution being
compared. The directory must supply `MACH3.EXE` and `DATA7` through `DATAH`.
Names are case-insensitive. Device context has the tab-separated header
`call`, `interrupt`, `port`, `value`. Calls start at 1. Each call needs a port
`0x61` read at interrupt 0. Keyboard deliveries use paired `0x60`/`0x61` reads
at increasing positive interrupt numbers, delivered before that numbered IRQ0.
Values accept decimal or `0x` hexadecimal. Context supplies device reads, not
sample counts or source bytes.

The new output directory receives `calls.tsv`, `ports.tsv`, `effects.tsv` and
`diagnostic.txt`. The export includes source annotations and entry-state
snapshots. By default, its time column advances by 60 nominal PIT clocks per
IRQ0, without executing a PIT. Saved vectors are explicitly synthetic.
Sound-disabled ordinary
keyboard continuation is exposed by the library and rejected by this bounded
exporter. Host playback, live timer integration and procedural gameplay sound
routines remain unimplemented.

Add `--device` to execute writes through the
[shared PIT and speaker model](../../../crates/reforge_audio/README.md). IRQ0
then follows actual virtual channel-0 rising edges, and `device.tsv` records
timer pins and speaker-drive changes. The `nominal_pit_ticks` column records
completed device clocks in this mode. The first programmed divisor-60 cycle
loads on the next clock and has its first rising edge at clock 61; subsequent
rising edges are 60 clocks apart. Restoration writes begin a new timer phase
rather than resetting an abstract elapsed-time counter.

This bounded diagnostic starts with explicitly synthetic high timer output pins
and unprogrammed counters. Sound begins enabled with zero saved scan and ordinary
keyboard words. It does not execute the outer intro or the ordinary keyboard
processing between intro and pre-game; its entry snapshots describe diagnostic
state rather than a captured complete DOS session. All I/O within a handler is
ordered at one integer timestamp with zero instruction gaps. Keyboard context is
delivered at its numbered IRQ0 edge before that handler. The supplied port `61h` values retain
unmodeled upper read bits; the lower two bits must agree with device state.
Its phase and absolute times do not claim to match a DOSBox execution. There is
no PIC or interrupt-service-latency model. The exporter stops at the pre-game
foreground return; channel 2 can still complete its final pulse afterward.
Digital drive changes remain distinct from acoustic output.

Device rows use a monotonic `sequence` in addition to `tick`. `kind` distinguishes
initial pins, PIT transitions and speaker-drive transitions; `channel` applies
to PIT rows. `call`/`interrupt` identify the current diagnostic call and IRQ0
delivery segment, not the source byte that caused an edge. For write-derived
edges, `action_order` identifies that write; for clock-derived edges it records
the last completed routine action. Source identity remains attached to the
count writes in `ports.tsv`.

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
