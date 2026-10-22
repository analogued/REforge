# DOS gameplay

This describes verified original behavior and its Rust implementation.
Code and data addresses follow [DOS startup](dos-startup.md).

## Update order

Before the main loop, initialization calls the player/input helper `2277` at
`03d7`, after resetting player and world records and before initial composition
at `044a` and pre-game sample playback at `044c`. This poll uses the ordinary
function-key priorities and counters. A held Right input starts lateral velocity
at one, moves the craft/shadow laterally by four and shifts world records 2–28
by minus eight. Space is retained as input but does not fire a projectile here.
The initializer resolves/project records and composes the scene without running
enemy movement, weapon advancement or a gameplay body.

`Gameplay::initialize_input` applies the returned poll mask to that initialization
operation. Normal live playback calls it before entering pre-game playback;
recording replay uses captured initializer input when available. The initial
HUD still draws its previously resolved indicator artwork even if the player
update selects a different orientation.

The game loop starts at `04b7`, repeating at `04c2`. Each completed body has
three ordinary calls to the tick wait `7437`, updates player input through
`2277` three times, and updates world objects through `2ae4` twice. Pass one
adds a fourth wait when its selected background layer matches the preceding
pass-one layer. Drawing is interleaved with those updates. `DS:b692` increments
once per body and selects alternating
HUD update paths. Event processing (`1426`) and object recycling (`316d`) run
at the end, before returning to `04c2`.

Consequently, one loop body includes several updates and framebuffer writes.
Its count is not a count of display refreshes. The tick wait polls and clears
`DS:17bf`; the timer interrupt's counter accumulation establishes its nominal
rate independently of the foreground instruction count.

## Encounter scripts

Gameplay initialization chooses a script using the low two bits of the result
from pseudo-random helper `5572`. The script selector is `DS:b6ac`, byte cursor
`DS:b69e`, and delay counter `DS:b6b8`. Commands are read at
`DS:0a3e + 50 * selector + cursor`, with decimal stride 50. Their high two bits
select terrain, formation, primary-wave, or control handling. Commands can
change terrain, palette, wave types, and later script progression.

The starting selector 0–3 chooses among four encounter sequences; it is not a
level or difficulty number. A subsequent new game draws from the retained
generator state after the previous gameplay, ending and intro selection.

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

## Rust gameplay simulation

`Gameplay` implements the reconstructed game. Its default bounded diagnostic
run covers the opening without keyboard input through completed body 836;
`Gameplay::for_play` runs without that export limit. The opening updates the HUD,
recycles and projects terrain, and starts nine enemy
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

Retirement selects its handler from the movement type after the update. A falling
explosion can recycle, consume initialization random words, and decrement its
depth into the retirement boundary in the same update. Its restored falling
movement then clears activity and animation phase while retaining the last
sprite request; an unrecycled explosion clears activity and its sprite request.

The original falling-variant wrapper at `2ef0` calls the explosion handler without
supplying its slot argument. The handler reads an uninitialized local word, which
can contain an old slot index or stack data left by an intervening interrupt.
The reconstruction fixes this defect by passing the current craft explicitly:
ground contact starts and advances that craft's explosion in the same update.
Incidental stack corruption is not reproduced.

The primary player-contact dispatch at `3865` accepts variants 0, 1, 2, 5 and 6.
Variants 0, 1, 5 and 6 submit a 1,000-point award through the ordinary score
multiplier/cap logic, start the craft explosion and hide its shadow, then request
damage two. Variant 2 uses falling recycle/explosion without that award. Variants
3, 4 and unsigned values 7–255 return without action. Existing explosions and
inactive records are excluded; player response phase suppresses later contacts,
while grace can suppress damage without suppressing an accepted target's effects.

Variants 5 and 6 use this handler even though the normal trajectories examined
keep their contacts out of reach: patrol depth remains 188,
boss approach stays at least 150 under primary speeds up to 50, and the ordinary
player depth plus descriptor extent reaches only 29. Terminal boss depth is
retired before collision scanning. This does not imply a reachable normal-play
boss or patrol contact.

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
The HUD marker's requested source and position can change on even bodies.
Its visible source, dimensions, and position remain cached until that same
render phase, including when a new difficulty changes the marker family.

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
Below depth 240 it seeks the player. Alternate modes select the corresponding shadow family.

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
captured frames or state. `advance_subframe` exposes each of the three displayed
passes; `advance_with_inputs` completes all three and then the end-of-body events.
Explicit export limits and the game-over exit apply only after the third pass.
Live gameplay has no loop limit. Its body counter wraps as an original 16-bit
word, preserving the HUD update phases across the wrap.

