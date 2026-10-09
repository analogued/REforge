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

Legacy recordings cover the first game from its initial scene through gameplay,
including three ordinary render passes, seven scene-transition draws, pauses,
F1 quits and F2/F3 polling. An optional ordered session timeline extends replay
through frozen ending titles, a returned intro and later games. Fresh-process
startup timing, audio synthesis and audio timing comparison remain deferred.
The CLI automatically uses the timeline when present. The library retains
`replay_first_game` and adds `replay_session` with optional externally loaded
`StartupFrames` for returned intro pixels.

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

## Ordered session timeline

`timeline.tsv` is optional. Its exact tab-separated header is:

```text
seq game body pass event site mask f1 f2 f3 last_scan frame_seq
```

`seq` is contiguous across all events. `frame_seq` is the next frame cursor for
input and lifecycle effects, or the corresponding `frames.tsv` index for a
`frame` event. Unused numeric fields are zero. Events preserve the original
interleaving rather than grouping all polls before a gameplay body:

- `control` samples F1/F2/F3 once at the classified helper entry.
- `input` carries the full returned mask at `player1` through `player3`, or
  the initializer `primer` poll. Only its low five bits steer gameplay; ordinary
  player diagnostics preserve the helper's high bits when no damage response
  replaces the input.
- `lifecycle-poll` carries a helper result at title release/approach/hold/departure,
  `intro`, or `intro-playback`. This result does not poll function keys again.
- `audio-clear` carries the raw `last_scan` from completed playback. Replay clears
  the keyboard table and the F2 counter low byte and applies the F2 disable rule
  through `SessionState::finish_audio_input`; disabled playback only tail-polls.
- `game-end`, `intro-entry`, `game-reset`, and `game-start` identify lifecycle
  boundaries. The reset event belongs to the upcoming game, at body/pass zero.
- `frame` sites are `initial`, `gameplay`, `transition`, `intro`,
  `ending-go-in`, `ending-go-out`, `ending-hs-in`, `ending-hs-overlay`, and
  `ending-hs-out`. Ending passes are zero-based draw ordinals within each game;
  `frames.tsv` uses the common kind `ending` for these sites.

The mandatory title pair writes 139 frames but takes 239 timer/stage steps.
Replay advances nondrawing holds from the modeled rules, and rejects a timeline
that would skip a framebuffer draw. Interruptible title polls prepare the next
native depth before source resolution and projection. A successful return
therefore retains the updated depth with the preceding source, rectangle and
world order. Titles also retain the native compositor's last background layer
and primary/alternate resource selection, including a black base with a retained
terrain strip. `Ending::returned_intro_choice`, `SessionState::remember_ending`,
and `SessionState::begin_game` derive the returned image choice and next game.
Only the first `games.tsv` row seeds simulation; later script/RNG rows are
assertions. Recorded world objects, draw order, state dumps and pixels never
replace native state. Returned intro pixels come from external DATA1/DATA6.

A recording can stop during gameplay, a transition, a title wait or prepared
poll, the returned intro, or reset before the next initial scene. Replay exports
exactly its displayed prefix. Extra recorded frames without timeline coverage
produce a partial diagnostic. Unknown nonneutral controls and any nonzero
initializer poll at CS:03d7 before INITIAL044a remain explicit unsupported cases.
An F3 toggle while native transition draws remain queued also stops explicitly:
`advance_subframe` may already have executed a later script palette write, so
its precedence relative to the interleaved BIOS mode reset requires phased
script execution. Previously displayed frames remain available in this case.
Fresh startup precedes this recording scope: when a reference
`session-state.tsv` exists, its first row must assert best score zero, mode 4,
sound enabled, F2 counter zero and intro audio enabled. Other initial preferences
produce a partial diagnostic; they are never silently copied into the session.
Legacy recordings without that table use this default baseline. Input and game
assertion tables may retain a longer prefix than the displayed stop boundary;
only polls and starts reached by the timeline enter replay and its assertions.

`session-state.tsv` exports per-frame RNG, best score, display mode, sound disable,
F2 poll counter and intro audio preference. Its header is:

```text
seq game body pass kind rng_a rng_b rng_c best_score display_mode sound_disabled sound_poll_counter intro_audio_enabled
```

`ending-state.tsv` exports title/stage/draw/hold/cycle, best score and RNG.
Ending and returned-intro frames also export the modeled 29 world records,
special extents/scale and complete draw order in the existing normalized tables.
Their gameplay `state.tsv` rows retain frame identity and modeled RNG, leaving
scalars absent from the ending model blank. Blank fields continue to identify
absent model values.

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
