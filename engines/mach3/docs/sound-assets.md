# DOS sound assets

This reference describes stored samples, digitized playback and procedural
sound in the DOS program. The Rust engine reconstructs intro/pre-game samples,
procedural setup/IRQ0 routines and gameplay waits on one PIT timeline. Normal
animated viewing includes synthesized PC speaker playback through SDL3.
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

`DosSampleMemory` models the established allocation-name suffix at that boundary.
The 12-byte overrun overwrites bytes 0–11 of DATAC's preceding MCB; offset
15,500 reaches MCB byte 12, the fifth byte of its eight-byte owner name.
DOSBox-X's split allocation propagates the program name there. The live
frontend supplies the padded name `MACH3`, making the first reverse byte ASCII
`3`. Other names can be supplied through the library. Unestablished bytes still
return errors; this is explicit DOS bootstrap context, not a general heap model.

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

### Host volume categories

The live adapter assigns `data7`–`dataa` announcements to Speech and `datab`–
`datah` intro material to Music. Both forward and reverse `datab` belong to Music
because they frame the intro's musical sequence; they have no gameplay playback
callers. Procedural tones, ramps, noise and the native pause tone use Sound
effects. The mapping is isolated in `sound/category.rs`;
partial and reverse playback retain the resource's category. Classification is
presentation metadata and never changes sample calls, device writes or priority.
Both Speaker and Enhanced PCM use the same categories. Filtered tails retain
their originating group; returning from sample playback to deliberate speaker
programming selects Effects. All category levels at 100% preserve original PCM.
See [audio settings](../../../docs/configuration.md#audio) for global/game
preferences and Master volume.

### Native sequence

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
return. `IntroTask` also executes outer routine `537a`: a zero sequence result
leads to 500 calls to `7437` with full AX=1, each followed by ordinary input
processing at `73b2`. A nonzero poll returns; otherwise the sequence repeats.
Temporary sample keyboard IRQs save the latest scan without aborting playback.
Space during a sample therefore waits for its call/group and the entire tail.
The selected intro image remains visible until this foreground returns; the
first game image is composed before the separate optional pre-game call `4f8d`.

## Procedural sound

The gameplay sound routines share priority arbitration at `74ad`. Requests use
unsigned word comparison: a request is accepted when the current priority is
greater than or equal to the requested priority. Lower values take precedence;
equal requests can restart an effect. Request `00ff` is always accepted and sets
the priority to `00ff`. Rejected setups leave state and hardware unchanged.

| Setup  | Priority | IRQ0 handler | PIT0 divisor | Effect                              |
| ------ | -------- | ------------ | ------------ | ----------------------------------- |
| `74c0` | `00ff`   | `7532`       | `ffff`       | Default timer and speaker off       |
| `74e8` | `0001`   | `7532`       | `9000`       | First tone table                    |
| `75d1` | `0001`   | `761b`       | `d000`       | Second tone table                   |
| `76f4` | `0005`   | `773e`       | `2600`       | Third tone table                    |
| `77da` | `0004`   | `7824`       | `2600`       | Fourth tone table                   |
| `78cc` | `0006`   | `79e7`       | Argument     | Masked noise                        |
| `7925` | `0000`   | `7a58`       | `03a4`       | Unmasked noise and first timer ramp |
| `795a` | `0002`   | `7a58`       | `0500`       | Unmasked noise and second ramp      |
| `798f` | `0003`   | `7a58`       | `04a0`       | Unmasked noise and third ramp       |

Every accepted setup programs channel 0 mode 3, writes its divisor and updates
the game's stored divisor at `7b27`. Default setup installs its vector before
device programming and clears speaker bits 0/1. Tone setup programs channel 0
before installing its vector, programs channel 2 mode 3/count `0200`, then sets
its own table cursor to 1. Noise/ramp setup resets the fourth tone cursor to zero
and the noise cadence byte to 15 before installing its vector and programming
channel 0. Ordinary setups contain no CLI/STI.

The seed bytes, ramp group counter, software timer phase and wait counter survive
effect changes. Default setup also retains every tone cursor. Normal setups
enable speaker bits 0/1 only when the sound-disable word is zero.

All six procedural IRQ0 handlers first execute the common timer-counter helper
at `7b2b`, documented in [timer pacing](dos-gameplay.md#timer-pacing). Its returned
AX is `sound_disable_word XOR ffff`. Exactly `ffff` therefore clears the speaker
and skips sound work while still advancing gameplay counters. Other nonzero
words disable the setup gate but do not take this IRQ skip branch. Each handler
finishes with PIC end-of-interrupt and IRET; none restores a previous timer.

### Tone divisor tables

Tables are little-endian 16-bit PIT channel-2 counts in the expanded executable.
Their bytes remain external game data. Normal playback skips entry zero,
programs mode 3 and writes the selected divisor before testing termination.

| Handler | Cursor word | Table start | Last advancing index | Repeated terminal index | Required words |
| ------- | ----------- | ----------- | -------------------- | ----------------------- | -------------- |
| `7532`  | `7581`      | `7583`      | 15                   | 16                      | 17             |
| `761b`  | `766a`      | `766c`      | 45                   | 46                      | 47             |
| `773e`  | `778a`      | `778c`      | 12                   | 13                      | 14             |
| `7824`  | `7870`      | `7872`      | 40                   | 41                      | 42             |

Termination uses signed word comparison. At the terminal index, the handler
releases priority to `00ff` and keeps rewriting that entry on subsequent IRQs.
The first two handlers also clear speaker bits 0/1; the other two leave them
unchanged. The cursor and installed handler remain active. A zero cursor skips
tone programming. The Rust loader validates table-addressing instructions and
reads only the required prefixes; it rejects unsupported indices rather than
clamping them or reading arbitrary instruction bytes.

The first two tables include count 1 in active positions as well as terminal
positions. Intel's specified mode 3 range starts at 2. Live playback and the
device exporter therefore explicitly select the shared PIT's
[`CountOnePolicy::DosBoxWrap`](../../../crates/reforge_audio/README.md#limits-and-validation):
raw 1 retains its original writes but gets an effective 65,537-clock period,
following DOSBox-X. This compatibility choice is separate from tone sequencing
and does not establish the unspecified behavior of original PC hardware.

### Noise state and timer ramps

Noise uses a cadence byte at `7b0a`, a persistent ramp group word at `7b0b`, and
four seed bytes at `7b0d`–`7b10`. Three word reads overlap: A at `7b0d`, M at
`7b0e`, and B at `7b0f`. Each active IRQ increments the cadence modulo 256.
Regeneration runs when bit `10h` becomes set, then clears the cadence. With
helper-return AX:

```text
AX = ((AX XOR A) + 2345h) modulo 65536
AX = AX XOR M XOR B
AX = AX AND db6dh                    # handler 79e7 only
A  = AX
B  = ((AX + 4567h) modulo 65536) XOR M
```

The final M read observes the byte just changed by storing A. Treating A, M and
B as independent words changes the generator. Regeneration also programs channel
2 mode 2/count 48. Every active noise IRQ then rotates A left by one bit, reads
port `61h`, forces bit 0 on, replaces bit 1 with rotated A's bit 9, and preserves
read bits 2–7. Repeated gate and timer writes remain executable operations.

Handler `7a58` omits the mask and performs a timer ramp between regeneration and
seed rotation. Ramp parameters are:

| Setup  | Initial current | Limit  | Step   | IRQs per group |
| ------ | --------------- | ------ | ------ | -------------- |
| `7925` | `03a4`          | `06f8` | `0008` | 4              |
| `795a` | `0500`          | `0868` | `0030` | 7              |
| `798f` | `04a0`          | `0568` | `0004` | 3              |

Each active IRQ increments the group word. A signed comparison at or above the
group size clears it and adds the step to current modulo 65536. When signed
current is below the limit, the handler reprograms channel 0 mode 3 and updates
the stored divisor. Otherwise it releases priority without stopping the handler
or changing that divisor. Current continues advancing beyond the limit; signed
wrap can permit later timer writes. Current and the stored divisor are therefore
distinct state. The group word is retained when another ramp is selected.

`sound::ProceduralState` implements these routines with externally loaded
`ProceduralProgram` tone tables. `timer_interrupt` consumes an explicitly
delivered active IRQ; `dispatch` checks the vector against `timer::Clock` and
applies the same actions to the shared device model. Device interrupts and
reprogramming remain operational without an audio renderer.

Setup exposes `prepare` followed by `PreparedEffect::finish` on the same state.
This permits a previous handler to interrupt after the early cadence/parameter
writes and before vector/device programming. `request` executes both stages
atomically. The original can also interrupt before those early writes and within
the later stage, including between PIT byte writes. Those finer instruction
windows and setup return-register effects are not reconstructed by this API.
Neither stage prescribes instruction delays; IRQ handlers retain the clock's
ordered, zero-gap execution convention. Normal animated viewing executes this
procedural state through `GameplayRuntime` on the same PIT timeline as samples
and gameplay waits. Step/static/custom-period previews retain silent stepping.

### Gameplay invocation

`Gameplay::advance_subframe_with_sound` selects foreground operations directly
from the gameplay algorithms. Its synchronous sink receives `GameplayAction`:
procedural requests, explicit death waits, and the guarded completion sample.
Requests retain their original call-site identity and are emitted even when
priority arbitration might reject them. A caller can apply requests to
`ProceduralState` with the same device bus used for IRQ execution; no renderer
selects effects or advances a second sequence. Default stepping uses a silent
sink. `PauseControl::poll_with_sound` exposes the native pause-entry tone.

| Gameplay condition                          | Selected operation                         |
| ------------------------------------------- | ------------------------------------------ |
| Accepted player projectile slot             | Fourth tone table, once for that shot      |
| Enemy projectile creation                   | Third tone table                           |
| Falling-enemy initialization                 | Second tone, for each initialized slot     |
| Grace-eligible damage helper                 | Second noise ramp, including zero damage   |
| Craft/falling/terrain conversion              | Third noise ramp                           |
| Terminal death response                      | First ramp, wait 1, wait 20, default timer  |
| Native pause entry or successful special gate | First tone table                           |
| Control commands `c3` and `c4`                | First tone table                           |
| Completion craft reaches its depth threshold  | First tone, then guarded `DATA8` sample     |

Shot setup occurs after counter masking and before initializing the selected
projectile. Falling initialization requests a tone even for a slot subsequently
deactivated by its count gate. Collision scans retain individual requests in
ascending record order. Ordinary craft conversion precedes the player damage
request; solid terrain damage precedes conversion. Contact damage during grace
is rejected by the damage helper while collection/conversion can still execute.
The helper checks grace before requesting the ramp and accumulating damage,
then checks the response phase. It can accumulate damage without restarting an
already active response; the outer collision scans have separate phase gates.

The body-noise setup at `067b` runs after the first ordinary framebuffer
checkpoint and before the second player poll. Its argument is the word at
`b690`, computed with wrapping word arithmetic:

```text
each player update: divisor = (signed height >> 1) + 1900
nonterminal response entry: divisor += (new phase - 8) << 4
normal enemy common path with pre-delta depth < 85:
    divisor = signed_max(divisor, (signed depth >> 1) + 2400)
after pass 1, before advancing speed: divisor -= (old forward speed - 28) << 3
```

Terminal response entries retain the height baseline even if they select a new
response phase. Later player polls overwrite the word again. Idle height −100
and forward speed 28 produce divisor 1850. The call requests priority 6 and
resets cadence when accepted; it is not an unconditional replacement of a
higher-priority sound.

Death sound starts at the terminal row-16 response, rather than at the start
of the death animation. Both wait calls load full AX, so their targets are
exactly 1 and 20. Default setup follows those waits before grace/life reset.
Gameplay return requests the default timer once, including an externally
requested exit at a body boundary. Reaching an export limit alone does not
represent a DOS game return and does not request that reset.

Completion wrapper `4fd9` uses 9,723 forward source bytes from `DATA8` when the
application's digitized-sample flag at `b666` is set. `IntroSample` identifies that
conditional call; the executor supplies the flag and the ordinary sample-disable
state, then uses the existing reconstructed sample routine. It does not replace
the sample with a procedural tone or infer its duration from a renderer.

The callback interface selects operations and completes a pass synchronously.
`gameplay::execution::GameplayTask` instead retains the pass continuation across
simulation budgets and drives ordinary, publication, death and native-pause waits
through the device clock. It runs completion playback through the same sample
cursor and executes its keyboard clear and disable effects before resuming the
enemy update. Sound-disabled sample entry performs the ordinary keyboard poll;
an unset application digitized-sample flag skips that call altogether. Startup reset
remains a lifecycle responsibility. Live scene transitions use normalized region
deadlines while device interrupts continue; CPU busy-loop costs remain untimed.
Terrain collision scans finish
their terrain mutations before delivering ordered contacts to the sink.
Normal animated viewing uses this continuation; recording replay and
step/static/custom-period previews retain silent stepping and nominal timing.
The existing speed-update checkpoint is unchanged; the sound argument uses the
old speed as above.

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
viewer uses this same clock through intro, pre-game, gameplay and ending waits.
At gameplay return, the caller's `74c0` reset restores handler `7532`, programs
divisor `ffff` and disables the speaker before the ending waits. `GameplayTask`
owns this reset through `Gameplay::return_sound`; the frontend does not repeat
its ordered vector, timer and speaker transactions. The clock retains
software counters, phase and tone cursors rather than constructing a fresh clock.

Every sample count write carries the original eight-bit byte, resource offset,
playback identity and source-visit identity. Two writes for one byte share a
visit; adjacent equal bytes remain separate visits. Out-of-asset bytes carry
external-memory provenance. Device actions retain repeated writes, handler
changes, interrupt-control effects and keyboard-state clearing. Rendering does
not select samples or advance another cursor.

### Live playback

The normal animated CLI loads `MACH3.EXE` and `DATA1` through `DATAH`. A single
clock retains PIT, software counter/phase, vectors, keyboard state and DI across
samples and ordinary execution. `IntroTask` and `GameplayTask` suspend the guest
foreground at blocking calls while the host keeps processing SDL events. The
frontend supplies explicit bootstrap pins/vectors and the DOS allocation name
`MACH3` for `DosSampleMemory`'s modeled MCB suffix; that context is not sample data
or a universal DOS heap assumption.

By default, PC speaker drive edges feed the shared signal renderer at their simulation ticks.
The viewer synthesizes 48 kHz mono signed 16-bit PCM and feeds timestamped blocks
to the shared [`reforge_sdl::audio::queue`](../../../crates/reforge_sdl/README.md).
Its shared policy prefills 50 ms, preserves buffer-covered interruptions, and
recovers only when output exceeds the conservative lateness budget. Recovery
discards expired presentation audio and retains future PCM; ordinary submissions
do not trim a fixed-age tail. SDL handles native device format conversion.
Neither recovery nor device failure resets the PIT or advances gameplay from a
callback. Device failure warns and mutes host output; sample, procedural and wait
execution continues on the same native timeline.

Ending playback uses this clock for the guarded mandatory Game Over `DATA9`
sample and the strictly improved High Score `DATA7` sample before their hold
waits. The wrappers' flags, ordinary sound-disable tail poll and sample keyboard
teardown retain their gameplay effects. See
[game-over execution](dos-gameplay.md#game-over-and-return) for call sites and
selection rules.

Pause/Break freezes host scheduling and queued playback together. Native F1
retains the original keyboard handshake and PIT-driven waits; the device continues
to execute during native pause. F2 changes native sound preference while ordinary
timer counters continue. Step/static/custom-period previews and recording replay
remain silent. Instruction gaps and CPU delay loops are not timed by this model.

### Enhanced PCM playback

`--audio-renderer enhanced-pcm` selects full-byte rendering during recognized
sample playback; `speaker` remains the default reference. The engine still
executes every PIT write, interrupt, wait and keyboard effect. Outside sample
intervals, enhanced output uses the same speaker drive for procedural effects.
This policy is implemented in `sound::render::RenderingPolicy`; synthesis and
SDL output remain shared.

Each new `(playback, visit)` supplies its annotated byte at its actual execution
timestamp. The level is held until the next visit or return, with no independent
sample clock. Repeated hardware writes for one visit do not select another byte;
adjacent equal bytes retain their separate visits. Partial lengths, inherited
interrupt parity, reverse order and external-memory boundary reads therefore
follow execution. The nominal 9,943.18 Hz cadence is a result of the timer
programming, not a second scheduler or the demo player's 10,000 Hz rate.

Speaker LOW/HIGH remain 0/8,192. Enhanced bytes map to
`(byte - 128) * 32 + 4096`, using a wide signed type: midpoint 128 is 4,096,
and the range is 0..8,160. This places both signals on the same unipolar scale
before the backend's high-pass filtering. No automatic loudness normalization
or clipping repair is applied in live output. The shared blip renderer retains
filter history across chunks and handoffs; its hold/filter response is not a
reconstruction of a Sound Blaster's analog filtering.

`SourceBegin` selects midpoint silence until the first visit. `SourceEnd`
selects midpoint again and suppresses the remaining sample-derived PWM pulse.
Device output continues to be tracked. Speaker rendering resumes on the next
deliberate channel-2 control/count write or a change to port `61h` bits 0/1;
channel-0 programming, counter latches and keyboard acknowledgements that only
change upper bits do not release that suppression. Another `SourceBegin` starts
a new sample interval. Handoffs are immediate level changes at their ordered
timestamps, smoothed only by the common synthesis filter; there is no crossfade,
invented device delay or phase reset. These are explicit enhancement policies,
not changes to the reconstructed sample routine.

### Bounded diagnostic

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
exporter. This exporter does not execute procedural gameplay sound; live viewing
uses the separate continuations described above.

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

Add `--wav` alongside `--device` to render the same execution into
`speaker.wav` and `enhanced-pcm.wav`, using separate instances of the shared replaceable
[signal renderer](../../../crates/reforge_audio/README.md#replaceable-signal-rendering).
The default is mono signed 16-bit PCM at 48,000 Hz; `--sample-rate HZ` selects a
different rate and `--audio-chunk-ticks TICKS` selects an internal synthesis
chunk bound (default 11,931 PIT clocks). Both accept positive decimal integers
and require `--wav`. The backend checks rate and chunk limits before creating
the output directory.

Both renderers use nominal `PIT_HZ = 1,193,182` and preserve filtering and
fractional phase across chunks and sample calls. The speaker WAV follows drive
edges; the enhanced WAV follows the annotated visits and handoff policy above.
Neither selects samples, changes interrupts or advances an independent cursor.
Both WAVs end at the same committed foreground
return, with no added timer interval or filter tail. `diagnostic.txt` records
their rate, gain and phase assumptions. The outputs are respectively a filtered
digital speaker signal and an enhanced interpretation of the eight-bit assets;
neither is a validated acoustic recording. Normal animated viewing uses the
same policies and synthesis backend for queued host playback.

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
