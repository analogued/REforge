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

Use the arrow keys to move and jump. Esc, P and Pause/Break pause the game;
press any fresh key to resume. Ctrl+Q or closing the window exits, including
while paused. Held-key repeats do not resume play.

Pause preserves the game state and virtual clock without catching up on resume.
The overlay reads the original pause captions from the executable and uses their
original placement and palette color. Its lettering currently uses SDL's built-in
font rather than the original BIOS font.

`--machine pcjr` selects the PCjr palette and player countdown; the default is
`cga`. Both modes use the same packed 320×200, four-colour framebuffer. Pixels
have a 5:6 width-to-height ratio: the initial 1280×960 window displays them at
4:3, with nearest-neighbor sampling and no CRT shader. `--scale N` selects an
initial window size of `320*N` × `240*N`, with a default multiplier of 4 and
accepted values from 1 through 16. Use `--scale 3` for 960×720.
Resizing or maximizing preserves 4:3 with black bars as needed.
Host display vsync is requested when
supported; game timing still follows the virtual PIT/video clocks. Composite
monitor simulation is not implemented. Raw framebuffer and PPM exports remain
320×200 for data comparisons.

`--seed` selects a nonzero 16-bit random seed, in decimal or `0xHEX` notation.
The default seed is `0xfa59`.
`--difficulty` accepts 0 through 7 and affects the initial alley layout.
These controls are diagnostic setup options; startup and difficulty selection
screens are not implemented.

## Current behavior

The implemented slice includes seeded alley initialization, CGA bank addressing,
sprite composition and background restoration, and player movement on the floor,
dumpsters, and occupied upper platforms. Ground decorations use the original's
weighted random selection and external sprites. Current score, high score and
three starting lives use the original digit glyphs. Mouse catches award points;
life loss and other scoring events are not implemented.

In the default timing mode, the clotheslines scroll right/left/right, generate
new garments, update landing supports, and carry an attached cat. Their original
call countdown, BIOS-tick and retrace gates control motion; scrolling pauses when
the cat is airborne in the relevant row. Releasing a cat at a clothesline edge
starts a fall without losing a life. The renderer composes complete scenery and
cat images rather than simulating partially updated display memory.

The bin cat emerges from randomly selected bins and pushes a supported player
off on contact. It also sets the original dog-alert state; the dog remains absent.
Three clothesline mice move and animate using the shared random generator and
their own BIOS-tick gate. Contact knocks an attached cat off its line, with the
original eight-tick pause (about 0.44 seconds). An airborne cat instead catches
the mouse for 90, 70 or 50 points on the top, middle or bottom line. Caught mice
respawn after 54 ticks (about three seconds). Neither interaction removes a life.
Actor sprites and motion tables come from the required original executable.

The player starts settled on the floor, bypassing the original entry animation.
Falling behind the fence hides the cat and starts a clipped re-entry from the
nearer alley edge. Its speaker effect is silent. The timed runner blocks game
work until seven BIOS ticks have elapsed and advances the effect's shared random
state through its original call-count algorithm, using approximate loop costs.

The dog, window projectiles, window entry, rooms, menus, and audio are absent.
Scene entry and collision-effect paths beyond this slice remain
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
