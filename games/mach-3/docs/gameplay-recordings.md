# Gameplay recordings and native replay

The Rust replay tool consumes an externally supplied recording of the original's
sampled controls and render checkpoints. From the combined repository root:

```sh
cargo build --locked -p mach3 --release --bin mach3-replay
/path/to/target/release/mach3-replay /path/to/game /path/to/recording/reference /path/to/new-replay
```

The game directory must contain the original resources. The replay output directory
must be new and outside the source and recording inputs. The tool returns 0 for a
complete replay, 1 for differences or partial replay, and 2 for invalid input or
tool errors. It exports graphics and modeled state for external comparison.
Capture tooling and game-specific emulator profiles are maintained separately;
a standalone source clone does not require them to build or test.

Recording replay currently covers the first game, from its initial scene until
game over or window close, including three ordinary render passes and seven
scene-transition draws. Pauses, F1 quits and F2/F3 polling are represented.
The post-game title sequence, subsequent games within one recording, startup
timing and audio comparison remain outside this recording format's replay scope.

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

`objects-normalized.tsv` augments captured records with sprite IDs resolved
from the captured sprite pointer table. Null, unknown, and unavailable pointers
remain distinct from a known sprite ID. The comparator skips only the unresolved sprite ID for
such records and preserves the other fields. Rust exports its modeled state at
the same ordinary render checkpoint, before end-of-body updates, and at queued
transitions. It intentionally leaves absent model fields blank.

State comparisons are external diagnostics. The native replay exports modeled
state for comparison, including explicit gaps in its current model. CPU registers
are not compared against Rust, which does not emulate the CPU. Captured dumps and raw register state provide
context for investigating a discrepancy; they do not guarantee that every cause
can be diagnosed without additional targeted observations.
