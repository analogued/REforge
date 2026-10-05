# DOS opening gameplay

This describes verified original behavior. The Rust viewer currently presents
the static initial scene; gameplay updates are not implemented there yet.
Code and data addresses follow [DOS startup](dos-startup.md).

## Update order

The game loop starts at `04b7`, repeating at `04c2`. Each completed body calls
the tick wait `7437` three times, updates player input through `2277` three
times, and updates world objects through `2ae4` twice. Drawing is interleaved
with those updates. `DS:b692` increments once per body and selects alternating
HUD update paths. Event processing (`1426`) and object recycling (`316d`) run
at the end, before returning to `04c2`.

Consequently, one loop body includes several updates and framebuffer writes.
Its count is not a count of display refreshes. The tick wait polls and resets
`DS:17bf`; these waits do not establish a native wall-clock rate by themselves.

## Encounter scripts

Gameplay initialization chooses a script using the low two bits of the result
from pseudo-random helper `5572`. The script selector is `DS:b6ac`, byte cursor
`DS:b69e`, and delay counter `DS:b6b8`. Commands are read at
`DS:0a3e + 50 * selector + cursor`, with decimal stride 50. Their high two bits
select terrain, formation, primary-wave, or control handling. Commands can
change terrain, palette, wave types, and later script progression.

The parser requires primary object slots 5 through 9 to be inactive before it
advances the delay. It compares the old delay against `20 - (DS:b6c6 << 3)`,
increments it, and fetches a command when the threshold is met. After handling
a command it repeats the activity and delay checks. It does not necessarily
consume the next command in the same invocation.

The verified fresh-process opening selects script 0. With no gameplay input
and the initial threshold, its first events are:

| Completed loop body | Event |
| --- | --- |
| 21 | Terrain selector becomes 5; terrain slots 15 through 22 become active |
| 41 | Palette-family-1 command |
| 61 | First primary wave starts in slot 5; further script delay waits for the wave to clear |

These counts are logical loop positions, not durations. Newly created objects
may receive their resolved sprite and projected rectangle in subsequent updates.

## Repeatability

Helper `5572` updates three 16-bit state words at `DS:17c1`, `DS:184b`, and
`DS:184d`. It uses arithmetic, XOR, and rotate operations, including carry in
the rotate-through-carry step. It does not read the clock. Gameplay consumes
its results for script selection, object placement, and some behavior and HUD
choices. Preserving the order and number of calls matters even when an object
is inactive or not visible.

Two fresh executions with matching configuration and intro input matched every
visible packed framebuffer byte, all 29 scene records, and selected control
and random-state words through 80 completed loop bodies. This includes the
first primary wave. This establishes a repeatable opening under those conditions;
different input, collisions, later script branches, or restarting within an
existing process can change the subsequent sequence.

See [CGA graphics](cga-graphics.md) for composition and object records. Captures,
raw state, and comparison reports remain external to the repository.