The first pass updates the player and world, projects and sorts the full world,
and updates the HUD. The second updates and projects only the player and shadow.
The third updates the player and world again, then reprojects the near list.
Far-object geometry and ordering persist from the first pass. All three passes
include the viewport corner masks; HUD pixels persist from the first pass.
Player and shadow updates change sprite requests; resolved sources remain cached
until projection. A scene transition between update and projection uses those
cached sources.

When old and new sprite placements overlap, background restoration merges their
bounding rectangle before clipping and expansion to even byte and scanline
boundaries. Disjoint placements restore separately. The merged region includes
corners outside either individual placement.

The three screens before the waits at `0698`, `06dc`, and the end-of-body code
at `0733` match original captures throughout the 90-body steering prefix.

### Timer pacing

The original software state is separate from PIT hardware state. Helper `7b19`
stores the divisor at `CS:7b27` and writes channel 0. Helper `7b2b` adds that
stored word to the phase at `CS:7b29` twice, incrementing `DS:17bf` on each
16-bit carry. Both the phase and counter wrap. It performs these additions
before checking the sound-disable word; disabling native sound does not stop
gameplay timing. The phase survives handler changes and foreground waits.

Wait routine `7437` loads only AL from its stack argument, preserving incoming
AH. It waits until this unsigned AX is no greater than `DS:17bf`, then clears
the **entire** counter, including excess units. It does not reset the software
phase or PIT phase. An inherited nonzero counter can make a wait return
immediately. Hardware count zero represents 65,536 clocks, whereas stored
software divisor zero contributes zero to this helper.

Digitized playback temporarily replaces IRQ0 with a handler that does not call
`7b2b`, so its software phase and wait counter stop accumulating while PIT
execution continues. Playback return restores the previous handler and writes
hardware divisor 19,560 directly, leaving the stored divisor unchanged. Startup
installs handler `7532` and stored divisor 65,535 before the intro. The pre-game
sample occurs before entry into the main loop; body one inherits that handler,
the post-sample hardware divisor and existing counter/phase. Its first accepted
per-body setup later installs `79e7` and programs the body divisor through `7b19`.
The `74c0` call after main-loop return is a post-game reset.

The timer helper accumulates the PIT divisor twice per IRQ and increments the
counter on carry. When hardware and stored divisors agree and interrupts are
delivered without loss, a timer input frequency of 1,193,182 Hz gives one nominal
counter unit is `65536 / (2 * 1193182)` seconds, approximately 27.463 ms.
Four such units take approximately 109.851 ms, the baseline used by the silent
nominal preview. Normal animated viewing delivers actual modeled IRQs and
inherits the current divisor/counter/phase rather than assigning each wait a
fixed host duration. Both paths retain three input/update passes.

Pass one's ordinary wait precedes the player/world update and scratch
composition. The selected layer is 1 when signed `DS:b698` is positive,
otherwise `DS:b6aa`. The previous selected layer is cached separately and starts
at zero. When these layers match, the call at `0550` adds one unit between
composition (`0547 -> 61cc`) and the CGA copy (`0555 -> 60a0`). When they differ,
the changed-layer drawing path skips this wait and updates the cached layer.
Ignoring instruction execution time, the nominal unchanged-layer schedule is:

| Relative timer unit | Input and update work         | Framebuffer publication |
| ------------------- | ----------------------------- | ----------------------- |
| 1                   | Pass one: player, full world   | Previous frame retained |
| 2                   | No additional input poll      | Pass one                |
| 3                   | Pass two: player              | Pass two                |
| 4                   | Pass three: player, near world | Pass three              |

The nominal preview retains a prepared pass-one image until its publication
deadline;
palette and display-mode effects follow their input processing independently.
A layer-change body omits the publication wait and therefore takes three nominal
units. `--period-ms` sets the four-unit body duration; all individual wait units
scale with it. Manual stepping completes the same algorithms without timed waits.
Publication models a completed framebuffer checkpoint; it does not reproduce
the CGA display's scanout during individual region copies.

