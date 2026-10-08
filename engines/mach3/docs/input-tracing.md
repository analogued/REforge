# Input tracing

The optional `dev-tools` feature adds `--trace-input PATH` to Mach 3's viewer.
It records host events and actual reconstructed control polls without changing
input processing, game clocks, keyboard priorities or native pause rules.
REforge forwards its `dev-tools` feature to the engine:

```sh
cargo run --locked --release -p reforge --features dev-tools -- \
  --original /path/to/mach3-data --controls --trace-input /path/to/new-trace.tsv
```

Use a new output filename and an existing parent directory. The file is created
exclusively; an existing capture is never overwritten. Write/flush failures are
reported. Output is buffered and flushed between event/catch-up batches and on
normal exit. Tracing adds host CPU and I/O work, so compare traced runs with
untraced runs when measuring responsiveness. Normal builds do not enable this
option and omit the engine recorder; optimization and `dev-tools` are independent.

## Records and clocks

Version 1 is UTF-8 tab-separated text. The first line identifies
`reforge-mach3-input-v1`; the second lists the columns. Rows have a monotonically
increasing sequence number. Blank fields mean “not applicable.” Times use
nanoseconds; reporting nanoseconds does not promise nanosecond precision.

| Column | Meaning |
| --- | --- |
| `sequence` | Recorder order, starting at 1. |
| `kind`, `label` | `event`, `poll` or `boundary`, with the site/event name. |
| `host_ns` | Processing sample, relative to the trace's host `Instant` origin. |
| `timeline_ns` | Engine scheduling time in the same origin, excluding host pauses. Poll rows use the current batch's scheduling sample, rather than its nominal deadline. |
| `sdl_ns` | SDL's event-clock reading near the host sample; its epoch is SDL initialization. |
| `clock_span_ns` | Host time between the two samples bracketing that SDL reading. |
| `event_sdl_ns` | Unmodified SDL event timestamp, in SDL's nanosecond clock. |
| `deadline_ns` | Poll's ordinary-pass deadline or the boundary's new deadline, relative to the host origin in the pause-adjusted timeline. |
| `actions` | Observed keyboard actions: Up=1, Down=2, Left=4, Right=8, Fire=16. |
| `functions` | Observed function-key bits: F1=1, F2=2, F3=4. |
| `result_mask` | Actual result of `Controls::poll`, including its F1/F2/F3 priorities. |
| `detail` | Named fields describing event identity, polling side effects or boundary state. |

`host_ns` is sampled after event handling, or after the control poll and snapshot
read. The SDL clock reading lies between `host_ns` and
`host_ns + clock_span_ns`. For an event, a local estimate of host time is
`host_ns + event_sdl_ns - sdl_ns`, with the measured sampling bracket and any
clock/platform uncertainty. The raw values are retained so analysis can choose
its correlation method. Event timestamps describe SDL/platform delivery, not a
measurement of the physical keyboard switch. SDL pumping may also affect when
those timestamps are assigned.

Host pause boundaries record both host and frozen/unfrozen timeline samples.
Do not compare an SDL timestamp directly with an engine deadline or subtract one
constant pause offset across a pause boundary. Native F1 waits and displayed
transition/ending waits have their own scheduling policy.

## Events and actual polls

All delivered key-down/up events are recorded, including unmapped keys, modifiers,
F1/F2/F3 and OS repeats. Rows retain layout-aware keycodes, physical scancodes,
modifier bits and repeat status. The held columns show the resulting DOS keyboard
table after frontend handling; a delivered event need not change that table.
Host shortcuts and events ignored during host pause are identified explicitly.
Focus/window events and quit events are also recorded. Desktop shortcuts consumed
before SDL receives them cannot be captured.

Poll labels correspond to the production call sites:

- `body_entry`: the native pause gate at a body boundary.
- `steering`: the input read before each actual render/simulation pass.
- `native_pause`: the release/press/decision polls during the F1 handshake.

Poll details include the **pre-advance** body/pass state, whether player controls
are enabled, native pause phase, F2 counter and sound/display toggle results.
With `--controls` absent, polls still occur but steering is suppressed. Native
pause polls have no ordinary-pass deadline; their polling cadence follows the
frontend's pause loop. Step-mode polls likewise do not claim an animated deadline.

Boundary labels include startup, game entry, return to intro, host pause,
native pause phases/ticks/waits/resume, transition entry and intermediate waits,
transition completion, ending waits, manual step waits and preview completion.
Rebase rows include `old_deadline_ns` in `detail` and the new deadline in the
column. Ordinary catch-up retains its deadlines; this engine has no ordinary
100 ms discarded-time clamp. `batch` records identify scheduling samples between
event pumps, including batches that perform no control poll.

## Temporal comparisons

Use actual poll deadlines as the reference. Under a delayed event pump:

| Input around a poll at P | State expected at P |
| --- | --- |
| Press before P, release before P | Released; missing the tap can be faithful. |
| Press before P, release after P | Held, even if processing occurs after release. |
| Press after P | Released at P, even if the press is processed before overdue work. |

The current frontend drains queued events before overdue work. These traces
observe that behavior; they do not replay timestamped input or retain every tap.
Synthetic tests cover all three histories through the real scheduling/poll path,
plus equivalence of traced/untraced catch-up, F-key effects and deliberate waits.
Those tests establish recorder placement and the host-sampling baseline, rather
than original DOS input timing or physical keyboard latency. Native captures and
original-program comparisons remain external evidence.
