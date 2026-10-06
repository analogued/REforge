# DOS opening gameplay

This describes verified original behavior and its bounded Rust implementation.
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
| 98 | First primary wave clears after passing the player depth |
| 118 | Terrain selector becomes 7; recycled slots adopt a second sprite family |
| 138 | Second primary wave starts and steers toward the player |
| 174 | Second primary collides with the player, becomes an explosion, and adds 1,000 pending score points |
| 181 | Second primary explosion clears |
| 184 | Player death completes; remaining lives decrease to five and fuel resets |
| 187 | The removed life icon's source resolves on the bottom HUD update |
| 201 | Terrain selector becomes 6; recycled slots 20 through 22 remain inactive |
| 221 | Third primary wave starts with orbital movement |
| 257 | Third primary collides with the recovered player and becomes an explosion |
| 264 | Third primary explosion clears |
| 267 | Second player death completes; remaining lives decrease to four |
| 284 | Terrain selector becomes 3; recycled rows choose terrain kinds 0 or 1 |
| 304 | Fourth primary wave starts with lateral orbit and fixed height |
| 312 | A kind-0 terrain row becomes a frozen near effect |
| 313 | The near effect deals one damage unit to the player |
| 340 | Fourth primary collides with the player and becomes an explosion |
| 347 | Fourth primary explosion clears |
| 350 | Third player death completes; remaining lives decrease to three |
| 355 | The third removed life icon resolves on the bottom HUD update |

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

## Rust opening simulation

`Opening` implements the opening without keyboard input through completed body
360. It updates the HUD, recycles and projects terrain, and starts four enemy
waves. The generator runs from fresh startup state; branch decisions consume
its results in the original order. Source resolution remains separate from
requested sprite selection.

Projectiles update before the enemy chooses its next target, so they use the
preceding target. The shadow uses the craft's position before its forward delta.
All world motion precedes collision checks and source resolution. Objects retire
before collision checks. Collision uses signed inclusive bounds
from previously resolved descriptors; the target's extent is used on both sides
of the height test. Near-list source resolution includes objects at depth below
70, including inactive records whose projected rectangles move offscreen.

The second enemy steers by half the remaining coordinate difference, capped at
30 units per axis per half-step. Its collision changes it to a stationary
explosion, hides its shadow, and stops its movement and random targeting calls.
It awards pending points after that body's HUD update; later even bodies apply
at most 500 points each. The player receives two damage units and enters the
hit response. The original player update runs three times per body, recenters
the damaged craft, and advances the response into the death animation. Completion
consumes a life, clears damage and response phase, restores fuel to 3,583, and
starts four completed bodies of damage immunity. The next player update
selects its normal sprite again, preserving the recovered height and horizon.

The third enemy derives a 64-phase integer orbit from its old depth. Its center
approaches the current player coordinates by at most nine units per axis per
half-step. The lateral coordinate changes before shadow copying and firing;
height changes afterward and is clamped to -100 through 250. Below old depth
240 it switches to the same seeking movement as the second enemy. Projectile
targets use the current player position plus independent random offsets from
-32 through 31, including after recovery. The fourth enemy follows the same lateral wave at
every depth, keeps its initial height zero, and never switches to seeking.

Terrain selector 6 activates only slots 15 through 19 when recycled. Excluded
slots retain their prior kind and phase and still consume random words while
inactive. Life icons resolve every eighth body at phase three. The first death
changes the request at body 184 and the visible icon disappears at 187. The
second death occurs after body 267's HUD update, so five icons remain visible
through body 270 despite four requested lives. The fuel bar likewise retains
its last drawn width until the HUD render phase at body one modulo four; fuel
consumption updates its logical width separately.

Selector 3 consumes two random words for each recycled row, first for lateral
placement and then for its kind. Kind 1 is selected when the second word's low
two bits are zero; the other choices select kind 0. These objects choose sources
from their depth and preceding projection phase. Proximity can convert them
into stationary animations after applying one final forward delta. Near effects
hold their depth and advance each animation source for two world updates.

A kind-0 near effect appears at body 312 and deals one damage unit at 313.
Normal kind-0 and kind-1 descriptor collisions request two damage units;
kind-1 collection adds 512 fuel and 400 pending points. Its proximity path adds
fuel without points. Fuel can exceed its normal maximum until the next
consumption call clamps it to 3,583. Player response phase blocks collision
dispatch, while post-death grace blocks only damage; target collection and
conversion can still occur during grace. Far effects have no player collision
handler. Kind-1 collection, far effects, and immune overlap paths are covered by
static inspection and authored synthetic tests; this no-input batch exercises
kind-0 near effects.

Camera height changes with the player's response movement. The terrain strip
position and vertical projection origin are cached by the first render pass;
subsequent projections use the current camera height. Shadows precede ordinary
objects, world and player sprites clip to the viewport, and distant objects
retain the first pass's drawing order between updates. The depth-sorted list
persists across bodies, so equal-depth objects keep their preceding order.
The initial HUD indicator artwork is preserved.

Native frames and checked HUD, player, terrain, enemy, target, camera, and random
state match the original at every completed-body boundary from 0 through 360.
The simulation uses interpreted rules and decoded resources. It does not load
captured frames or state and does not reproduce every intermediate framebuffer
write within a body.

The viewer presents completed-body frames. It uses a nominal interval of
about 82.388 ms: three units of the approximately 36.413 Hz counter. The timer
helper accumulates the PIT divisor twice per IRQ and increments the counter on
carry. Original rendering cost, discarded timer phase, and the startup timer
transition make precise DOS elapsed time dependent on execution conditions.
The calculation uses the IBM timer input frequency of 1,193,182 Hz, also used
by [DOSBox-X's timer implementation](https://dosbox-x.com/doxygen/html/timer_8h_source.html).
The viewer offers an interval override and manual stepping.

The simulation holds after the current verified batch of 360 bodies. This is a
comparison checkpoint, not a randomness barrier. Subsequent encounter commands
need further implementation and validation. Keyboard movement and firing,
restart within an existing random stream, and audio are unsupported.
