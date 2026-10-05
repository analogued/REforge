# Alley Cat

An early Rust reconstruction of **Alley Cat** for modern desktop systems,
initially targeting Linux. The current program draws the alley and supports
walking, jumping, and landing on its platforms. It is not a complete game.
A public recreation name has not been chosen.

## Original game required

The program requires your original `CAT.EXE` at runtime. It reads embedded
sprites, scenery, animation tables, and platform data from that file. Rust
implements the reconstructed behavior; it does not execute the DOS program.
Original game data is not included in this repository or embedded in builds.

Currently one executable is supported, identified by its exact bytes rather
than an unverified historical release name:

```text
Size:    55067 bytes
SHA-256: 4979c8867826e08881d1d4bec95c95d6bc1ac70fd21050ccdb4f733a50d7bfdd
```

The loader opens the file read-only and rejects other versions. Keep original
files and any generated captures outside this repository.

## Build and run

Install Rust 1.85 or newer with Cargo, and an SDL3 development package providing
the native `SDL3` library. The window backend was checked against SDL 3.4.16 on
Linux. It uses a small authored C interface; no Rust SDL binding crate or SDL
image/font/mixer extension is required. Cargo fetches `sha2` and its locked
dependencies for executable verification.

From this repository:

```sh
cargo run --locked --release -- --original /path/to/CAT.EXE
```

Use the arrow keys to move and jump. Escape or closing the window exits.
`--machine pcjr` selects the PCjr palette and player countdown; the default is
`cga`. Both modes use the same packed 320×200, four-colour framebuffer. Pixels
have a 5:6 width-to-height ratio: the initial 960×720 window displays them at
4:3, with nearest-neighbor sampling and no CRT shader. Resizing or maximizing
preserves 4:3 with black bars as needed. Host display vsync is requested when
supported; game timing still follows the virtual PIT/video clocks. Composite
monitor simulation is not implemented. Raw framebuffer and PPM exports remain
320×200 for data comparisons.

`--seed` selects a nonzero 16-bit random seed, in decimal or `0xHEX` notation.
`--difficulty` accepts 0 through 7 and affects the initial alley layout.
These controls are diagnostic setup options; startup and difficulty selection
screens are not implemented.

## Current behavior

The implemented slice includes seeded alley initialization, CGA bank addressing,
sprite composition and background restoration, and player movement on the floor,
dumpsters, and occupied upper platforms. Scenery remains static after setup.
The player starts settled on the floor, bypassing the original entry animation.
Falling behind the fence hides the cat and starts a clipped re-entry from the
nearer alley edge. Its speaker effect is silent. The timed runner blocks game
work until seven BIOS ticks have elapsed and advances the effect's shared random
state through its original call-count algorithm, using approximate loop costs.

Actors and hazards, moving scenery, window entry, rooms, scoring, menus, and
audio are absent. Scene entry and collision-effect paths beyond this slice remain
unsupported. Windowed execution uses a virtual BIOS/PIT clock, two-tick control
polling, the original 32/16-call player countdown, and a retrace gate for idle
movement. The reference is DOSBox-X normal CPU core at fixed 240 cycles. Loop
costs are fitted approximations; original-PC timing fidelity is not established.
Rendering refresh does not determine game ticks. See
[timing contracts](docs/timing.md) for the schedule and its limits.

`--step-hz 36.413` explicitly selects the earlier evenly spaced diagnostic
accepted-update mode. It bypasses the original player timing gates.

## Headless use

A headless build needs no SDL library:

```sh
cargo run --locked --no-default-features -- \
  --original /path/to/CAT.EXE --headless --steps 60
```

`--script /path/to/controls.csv` supplies one accepted update per row, with
columns `x,y`. Each value is -1, 0, or 1: negative is left/up, positive is
right/down. Blank lines, `#` comments, and an optional `x,y` header are accepted.
Without `--steps`, a script runs for its row count; additional requested updates
use neutral controls.

Add `--timed` to execute virtual loop attempts, input polls and rejected player
calls without wall-clock waits. Script rows still select held directions by
accepted-update index; they are not timestamped hardware input recordings.
`--trace-timing /path/to/timing.csv` records guest times, BIOS/PIT samples,
retrace, polling, acceptance and actor-divider events (and fence wait checks).
In headless runs it requires `--timed`; in windowed runs it uses default timing.

`--trace`, `--dump-cga`, and `--dump-ppm` write player-state CSV, the final
16384-byte packed framebuffer, and a 320×200 PPM image respectively. Use paths
outside the repository: these outputs contain original graphics or reference
evidence. Output files must not already exist. See `--help` for all options.

## Development and licensing

See [CONTRIBUTING.md](CONTRIBUTING.md) for validation, formatting, and commit
conventions, and [the analysis tool](tools/README.md) for read-only executable
reconnaissance. Development does not require a particular parent workspace.

The project's license has not been selected; no project license is granted yet.
Dependency attribution and license notices are in
[THIRD_PARTY.md](THIRD_PARTY.md). Those notices do not grant rights to the
original game data.