When presentation is slower, due updates and publication events advance in
order before presenting the latest published image. Each pass still runs its
input polls, state changes, composition and random draws; only intermediate
host presentations are omitted. The silent nominal preview processes at most
64 scheduled events before repumping host events, retaining deadline debt without another
presentation wait. Native pause and blocking transitions stop ordinary catch-up.
Each transition and
ending frame retains its own visible wait rather than being skipped to spend
ordinary timing debt. Original rendering cost, discarded timer phase, and the
startup timer transition make precise DOS elapsed time
dependent on execution conditions.
The calculation uses the IBM timer input frequency of 1,193,182 Hz, also used
by [DOSBox-X's timer implementation](https://dosbox-x.com/doxygen/html/timer_8h_source.html).
The viewer offers an interval override and manual stepping.

The headless `mach3::timer` execution layer reconstructs these counters and
foreground waits against the shared PIT clock. It stops at interrupt entry,
letting the engine execute the installed handler before further advancement.
The reconstructed [procedural sound handlers](sound-assets.md#procedural-sound)
dispatch through this execution layer, including in-handler timer reprogramming.
Gameplay exposes ordered sound requests and blocking-operation selections through
`Gameplay::advance_subframe_with_sound`. Immediate stepping drains the same
continuations used by `gameplay::execution::GameplayTask`, a headless, budgeted
main-loop executor. That executor drives the three ordinary waits, the conditional
pass-one publication wait, terminal death's separate one- and twenty-unit waits,
and native pause's four one-unit waits from the delivered interrupt helpers.
It retains a prepared image until publication and stops there before executing
the pass tail. Completion sample playback suspends enemy mutation, subsequent
random draws and the remaining world update until the sample routine returns.

The executor owns its pending game and receives explicit initial software sound
state, PIT state, installed vectors, DI and keyboard state. Scheduled scan bytes
enter the ordinary or temporary sample keyboard handler on that same timeline.
At an equal timestamp input is latched before dispatch and foreground polling;
IRQ0 retains priority over IRQ1. Past polls are committed: a late arrival may
affect future execution but cannot be backdated. Synthesis and audio output are
absent from this executor and cannot determine how far gameplay advances.

Foreground segments and handlers currently have zero instruction cost. Native
pause's busy input loops use a caller-supplied positive polling interval in PIT
clocks, rather than an emulated CPU speed. Stage-compositor CPU delay loops and
individual framebuffer copies are not timed here. Immediate stepping retains
transition images without their live delays. The speed adjustment retains its existing
pre-composition snapshot placement within an atomic pass. These approximations
limit absolute timestamp comparisons with DOS execution; they do not introduce
a separate nominal clock for the reconstructed waits.

Normal animated viewing uses this device clock from intro through pre-game,
gameplay and ending waits. `live::GameplayRuntime` retains the most recently
published frame while a pending pass composes another. Host time supplies bounded
PIT budgets of about 10 ms; overdue budgets continue after repumping SDL events.
The live bridge presents each scene-transition region at its simulation deadline,
retaining the current image between publications. Region snapshots are consumed
as they are presented rather than retained throughout the game.
PC speaker synthesis observes the same device edges. Output failure or buffer
recovery cannot change the guest continuation, PIT phase or random stream.
Native F1 pause continues device execution; modern Pause/Break freezes both
the guest timeline and queued host playback. Step/static/custom-period previews
retain the nominal schedule above and remain silent.

## Expanding scene transitions

HUD background restoration covers its current and preceding sprite rectangles.
Static border pixels outside those rectangles and the playfield retain their
previous values when the alternate scene background is selected. In particular,
the border immediately above the reveal must survive subsequent gameplay passes;
repainting the entire area outside the playfield would incorrectly replace it.

Routine `1edd` replaces the background through seven cumulative clipped regions,
retaining the frozen world over them. Each draw calls `6147`, followed by
`5bac` repeated `[10, 10, 10, 8, 5, 2, 2]` times. `5bac` polls bit 3 of CGA
status port `03da` with interrupts disabled and a 30,000-iteration bound. It
returns when retrace is already active; it does not require a fresh edge. Several
calls can therefore complete in one retrace window. The repeat counts are not
gameplay timer units, and the original duration depends on instruction/rendering
cost and display phase.

Normal live playback deliberately normalizes this sequence: each region remains
published for `floor(PIT_HZ / 60)` clocks (19,886 at the configured frequency),
giving about 116.7 ms for all seven. This is a CPU-independent presentation
policy, not a reconstruction of exact original wall-clock duration. The original
repeat counts remain in `TransitionFrame` for reference and silent preview use;
live playback does not turn them into extra refresh edges or emulate busy-loop
cost, interrupt masking or timeout behavior.

Gameplay suspends at the actual transition call. Goal progression, weapon
updates, subsequent script commands and their sound requests resume only after
the last region's interval. PIT/audio and ordinary keyboard interrupts continue
on the same timeline, without adding foreground control polls. Completed taps
can still be missed by the next ordinary poll. Modern host pause freezes this
timeline and preserves the pending region/deadline. Slow host presentation can
omit intermediate displayed images, but cannot omit their simulation intervals
or reorder the suspended foreground work.

## Player steering

`2277` polls held inputs three times per body. Original Up/S, Down/X, Left/B,
and Right/N map to bits 1, 2, 4, and 8. The native viewer exposes these aliases and arrow keys.
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
animated playback samples held state separately at each pass. Manual stepping
samples the same held state for all three updates in a body. Live gameplay
continues until game over; bounded exports retain explicit limits for comparisons.

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
retain the final gameplay frame.

Before its fifty hold waits, the mandatory Game Over title calls `4fff` at
`27a4`, playing `DATA9` forward from offset zero for 10,874 bytes. The mandatory
High Score title calls `4fb3` at `29e6` after drawing the score overlay, playing
`DATA7` forward for 11,872 bytes only when the current score strictly exceeds
the retained best score. Equal scores do not select that sample. Both wrappers
require the application digitized-sample flag (`DS:b666`); optional repeated titles
omit these calls.

These calls use the existing sample continuation and device clock. The visible
approach/overlay remains while sample IRQs execute; hold waits begin after its
teardown restores vectors/timer state and clears the ordinary key table. Native
sound disable bypasses playback with an ordinary `73b2` poll whose return mask
the ending ignores. Host mute or unavailable output does not bypass execution.

Every title begins with an ordinary input poll (`26f1` for Game Over, `2864`
for High Score), repeated until its returned mask is zero. This gate precedes
title setup, the first drawing wait and High Score's comparison. It can process
F2/F3 even during the mandatory first pair; a zero mask does not necessarily
mean that every physical key is released. Entry busy-loop polls use the live
frontend's positive 64-PIT-clock interval, without modeling instruction cost.

The mandatory first pair skips the optional ordinary polls in its approach,
hold and departure loops. Conditional sample calls before the hold have their
own keyboard behavior, distinct from animation polls.
Later pairs poll before each two-unit animation wait, returning on any nonzero
mask. Game Over uses sites `2736`, `27b8`, `27ea`; High Score uses `28fd`, `29fa`,
`2a2c`. The score overlay has neither a poll nor a wait. Between repeated titles,
the caller performs separate one- and five-unit waits, clearing excess software
counter units after each, before the next entry gate.

Return control follows the intro path at `03ae`. The random generator continues
from the ending state: its next low-three-bit choice selects raw `DATA6` for
values below four and raw `DATA1` otherwise. The verified no-input run selects
`DATA1`, so the splash artwork appears on this return. This is the intro path's
image selection; it does not replay the entire fresh-process startup.

Both initial and returned intro entry call `74c0` at `03c2`, separately from
the `0452` reset before the ending titles. This restores IRQ0 handler `7532`,
priority `ff`, channel-0 mode 3 and stored/hardware reload `ffff`, and stops the
speaker. Reprogramming restarts the hardware count even when it already matches.
Completed ending samples can leave hardware reload `4c68` while the stored
divisor remains `ffff`, making this return operation necessary. It preserves
software timer phase/units, tone cursors, noise state and held input; keyboard
clearing belongs to completed sample playback, not this reset.

`Ending` implements the title stages, best-score retention, frozen projection,
and return image choice. In normal playback, Space or another nonzero gameplay
input can return during an optional pair. It then runs the selected intro's
sample/input continuation before starting another game
with the continuing random stream and retained best score. Silent previews use
another fresh Space press to advance that intro screen.

The ending retains gameplay's palette family and display mode. An ordinary F3
poll selects family 1 as in gameplay; crossing into the ending does not itself
change the palette. The live viewer repaints palette or display-mode changes
even when native pause leaves the framebuffer unchanged.


## Shooting and progression

Space/keypad 5 supplies Fire bit 16. Weapon updates run after collision on the
first and third player polls. Two player slots alternate when the other shot
has passed depth 130. New shots start fifteen depth units beyond the player;
forward motion adds `(depth >> 4) + 22`, and depth above 300 retires them.
A signed firing counter limits sustained fire and controls the right HUD bar.
Its clamp occurs on HUD updates, so it feeds back into later weapon updates.
The near list is selected before weapon motion; a newly spawned near shot can
retain its preceding projection until the next eligible resolution.

Player shots destroy ordinary craft variants 0, 1, 2, and 5 for 1,500 points,
and variant 6 for 2,600. They cancel the shared enemy projectile. Terrain kinds
0 and 1 convert to effects for 900 points without consuming the player shot or
granting fuel. An active source continues its ascending target scan even after
a hit clears its activity. Damage responses replace Fire along with directions.

`ScriptRules` reads the 21 scripts and their two progression edges from the
expanded external program data. The maintained decoder expands the reverse-RLE
load image without executing or relocating it. Normal endings follow edge zero;
entering a goal follows edge one. Both reset the cursor and clear primary and
special activity while retaining the random stream and current terrain selector.
The parser preserves signed, wrapping delay arithmetic, including several
commands in a single invocation at higher difficulty.

The implementation handles the enemy command families, all terrain selectors,
procedural special modes, continuous gate recycling, palette/background
selection, and acceleration/deceleration of the three world speeds. Scene
changes produce seven expanding viewport draws over a frozen world. Difficulty
remains a wrapping word. HUD marker and patrol artwork selection reuses visual
family 9 at higher difficulties, including its marker placement. The original
indexes beyond the ten visual families: at difficulty 10 its zero-filled choices
can leave HUD selection retrying forever before the first encounter. This defect
is fixed without limiting difficulty, command pacing or the other gameplay rules.
The external rule decoder retains raw wrapping table access for inspection;
runtime visual selection applies the correction. Invalid supplied resources can
still produce a reported retry error. These later paths
have static analysis and synthetic algorithm tests; runtime comparisons cover
the complete first no-input game and targeted controlled prefixes.

## Session and gameplay controls

A new game resets gameplay counters, speeds, lives, and active records but keeps
the generator, best score, display mode, and sound preference. Object reset is
partial: unused coordinates, subtypes, and resolved projection fields can survive
until subsequent allocation or resolution. `SessionState` carries those fields
between the ending and `Gameplay::from_game_start`.

The original polls gameplay controls at body entry and at each of its three
player updates. F2 toggles its sound flag after four active polls; F3 switches
on each active poll. Mode 6 displays the same video bytes as 640 one-bit pixels
per row. Both modes fill a 4:3 display.

Interactive viewers default to `--f3-mode press`: one toggle per press, with a
release required before the next toggle. This frontend policy is a modern
convenience, not reconstructed original behavior. `--f3-mode original` retains
the per-poll behavior for physical-input comparisons. Headless execution and
original-game recording replay retain the original rules unless a caller
explicitly enables the frontend policy; F1/F2 processing remains unchanged.

F3 saves the framebuffer bytes, switches the BIOS mode and restores those bytes,
but does not restore the palette register. Both CGA mode sets select family 1;
returning to color therefore restores bright cyan/magenta/white until a later
script command selects another palette. The viewer and recording replay apply
this reset at each effective F3 control poll, including native pause polls.
Subsequent CD/CE commands select family 1/0 in execution order; the last script
selection is not automatically reapplied after a mode switch.

F1 enters a release/press/release pause handshake with two timer waits before
press detection and two before the decision. The decision samples whether F1
is still held; that requests game over, while other input resumes. The current
committed body completes before the exit flag is observed. Native Enter stepping
is a development control, separate from the original Fire input.

The modern host pause preserves the native F1 phase and its pending waits.
Clearing frontend-held controls is observed as a release at the next native
sample; it does not restart the handshake. OS repeats cannot repopulate cleared
controls before a fresh press. Received scan transitions are retained until
their scheduled polls, so catch-up uses the historical keyboard state. A short
press/release pair completed between polls can still be missed. Events delivered
after an executed poll apply prospectively; no rollback is performed.
The viewer does not emulate hardware keyboard transport or physical key timing;
externally scripted per-poll input is a separate replay interface. See
[input timing and tracing](input-tracing.md) for frontend boundary rules.
