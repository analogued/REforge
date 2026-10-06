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
| 367 | Terrain selector becomes 7 |
| 387 | Formation count increases to two |
| 407 | Fifth primary wave starts with both axes orbiting |
| 464 | Terrain selector becomes 9; recycled rows mix two sprite families |
| 484 | Five procedural gate records are created |
| 504 | Sixth encounter starts a single craft on the scripted sweep |
| 542 | The sweep craft collides with the player and becomes an explosion |
| 549 | Sixth primary explosion clears |
| 552 | Fourth player death completes; remaining lives decrease to two |
| 555 | The fourth removed life icon resolves on the bottom HUD update |
| 569 | Terrain selector 9 is selected again |
| 589 | A second group of five procedural gates is created |
| 609 | Seventh encounter starts two craft on staggered relative sweeps |
| 645 | Second sweep craft collides with the player and becomes an explosion |
| 648 | First sweep craft reaches the terminal row and retires |
| 652 | Second sweep craft's explosion clears |
| 655 | Fifth player death completes; one life remains |
| 659 | The fifth removed life icon resolves on the bottom HUD update |
| 672 | Terrain selector becomes 2; recycled rows use the fuel-bearing family |
| 692 | Formation count increases to three |
| 712 | Eighth encounter starts three seeking craft |
| 756 | Sixth death clears the last requested life icon; play resumes |
| 758, 779, 797, 799 | Kind-1 proximity effects grant fuel |
| 772 | Terrain selector 2 is selected again |
| 792 | Ninth encounter starts three craft that retain far coordinates and seek near the player |
| 826 | The second craft in the ninth wave collides with the player |
| 836 | Seventh death sets the game-over flag; the current body completes |

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
836. It updates the HUD, recycles and projects terrain, and starts nine enemy
encounters. The generator runs from fresh startup state; branch decisions consume
its results in the original order. Source resolution remains separate from
requested sprite selection.

Projectiles update before the enemy chooses its next target, so they use the
preceding target. The shadow uses the craft's position before its forward delta.
All world motion precedes collision checks and source resolution. Objects retire
before collision checks. Collision uses signed inclusive bounds
from previously resolved descriptors; the target's extent is used on both sides
of the height test. Near-list source resolution includes objects at depth below
70, including inactive records whose projected rectangles move offscreen.
Perspective lookup uses the depth's low ten bits and treats the scale word as
signed, including for a projectile created at negative terminal depth. Source
resolution still precedes projection.

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

The fifth encounter contains two craft. The formation-count command consumes
one random word before increasing the count. Each craft starts at depth
1,600 minus 100 times its index within the formation and keeps its own cached
source, projection, shadow, and explosion state. Both axes continue orbiting
at near depths. Craft update in ascending slot order, sharing the orbital
center, projectile slot, and target. Each moving craft consumes three random
words, even while the projectile is already active. A collision converts only
the identified craft; script progression waits until every primary is inactive.

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
handler. Selector 2 assigns kind 1 to every recycled row with one lateral-placement
random call, preserving live rows. It does not consume a separate kind-selection
word. Kind-1 collection, far effects, and immune overlap paths are covered by
static inspection and authored synthetic tests; this no-input run exercises
kind-0 near effects and kind-1 proximity fuel grants.

Selector 9 assigns kind 2 to recycled slots 15 through 19 and kind 3 to slots
20 through 22. Each inactive row consumes its usual single random word; live
rows keep their previous kind until retirement.

The later gate command consumes two random words and allocates five records
with a shared lateral position and staggered depths. These use movement subtype
one and retain collision extents independently of sprite descriptors. They
bypass ordinary source resolution and near projection, preserving logical
width, zero bitmap height, and the first pass's scale and placement. The
procedural renderer clears packed pixel bits to form a beam and two pillars,
with perspective-dependent thickness and viewport clipping. Gates participate
in the same persistent world order as ordinary sprites. Successful containment
adds pending points; a failed overlapping crossing calls the player response
with zero damage. Phase and grace still govern dispatch and response separately.
The no-input run misses the gates; containment and response paths are covered by
static inspection and synthetic tests.

