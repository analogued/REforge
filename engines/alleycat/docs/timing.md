# Timing contracts

These contracts describe the executable identified in the
[README](../README.md#original-game-required). Code addresses below are offsets
within its original code segment; data addresses are offsets within its game
data segment. They identify behavior, not a required DOS load address.

## Clocks and hardware access

| Mechanism | Verified use |
| --- | --- |
| BIOS timer | `INT 1Ah`, function `00h`, supplies ticks. Many routines use only the low 16-bit word, with wrapping unsigned subtraction. |
| PIT channel 0 | `13b7` latches the existing counter through port `43h`, then reads its low/high bytes from `40h`. Input polling uses this count for waits and joystick measurements. |
| Retrace status | `13d8` reads port `3DAh` and masks bit `08h`. Different callers test different phases. |
| Call countdowns | Some gates count routine calls rather than elapsed time. CPU speed and intervening work affect their expiration time. |
| Fixed instruction delays | Some paths also contain fixed-count busy loops, including top-of-screen sprite clipping. |

The reviewed startup installs keyboard handlers for `INT 09h` and, on PCjr,
`INT 48h`. No replacement for timer interrupts `08h` or `1Ch` was found in the
reviewed paths. The channel-0 writes above latch its count; they do not change
its clock divisor. Reviewed sound paths separately program PIT channel 2.
The PIT read at `2e10` seeds the shared random state; it is not evidence of a
CPU-speed benchmark.

## Player acceptance

The gate at `08e5` precedes the movement body. It retains the last accepted low
tick at `057d` and a word countdown at `0684`:

1. Read BIOS time. If its low tick differs from `057d`, load the countdown with
   32 on the PC path or 16 when BIOS model byte `0697` is `fd` (PCjr).
2. Otherwise, reject immediately when the countdown is zero. Decrement a
   nonzero countdown and reject unless it has just reached zero.
3. Apply the retrace gate. Scene 2 always tests it. Other scenes test it when
   both vertical direction `0571` and horizontal direction `056e` are zero.
   Where tested, reject unless retrace bit 3 is set.
4. At `0926`, commit the accepted low tick to `057d` and AX to `057f`, then test
   scene-specific movement suppression before entering the movement body.

On the changed-tick path AX is the machine-specific countdown value. On
same-tick expiry it retains the BIOS-call result; captured ordinary cases have
AX zero. Do not replace `057f` with an unconditional machine constant.

With a normally progressing clock and active motion bypassing retrace, this
permits a changed-tick acceptance plus at most one later countdown-expiry
acceptance in the same tick. The countdown stays zero after expiry. The two
updates need not be evenly spaced; a slow loop may not reach the second one.
The 32/16 counts measure **calls**, not CPU cycles or milliseconds.
If retrace rejects a same-tick expiry, the countdown remains zero and that
tick's extra opportunity is consumed. A changed-tick rejection leaves `057d`
uncommitted, so another call can retry the changed-tick path.

## Input and outer-loop ordering

Control routine `1200` polls when the wrapping low-tick difference from `069f`
is at least two, then records the new baseline. The input decoder leaves logical
requests at `0698` and `0699` latched between polls; other game logic can overwrite
them. The keyboard path decodes held keys and then performs a PIT-counter wait;
the joystick path also uses timed PIT reads. The wait tests a wrapped counter
difference, which alone does not establish a fixed duration in milliseconds.

The ordinary alley loop calls controls, player, and ground dog in that order.
When the dog-death flag is clear, it increments byte `040f`; every fourth pass
also calls the sound-state routine, clothes scrolling, windows, projectiles,
dumpster cat, mice, and the lives display in their original order. During
dog death this additional group runs every pass. The divider advances even
when the player gate rejects an update.

Actors therefore cannot all advance once per accepted player update or once
per displayed frame. Their individual BIOS-tick, call-count, and retrace gates
also matter. For example, the window gate at `1936` proceeds with retrace bit 3
**clear**, unlike the player's retrace acceptance test.

The blocking fence-reset effect at `59cb` waits until seven low BIOS ticks have
elapsed. During that wait it repeatedly advances the shared random state even
when the sound setting masks the speaker output. Its iteration count depends
on execution timing. Replacing it with a silent delay preserves neither that
random stream nor the full original schedule.

## Current Rust implementation

The default windowed runner executes virtual loop attempts rather than evenly
spaced accepted player updates. `Runtime` polls controls, applies `PlayerGate`,
advances an accepted `Player::update`, then advances the ordinary alley divider.
The divider runs even on rejected player calls. Grouped passes execute the
clothesline routine's call-count, retrace, suppression and BIOS-tick gates, then
windows, projectiles, the bin cat and mice. The ground dog runs after the
player on every pass. Each actor samples virtual time at its own call stage.
Selected running-dog and moving-projectile costs use scoped XT measurements;
unmeasured actor states remain approximate.

Clothes scrolling (`04a0`) decrements byte `0531` before other tests. On expiry
it reloads 1, requires retrace clear, no support-disable flag or active window
projectile, and a changed low BIOS tick relative to `0544`. It then reloads a
difficulty-dependent call interval, quartered when player Y is at most 96.
An airborne cat in the selected row can inhibit movement. A completed garment
advances its packed occupancy by two support cells before its four successive
four-pixel columns finish entering; player support checks compensate for this
phase. Attached cats move with the selected line.

The bin cat (`2216`) requires a changed BIOS tick and retrace set. A selected
animation advances through 27 count decrements, including two full-height
frames. A supported player touching it receives a randomized left/right down
request before movement initializes the fall; that request remains latched until
the next input poll. Its contact also alerts the ground dog.

Mice (`237b`) have a separate changed-tick gate and cycle one of three rows per
accepted call. Direction changes and pursuit use the original shared generator
and difficulty tables. A captured mouse retains its catch image until its
54-tick respawn. Captures award 90/70/50 displayed points: the decimal addition
at `2706` updates six of the seven displayed digits, leaving the rightmost digit
unchanged. High-score comparison belongs to game entry rather than each catch.

Windows (`1936`) decrement byte `166a` on every grouped call. Expiry reloads
13 before requiring retrace clear; the initial zero wraps through 255 first.
An active window advances one packed row per accepted step and holds at phase
15 for the difficulty-dependent BIOS-tick interval. Player overlap can hold its
animation. Entry (`1b05`) requires a descending cat, no forced-fall latch,
player Y below 96, and phase 5 through 24. The caller dispatches entry after
the remaining actor batch, including any blocking mouse knockdown. Projectile
launching during the phase-15 hold consumes the original shared random draws. The original one-tick hold at
difficulty 6 is corrected to 18 ticks by default; development builds can
restore it with `--original-window-timing`. See [deliberate deviations](deviations.md).

New fence support at Y96 records the accepted BIOS tick at `0556`. Continued
support preserves it; leaving and landing again starts a new interval. After
72 ticks on the fence (about 3.95 seconds), a new ordinary window selects only
the top row and arms projectile alignment stopping. Earlier selections use the
ordinary window pool. The Hearts-entry override suppresses this targeting mode.
The mode belongs to the selected window; it does not retarget an existing fall.

`VirtualClock` uses integer rational time with a PIT input frequency of 1,193,182
Hz and a channel-0 divisor of 65,536. The BIOS rate is consequently about 18.2065
Hz. The modeled mode-3 latch reloads twice per BIOS period and follows DOSBox-X's
fractional-count interpolation. Keyboard polling executes the original wrapped
counter comparison, advancing virtual time between reads; it does not substitute
an arbitrary fixed delay for that predicate.

Retrace is independent of host rendering. The default BIOS-mode-4 reference
pulse uses 76 PIT input clocks per line, 262 lines per frame, and retrace from
line 224 through line 240, following DOSBox-X's endpoint test. This corresponds
to about 59.92 frames per second. CGA and PCjr use the same pulse profile in this
model; the player countdown differs. The starting video/PIT phases are synthetic
and deterministic, not a replay of the original boot and initialization.
See DOSBox-X's [video setup](https://github.com/joncampbell123/dosbox-x/blob/master/src/hardware/vga_draw.cpp),
[video mode registers](https://github.com/joncampbell123/dosbox-x/blob/master/src/ints/int10_modes.cpp),
[status-port test](https://github.com/joncampbell123/dosbox-x/blob/master/src/hardware/vga_misc.cpp),
and [PIT implementation](https://github.com/joncampbell123/dosbox-x/blob/master/src/hardware/timer.cpp).

### Broom launches and the top of the screen

Broom contact normally calls `0872`, which starts a push toward the room exit
reference `2650/2652`, with an initial vertical step of eight pixels. Its
phase decrement comes from horizontal separation using the original NOT-based
expression and has a minimum of 36. A cat far from the window can therefore
decelerate slowly enough to travel completely above the visible screen. Cheese
uses an exit at X264, so launches from its left side commonly use that minimum.
The push latch prevents repeated contact from restarting the launch.

Vertical movement retains the original 50-row padding above the framebuffer.
Partly hidden sprites are clipped; a completely hidden cat has its last visible
image erased and sets `0583`, rather than retaining a fragment at the top.
Broom contact during cheese-hole transfer bypasses the normal push. While milk
drinking is active, contact uses the wider adjusted predicate at `47b0` instead
of starting `0872`. Neither response removes a life.

### Cheese room schedule

Scene 4 polls controls, expires the room tone, attempts a player update, advances
the paired-hole transfer, advances one collectible mouse slot, advances the
broom, and checks scene termination, in that order. Furnished ordinary rooms
retain floor tracking and the shared support, broom, scoring and window-return rules.

The mouse scheduler accepts changed BIOS ticks and cycles four slots. Slot three
leaves its tick unlatched, allowing the following slot-zero call in the same
tick. Consumed slots remain inactive. Visibility phases, hole relocation,
proximity rejection and avoidance use external tables and the shared generator.
Only a visible mouse in a phase below 20 can be collected. The fourth collection
sets ordinary-room success.

A hole transfer requires Alt, a support-derived hole marker, no active alternate
player animation, no broom overlap, and at least twelve wrapping BIOS ticks
since the previous transfer. Its seven drawing steps are gated by changed BIOS
ticks. The player timing gate still runs during the animation, but the movement
body is suppressed; mouse and broom work continues. The normal cat image stays
hidden until the next player draw after the destination background is saved.
Hole pairing depends on difficulty modulo four, using the executable's tables.

Active mouse and transfer costs use scoped XT captures described below,
separate from these original gates. Sound routines consume their native random
draws, but remaining provisional execution timing prevents claiming that a
matching seed alone synchronizes an original full-game run.

### Birdcage room schedule

Scene 5 runs controls and pending-tone expiration, cage motion (`45ab`), bird
motion (`4340`), the common player gate and movement, then the broom (`3339`).
These calls run every room-loop attempt, without the alley's four-pass divider.
Cage pushing, bird updates and broom motion have distinct BIOS-tick gates;
the bird additionally skips every fourth changed-tick call. Room supports and
the descending exit test come from `16c6` and external parallel tables. Room
entry preserves inherited player animation/speed state as `07a1` does.

Once a cage leaves its shelf, `46be` blocks ordinary controls, player and broom
work. Each changed low BIOS tick moves it down five pixels. The next changed
tick after it reaches Y 164 breaks it and releases the bird; execution resumes
at the bird call without repeating input or the cage prelude. Capture awards
the external room score plus twice the unused part of a 1350-tick allowance.
Completion blocks gameplay through the shared scoring presentation before
returning to the saved alley position. It awards once, and a neutral window
exit awards nothing. Scene changes
reset the player acceptance gate and logical controls.

The candidate selector uses the shared generator, difficulty weighting and
two-entry room history; it avoids a third consecutive identical candidate.
Dispatch uses the selected candidate directly; scenes 1, 3, 4, 5 and 6 are all
implemented. Candidate history is the ordinary destination history. Failed
hearts attempts leave it unchanged: birdcage → hearts failure → birdcage →
hearts failure still leaves two birdcage selections in history, so the next
ordinary selection must differ. Direct `--scene` previews bypass selection and
do not enter that history.
Success preserves the pending-bonus flag; the next eligible window selects scene
7 without
advancing the selector's random state or ordinary candidate history.

Room tones emit ordered speaker writes. A pending tone expires once per
distinct tick rather than catching up skipped ticks. Initial and continuation
divisors remain distinct; bird requests and ambient shared-generator draws follow
their original predicates. Instruction-level interrupt placement remains outside
the model; full original-loop random-stream equivalence is not established.

### Bookcase room

Scene three follows the original caller order: input, shared tone expiry,
accepted player update, spider (`3cb1`), plants (`3b42`), broom, then termination.
The spider accepts at least two wrapping BIOS ticks between updates; plants
accept one changed tick and consume at most one target in reverse slot order.
The last plant sets success only while failure is clear. Spider contact reports
an overlap and starts its nine-tick lethal effect, unless success is already
set or spider damage is disabled. The failure flag is committed after the wait;
the suspended caller then resumes at the plant stage.

Bookcase shelf support (`3c43`) precedes the ordinary furniture table but follows
the descending room-window exit test. It aligns X to four pixels, accepts
164..280 inclusive, and snaps to shelf rows 42, 58, 74, 90, 106, 122, 138 and
154 using the original byte arithmetic. These supports span the bookcase
independently of the randomized book tiles. Spider and plant drawing costs use
scoped XT measurements described below; no full original-loop pacing equivalence
is claimed.

### Fishbowl and underwater section

Scene 1 calls controls, shared tone expiry, accepted player movement, broom,
then bowl animation (`3850`). The bowl advances every six wrapping BIOS ticks
and tests player contact only on those drawing opportunities. Its entry flag
has priority over ordinary room termination. Scene 2 replaces the furnished
room with the underwater scene while preserving the original alley return
coordinates and ordinary-room timer.

Scene 2 calls controls, shared tone expiry, the common acceptance gate and
swimming body, fish/eel motion (`3675`), then water-surface motion (`37e5`). Its
player gate requires retrace even during active movement. Swimming accelerates
horizontal and vertical inertia with separate external difficulty caps; neutral
input decays inertia and eventually requests upward motion. Animation depends
on direction changes, relative inertia, and surface/bottom position. Surface
breathing requires the pre-update selected sprite to match the external breathing
sprite, rather than just testing the cat's Y coordinate.

The actor scheduler increments through 24 slots before processing each selected
slot: twelve fish followed by twelve eels. It commits its changed-tick baseline
at slots 12 and 0, except PCjr skips the slot-12 latch while player Y is below
48. Each active slot consumes shared random values for direction changes and
uses its external vertical band. Old images restore uniform water pixels.
The separate surface cursor visits forty tiles, accepts at least eight wrapping
ticks and commits its baseline only at cursor wrap; tiles near the player skip
drawing. Underwater initialization uses the external difficulty count to disable
some eels and preserves actor/surface clocks and eel animation history.

Accepted swimming calls check contacts in ascending slot order, collecting fish
before checking eels. Fish collection reactivates disabled eels in reverse slot
order, spawning at the edge opposite the player. The twelfth fish sets success
and starts shared ordinary scoring followed by alley return. Ordinary eel
contact sets failure before its thirteen-tick effect and stops the contact scan,
unless success or sinking suppresses harm. Air expiry at the external `0589`
threshold forces sinking, clears horizontal motion and draws rising bubbles
every six accepted updates. `0599` fails the room 72 ticks later. Disabling air
damage retains warning colors but suppresses sinking and failure.
Bowl, actor, surface, swimming and nonblocking contact execution costs use
scoped XT measurements described below. Lethal-contact entry and room
transitions retain provisional allowances.
Controlled routine comparisons establish these rules, not full original-loop
timing or random-stream equivalence.

### Dogs, projectile hits and room failure

Ground-dog initialization (`1e40`) clears active/death state and selects Y177.
Updates (`1e63`) require retrace set and an alternating one/two-BIOS-tick gate.
Dormant ordinary spawns depend on difficulty, player height, entry state and
the shared generator; the bin-cat alert bypasses those spawn gates. Contact
(`20f5`/`2136`) uses inclusive X bounds and player Y at least 163, then starts
the pursuit countdown and relocates the cat. Player updates do not move or
leave floor tracks during pursuit. An alley death resets its acceptance gate
and direction requests; the extra actor group runs every pass without advancing
its ordinary divider. After four clipped exit steps, alley pursuit removes one
life and resets the dog; room pursuit instead sets failure for the common return.

Window launch (`1a05`) selects jitter, type and direction using the shared
generator. Projectile motion (`184b`) requires a changed BIOS tick. Contact
checks (`1b7a`) occur before movement and at the original movement checkpoints,
including before fence clipping or edge reset. First contact pauses gameplay
for ten BIOS ticks (`1166`), then charges one life and reverses the projectile.
Repeated contact retains forced descent without repeating that charge.
Projectile images and the hit effect are composed separately from scenery.

Ordinary failure (`1bf0`/`1d76`) clears the saved window return, performs the
black wipe and removes one life. It cycles through the external bitmap messages
"OUCH!", "ALAS!", "PITY!" and "ARGH!", revealing each over 28 changed ticks.
Message selection is independent of the remaining life count; dog failure
at nonzero difficulty with lives remaining uses
the alternate eight-frame, four-tick dog presentation (`5be0`). The message
baseline is sampled after drawing each image; the final message/dog poll stops
the speaker before the colour wipe begins. The color wipe
returns the cat through a ground-level alley entry. Success takes precedence
when both success and failure flags are present. Zero lives returns to the later
animated title, described below. Headless runs stop at this last-life boundary.

`--disable-damage` bypasses each selected response while leaving ordinary actor
motion and rendering active. `--disable-actors` skips the corresponding actor;
`milk-dogs` controls sleeping attention and its pursuit separately from other
room dogs. `--infinite-lives` preserves life counts without skipping hit effects
or room-failure presentation. These controls require the `dev-tools` feature.
The ten-, nine- and thirteen-tick hit effects suspend other gameplay and resume
at their original caller stage; host pause/quit remain responsive. Their speaker
programs are implemented; instruction-level interrupt placement remains outside
the model.

### Ordinary-room scoring presentation

Ordinary window entry and fishbowl-to-underwater entry also run the shared black
and colour wipes before constructing the destination scenery. Underwater and
Hearts use logical colour one for the second wipe; the other rooms and alley
returns use colour two. The source palette remains active through the black
wipe, then changes to the destination palette. Scenery construction is atomic;
the recreation does not expose the native intermediate drawing stores.

A neutral ordinary-room exit runs the same 28-tick message reveal as failure,
using the fixed external image at `185b`, between its wipes. It preserves lives,
message rotation, score and saved-window return state. Hearts retains its own
falling-cat exit. Gameplay input and actors remain suspended during these
presentations; host pause, settings and quit remain available.

Success enters the shared `1bf0`/`38b0` transition. Two `1c67` wipes expand around
`((playerX+12)&fff0, playerY+8)`, first filling black and finally logical color
two. Width starts at one pixel, grows by 32 and is rounded down to eight-pixel
words for drawing; height starts at eight and grows by 16. Each boundary clamps
independently. The wipes have no explicit tick or retrace wait: their pace comes
from the execution-cost allowance for drawing and sound I/O.

On the stock-XT profile, cumulative wipe time in milliseconds is approximated by
`0.149501 * steps + 0.051169 * rows + 0.002722 * packed_bytes`. Rows and bytes
include repeated overdraw of growing rectangles. Each step adds its drawing work
to this curve; no individual frame timestamps are stored. Even the initial
zero-byte rectangle executes its row loop and consumes time. The measured wipe
body replaces the presentation poll allowance for that step, so dispatch does
not add another copy of its loop cost. This shared curve applies to entries,
ordinary completion, neutral/failure returns and Hearts returns. Sound-enabled
stock-XT checks at different origins agree within about 1.1 ms at drawing-step
boundaries. Tone programming precedes each rectangle; the final tone stops after
the last rectangle's drawing time. The older DOSBox profile retains its previous
byte-based allowance.

Face preparation takes about 0.373 ms, a four-face row about 2.985 ms, and the
complete twelve-row silhouette display about 36.50 ms before its separately
charged face preparation and unmeasured surrounding setup. These fixed native
graphics use their own allowances. Failure messages and the alternate dog
animation use the separate fits below; other presentation sprites and BIOS text
retain provisional byte-based costs. Deliberate tick/retrace waits stay separate.
The separate face/row fits use silent native paths. Exact instruction-level
sound I/O and BIOS sampling within a presentation step remain approximate.

Between wipes, `3a96` masks the external cat-face sprite into writable scratch
data. `3aac` draws twelve rows of four silhouettes, bottom to top, without a
tick wait. The time bonus is sampled after this initial drawing; ordinary
rooms 1..5 use twice the unused part of 1350 wrapping BIOS ticks, while room 6
uses the unused part of 2700 ticks. The external per-room base award is added
to the seven-digit decimal award and then to the current score. High score is
unchanged. The completed-room count increments before the silhouettes; score
addition occurs once after them.

The second pass uses the full sprite and reveals rows from Y176 upwards in
16-pixel steps, stopping at `((2700-timeBonus)>>4)&f0`. Each row, including the
last, is followed by a two-tick wait. Four award digits are then displayed at
X144 and the cutoff Y, preserving leading zeros. The original `3a3a` prints
through the BIOS font; the reconstruction uses attributed SDL 8×8 digit glyphs
in color three, matching the captured CGA/PCjr BIOS reference.

Score-overlay execution still uses provisional packed-byte allowances: 0.256 ms
for four digits, 1.536 ms for the combined Hearts background save and text, and
1.536 ms for restoring its background. Scoped XT captures with GLaBIOS 0.2.6
measure about 7.2 ms, 44.3 ms and 2.2 ms respectively. Most combined-frame time
is BIOS printing; these observations do not establish original IBM BIOS costs
or deliberate pacing. The engine retains its drawing estimates rather than
adding that firmware-dependent slowdown. Text appears as a complete update;
the separate 30-tick ordinary and 68-tick Hearts holds remain unchanged.

The final `3a1c` loop samples a BIOS tick before waiting for retrace set. It
sets logical background color zero to hardware green when sampled tick bit 2
is clear, and black otherwise. After 30 wrapping ticks, it restores black and
runs the final color wipe before initializing the alley. Pending retrace samples
and animation phases remain frozen during a desktop pause. Input, player and
ordinary actors stay blocked throughout; host pause and quit remain responsive.
Sound counters and the synthesized table-driven tone requests advance with these
stages, without additional random draws from the presentation helpers.

The original sets scene zero before presentation. The recreation's scene
diagnostic retains the completed room number until the alley has been
initialized; a dedicated completion state blocks all room work meanwhile. With `dev-tools`,
`--scene completion` starts this sequence directly for testing. The pending bonus flag survives alley
return and selects the hearts scene at the next eligible window entry.

### Milk room schedule

Scene 6 runs controls, pending-tone expiration, drinking (`4943`), sleeping-dog
attention (`47d6`), ordinary player movement, then the broom while alive or
shared ground-dog pursuit while dying, before checking success, failure or exit. Its twelve bowls and seeded sleeping-dog placement use
external tables. Drinking rejects unchanged BIOS ticks, selects the closest
unfinished bowl on the exact matching level, and favors the lower slot on ties.
The original SUB/NOT distance calculation is retained. Alignment replaces the
latched horizontal request; successful drinking advances an eight-bit phase by
48, consuming milk only when that addition carries. Emptying the final bowl
sets success. Normal player timing still commits accepted updates while the
drinking-state flag suppresses movement.

Attention requires elapsed ticks strictly greater than its difficulty-table
threshold. Nearby dogs advance toward alert level two; away dogs can calm using
the shared generator. The warning flag and eye images are retained. Level-two
contact erases that sleeping dog's body and starts shared pursuit at its
position. The cat is relocated to the opposite edge; drinking and floor tracks
stop during pursuit. Its final exit sets the dog-failure marker `552=dd`. The drinking
image is suspended around room actor drawing and recomposed from its current
phase, keeping it out of saved player and broom backgrounds. Disabling
`milk-dogs` removes eye updates and sprites without disabling milk objectives.

Drinking and attention use scoped XT costs described below. Drinking waits for
retrace set before its image update; dog-eye work also waits when it overlaps
the player. An already active retrace satisfies these waits immediately.
Warning audio starts after attention drawing and blocks ordinary game work
until the second BIOS boundary after that entry. Its enabled-sound polling
cadence uses an XT fit; silent-path and PCjr-specific timing remain approximate.
See [PC Speaker programs](#pc-speaker-programs).
The shared scoring presentation uses scene 6's original time bonus:
`max(2700 - elapsed BIOS ticks, 0)`, plus the external room base score. Its
success returns at the saved alley position and makes the next eligible window
lead to hearts.

### Hearts scene schedule and progression

Scene 7 runs controls, pending-tone expiration, common player movement, arrows
(`6106`), automatic pickup (`2f66`), Alt-key placement (`2e60`) and cats (`4c10`),
then tests success/exit. Action is latched by the same two-tick input poll as
directions. These calls bypass the alley's actor divider. Arrows and pickup
reject unchanged BIOS ticks; the cat scheduler commits its tick only at indices
1, 4 and 0, allowing the other cats within a group to run in that same tick.


The stock-XT profile prices these four native calls by their executed paths,
rather than a shared room gate and generic drawing increment. Equal-tick returns,
hidden cats, arrow blocking before/after movement, cat movement/restoration,
active gift scans, pickup latches, placement gates and gift exchanges have scoped
allowances fitted to original-code captures. Counts stop at the actual first hit.
Host compositor redraws do not add another native drawing charge. The retained
DOSBox profile continues to use its earlier whole-call allowances.

Ordinary cat drawing is about 1.92–2.11 ms before an old-image clear (about
0.50 ms); arrow movement is about 3.06 ms and a platform toggle about 6.27 ms.
Gift pickup and placement include the native player-image work; an overlapping
arrow adds roughly 2.56 ms. These are bounded fitted costs, not instruction-exact
execution. Unsampled positions, sprite phases, active-cat placement combinations,
and intra-call sound-request placement remain approximate. Both game modes use
the agreed XT baseline; physical PCjr timing is a separate comparison.

Seven fixed cat rows include a rival on the player's starting row. That bottom
rival (index zero, Y179) starts hidden. It appears after twice the external
difficulty-dependent hiding duration: 1080, 720, 540, 360, 350, 340, 320 or 280
BIOS ticks at difficulties zero through seven (about 59.3 to 15.4 seconds).
Other cats do not move between rows; the goal cat is index six at Y35.

Initializer `300f` draws sixteen heart cells per row but preserves eighteen
support cells. Landing uses `(Y-5)&f8` and shifted/clamped `(X+7)>>4` indexing
from `30fa`, retaining the original mismatch between sprites and geometry.
The horizontal movement helper restricts scene seven to X36..270, inside the
cupid columns; other implemented scenes use X8..290. Overshooting either limit
while airborne stops horizontal motion and requests descent with a two-pixel step.
Reaching the exact limit without overshooting does not trigger that response.
Whole/broken states are zero/two. Each arrow toggles a crossed cell once with
XOR two; a cell latch prevents repeating the toggle within that cell. Cat gifts
use per-difficulty hiding timers; cat zero doubles its timer and the goal cat
does not hide. Their contacts and arrow contacts request downward motion
without life loss. Scene-seven falling continues past Y180, clips below Y187,
and exits by secondary coordinate248 or when its sprite is completely clipped.

The completed ordinary-room count supplies up to eight gifts. A neutral hearts
exit retains that count and other progression state. Its `6040` falling-cat
presentation waits for retrace and changed ticks, then holds for eighteen ticks.
Successful goal-cat contact runs `5060` approach animation and the special
`38b0` score branch, followed by three `519b` pairing animations. The score uses
half the unused portion of 10800 wrapping BIOS ticks since the progression
timer, plus an external base, multiplied by the old difficulty's external
multiplier. Carrying a gift doubles that multiplier. The displayed four digits
show the base-plus-time amount; the multiplier label shows the separate factor.
The score display flashes the background for 68 ticks, while approach and pair
frames use their original distinct tick delays.

After scoring/pairing, `528b` adds a life up to nine, increases difficulty up to
seven, clears the ordinary completion count and resets the progression timer.
The shared black wipe is followed by `5313` decoration for new difficulties
two and above, using five-tick frame gates for forty ticks, then the final color
wipe. Both outcomes reconstruct the alley and its HUD with the updated state,
restore the saved window coordinates and start descending. Presentation blocks
gameplay throughout; pause freezes virtual time and quit remains responsive.

Scene-specific palette tables, carried-gift background changes and presentation
palette requests are independent of framebuffer memory. BIOS font lettering uses
the attributed SDL 8×8 glyphs shared with the pause display; scoring characters
match the captured CGA/PCjr BIOS font.
Speaker synthesis and native sound random consumers are implemented. The
approach sweep's counted sound delay uses an XT fit. The XT profile also prices
callable presentation helpers separately from their deliberate waits:

- Celebration frames cost about 10.34–87.76 ms for the original difficulty
  lists of 4, 8, 16, 24, 26 or 34 decorations. Both image phases use the same
  fitted per-copy cost; the five-tick cadence and forty-tick lifetime remain separate.
- The approach cat draw costs about 24.88 ms and its panel 0.93 ms.
  Pairing renders one image on its initial frame and eight thereafter, but
  omits simulated sprite-drawing latency. Its three groups retain their native
  one-, two- and three-BIOS-tick gates and movement/bookkeeping allowances.
  One position update takes about 33–101 microseconds, depending on movement
  and clamping. The measured small/medium/large drawing fits (2.00, 7.35 and
  24.88 ms) remain available for isolated helper comparisons; pairing does not
  charge them or a fallback byte allowance. See the
  [pairing presentation policy](deviations.md#hearts-pairing-presentation).
- Each falling-cat copy costs about 0.77 ms, including the equal-sized blank
  erasing its previous position. Its landing image costs about 1.47 ms.

These are scoped helper fits, not general sprite-area rates or complete animation
measurements. The opaque copy loop is independent of pixel values; the transparent
save/draw loop branches on source pixels, not the saved background. Sprite source
and shape therefore matter, while changing only the background does not change
their instruction paths. Video phase and interrupts still limit repeatability.
Charged drawing advances the same timeline as waits, rather than adding a second
delay after them. Pairing excludes drawing latency to avoid reproducing historical
throughput stalls. Its tick gates still use integer BIOS ticks, so their elapsed
duration depends on the starting tick phase.

Score arithmetic uses the transition-bookkeeping allowance below. Bounded Hearts
state work has separate XT allowances, additional to polling and drawing:

| Operation | Allowance |
| --- | ---: |
| Approach initialization | 0.0700 ms |
| Approach movement and destination calculation, before drawing | 0.1163 ms |
| Completed approach return, without drawing | 0.0381 ms |
| Pairing group initialization | 0.5676 ms |
| Life/difficulty reward and progression reset | 0.1062–0.1155 ms |
| Celebration initialization / difficulty-gated skip | 0.1204 / 0.01095 ms |

Reward work adds 0.0040 ms when a life increases and 0.0053 ms when difficulty
increases. Each stage reports this work once; subsequent wait polls do not repeat
it. Sprite-byte accounting is independent, and the older DOSBox profile has no
additional Hearts bookkeeping charge. Both software display modes use the XT fit.

These fits use relocated native instruction blocks with drawing and waits omitted.
Pair initialization includes eight silent melody checks but excludes the outer
group loop and its call dispatch. All 24 sampled cases fit within 0.005 ms of their
calibration medians; relocation, prefetch and interrupts limit precision. Enclosing
dispatch, score text and polling outside the measured phases below retain
provisional allowances.
Sound-enabled execution and request timestamps within a drawing phase are not
independently fitted by the muted captures. Controlled helper/frame comparisons
do not establish full original-loop timing or random-stream equivalence.

### Completion polling

On the XT profile, these individual passes replace the generic presentation-poll
allowance while the corresponding wait phase is active:

| Wait loop | Sound disabled | Sound enabled, no new note | Extra for a note update |
| --- | ---: | ---: | ---: |
| Hearts approach/pairing | 0.1081 ms | 0.1882 ms | 0.0833 ms |
| Ordinary score-row reveal | 0.0830 ms | 0.0830 ms | — |
| Hearts celebration | 0.1056 ms | 0.1841 ms | 0.0658 ms |

The sound flag is the original game's flag, independent of host mixer levels.
Note work is added only when the native melody/tone cursor advances. The
representative note-update fit also covers rests approximately; equal-note stop
branches and every table entry are not separately calibrated. Drawing and one-shot
bookkeeping remain separate. The older DOSBox profile retains its generic poll
allowance, and both software display modes share the XT fit.

These values price polling, not the complete wait. Full aged captures preserve
the real tick loops; ordinary intervals between BIOS reads isolate their per-pass
costs. Their phase-dependent totals are not added as extra delays. The native
two-tick reveal, approach/pair delays and five-tick celebration cadence remain
intact. Native sound and decision reads still share one supplied BIOS sample in
the adapter; interrupts between the original reads can change a boundary decision.

The score-flash `3a1c` helper still uses virtual-retrace polling with its existing
allowance. Its measured total includes the current video phase and its palette
tail uses configured BIOS video service; neither is treated as a universal fixed
delay. Score-flash and enclosing completion dispatch remain outside these fits.

### Initial splash

The first title presentation (`5cb0`) draws the external title artwork over its
fence, sidewalk and bin scenery (`2a30`). The cat starts at X0/Y96 with the
original clipped edge entrance. Its automatic direction routine (`5dd4`)
turns inward at X32/X288 and considers a shared-generator direction choice
after 18 BIOS ticks in the interior. Retrace must be set before the existing
32/16-call player gate is even entered. Movement and sprite tables remain the
same as the alley's; the title supplies horizontal step four.

Copyright drawing (`5e3b`) alternates the two external bitmap entries at
`6a8f`. SynSoft appears first; a changed elapsed interval of 36 BIOS ticks
selects IBM, then alternates back. A fresh key make dismisses the title after
the animation opportunity. The first entry expires strictly after the external
`56da` duration plus six ticks: with the supported input, elapsed tick 852
continues and tick 853 exits. Copyright updates precede this expiry test.
The music sequence follows its native tick gate, including rests and the
non-advancing end marker, and emits PIT channel-two writes.

The host uses the virtual BIOS/retrace clock and scoped XT title-helper costs.
Steering with the outer retrace gate clear takes about 0.041 ms at the left
edge, 0.052 ms at the right, 0.131 ms for a recent interior decision, and
0.183–0.193 ms for a fresh random decision. A copyright redraw takes about
1.45 ms. Accepted player calls cost about 0.241 ms during the entry countdown,
1.836/2.794 ms for the two clipped shapes, 3.032 ms for the final handoff and
2.953 ms for ordinary walking. These player values include their acceptance
gate; the title wrapper does not add another gate allowance. Copyright drawing
and player work are both charged when they share a pass. Call-count rejection,
idle drawings and other unsampled paths retain existing player allowances.

These are muted, isolated helper fits on the shared XT baseline. Title/music/key
bookkeeping and the extra wrapper work on its retrace-set path remain approximate;
an entire title pass still uses one supplied BIOS/retrace sample. The native
32/16-call gates, steering thresholds, copyright interval and timeout rules are
unchanged. CPU work is independent of host presentation.

The sampled scenery initializer takes about 119.49 ms for its selected seed;
its six artwork copies total about 10.81 ms, plus the first copyright redraw.
Static construction produces the initial image outside the title-loop clock.
These measurements are recorded separately and introduce no added startup wait;
other seeds, setup callers and full startup timing remain uncalibrated. Exact
playback cadence is not established by these helper captures.
The completed first presentation hands its shared data to the menu and initial
game. Native scenery and animation random draws therefore influence the initial
alley. The first menu selects skill without reseeding or clearing that state.
A new gameplay/audio presentation epoch starts after the menu; title timers and
other shared fields remain in the carried data, while run initialization resets
its own fields. This does not establish instruction-level BIOS/video phase
continuity across title, menu and gameplay.
Explicit development scene previews bypass startup. In a splash preview, finite
limits count virtual BIOS ticks and frame exports contain the splash itself.

### Last-life return to the title

The main loop returns through `0081` after the final life. High-score comparison
(`2690`) happens before title drawing. The later `5cb0` entry preserves current
score, high score, selected skill (`6df8`), sound flag, shared random state and
ordinary-room history. Its scenery uses difficulty zero and the selected-skill
fence sign; the title's lives display is nine.

Unlike the first entry, this title never expires. At elapsed duration `56da`
plus 72 ticks (918 with the supported input), it resets the title, music and
copyright time baselines before considering movement or key dismissal on the
next pass. The cumulative key make remains pending across that reset.

A fresh key exits; the previously initialized menu flag (`41a`) skips `5ee5`.
The next run copies the selected skill to active difficulty, clears current
score and progression, sets three lives and starts the clipped alley entry.
It retains high score, room history and the random state advanced by title
scenery and animation. Host pacing and pause state restart at this boundary.
The CLI's initial difficulty override is retained for subsequent
runs. Speaker output restarts at a new host synthesis epoch while native sound
preferences and the carried game state remain intact.

### Keyboard menu

The post-title routine (`5ee5`) clears the framebuffer, asks about joystick use,
then displays the four skill choices. The recreation omits joystick setup and
starts with the skill question. String pointers at `6d37` and cursor positions
at `6d63` supply the external text and rows; `5e5b` forces column zero. BIOS
teletype output uses color index two and 8×8 lettering, reproduced with the
attributed font shared with scoring.

K/H/T/A select `6df8` values 0/1/2/3. Startup copies the selected skill to the
initial gameplay difficulty; subsequent progression changes difficulty without
changing the selected-skill fence sign. Controls are appended beneath the skill
question, and `5f97` waits for a new key make before returning. Invalid skill
keys leave the question unchanged. These menu waits have no timeout and do not
advance the reconstruction's gameplay clock or random stream. They retain the
random state already advanced by the title; only explicit later manual menu or
restart requests reseed it.

The host also accepts Ctrl+R to restart and Ctrl+M to select a new run, including
while paused. Returning to play resets host pacing and pause state, preserving
the high score and development controls. The two updated instruction rows and
explicit CLI difficulty bypass are described in [deviations](deviations.md).

### Reference execution costs

The default profile targets one stock 4.77 MHz 8088 PC/XT baseline for both
CGA and PCjr, preserving the game's PCjr branches. Bounded MartyPC 0.4.1 captures
use the IBM 5160 configuration, turbo off, at `157500000/33 Hz`. They measure
executed guest cycles, independently of host display refresh and tracing speed.
This remains an execution-cost model, not an instruction-level CPU. Ordinary
player actions are normalized as described below; other helpers use scoped fits.

The measurements below cover selected routine states and positions. Their fit
checks do not independently establish whole-game pacing, instruction-level
interrupt placement or random-stream synchronization. Path-specific limits and
uncalibrated work are identified alongside the costs.

Ordinary cat actions use shared execution allowances across the alley and the
fishbowl, bookcase, cheese, birdcage and milk rooms. Comparable Hearts walking,
idle, descent and silent platform landings also use these allowances. These are
chosen modern pacing values based on the XT measurements, rather than a promise
to reproduce every measured search or drawing overhead:

| Accepted action | Execution allowance |
| --- | ---: |
| Walking, complete eleven-row image | 2.720 ms |
| Idle redraw | 2.480 ms |
| Idle without redraw | 0.360 ms |
| Begin falling after losing support, without redraw | 0.360 ms |
| Jump initiation from floor or ordinary furniture | 0.300 ms |
| Rising | 2.870 ms |
| Unsupported descent, complete twelve-row image | 3.240 ms |
| Ordinary silent twelve-row landing | 3.230 ms |
| Settled attached hold | 0.115 ms |

These are simulated execution costs, not additional waits or frame periods.
The BIOS-tick, call-count and retrace gates still decide when an action can run.
Movement distances, acceleration, animation, support selection and room-specific
rules remain unchanged. A deeper support-table match no longer increases the
cost of an ordinary walk, jump, idle, fall or landing. The first matching entry
is still selected by the native search. Floor marks retain their native counts,
tile selection and pre-movement cell indexing, but drawing a new mark does not
increase the shared walking allowance.

Floor takeoff retains the current image. Ordinary state-zero furniture uses the
same jump-initiation cost when its successful support hit clears into an upward
transition. Displacement starts on subsequent accepted calls. Losing support
and starting a stationary fall without drawing uses the shared 0.360 ms
no-redraw allowance, including the bookcase's failed furniture search. This
is a policy allowance, not a universal measured support-search duration or a
full falling movement update.

Ordinary silent alley-floor landings use 0.950 ms plus 0.190 ms per retained image
row, for complete six-byte-wide sprites of ten through thirteen rows. Twelve-row
floor landings in the five ordinary rooms use the same 3.230 ms total. Ordinary
state-zero furniture landings use the same row adjustment: 2.850 ms at ten rows,
3.230 ms at twelve and 3.420 ms at thirteen. Their existing retained-image,
shape and support-result guards remain in force. Other image histories keep
separate or fallback allowances.

Ground idle and ordinary state-zero furniture idle share the same redraw and
skipped-draw costs. The modulo-eight idle animation gate and redraw-only random
consumption are preserved; no table-setup or per-visited-entry charge is added.
Swimming and special Hearts clipping paths retain their own costs.

Ordinary lower-list landing and takeoff retain separate sound bookkeeping:
3.179 ms for landing or 0.641 ms for takeoff, plus 0.050 ms for an initially clear
support-sound latch. Each emitted beep adds its blocking duration and a fitted
0.055 ms caller/programming allowance. Landing emits attachment only with a clear
latch. Takeoff can attach and release in the same accepted call, producing two
beeps with a clear latch or one with an already set latch. Native mute suppresses
the tones while retaining the logical latch changes. These paths are distinct
from the ordinary silent landings in the table above.

Bounded original-data comparisons cover movement/state fields, complete video
memory, floor-mark and support-search outcomes, and speaker programs. Repeated
original cheese-room loops on the floor and early/deep furniture retained the
same acceleration and animation sequence and all eighteen matched countdown
expiry pairs. The deeper condition shifted the second update by about 1 ms in
these samples. This supports omitting incidental search work; it does not prove
universal cadence or whole-game random-stream synchronization. Capture
instrumentation adds work, and native IRQ phase varies. Normalized allowances
are checked as policy values, separately from the original measured durations.
Physical PCjr timing remains unmeasured; the older DOSBox profile is unchanged.

Hearts common-player movement remains separate from the scene's cat, arrow,
gift and presentation helpers. Ordinary solid-platform walking, idle redraw and
skipped idle use the shared 2.720, 2.480 and 0.360 ms allowances. Unsupported
descent with an actual platform search uses 3.240 ms, and silent platform
landings with a complete retained twelve-row jump image use 3.230 ms. Native
support-lookup work no longer gives these actions different pacing from other
rooms. Platform geometry, gaps, movement boundaries and fall/exit rules remain
scene-specific.

These paths require no entry/death or attached state, with the corresponding
visible, non-alternate saved walking/idle/transition shape. The descent and
landing guards require the actual support-search flag. The selected lower-edge
draw from twelve saved rows to ten visible rows retains its separate 2.659 ms
fit. It covers Y189 with the same original twelve-row source, ten drawn rows
and no support search. Further valid clipped histories use the row-based fits
below; alternate images and hidden-image reappearance retain their fallback.
Jump initiation keeps its existing no-redraw fallback; rising uses the shared
2.870 ms allowance and losing support over a gap uses 0.360 ms.

Twenty cases pair FC/FD software IDs on the shared XT. Forty repeated
measurements plus two controls match 53 selected fields, complete backing and
presented video memory, actual support/landing flags and zero PIT-tone programs.
The selected captures measure walking near 2.896 ms, idle redraw 2.678 ms,
skipped idle 0.547 ms, descent 3.028–3.085 ms and landings 3.017–3.076 ms.
Normalized policy values are checked separately from these measured durations;
they are not exact native timing fits. This bounded comparison samples fixed
sprites, positions and difficulty zero, with landings at two platform heights.
It excludes other geometry/shapes, actor collisions, the
acceptance gate, outer failure dispatch, whole-loop cadence and physical PCjr.
Native motion, platform results, modulo-eight idle gating, redraw-only random
consumption and deliberate waits are unchanged. Execution costs do not establish
intended delays; the legacy profile keeps its previous timing.

Further Hearts clipping separates setup, restoration and drawing. For a valid
visible saved image from the native twelve- or thirteen-row transition shape,
the XT profile charges 0.060 ms per restored row and 0.128 ms per drawn row.
Setup is 0.650 ms for lower-edge clipping, 2.000 ms for partial top clipping and
1.893 ms when the sprite becomes fully hidden at the top. The existing full
12→10-row control keeps its 2.659 ms total. The larger top allowance includes
the native 360-iteration loop; these are bounded aggregate fits, not instruction
or per-row cycle counts. No additional wait or intermediate presentation is added.

The player reports clipping kind and actual restored/drawn rows only when the
previous visible dimensions agree with the current top/bottom geometry. Hidden,
alternate, mismatched and other original sprite shapes are excluded. Metadata
comes from the actual clipping branch, including zero drawn rows on the hidden
return. The final bottom exit retains the native coordinate clamp and exit flag;
its one- or two-row image is charged without running the outer failure sequence.

Eighteen cases pair FC/FD software IDs on the shared XT: progressive lower
clipping, one/two-row bottom exits, a thirteen-row fall shape, partial
top clipping and a fully hidden return. Thirty-six repeated measurements plus
two controls match 53 selected fields, complete backing/presented video memory,
clipping metadata and zero PIT-tone programs. All totals are within 0.012 ms of
the medians. Native random consumption remains zero and idle/motion/wait rules
are unchanged. Coverage uses fixed positions, movement phases, support delay,
difficulty and sources; other clipping rows, alternate images, hidden reappearance,
actor collisions, whole-loop cadence and physical PCjr remain outside this
same-capture calibration. The legacy profile retains its previous charges.

Already-attached upper-support bodies share allowances across the alley,
fishbowl, bookcase, cheese, birdcage and milk rooms: 2.925 ms for the first
fourteen-row cling pose, 0.115 ms for a settled hold, 0.281 ms for jumping off
and 0.266 ms for dropping off. The first pose advances support state one to two;
release preserves that image while selecting the next airborne sprite. These
calls produce no tone or random draw. Leading room checks do not need different
allowances. Entry, death and combined pose/release paths retain their separate or fallback
costs.

Initial upper-support landings separate shared execution work from the native
two-BIOS-tick sound wait. Alley bitfield, bookcase shelf and eligible ordinary
room-table hits use the same prefix and suffix; search depth adds no time:

| Retained image | Prefix before sound | Suffix after sound | Total CPU work |
| --- | ---: | ---: | ---: |
| Ten-row side fall | 0.780 ms | 2.265 ms | 3.045 ms |
| Twelve-row left/right jump | 0.780 ms | 2.640 ms | 3.420 ms |
| Thirteen-row stationary fall | 0.760 ms | 2.8275 ms | 3.5875 ms |

The suffix adds 0.1875 ms per restored/drawn row above ten; the stationary
thirteen-row image keeps its 0.020 ms prefix adjustment. Eligibility requires an
initially descending, unattached cat, a complete retained airborne image and a
new state-one support hit. The alley/bookcase admit the listed ten-, twelve-
and thirteen-row shapes. Other ordinary rooms admit twelve-row left/right and
thirteen-row stationary images with an actual table hit. Clipped, alternate,
other-shape and otherwise ineligible images retain their separate allowances.

The prefix advances the game clock before the helper samples its BIOS tick and
PIT phase. The wait still lasts until two BIOS ticks have elapsed from that
sample, including with native sound off. No fixed captured total replaces it.
Normalization can therefore alter the phase-dependent pulse and random counts;
the sound polling algorithm and PRNG rules remain unchanged. Checks use several
initial PIT phases to verify current-tick sampling, pitch/seed consistency,
final stop and the two-tick boundary, alongside selected state and complete
final graphics. These checks do not establish exact native pulse counts,
instruction-level IRQ placement or whole-loop phase equivalence. The older
DOSBox profile retains its existing charging rules.

Bin-forced downward transitions instead separate bookkeeping from committed
sound. Their baseline is about 0.687 ms, plus 0.050 ms for an initially clear
support latch and 0.088 ms for first contact. Each actually emitted beep adds its
existing blocking duration and a fitted 0.055 ms caller/programming allowance.
The attachment, three-tone first-contact effect and release can produce five,
four, two or one beeps, depending on the initial latches; native mute emits none.
The trio is included in this charge, not added again as a separate whole helper.
Upper-support noise remains separate work.

Sixteen complete accepted-body cases pair native mute/enable and both latch
states with FC/FD software machine IDs on the same XT hardware. Muted medians
are 0.687–0.885 ms; audible totals span 23.970–117.177 ms for FC and
12.321–59.123 ms for FD. The latter retains its half-length native sound loops.
All fitted totals are within 0.150 ms of these medians, with selected state,
full video memory and PIT-program checks. This is a bounded same-capture fit;
two repeats show up to 0.126 ms spread. It does not establish arbitrary contact
geometry, whole-loop cadence, individual instruction placement or physical PCjr
timing. The earlier DOSBox profile retains its previous composition.

Clipped alley entry retains its twelve accepted-call countdown; an active dog
can hold that countdown. Hidden countdown bodies take about 0.122 ms. The first
and second cropped walking frames take about 1.711 and 2.669 ms, including native
cropping and saved-background work. Final handoffs take about 2.550 ms for idle,
2.726 ms for continued walking and 0.402 ms for a jump request without redrawing.
These costs replace only the selected accepted-body allowance, not the player
gate or a fixed entrance-duration wait. Left/right and sampled animation-wrap
paths share the fits; other actor contacts remain outside this calibration.
The native edge initializer measures about 1.050 ms but is not charged again
separately from an enclosing caller allowance.

On the last entrance step, logical X moves inward before controls are processed.
An idle redraw retains the previous image offset, while walking calculates a
new destination. Jump requests keep the existing image on that step. These
drawing decisions are independent of the fitted execution costs.

A valid running-dog helper measures about 8.489 ms, including background restore
and draw; its active increment is above the roughly 0.094 ms quiet allowance.
Moving projectiles add about 2.644 ms above the inactive helper. Clothes shifts
add about 7.804 ms, or 8.770 ms with tile generation; normal mouse redraw adds
about 1.362 ms above equal-tick work. These are scoped conditions, not universal
costs for every actor state.

Window calls preserve their thirteen-call countdown, retrace-clear gate and
explicit fully-open hold. A countdown rejection measures about 0.009 ms;
expiry rejected by retrace takes about 0.040 ms. Ordinary black opening rows
measure 0.409 ms and copied closing/bonus rows about 0.435 ms. Starting the
fully-open timer adds about 0.075 ms, and its expired-hold path adds about
0.085 ms to the closing work. Player/mouse contact holds measure about
0.330/0.286 ms, and an ordinary open hold about 0.359 ms. Launching its projectile
adds about 0.190 ms without drawing a projectile image yet.

Successful new-window selection takes about 0.620 ms on the sampled one-draw
path, and 1.027 ms when two overlap retries are needed. Work records count actual
selection draws without adding random calls. The top-row restriction's BIOS
check has a bounded allowance; other retry/position combinations remain fitted
rather than instruction-exact. Runtime charges window work above its quiet
countdown share of the existing grouped-call allowance. Quiet grouped work and
the window's explicit delays are not charged a second time. Initialization
measures about 0.031 ms but remains within uncalibrated caller setup. The native
difficulty-six delay and its deliberate correction remain separate timing policy.

Bin animation uses valid saved-background rectangles at six emergence/retraction
phases. A bounded fit uses about 0.399 ms per active call, 0.0394 ms per restored
row, 0.268 ms per drawn row and 0.105 ms for a new spawn, above quiet grouped
work. The measured frames differ from that fit by at most about 65 microseconds;
short/final frames have proportionally larger errors. These are coefficients
for the fixed original sprite, not general physical per-row costs.

Airborne mouse-contact catches on all three rows measure about 7.51 ms, including
score/HUD drawing and the initial catch tone. The active allowance is about
7.408 ms above the roughly 0.104 ms inactive-contact check already represented
in quiet work. The native continuation remains BIOS-tick-driven. The separate
CGA bin-contact three-beep helper measures about 69.9 ms; its sound time is
counted once within that total. The original PCjr software machine-ID branch
measures about 35.108 ms on the same XT CPU baseline. This checks the halved
counted loops without claiming a separate physical PCjr hardware measurement.

Nonblocking cage movement, including background restore and drawing, measures
about 3.54–3.61 ms. A center-contact push, including player restoration/redraw,
measures about 2.688 ms. The respective active increments are 3.378 and 2.488 ms
above the provisional 0.200 ms room-actor gate. The push increment accounts for
player restoration/redraw when the cage moves the player.
These fit selected positions and do not establish every cage-contact cost.
The blocking fall uses a separate fit: about 0.0966 ms per quiet BIOS poll,
3.210 ms for each restore/five-pixel fall/draw above that poll, and 3.893 ms
for the final restore/broken-cage image/release. An audible step adds about
0.0264 ms for PIT programming. Native read-interval histograms separate quiet
polls from drawing and interrupt outliers. Whole falls from heights 134 and
159 preserve their seven-/two-changed-tick waits; their measured durations are
phase-dependent, not fixed delays. Dog-pause and entry/caller overhead remain
approximate; these cases do not measure complete room transitions.

The bird's separately entered motion/drawing body measures about 1.04–1.09 ms
on first appearance and 1.333 ms when it restores its previous image. Normal XT
play prices the whole scheduler call using the decision fit below; these isolated
motion costs are not added again. Cage measurements use native-muted inputs;
its occasional asynchronous tone-request overhead remains approximate.

Bird scheduler (`4340`) calls on the shared XT baseline measure 1.434–1.957 ms
across sampled avoidance, waypoint and random-direction paths. Native same-tick,
unreleased-cage and broom gates use 0.079, 0.111 and 0.279 ms respectively.
The active fit counts actual random draws, separation tests, waypoint following,
releases, direction changes, requested tones, moving axes and saved-background
restoration. It consumes no extra random values. First-appearance and visible
calls therefore pay their respective work once, replacing the generic room gate
and fixed bird-drawing allowance.

The fitted whole-call base is 1.332 ms, including normal first drawing. Each
random draw adds 0.030 ms; separation testing 0.1142 ms; waypoint following
0.0438 ms; waypoint release 0.0455 ms; direction changes 0.023 ms; each moving
axis 0.008 ms; and old-background restoration 0.2775 ms. An audible asynchronous
request adds 0.1661 ms plus its counted random draw (about 0.196 ms total).
Native sound enable/busy flags select this work independently of host volume.
All eighteen sampled whole-call medians fit within 0.011 ms. These correlated
coefficients are bounded fits, not independently measured instruction costs.

The shared original difficulty thresholds, fourth-slot tick rule, waypoint tables
and sound-dependent random consumption are unchanged. Selected-state, complete
VRAM, random-draw and sound-program comparisons cover the captured cases, not
whole-room cadence or every seed/position. Post-motion broom suppression retains
a conservative active allowance including first drawing; that collision branch,
exact in-call tone timestamps and additional boundaries remain approximate.
The older DOSBox profile retains its generic gate and fixed drawing price.

The shared broom helper (`3150`) measures about 0.097 ms for a rejected tick,
3.807 ms for first drawing and 5.507–5.522 ms for ordinary visible movement
across the five furnished rooms. Its XT allowance separates the tick check,
first drawing and optional old-background restoration. Floor cleaning also
restores the old image before redrawing; it pays that restoration once. The
same drawing fit underestimates the measured cleaning path by about 0.204 ms;
its track update and movement decisions remain a bounded approximation.
Blocked/contact paths use about 0.297 ms total against measured 0.235–0.369 ms.
These selected native-muted paths do not calibrate every random decision,
asynchronous sound request or complete room loop.

Bookcase spider movement (`3cb1`) measures about 5.661 ms for first drawing
and 6.486–6.637 ms for the sampled visible movement paths. The allowance splits
first drawing from old-background restoration: about 5.461 and 0.890 ms above
the provisional 0.200 ms room-actor gate. Plant collection (`3b42`),
including native player/spider background work, measures 9.093–9.197 ms; its
active allowance is about 8.929 ms above that gate. Final collection and
failure-suppressed success retain the same fit. These native-muted measurements
exclude the spider-contact image. Its separate transparent draw measures
14.201–14.261 ms at center and clamped edge positions, fitted as 14.241 ms in
addition to caller work. The nine-tick sound wait starts after image drawing;
its blocking sound polls have a separate XT fit.
The quiet tick checks, plant misses and broom blocks/rollback now use the
separate measured return costs below. Active drawing retains its 0.200 ms gate
plus the existing increments. More positions, plant populations, sound-request
overhead and full bookcase passes remain unverified.

Cheese mouse calls (`40c2`) measure about 1.967 ms for first appearance,
2.640–2.674 ms for sampled visible updates and 4.078 ms for collection, including
native player-background and HUD work. The fit separates collection from drawing
and adds restoration only when a new mouse image is also drawn. Restore-only
relocation conservatively uses the first-drawing allowance, about 0.195 ms above
the selected measurement; random retries and other positions remain approximate.
Difficulty comparison keeps the same room/positions and varies its threshold
lookup: distant updates at levels 0/7 both measure 2.657 ms; at a closer position,
levels 0/1 measure 2.657–2.659 ms and levels 2–7 retreat in 2.696–2.697 ms.
Those branch differences fit the same allowance; the native difficulty table
still decides retreat, without a separate difficulty timing multiplier.

Hole transfers (`3e90`) measure about 5.918–5.958 ms for ordinary visible-broom
frames and about 6.975–7.006 ms at start/end. The fit separates ordinary frame
work, endpoint player-background work and optional broom drawing/restoration.
The 120-byte broom restoration reuses its measured shared allowance; transfer
broom drawing has its own increment, excluding the movement decisions in `3150`.
Hidden-broom start measures 5.267 ms and dog-suppressed broom start 2.248 ms,
checking that optional work is counted once. Measured inactive, same-tick, cooldown, collected-slot and broom-blocked returns
use the quiet costs below. Other contacts and active paths retain their existing
gate. These are selected native-muted paths, not complete difficulty-specific
room loops or exhaustive random-retry timing.

Milk drinking (`4943`) measures about 0.010 ms during pursuit, 0.097 ms for a
rejected tick, 0.160 ms for released action, 0.398 ms for a bowl-height miss and
0.500–0.505 ms for the sampled alignment paths. Native drinking-image restoration
(`4b03`) measures 0.680 ms; AND/save drawing (`4b1d`) measures 1.323–1.327 ms
across selected directions and animation halves. Consumption adds about 0.677 ms
for bowl work and depletion about 0.063 ms. Starting/stopping also accounts for
normal-player restoration, background saving or redrawing using its dimensions.
Host compositor redraws do not add extra native execution charges.

Sleeping-dog attention (`47d6`) measures about 0.095 ms for rejection and
0.112 ms during pursuit. One quiet active slot measures 0.435 ms; a staged
twelve-slot quiet scan measures 0.694 ms and twelve distant alert slots about
3.998 ms. The fit counts actual slot tests and eye updates; twelve-slot cases
bound the helper rather than describe a natural dog population. Eye copying
(`4916`) measures 0.114 ms for either orientation. Overlapping eye updates
restore and redraw the appropriate normal or drinking image.

The waits at `4aa2` and `4902` use the virtual retrace phase rather than a fixed
average duration. CPU work before the poll can move it beyond the current pulse;
work after the final status read is charged separately. Sound requests use the
post-wait time, and warning playback follows all attention work. Thirty-six
fixed/aligned checks fit within 0.150 ms, with a largest sampled median difference
of about 0.076 ms. Unrecorded starting phases only support timing envelopes.
The shared pursuit entry (`2136`) adds a one-time 9.850 ms allowance when the
normal eleven-row cat image is retained, either by the ordinary compositor or
in the suspended normal-image cache while drinking. Both activation paths use
the same entry cost. Its original temporary dog image, restoration and
relocation are included; preceding sleeping-body removal and attention scanning
remain separate. Other retained image shapes keep their existing allowance.
Measured full normal/drinking activation bodies take about 16.76–16.88 ms, but
include cleanup and construction work; this total is not added as a separate
delay or charged again over the shared entry. Other bowl-search patterns, broom
overlap, lethal caller cleanup, audible-request overhead and complete natural
room pacing remain approximate.

Complete running-dog contact/entry (`20f5`) uses 3.120 ms in the alley, including
its edge reset, and 2.065 ms in ordinary rooms. Both return sides and FC/FD
software IDs share each fit. These apply to a complete eleven-row player and
fifteen-row dog image; other saved-image shapes retain their existing allowance.
Entry is charged once, whether player work or dog movement detects the contact.
It replaces no pursuit tick gate, movement step or animation rule. Existing
caller/gate allowances remain separate; their overlap and unsampled movement
preludes remain approximate. Twenty-six XT samples cover these three contexts;
all entry fits are within 0.050 ms of the sampled medians. This does not establish
complete natural-contact or whole-room pacing.

Fishbowl animation (`3850`) measures 0.752–0.754 ms, or 0.781 ms when contact
sets the entry latch. Its quiet tick check measures 0.085 ms. The fit keeps that
check separate from drawing and uses one drawing allowance for both phases and
contact; entering the underwater room is outside this measured call.

Underwater actors (`3675`) measure 0.078 ms for a rejected tick and 0.145 ms for
an accepted inactive slot. First fish drawing measures 0.708 ms; restoring the
previous image and drawing again measures 1.044–1.049 ms. The smaller eel image
measures 0.538 ms first and 0.694–0.703 ms when visible. Runtime costs distinguish
fish/eel images and charge restoration only when it occurs, above quiet and
accepted-call work. The sampled wrap prelude measures 1.074 ms with a visible
fish and shares the ordinary allowance, differing by about 0.030 ms. Isolated
slot-zero body measurements exclude that prelude. PCjr software-ID midpoint
cases, at player Y20/Y80, measure 0.695/0.702 ms on the same XT/CGA baseline and
retain their native latch decisions. They are not physical PCjr hardware
measurements.

Water-surface updates (`37e5`) measure 0.362–0.373 ms, including the cursor-wrap
case. Quiet rejection measures 0.085 ms and a player-adjacent skipped tile
0.139 ms; the fit distinguishes accepted opportunities from actual drawing.
Twenty controlled paths, each measured twice with empty-call subtraction, check
these totals within 0.040 ms. They cover selected positions and sprite phases,
not random-direction branches, all animation images, contact scans or swimming.

Accepted swimming bodies (`0926`) measure 3.396–4.385 ms on the sampled ordinary
CGA paths, with the old player image restored, a new native image drawn and an
all-inactive contact scan. Air-expiry/bubble frames measure 4.914–4.917 ms.
The fit separates a 0.700 ms movement/palette prelude, optional bubble work,
old-player restoration, new-player drawing and contact work. Pure native
`11e3`/`1145` calls measure five actual player shapes, from 9 rows of 4 packed
bytes to 14 rows of 6 bytes. Restoration measures 0.544–0.928 ms and AND/save
drawing 0.919–1.818 ms. Their setup/row/byte coefficients are fitted allowances,
not universal CGA-memory access costs. Recording both previous and new dimensions
avoids treating every animation frame as equally expensive.

Nonblocking underwater contacts (`34a0`) measure 0.978 ms with every slot inactive
and 3.099 ms for 24 active misses. A hidden/visible fish measures 2.121/2.456 ms;
two visible fish measure 3.131 ms. The fit counts active tests, contacts, the
first player restoration and actual fish/eel restorations. An accepted swimming
body also redraws the player after contacts, as the native caller does. Final-fish
success and immune/sinking eel contacts retain native ordering and reactivation.
Lethal eel-contact work has separate bounded measurements. A single-slot active
miss costs about 0.107 ms, versus 0.145–0.146 ms for a lethal decision. Its
0.0386 ms increment is retained only for unsupported diagnostic combinations. The opaque failure-image
copy (`3506..3538`) costs about 1.526 ms across eight sampled positions, including
its coordinate clamps. Movement, player drawing, the scanned contacts and this
image are charged before sampling the thirteen-tick effect's baseline. The
post-contact player redraw is not charged before a lethal wait.

A lethal scan records how many slots it visited and actively tested. Complete
prefix captures (`34a0` to `3506`) cover lethal slots 12, 16 and 23 with varying
numbers of earlier active misses. They replace the prorated quiet-scan estimate
with a fitted 0.127 ms fixed allowance, 0.0375 ms per visited slot and 0.0881 ms
per earlier active miss. This total already includes the final lethal decision;
the generic scan and single-slot decision costs are not added again. The
failure-image copy retains its separate allowance.

Nine no-image totals, about 0.615–3.052 ms, fit within 0.005 ms of their sampled
medians. Nine paired image-inclusive prefixes (`34a0` to `3538`) measure about
2.161–4.697 ms and fit within 0.150 ms, with a largest difference of about
0.118 ms. The longest image-inclusive case has about 0.124 ms of repeat spread;
two samples do not identify its cause or justify another fixed cost. Eighteen
scoped state/full-VRAM comparisons preserve actor flags, lives and native image
output. These are same-capture fits, not independent timing validation.

Mixed prefixes additionally cover one or two fish collected before a lethal eel
at slot 12 or 23, hidden or visible fish, and a case with 21 earlier active
misses. Collection adds a fitted 0.215 ms per fish above an inactive slot;
collected fish are excluded from the active-miss count. The first player
restoration and each visible fish restoration retain their separate rectangle
allowances. This replaces the prorated fallback for these mixed paths without
adding the generic contact costs or charging restoration twice.

Nine mixed inputs with paired endpoints measure 1.653–4.813 ms before the
failure image and 3.178–6.337 ms including it. The composed allowances fit those medians
within 0.050 ms, with a largest difference of about 0.026 ms. Twenty-one scoped
state/full-VRAM comparisons also cover three complete nonlethal controls: a
protected eel, sinking fish/eel contacts, and final-fish success followed by an
eel. These controls retain the existing nonblocking timing allowances. Protection
is global within a scan, so a protected eel cannot precede a lethal one unless
state is changed externally.

The prefix captures exclude subsequent effect sound initialization, effect-loop
polling and the surrounding accepted swimming caller. Native waits remain
separate. Both software modes share the XT fit; physical PCjr timing is not
established. Same-capture fits, relocated slice code and two repeats do not
establish exact execution costs for every input. The older DOSBox profile retains
its prior lethal-entry timing.

Asynchronous contact requests (`593b`) have separate sound-enabled/muted pairs.
Direct calls measure about 0.009–0.011 ms muted and 0.135–0.137 ms enabled,
including PIT programming and a BIOS timer sample. An already-pending request
is replaced normally. FC/FD software IDs show the same cost on XT hardware.
The fitted extra work is 0.126 ms per enabled underwater request, above the
muted call already represented in contact allowances. One- and two-request
mixed prefixes measure extra work of 0.126 and 0.252 ms respectively. Six paired
deltas fit within 0.005 ms; four composed prefix totals fit within 0.050 ms.

Contact metadata counts requests using the native sound-enable flag, independently
of host volume or host muting. The surcharge applies once to both lethal and
nonlethal underwater scan routes. It is CPU execution work, not a wait for the
tone's duration; asynchronous requests remain nonblocking. Other callers and
already-audible whole-call fits receive no global surcharge. Twelve scoped
state/full-VRAM/PIT-program comparisons cover the direct and mixed cases.
Exact within-call audio timestamps, other audible caller costs and physical
PCjr behavior remain outside these measurements.

Complete accepted-body checks (`0926`) also cover inactive controls, hidden and
visible fish, two-fish collections, final-fish success and both horizontal
direction changes, each with native sound off/on. Twelve selected cases measure
3.395–6.344 ms. Nonblocking contact bookkeeping uses 0.200 ms per contact above
the separate gate/active-test allowances; first-player restoration, actual actor
restoration and the caller's redraw remain separate. The existing movement,
image, sound-request and lethal-prefix coefficients are unchanged.

Actual `Alley::update_swimming` state, backing/presented VRAM and PIT-program
counts match all twelve native cases. Its returned work is priced by the actual
runtime; all totals fit the existing 0.150 ms accepted-body bound, with a largest
difference of 0.113 ms. Two cases have about 0.115–0.121 ms repeat spread, whose
cause is not established by two samples. These complete-body measurements check
composition against separately measured helper fits; they do not prove arbitrary
contact populations, exact interrupt timing or the surrounding room-loop cadence.
The enclosing acceptance gate, lethal waits and physical PCjr remain outside
this batch. The 41 movement/contact/split-helper comparisons and older DOSBox
charges also remain within their established checks.

The native FD near-surface branch (`0b72`) waits for retrace clear then set before
a counted delay. Its variable full-body duration is not a fixed CPU surcharge.
Runtime advances to the next virtual retrace start, skipping a current pulse,
then charges about 2.637 ms for post-poll counted work before player composition.
Phase-aligned tail captures measure 21.956/22.976 ms from clear/set alignment,
versus 3.714 ms for the post-wait tail alone. First/last 3DA read cycles separate
polling from post-poll work; full-body duration without a recorded start phase
can only be checked against a timing envelope.

Scoped state/full-frame and charging checks cover 41 cases across the full-body
and split-helper batches. Fixed/aligned totals fit within 0.150 ms; the largest
sampled difference is about 0.125 ms. Other palette/animation phases, audible
requests, rare contact combinations, masked-actor cleanup and physical
PCjr BIOS/video timing remain outside the calibration. Both game modes use the
same XT CPU baseline.

Control hold/poll helpers measure about 0.084/0.306 ms. Gate and BIOS sampling
splits remain approximate; whole-helper totals do not identify each instruction
boundary. Quiet grouped-call work uses a fitted 0.800 ms allowance. Active work
is additional to that baseline rather than charged as another entire helper.
Consecutive original idle/walking batches of 64 and 256 passes check the combined
fit. Their counter instrumentation adds measurable overhead, and their exact
initial BIOS/video phases are unknown. Phase/overhead sweeps compare duration,
final position and BIOS tick together within that uncertainty range.

`ReferenceCosts::dosbox_x_240` retains the older DOSBox profile for explicit
comparisons. Its room drawing allowances do not include the XT profile's added
work breakdown or phase-dependent swimming surface wait. Ordinary play uses
`ibm_xt`; unmeasured contact/reset callers, room actors, transitions, entry
callers and startup-specific work retain provisional allowances. Some of those derive
from DOSBox-X normal core at fixed 240 cycles/ms, an emulator throughput setting
rather than a physical CPU clock. Changing measured costs is not a global speed
multiplier and does not complete physical-PC timing calibration. Midnight clock
reset and original keyboard transport remain outside this model.

Actor helpers currently share one BIOS sample within each nonblocking call;
an original interrupt between its internal timer reads can change a boundary
decision, such as a mouse's 53/54-tick respawn. Instruction-level interrupt
placement is outside this execution-cost model.

### Score, HUD and decoration execution costs

Scoped stock-XT measurements of the original helpers give these approximate
durations. Values include each helper's drawing where indicated; they are not
additional presentation waits.

| Helper | Measured duration |
| --- | ---: |
| High-score comparison/promotion (`2690`), selected early/late decisions | 0.027–0.165 ms |
| Lives display (`26b3`), unchanged cache | 0.011 ms |
| Lives display, one digit redrawn | 0.443–0.444 ms |
| Current/high score (`26fc`/`26f2`), seven digits | 3.376–3.377 ms |
| Alley point award (`2706`), including current-score redraw | 3.503–3.513 ms |
| Seven-digit decimal addition (`271e`), without drawing | 0.209–0.210 ms |
| Clear one score array (`26da`/`26e1`) | 0.043 ms |
| Ground decorations (`5400`), three difficulty/seed examples | 1.278–4.479 ms |

The calibrated mouse-catch allowance already includes its score update and
redraw. These measurements therefore do not add another HUD charge to that
path. Setup/transition callers still need separate accounting to avoid charging
shared work twice. Ground-decoration examples do not establish an exhaustive
seed-dependent cost model. Two repeats per case and approximate empty-call
subtraction also leave interrupt-phase uncertainty. The recreation presents
ordinary construction after its memory updates, without exposing intermediate
CGA-bank or tile writes.

### Actor initialization and scene setup

Post-scenery setup suffixes use these approximate stock-XT allowances:

| Destination | Setup allowance |
| --- | ---: |
| Alley, ground-level return | 8.267 ms |
| Alley, saved window return | 8.354 ms |
| Fishbowl | 1.590 ms |
| Underwater | 5.330 ms |
| Bookcase | 1.667 ms |
| Cheese | 2.924 ms |
| Birdcage | 3.764 ms |
| Milk | 1.600 ms |
| Hearts | 3.1495 ms |

The XT runtime charges the combined allowance once when synchronizing a scene
change. Player-entry preparation, actor resets, logical tone initialization and
any score redraws within that suffix are included; their isolated helper costs
are not added again. Ground-level and saved-window returns use their own
measured suffixes. Initial static launch remains outside this scene-change
accounting. The older DOSBox profile has no added setup charge. Both game modes
use the shared XT baseline.

These bounded fits come from selected difficulty/seed states and reassembled
native caller suffixes, after scenery preparation. Random placement retries and
interrupt phase can change individual durations. The appended caller's relocated
CALLs and final RET also limit instruction-level equivalence. Initialization
currently samples supplied BIOS time rather than reproducing each internal
timer-read position. Full scenery construction and initial startup work remain
separate; these small setup charges neither add bulk construction waits nor expose partial CGA-bank writes.

### Failure presentation execution costs

Masked failure-message drawing (`1e17`) takes about 2.843 ms for the four original
messages with sampled masks. Each large dog image copy takes about 4.832 ms;
the header takes 1.144 ms and each detail copy about 1.817 ms. Runtime counts the
header/detail work as well as the main image, replacing their generic byte cost
on the XT profile. Two-sample medians with interrupt outliers differ by up to
about 0.062 ms; message identity/mask do not change the fixed copy-loop bounds.

Steady message polling takes about 0.1026 ms with native sound muted and
0.1861 ms when audible; dog polling takes about 0.1056/0.1911 ms. Advancing the
sound table adds about 0.0754 ms, fitted to a nonzero note. Terminal return/stop
adds about 0.0577 ms for a message or 0.0612 ms for the dog, above its poll.
Zero-note stops and exact placement of the sound and decision BIOS reads remain
approximate; the adapter supplies one BIOS sample per poll. Native sound flags,
not host mixer volume, select these paths.

Terminal and aged one-tick wait captures separate polling from graphics. Their
whole durations depend on PIT entry phase and do not replace the original
28-step message reveal or four-tick dog frame gate. Each message wait starts
after drawing; the final return retains its last drawing baseline. Complete
failure playback and surrounding transition costs remain outside these helper
fits. Life/message rotation uses the separate bookkeeping allowance below. The older DOSBox profile retains generic
byte-based pricing, now including previously uncounted dog header/detail bytes.

### Blocking effects and diagnostic modes

Fence fall runs the edge initializer before starting the blocking effect. The
runner clears logical directions, accepted tick and countdown, retaining the
input-poll baseline, divider and last accepted AX. `FenceEffect` retains the
original two-tick initial wait, variable call countdown, wrapping accumulator,
and seven-tick exit before a random call. It advances the shared random state
once per completed effect cycle even with silent output. Ordinary controls,
player and actors remain suspended until the effect returns; then the interrupted
pass resumes with post-player work.

Fence polling and random/reload costs use scoped XT fits described under
[PC Speaker programs](#pc-speaker-programs). Internal timer/I/O placement remains
approximate, so exact random-call counts are not established. Its port-61 toggles
are synthesized, and ambient sound-state random consumers are included.

Mouse knockdown (`2567`) clears attachment and horizontal direction, sets a
17-update support delay and downward direction, and preserves vertical step,
animation phase and sprite history. Its temporary image blocks input, player
and scenery work until eight wrapping low BIOS ticks have elapsed. The
`5a90` model emits a tone and consumes one random value per changed tick when
native sound is enabled. The sound timer sample precedes the exit timer sample,
so the exit can observe the eighth tick while the last tone belongs to the
seventh. A tone on the final tick is therefore not guaranteed. Completion
resumes after the interrupted actor batch without advancing its divider again.

The XT fit prices about 0.1056 ms per muted poll, or 0.1810 ms per audible
same-tick poll plus 0.0754 ms when a new tone is programmed. The audible loop
splits work around its two BIOS samples, about 0.0995 ms before sound sampling
and 0.0815 ms between sound and exit sampling. Drawing the temporary image adds
about 5.409 ms before taking the eight-tick baseline; final background cleanup
adds about 0.742 ms. Poll/cleanup totals match the sampled terminal calls within
about 0.005 ms. The initial collision/caller split and exact interrupt placement
remain approximate. Whole-effect duration varies with PIT entry phase; these
costs do not replace the original elapsed-tick wait.

The following diagnostic controls require a build with `dev-tools`.
`--step-hz` explicitly selects the former uniform diagnostic movement scheduler.
Untimed headless execution also advances accepted movement bodies directly.
These accepted-update diagnostic modes do not advance scenery or actor schedules.
`--headless --timed` executes the reference schedule without wall-clock waits;
script rows choose held directions by accepted-update index. `--trace-timing`
records loop attempts and blocking-effect checks, including rejected movement.
On player attempts the BIOS/PIT fields describe the BIOS-sample stage; retrace
is sampled later, after the gate's execution allowance.
The windowed runner uses monotonic host time only to pace virtual execution.
Each host sample adds at most 100 ms to the pacing target, discarding longer
stalls without resetting guest phases. Catch-up yields after 256 complete
passes or a 4 ms host time slice, whichever comes first; a single pass can
overrun that slice. Pending work remains due. The runner polls events again
before continuing and skips intermediate presentation and sleep while behind.
These bounds affect host responsiveness, not guest execution costs or BIOS
ticks. Display refresh never supplies BIOS ticks.

Gameplay, title, skill-menu and pause redraws use the shared SDL frame ceiling.
The runner checks whether presentation is due without sleeping in that check,
so event polling and due simulation work continue between capped redraws. VSync
may still block presentation itself; elapsed host time includes that wait and
simulation catches up afterward. Pending catch-up work skips intermediate
presentation. There is no additional engine-level redraw ceiling.

Gameplay directions and Alt retain SDL key-transition timestamps. The runner maps
them onto the paced virtual timeline, including during catch-up. Interactive
play defaults to Responsive input: direction and action requests are refreshed
on each ordinary pass. The original control-gate work is still charged only
when that gate opens, so this adds no virtual execution cost or movement updates.
Original input, selectable under Alley Cat overrides → Controls, samples only at
the modeled control gate. Headless reference runs retain Original input.

Both modes preserve movement gates, blocking effects and timestamp ordering.
A press spanning an overdue sample is retained, and a later press does not affect
that earlier sample. A complete tap between samples can still be missed; taps
are not latched until consumption. Opposing directions retain left/up precedence,
and either Alt key holds the action.

When a host interval exceeds 100 ms, only its latest 100 ms are retained. Older
input changes collapse to the start of that interval, preserving its final held
state without replaying discarded time. Earlier catch-up debt retains its own
history. Settings pause preserves that debt; manual pause discards it as before.
Paused input is not replayed, settings keys remain suppressed until released,
and restart/title return begins a fresh history. Host shortcuts remain immediate.

This is host-event history, not original keyboard transport or instruction-level
input emulation. Missing/delayed OS events, snapshot-only state corrections and
events arriving after an atomic guest pass has already crossed their timestamp
cannot be reconstructed retroactively. Within-helper key-read/interrupt placement
also remains approximate. SDL keyboard timestamps use the same nanosecond clock
as [SDL_GetTicksNS](https://wiki.libsdl.org/SDL3/SDL_GetTicksNS).

The original keyboard pause (`5e70`) saves the BIOS tick count, displays the
pause captions, waits for a later keyboard make event, then restores that count
with `INT 1a/AH=1`. Key releases do not resume it. The text is drawn through
BIOS `INT 10/AH=0e`, using the BIOS ROM font rather than glyphs in CAT.EXE.
Its saved CGA rectangle covers source pixels `(40,88)` through `(295,103)`;
the two text rows start at `(40,88)` and `(40,96)`, in palette color two.

The recreation toggles pause on fresh Esc, P or Pause/Break presses, with no
Alt/Ctrl/Super modifiers. Other keys and focus changes leave the pause state
unchanged. This default usability exception keeps desktop shortcuts such as
Alt+Tab from resuming the game. SDL auto-repeat events are ignored to avoid
resuming while a pause key remains held. The host runner stops all guest work
while paused and removes elapsed host time from its pacing budget. It preserves
the complete virtual clock, including
PIT/retrace phases; the original restores BIOS ticks without restoring those
hardware phases. Diagnostic pacing shifts pending movement, input and blocking
effect deadlines by the pause duration. The overlay is a presentation layer and
does not alter the packed framebuffer or actor background saves. Its current SDL
font is a visible approximation of the original BIOS lettering, and its resume
hint describes the toggle keys rather than the original any-key behavior.


### PC Speaker programs

`Sound` translates the original speaker helpers into ordered PIT-clock writes
on ports 43h, 42h and 61h. Note/divisor, pattern and waveform tables come from
the external executable. The shared `reforge_audio` PIT and PC Speaker devices
execute those writes, and the band-limited renderer supplies mono PCM to SDL
at 48 kHz. Audio rendering and host buffer requests do not advance game state.

Sound paths cover the title, asynchronous first/continuation notes,
ambient/dog/broom patterns, collection tones, damage programs, milk warning,
hearts sweep/melodies, failure messages, falling cage, hearts failure
drop/landing and presentation wipes. Spider/eel pulses and fence changes use
speaker-control bits directly. Ambient and upper-landing random calls use the
same generator as gameplay. Host enable, volume, device selection and focus
mute affect playback only; Ctrl+S changes the original sound flag and its
native predicates.

Counted beeps and the hearts sweep preserve the original divisor/write sequence.
Their stock 4.77 MHz XT fits use 18 effective CPU cycles per counted iteration,
plus 112 cycles for the beep gate window or 360 for the sweep caller/helper
cadence. Four CPU cycles correspond to one PIT input clock on this baseline.
The measured CGA beep gate is about 23.195 ms and the sweep note interval about
15.523 ms. PCjr game branches halve the original loop counts while using the
same CPU baseline. The sweep waits with native sound disabled; a disabled beep
returns without its counted delay. PIT-counter bursts use the mode-three
counter-difference threshold to determine their wait.

Milk warning and upper landing end at the second BIOS boundary after entry.
Milk-warning pulses use 153 PIT clocks per poll, about 128.23 microseconds.
Upper-landing polls use 261 PIT clocks with native sound enabled and 169 with
it disabled, about 218.74 and 141.64 microseconds respectively. The enabled
path includes random note programming; the disabled path consumes no random
values. The PCjr milk-warning branch changes the waveform's phase shift but
shares the XT polling allowance.

The blocking spider effect uses about 151.8 microseconds per pulse and ends
after nine BIOS ticks. Eel contact uses a thirteen-tick wait with a
retrace-dependent sequence: poll/pulse until retrace is set, emit an extra
pulse, apply the saved outer BIOS sample's blue/white background, emit another
pulse, then sample time and restart. Its polling allowances are about
84.53 microseconds on the retrace-poll path, 224.13 around the palette call,
and 148.57 around the outer time check. Native-muted paths still advance the
waveform counters.

Fence-reset pulse spacing uses about 462 XT cycles per counted BIOS check
plus 380 cycles per random/reload cycle. Its accumulator/countdown algorithm
and two-/seven-tick gates also run when native sound masks the output. Runtime
charges the whole counted-check cost at the BIOS sampling boundary. Its entry
(`59cb..59df`) costs about 0.093 ms; the final shared speaker-stop call costs
about 0.020 ms. Spider initialization and its BIOS baseline (`3dcd..3dd8`) cost
about 0.106 ms; its final failure-flag assignment adds about 0.006 ms before
the caller's shared speaker stop. Eel initialization and its BIOS baseline
(`3538..3543`) cost about 0.122 ms, including the initial speaker stop. The
effect baselines are sampled after these setup allowances. Exit work is charged
only when the effect finishes, separately from its final polling allowance.

Complete descending fence falls with a retained twelve-row image use a shared
2.4 ms player/reset body allowance for both return sides. It replaces the
generic airborne body. Eight complete accepted-body cases pair left/right,
native sound on/off and FC/FD software IDs on the same XT. First/final speaker
writes isolate about 2.393–2.407 ms of surrounding work from the phase-dependent
effect; setup/return overlap at these port boundaries remains approximate.
The existing effect entry, polling and final-stop allowances remain separate,
preserving the two-/seven-tick gates and shared random consumption. The native
reset (`070d`) alone measures about 1.053 ms including background saving, versus
0.235–0.238 ms without that save; neither is added again to the complete body.
Other image histories retain their previous allowance. These costs introduce no
intermediate presentation or extra deliberate wait. Other
unmeasured caller work likewise remains approximate. Both
software modes share these XT fits; the FD branch is not a physical PCjr
measurement. Other
room effects use the shared room-effect polling allowance where no specific
cost applies.

These execution costs are bounded fits rather than instruction-level CPU
emulation. Entry/exit overhead, individual timer reads, I/O placement and
interrupt timing remain approximate. Actor writes are timestamped at their
execution stage, with within-stage instruction timing unresolved. Silent sweep
helper overhead and silent milk-warning polling cadence are also approximate;
the latter preserves its two-tick wait and unchanged waveform phase. Both game
modes use the XT baseline; a separate physical PCjr cadence is not established.
The shared device and renderer models do not establish physical speaker response
or acoustic fidelity.

Controlled comparisons check sound-state changes and ordered speaker-port
writes, including the modeled low two bits of port 61h. They do not establish
full-game port timestamps or audible waveform equality. Synthetic checks cover
note/rest gates, counter wrap, native versus host muting, blocking work,
category gains and PCM continuity across chunk boundaries.

The host keeps at most 100 ms in SDL's queue, feeding longer committed blocking
programs from a bounded two-second staging buffer. Pauses preserve pending
playback; run/title resets clear it. Applying audio settings discards stale
presentation audio. Device failures warn and leave game logic running.
Uniform accepted-update diagnostics remain silent because they have no
meaningful speaker timeline. The timed headless runner executes sound logic
without retaining output writes.

### Transition bookkeeping

The XT profile charges arithmetic and state updates separately from drawing,
sound and deliberate waits. Each phase reports its work once; later wait polls
cannot award points or charge that arithmetic again. The older DOSBox profile
has no additional bookkeeping charge. Both display modes use the shared XT fit.

| Operation | Captured duration |
| --- | ---: |
| Room selection, pending Hearts | 0.132 ms |
| Room selection, sampled ordinary paths | 0.223–0.317 ms |
| Ordinary-room score arithmetic | 0.836–1.957 ms |
| Hearts score arithmetic | 2.183–9.491 ms |
| Progress increment and pending-Hearts flag | 0.0136–0.0138 ms |
| Life decrement and message rotation | 0.0344–0.0390 ms |

Selection counts actual candidate attempts, additional random draws and history
rejections without consuming extra random values for timing. The allowance is
0.132 ms plus 0.0915 ms per candidate, 0.0353 ms per extra draw and 0.0022 ms per
history rejection. Longer retry paths extrapolate these bounded cases; they are
not independently calibrated for every seed.

The original binary-to-decimal conversion adds a seven-digit value for each set
bit of the bonus. Ordinary arithmetic uses 0.837 ms plus 0.224 ms per set bit.
Hearts uses 0.610 ms plus the same bit allowance and 0.238 ms per multiplier
iteration. These are bounded execution fits, not exact instruction costs: carries,
interrupts and the separately sampled BIOS time limit precision. All nine sampled
award cases fit within 0.052 ms. The already included decimal helpers are not
charged again. Progress uses 0.0137 ms; life/message rotation uses 0.0387 ms with
positive lives or 0.0344 ms at zero. Preserving lives through a development option
retains the corresponding native-path allowance.

The captures isolate reassembled native blocks and selected caller suffixes.
Graphics, BIOS text, waits and omitted branch dispatches are outside arithmetic
markers. Two repeats, approximate empty-call subtraction and relocation/prefetch
changes limit precision. Controlled original-state comparisons cover the selected
fields and final VRAM; they do not establish complete caller timing, exact internal
BIOS-read placement or whole-game equivalence.

### Quiet room-helper returns

On the XT profile, these sampled non-drawing paths replace the generic 0.200 ms
room-actor gate with their complete measured return allowance:

| Helper | Return path | Allowance |
| --- | --- | ---: |
| Cage | Same tick | 0.07847 ms |
| Cage | Already released | 0.09565 ms |
| Cage | Idle without a push latch | 0.21256 ms |
| Spider | Tick interval not reached | 0.08465 ms |
| Spider | Broom blocks before movement | 0.18333 ms |
| Spider | Broom rolls back proposed movement | 0.42973 ms |
| Plants | Same tick | 0.07962 ms |
| Plants | No plant collected | 0.35912 ms |
| Cheese mouse | Same tick | 0.07962 ms |
| Cheese mouse | Already collected slot | 0.13148 ms |
| Cheese mouse | Broom blocks movement | 0.30632 ms |
| Hole transfer | Inactive early guard | 0.03331 ms |
| Hole transfer | Active, same tick | 0.08800 ms |
| Hole transfer | Cooldown not finished | 0.18019 ms |

The helper reports the return path it actually executes. The runtime charges its
quiet allowance once, rather than adding it to the old gate. Active drawing,
collection, restore and transfer frames keep their existing calibrated prices;
unsupported quiet/contact branches retain the generic allowance. The older
DOSBox profile also keeps its previous gate. Both software display modes use
the shared XT baseline.

These rounded medians reuse the bounded cage, bookcase and cheese captures.
They are not new whole-room measurements. Different plant populations, early
transfer guards, positions, sound requests and interrupt phases can change the
work within a category. Selected native-state/full-VRAM comparisons cover the
sampled quiet paths and neighboring active cases; exact full-loop cadence and
physical PCjr timing remain separate validation.
