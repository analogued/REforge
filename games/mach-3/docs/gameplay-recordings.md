# Gameplay recording and comparison

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

Build the replay tool from the combined repository root:

```sh
cargo build --locked -p mach3 --release --bin mach3-replay
```

Build DOSBox-X Observer using its separate component checkout and an external working directory:

```sh
python3 /path/to/dosbox-x-observer/tools/build_emulator.py --work-dir /path/to/emulator-build
```

The shared component owns the generic observer, emulator adapter, and minimal
upstream patch. This repository supplies `capture/mach3.toml`, which defines the
game's checkpoints and memory layouts. Build requirements and upstream revision
are documented in that component. See [dependency notices](dependencies.md#capture-emulator).

## Record and compare

Run Python commands from `games/mach-3/`. Supply the replay executable from the
combined workspace's configured Cargo target directory; with Cargo's default
output path, it is `../../target/release/mach3-replay` from this game directory.

```sh
python3 tools/record_gameplay.py /path/to/game /path/to/new-recording \
  --observer-tool /path/to/dosbox-x-observer/tools/observer.py \
  --emulator /path/to/emulator-build/dosbox-x-observer \
  --replay-binary /path/to/target/release/mach3-replay --scale 8
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

`state-report/summary.json` separately reports differences in modeled gameplay
state, scene records, and drawing order. `state-report/comparisons.ndjson` retains
every comparable field and identifies skipped fields. Pixel equality and state
equality are separate results; inactive or stale fields can differ without changing
the display. Missing native fields remain explicitly unmodeled. The recorder
returns 1 if either comparison differs or replay is partial.

Keep recordings even after fixing a discrepancy. To compare one again with an
updated implementation, use new output directories:

```sh
/path/to/target/release/mach3-replay /path/to/game /path/to/recording/reference /path/to/new-replay
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
body, player, pause, and disabled-playback polls. Disabled playback tail-calls
the input helper, so these extra polls can change function-key preferences
without advancing gameplay. Older profiles label these polls `other`; replay
accepts such legacy rows only when all function keys are released.
`ends.tsv` records game termination, which the
replay also checks against its own game-over condition. Its legacy `quit` column
records the original exit word and does not distinguish death from a player quit.
Input positions use a wide sequence number plus the wrapping 16-bit game body
and pass number; wall-clock key timestamps are
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

## Diagnostic evidence

The profile records full 32-bit general registers, segment selectors, instruction
pointers, and materialized FLAGS/EFLAGS in `registers.tsv` at each framebuffer
checkpoint. The low halves retain the 16-bit register values.
`stack.tsv` indexes a 128-byte physical memory window from SS:SP in `stack.bin`
at each checkpoint; its address and length are explicit.
`state.tsv` records random state, script cursor/delay, movement, player, weapon,
score, fuel, and HUD fields. `objects.tsv` contains all 29 raw scene records and
resolved source pointers; `draw-order.tsv` records their full render order.
`data-segment.tsv` indexes complete 64-KiB snapshots in `data-segment.bin`, sampled
at initialization and ordinary pass 3. Raw registers, pointers, and memory are
diagnostic evidence, not replacement native state.

`decode_gameplay_state.py` uses the captured sprite pointer table to produce
`objects-normalized.tsv`. Null, unknown, and unavailable pointers remain distinct
from a known sprite ID. The comparator skips only the unresolved sprite ID for
such records and preserves the other fields. Rust exports its modeled state at
the same ordinary render checkpoint, before end-of-body updates, and at queued
transitions. It intentionally leaves absent model fields blank.

To compare state again after a fresh replay:

```sh
python3 tools/compare_gameplay_state.py /path/to/recording/reference \
  /path/to/new-replay /path/to/new-state-report
```

State comparison exits with 0 for equal comparable fields, 1 for differences or
missing rows, and 2 for invalid input. It does not compare CPU registers against
Rust, which does not emulate the CPU. Captured dumps and raw register state provide
context for investigating a discrepancy; they do not guarantee that every cause
can be diagnosed without additional targeted observations.
