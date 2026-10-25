# Input tracing

The optional `dev-tools` feature adds `--trace-input PATH` to Mach 3's viewer.
It records host events and actual reconstructed control polls without changing
input processing, game clocks, keyboard priorities or native pause rules.
REforge forwards its `dev-tools` feature to the engine:

```sh
cargo run --locked --release -p reforge --features dev-tools -- \
  --original /path/to/mach3-data --trace-input /path/to/new-trace.tsv
```

Use a new output filename and an existing parent directory. The file is created
exclusively; an existing capture is never overwritten. Write/flush failures are
reported. Output is buffered and flushed between event/catch-up batches and on
normal exit. Tracing adds host CPU and I/O work, so compare traced runs with
untraced runs when measuring responsiveness. Normal builds do not enable this
option and omit the engine recorder; optimization and `dev-tools` are independent.

The startup boundary records `f3_mode=press` or `f3_mode=original` in `detail`.
Interactive viewing defaults to one F3 toggle per press. This intentional
frontend convenience also applies when tracing; the recorder does not change
the selected policy. Use `--f3-mode original` for physical-input fidelity
comparisons requiring the original toggle at every active poll. Original-game
recording replays retain per-poll F3 behavior independently of this viewer option.

Normal animated viewing schedules raw scans on its PIT timeline. Gameplay
observations use `boundary` rows labeled `pit_event`, with the exact simulation
`tick` and `GameplayEvent` in `detail`. An `InputPoll` event carries its site and
result mask there. Ending entry and optional animation polls use the same event
format with their reconstructed instruction sites. The row's held-key columns
describe state at the completed budget (`state_at_budget=true`), not a snapshot
at that earlier event. Intro and pre-game sample handlers do not emit gameplay
poll rows. `TransitionPublished` events identify each live reveal region at its
simulation tick; the intervening intervals add no foreground control polls.
Ending sample selection emits `PlaySample` actions, and native sound-disable
bypass emits an ordinary `73b2` poll. The named `poll` labels
and nominal deadlines below describe the silent step/static path.

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
| `actions` | Currently consumed DOS keyboard actions: Up=1, Down=2, Left=4, Right=8, Fire=16. |
| `functions` | Currently consumed function-key bits: F1=1, F2=2, F3=4. |
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

Silent-preview host pause boundaries record both host and frozen/unfrozen timeline samples.
Do not compare an SDL timestamp directly with an engine deadline or subtract one
constant pause offset across a pause boundary. Native F1 waits and displayed
transition/ending waits have their own scheduling policy.

## Events and actual polls

All delivered key-down/up events are recorded, including unmapped keys, modifiers,
F1/F2/F3 and OS repeats. Rows retain layout-aware keycodes, physical scancodes,
modifier bits and repeat status. The held columns show the consumed DOS keyboard
table after frontend handling. A `queued` event has not yet changed that table;
its `effective_input_ns` detail gives its correlated event time on the pause-adjusted
timeline. Silent previews include their ordering/prospective-delivery clamp;
the live PIT path additionally clamps late scans to its committed tick during
delivery, which is not separately recorded in that event row. This value comes
from the input mapper, whose clock sample precedes the recorder's separate sample.
Host shortcuts and events ignored during host pause are identified explicitly.
Focus/window events and quit events are also recorded. Desktop shortcuts consumed
before SDL receives them cannot be captured.

Poll labels correspond to the production call sites:

- `body_entry`: the native pause gate at a body boundary.
- `steering`: the input read before each actual render/simulation pass.
- `native_pause`: the release/press/decision polls during the F1 handshake.

Poll details include the **pre-advance** body/pass state, whether player controls
are enabled, native pause phase, F2 counter and sound/display toggle results.
Player controls are enabled by default. Native pause polls have no ordinary-pass
deadline; their polling cadence follows the frontend's pause loop. Step-mode polls likewise do not claim an animated deadline.

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

Normal animation retains received make/break bytes and delivers them to the
ordinary or temporary sample keyboard IRQ at their PIT ticks before foreground
polling. IRQ0 keeps priority over IRQ1 at equal times. Silent nominal previews
apply eligible transitions before each ordinary pass's scheduled poll. Body entry
and its first steering
poll share that deadline; both retain their existing control processing. Scans
pass through the reconstructed DOS keyboard handler, including duplicate-byte
filtering, rather than restoring snapshots of held keys. F-key priorities,
counters and palette effects remain at their original poll sites.

The input mapper correlates SDL timestamps using the midpoint of a measured
host/SDL sampling bracket, then removes completed host-pause time. Unknown zero,
future or unrepresentable timestamps use receipt time; estimates never exceed
receipt time. Equal or nonmonotonic timestamps keep receipt order. Transitions
dated before an already executed poll become eligible at the next unexecuted
poll. There is no rollback, added input latency or guarantee that SDL timestamps
measure physical key changes accurately.

## History boundaries

P or Pause/Break toggles modern host pause on a fresh press. OS repeats and
Alt/Ctrl/Super combinations are ignored; unrelated keys and focus changes do
not resume playback. These shortcuts do not generate native F1 input.

Settings temporarily host-pauses simulation and audio, restoring the preceding
host-pause state when closed. Both menu boundaries clear held gameplay controls
and pending history while preserving the native F1 phase and device deadlines.

In the silent nominal preview, native F1 waits, displayed transitions, ending
animation, static previews and
manual stepping fold received history to the current scheduling time without
adding ordinary control polls. Completed taps in those discarded intervals do
not become delayed actions; keys still held remain held. New game initialization
also folds history first, then uses the engine's existing session reset rules.
Startup Space advances presentation without becoming a gameplay Fire make.
Normal animation also suppresses that navigation press's matching release.
Later intro/pregame input reaches the installed handler; sample IRQs save a raw
scan for their eventual return instead of updating the ordinary key table.

Focus loss and each host-pause toggle clear pending history, frontend repeat
eligibility and the DOS key table while preserving the native pause phase and
control counters. Buffered events preceding the reset's host epoch are ignored.
Host-pause toggles use their actual handling time as the cutoff, so events from
the paused interval cannot reappear after resume. Focus loss uses its correlated
event time as the cutoff and clears controls immediately. Host pause excludes
elapsed paused time from existing deadlines; resuming introduces no catch-up for
that interval. Normal animation freezes queued playback with the PIT timeline;
native F1 continues PIT execution and its original handshake. Quit and host-pause
shortcuts are handled immediately.

Synthetic tests cover historical input correctness separately from tracing
transparency, including catch-up, late delivery, F-key effects and boundary
resets. They establish frontend scheduling behavior, rather than original DOS
input timing or physical keyboard latency. Native captures and original-program
comparisons remain external evidence.
