# Shared PIT and PC speaker devices

`reforge_audio` provides deterministic device execution before signal rendering
or host audio output. It has no dependencies and does not execute game routines.
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

BCD, modes 4/5 and 8254 read-back commands return explicit errors. Modes 2/3
reject count 1. A new latch between incomplete live low/high reads is rejected;
repeated commands during an existing unread latch are ignored. Invalid supported
operations leave state unchanged. Unsupported behavior is not silently decoded
as another mode.

Port `61h` retains all written bits, but only bits 0/1 affect this circuit.
Refresh, parity, keyboard acknowledgement hardware, AT status inputs, PPI/PIC
emulation and analogue response are outside this subset. A machine adapter must
provide those other read inputs where required. Clock setup/hold apertures and
sub-clock instruction timing are not modeled; real gate races near clock edges
are outside the declared timing convention. There is no 8253 odd-count variant,
PCM renderer, audio device, or host buffer here yet.

Run `cargo test --locked -p reforge_audio` and
`cargo clippy --locked -p reforge_audio --all-targets -- -D warnings` from the
workspace root. Synthetic tests check mode sequences, repeated PWM writes,
partial loads, counter wrapping, latches, gate behavior, interrupt-boundary
reprogramming and invariance under different advancement chunks and speaker
enable states. They establish the declared digital subset; they do not establish
acoustic fidelity or emulator boot-state equivalence.

Authored code follows the repository-wide license decision, which remains
undecided. No third-party implementation code is included.
