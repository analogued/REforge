# Shared PIT and PC speaker devices

`reforge_audio` provides deterministic device execution and optional offline
signal rendering. Its default device implementation has no dependencies and
does not execute game routines.
Engines own sample selection, handlers, software counters and foreground waits.
Using or discarding its output events leaves device execution identical.

## Supported device subset

`pit::Pit` models three binary Intel **8254** counters in modes 0–3. Control words,
low/high byte accesses, count latching, gate levels, reload values and counting
elements are retained separately. Zero encodes 65,536. Loads take a clock without
decrementing; writes can reset a pulse or defer a periodic reload. Mode 1 needs a
rising gate trigger. Mode 3 uses the 8254's odd-count read sequence, which differs
from the older 8253's. These rules follow Intel's
[8254 datasheet](https://www.scs.stanford.edu/23wi-cs212/pintos/specs/8254.pdf),
particularly the mode diagrams and definitions on PDF pages 10–17.

`speaker::PcSpeaker` connects channel 2's gate to port `61h` bit 0. The digital
drive is channel 2 OUT AND bit 1, independently of the gate level. IBM documents
that wiring in its [PC Technical Reference](https://www.minuszerodegrees.net/manuals/IBM_5150_Technical_Reference_6322507_APR84.pdf),
section 1-25. Drive events describe the logic before the speaker driver and
physical response; they are not pressure samples or synthesized PCM.

## Clock and ordering

Timestamps are completed integer PIT clock pulses, using explicit nominal
frequency `PIT_HZ = 1_193_182` for conversions outside the device. An operation
at time `t` follows that pulse; a pending load is processed at `t+1`. Multiple
writes at one timestamp execute in caller order and remain separate operations.
Clock transitions precede caller writes at the same timestamp. Simultaneous
channel edges are returned in channel-number order.

`Pit::new(gates, outputs)` requires explicit initial pin levels and starts with
unprogrammed channels. `PcSpeaker::new(control, outputs)` ties gates 0/1 high and
derives gate 2 from the supplied control byte. These constructors do not claim
to reproduce power-on state or a particular BIOS phase. Reads without a loaded
count return an error; a completed count-register write need not yet be visible
in the counting element.

`next_deadline()` reports the earliest pending load or output transition.
`advance_to(target)` advances **at most to that deadline**, returning a `Step`
or speaker `Events`. A load can return no edges. The caller handles any returned
interrupt before continuing toward the target, allowing its handler to reprogram
the device. Advancing a long interval does not collect interrupts to execute
later. Counter changes between deadlines are computed arithmetically rather
than one host iteration per PIT clock.

Port writes and gate changes return immediate output/drive edges. The device
does not queue interrupts, install handlers, interpret an EOI, impose CPU work
budgets, or advance from audio-buffer requests. Those are machine/scheduler
responsibilities. It has no host pause or mute switch: freezing simulation time
freezes phase; suppressing presentation does not suppress device execution.
A sample routine returning does not itself disable speaker output or reset
channel 2.

## Limits and validation

BCD, modes 4/5 and 8254 read-back commands return explicit errors. Ordinary
construction rejects count 1 in modes 2/3. Intel specifies a minimum of 2;
count 1 therefore has no guaranteed device semantics. An explicit
`CountOnePolicy::DosBoxWrap`, selected through `with_mode3_count_one`, permits
mode 3 count 1 with an effective period of 65,537 clocks. This follows
[DOSBox-X's compatibility rule](https://github.com/joncampbell123/dosbox-x/blob/478c860050b20afdb58bce28f1b43163952fc541/src/hardware/timer.cpp#L622),
which Mach 3 needs for entries in its original tone tables. The raw programmed
count remains 1; this integer-clock model uses 32,769 high clocks and 32,768 low
clocks, whereas DOSBox-X splits its period into floating-point half-periods.
This is an emulator compatibility choice, not a claim about unspecified original
hardware behavior. Mode 2 count 1 remains rejected under either policy.

A new latch between incomplete live low/high reads is rejected;
repeated commands during an existing unread latch are ignored. Invalid supported
operations leave state unchanged. Unsupported behavior is not silently decoded
as another mode.

Port `61h` retains all written bits, but only bits 0/1 affect this circuit.
Refresh, parity, keyboard acknowledgement hardware, AT status inputs, PPI/PIC
emulation and analogue response are outside this subset. A machine adapter must
provide those other read inputs where required. Clock setup/hold apertures and
sub-clock instruction timing are not modeled; real gate races near clock edges
are outside the declared timing convention. There is no 8253 odd-count variant,
physical speaker-response model, audio device or host buffer here yet.

Run `cargo test --locked -p reforge_audio` and
`cargo clippy --locked -p reforge_audio --all-targets -- -D warnings` from the
workspace root. Synthetic tests check mode sequences, repeated PWM writes,
partial loads, counter wrapping, latches, gate behavior, interrupt-boundary
reprogramming and invariance under different advancement chunks and speaker
enable states. They establish the declared digital subset; they do not establish
acoustic fidelity or emulator boot-state equivalence.

## Replaceable signal rendering

`render::SignalRenderer` accepts ordered level changes at absolute integer
source-clock timestamps through `level_at`, then commits output through
`finish_until`. A callback receives bounded blocks of mono signed 16-bit PCM.
The caller may write, queue or discard them. Rendering observes execution;
it does not advance devices, run handlers or choose game samples. Hardware
writes, including repeated count writes, remain the responsibility of the
simulation even when they produce no drive change.

The optional `blip` feature exposes `BlipRenderer`, backed by the pinned Rust
`blip_buf` dependency. `Config` supplies explicit source-clock frequency, output
sample rate and maximum chunk length in source ticks. Output rates must be no
higher than the source frequency; their ratio and chunk size are checked against
the backend's arithmetic and 4,000-sample frame limit. The adapter conservatively
permits fewer than 4,000 generated samples per internal chunk. It automatically
splits longer intervals, so gameplay bodies need not fit one synthesis frame.
For example, `PIT_HZ`, 48,000 Hz and 11,931 ticks produce approximately 10 ms
chunks, independent of game-update boundaries.

A renderer starts at an explicit epoch and initial level, with zero prior filter
history. Levels range from −8,192 to +8,192 for synthesis headroom. Mapping
speaker LOW/HIGH to 0/8,192 is a presentation gain choice, not a voltage or
acoustic calibration. Same-tick changes retain caller order; a change at a chunk
endpoint belongs at offset zero of the next chunk. Past timestamps and invalid
levels are rejected before mutation. Output or backend errors permanently stop
that renderer, since an output consumer may already have accepted part of a block.

Chunk boundaries preserve fractional clock conversion and filter history;
`clear()` is used only at initialization. The backend uses fixed-point conversion
with a rounded rate factor, band-limited steps, a high-pass filter and signed
16-bit output. Its synthesis kernel delays the signal by approximately eight
output samples. This is a defined digital rendering path, not a measured physical
speaker response. Finishing at a horizon emits only its committed interval;
it does not invent later device time to flush the filter tail. A different
renderer can receive the same timed levels before this backend's filtering.

Host pause should freeze the simulation timeline and retain renderer state.
Mute or disabled host output must still execute devices and handlers; callers
can render and discard PCM to retain synthesis history. This crate does not
choose host pause, buffering or underrun policies.

`wav::WavWriter` streams mono PCM into a seekable RIFF/WAVE destination and
patches its length on explicit `finish`. It rejects sizes beyond the 32-bit
RIFF limit and does not synthesize or extend time.

Also run `cargo test --locked -p reforge_audio --features blip` and
`cargo clippy --locked -p reforge_audio --all-targets --features blip -- -D warnings`.
Synthetic tests cover PCM equality across internal/external chunk partitions,
several output rates, long silent intervals, repeated PWM operations, rendering
near the timestamp limit, output failures, level headroom and WAV boundaries.
Device results remain identical with different output rates.

Authored code follows the repository-wide license decision, which remains
undecided. See [dependency notices](docs/dependencies.md) for the optional
renderer dependency and its retained terms.
