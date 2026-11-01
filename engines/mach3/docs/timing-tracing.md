# Native timing diagnostics

Build with `dev-tools` and the default `sdl` feature, then supply
`--trace-timing /path/to/new-timing.tsv` to the standalone `mach-3` executable
or REforge. The normal standalone executable rejects this option; normal
REforge builds warn and ignore it. It can run alongside `--trace-input` using
a separate destination. Files are created exclusively, never truncated; write
and flush errors stop the run.

The trace observes the existing scheduler and renderer. It does not change the
body period, catch-up limit, input polling, scene algorithms or deliberate waits.
Tracing adds clock sampling and buffered file output, so measured host durations
include instrumentation overhead. It performs no framebuffer readback.

Normal animated viewing emits `simulation` spans around each bounded PIT budget.
Their detail contains `committed_tick` and `wanted_tick`; gameplay fields describe
the state after that budget and are blank outside gameplay. These spans include
guest execution and PCM servicing, and exclude SDL presentation. The nominal
`body_period_ns` metadata does not assign durations to reconstructed timer waits.
The per-pass, publication and presentation row labels below describe the silent
step/static scheduler; they are not emitted by the live PIT path.

The first line identifies `reforge-mach3-timing-v1`; the following TSV columns are:

```text
seq game kind body pass scheduled_ns timeline_ns host_start_ns host_end_ns body_period_ns script rng_a rng_b rng_c background_layer alternate terrain_selector primary_count difficulty enemy_active special_active detail
```

Times are signed nanoseconds from one monotonic host origin. `scheduled_ns` and
`timeline_ns` describe the native scheduler's pause-adjusted clock.
`host_start_ns` and `host_end_ns` bracket actual work on the unadjusted host clock.
Subtract host endpoints for execution cost; subtract scheduled from timeline for
scheduler lateness at the batch observation. The scheduler samples its virtual
clock once per batch, so catch-up passes share `timeline_ns` while each has its
own actual host endpoints. Do not subtract host timestamps from virtual deadlines
across host pauses. These clocks are not the original DOS PIT or emulated CPU time.

Rows are bounded by actual work and visible wait advancement:

- `game_start` brackets initialization and assigns game IDs starting at zero.
  Its script/RNG describe the native start; IDs increment after returned intros.
- `pass` brackets the actual `Gameplay::advance_subframe` invocation. This includes
  world updates, rendering, parser/recycle work and generation of transition
  images when applicable, but excludes input tracing and SDL presentation.
  Its body/pass and workload columns describe the state after the invocation.
  It records preparation, which may precede pass-one publication by one timer
  unit; it does not claim that this image is already visible.
- `pass_publish` records publication of a prepared ordinary image. Its deadline
  is the scheduled publication time; `delayed=true` identifies the separate
  pass-one wait. This event does not poll input or update gameplay. Publication
  exposes the indexed image to presentation; it does not measure monitor scanout.
- `transition_wait` identifies advancement after an intentional transition
  deadline; its short host span measures queue advancement, not the wait itself.
- `ending_start` brackets title initialization; `ending_step` brackets a title
  draw or nondrawing hold step. Detail records title/stage, draw/hold position,
  previous wait units and the next wait units.
- `present` brackets RGB conversion, texture upload, caption update and SDL
  presentation. It can include renderer/vsync blocking. Scheduled time is blank;
  detail identifies the screen and whether a transition, ending or host pause
  is active. Startup presentations have blank game and scene metadata.

One catch-up batch can execute up to 64 ordinary scheduled events before presenting.
Intentional title and transition waits keep their existing behavior and do not
spend ordinary catch-up debt. Thus pass rows measure simulation work, while
presentation rows measure separate display work. Neither claims monitor scanout
or physical input latency. Idle event-poll loops emit no timing rows.

Scene selection, script, difficulty, active counts and RNG help identify matching
workloads. They do not establish equivalence to an original capture by themselves;
compare game/session populations explicitly before drawing pacing conclusions.
All timing data is observational and never enters gameplay as a replay input.