The sixth encounter uses variant one and a resource-defined sweep. Its
coordinates and sprite requests follow integer affine segments reproducing the
original quantized path. The common update retains those requests, copies
coordinates to its shadow, and consumes three random words for firing and
shared targeting. Creation initializes the available primary slots but leaves
only the first craft and companion active. The seventh encounter activates all
allocated craft with sweep phases staggered by four rows. Each active craft
advances a shared center toward the current player by at most four units per
axis per half-step. Its row coordinates are offset by that center and a fixed
lateral formation spacing. Both variants preserve the row's depth and source
request rather than applying the ordinary forward delta. Forward retirement
clears the animation phase while retaining the requested source. In the seventh
encounter, the second craft collides before its terminal row while the first
reaches depth -50 and retires. That terminal update still copies shadow state
and consumes its three firing/targeting words before retirement. A projectile
created there retires at the next world update. The eighth encounter increases
the formation to three seeking craft; their shared projectile and target still
update in ascending slot order. The ninth uses the same randomized formation
initialization as the first wave. At old depths of at least 240 it retains
lateral position and height while moving forward by 22 units per half-step.
Below depth 240 it seeks the player. Alternate modes remain unsupported.

Camera height changes with the player's response movement. The terrain strip
position and vertical projection origin are cached by the first render pass;
subsequent projections use the current camera height. Shadows precede ordinary
objects, world and player sprites clip to the viewport, and distant objects
retain the first pass's drawing order between updates. The sorted list persists
across bodies. Entries with movement subtype zero keep their order in a prefix;
other entries sort by descending depth, retaining their preceding order at equal
depth. Inactive records still participate.
The initial HUD indicator artwork is preserved until banking changes its selection.

Native frames and checked HUD, player, terrain, enemy, gate, target, camera,
random state, and retained drawing order match the original at every
completed-body boundary from 0 through 836.
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

## Player steering

`2277` polls held inputs three times per body. Original Up/S, Down/X, Left/B,
and Right/N map to bits 1, 2, 4, and 8. The native viewer exposes arrow keys.
Up and Right acceleration run before Down and Left. Opposing directions remain
separate bits and suppress friction; at the positive speed limit the negative
operation wins. Release reduces each velocity toward zero by one per update.
Lateral speed is limited to seven units, vertical speed to six. Position changes
by four times velocity. Lateral bounds are -320 and 280; vertical movement
preserves a nonnegative camera height and caps world height at 230 with signed
integer rounding. Banking chooses among five orientations and three height bands.

Each player update shifts every world record from 2 through 28 by negative eight
times the clamped lateral velocity, including inactive records. New terrain and
randomized enemies subtract twice the current player lateral position from their
placement. HUD bank indicators update after the first player poll; changed
selections paint masks over retained pixels rather than rebuilding their artwork.
A damage response replaces held directions with recentering commands. All sixteen
response rows are represented by authored pose/duration programs and a separate
death program, verified against reachable original phases.

The controlled prefix is verified through body 90 using held keys, releases,
diagonals, opposite directions, coordinate limits, camera movement, and the first
enemy spawn. All 91 completed-body frames and checked state match the original.
The first injected key in that capture reached the keyboard handler after the
first player poll; a separate poll trace establishes samples zero, Left, Left.
`advance_with_inputs` and the exporter's input TSV support exact per-poll replay;
the viewer samples its held state for a completed body. Full controlled gameplay,
firing, and a second run remain to be implemented and verified.

## Game over and return

A completed death removes one requested life icon. When the first icon was
already clear, the death sets `DS:b66a` instead. Zero icons therefore allow one
more recovery. The game-over flag is checked at the next loop entry, so the
remaining updates and drawing of body 836 still run.

After gameplay returns to `0452`, `26eb` displays Game Over and `2859` displays
High Score. They reuse craft slot 5 over the frozen world. Each title has 35
approach draws at depths 1,000 down to 48, fifty two-unit hold waits, and 34
departure draws from 48 through 972. Title sprite selection follows the original
integer depth-size rule. High Score compares six decimal digits and draws the
best score relative to the retained closest title rectangle. Projection and
stable drawing order include all 29 world slots; pixels outside the viewport
retain the final gameplay frame. Audio during the hold remains deferred.

The first title pair ignores return input. Further pairs repeat until a key
returns control to the intro path at `03ae`. The random generator continues
from the ending state: its next low-three-bit choice selects raw `DATA6` for
values below four and raw `DATA1` otherwise. The verified no-input run selects
`DATA1`, so the splash artwork appears on this return. This is the intro path's
image selection; it does not replay the entire fresh-process startup.

`Ending` implements the title stages, best-score retention, frozen projection,
and return image choice. The viewer uses Space for return and holds the chosen
image. Firing, another game in the existing random stream,
additional encounter commands, and audio remain unsupported.
