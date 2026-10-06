# Gameplay recording and graphical comparison

The recorder lets a manually played original game become a repeatable graphics
test. It observes the original's sampled controls and each of its three CGA
render passes, then replays those controls through the Rust implementation.
Comparison uses all visible framebuffer pixels, palette selection, and display
mode. It includes the seven expanding draws of scene transitions.

Recording currently covers the first game, from its initial scene until game
over or window close. Pauses, F1 quits, and F2/F3 polling are recorded. The
post-game title sequence, subsequent games within a recording, startup timing,
and audio comparison are outside this recording format's current replay scope.
Use Space to advance startup normally before playing.

## Setup

Build the replay tool:

```sh
cargo build --locked --release --bin mach3-replay
```

Build a separate capture-enabled DOSBox-X in an external working directory:

```sh
python3 tools/build_capture_emulator.py --work-dir /path/to/emulator-build
```

The build script fetches its pinned upstream revision and adds the authored
observer to the normal CPU core. It requires Git, a C++ compiler, Make,
Autoconf/Automake, pkg-config, SDL2 development libraries, and ncurses development
libraries, and OpenGL development headers. The capture window uses nearest
OpenGL scaling to honor the requested size. It keeps the upstream source,
license, build logs, and binary outside
this repository. See [dependency notices](dependencies.md#capture-emulator).

## Record and compare

```sh
python3 tools/record_gameplay.py /path/to/game /path/to/new-recording \
  --emulator /path/to/emulator-build/upstream/src/dosbox-x \
  --replay-binary target/release/mach3-replay --scale 8
```

The supplied game directory needs the executable and game resources. The tool
copies them into the recording's own working drive; only that copy is mounted.
The recording directory must be new and should remain outside the source
repository. Normal play runs at fixed 3,000 cycles on the normal CPU core;
`--cycles N` changes that setting, and `--scale 1..16` changes the window size.

Game over closes the capture window automatically. Closing it during gameplay
also finishes recording. The tool then replays the captured input in Rust and
writes `report/index.html`. A divergence report includes the original image,
the recreation image, and a magenta pixel-difference image at the first mismatch,
displayed at 4:3. It also records missing frames and differing palette or display
mode. `report/mismatches.ndjson` lists all mismatching checkpoints. A partially
completed native replay remains available if it ends early or encounters an
unimplemented path.

Keep recordings even after fixing a discrepancy. To compare one again with an
updated implementation, use new output directories:

```sh
target/release/mach3-replay /path/to/game /path/to/recording/reference /path/to/new-replay
python3 tools/compare_gameplay.py /path/to/recording/reference /path/to/new-replay \
  /path/to/new-report --details
```

The comparator exits with 0 for an exact match, 1 for divergence, or 2 for
invalid input. The replay tool uses the same distinction for successful replay,
a partial replay, and an invalid recording or tool error.

## Capture format and synchronization

`games.tsv` supplies the initial script and three random-state words.
`inputs.tsv` records the original's action mask at each player poll, before
damage responses alter it. `controls.tsv` records function-key state at the
body, player, and pause polls. `ends.tsv` records game termination, which the
replay also checks against its own game-over condition. Input positions use a wide sequence number plus
the wrapping 16-bit game body and pass number; wall-clock key timestamps are
not used to align execution.

`frames.tsv` indexes `frames.bin`, which contains one 16,000-byte linear CGA
frame per row. The index records sequence, game, body, pass, frame kind, palette
family, and display mode. Normal passes are 1 through 3; transition draws are
1 through 7. Color pixels are 320 by 200; monochrome interprets the same bytes
as 640 by 200 one-bit pixels. Unused CGA bank padding is excluded.

The native replay reads frame metadata only to identify where a partial
recording stopped. It does not read reference pixels, load recorded object
state, or replace random state during execution. Its palette comes from its
own encounter algorithms, and its display mode comes from replayed controls.
This preserves discrepancies for the comparator to detect.

The observer reads guest state without adding guest instructions or stopping
at debugger breakpoints. Host recording work can still affect wall-clock pace;
comparison establishes logical update and rendering fidelity, not exact DOS
CPU-dependent timing or monitor scanout behavior.
