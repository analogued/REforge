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
windows, the bin cat and mice. Each samples virtual time at its own call stage.
Damaging actors are absent; their post-player work retains fitted time allowances.

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
the next input poll. Its contact also alerts the omitted dog.

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
launching and its random draws are omitted. The original one-tick hold at
difficulty 6 is corrected to 18 ticks by default; development builds can
restore it with `--original-window-timing`. See [deliberate deviations](deviations.md).

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

Active mouse and transfer drawing costs are provisional allowances, separate
from these original gates. Silent audio and omitted sound random consumers mean
a matching seed alone does not synchronize an original full-game run.

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
Until all rooms are implemented, dispatch makes a separate seeded choice among
scenes 1, 3, 4 and 5, using the upper two bits of one additional shared random
value. A separate two-entry history prevents three identical playable destinations
in a row. Original
candidate history is therefore not a history of actual ordinary destinations.
Success preserves the pending-bonus flag; the next eligible window selects scene
7 without
advancing the selector's random state or ordinary candidate history.

Logical room tone state is retained without audio output. A pending tone
expires once per distinct tick rather than catching up skipped ticks. Random
tone requests from the bird follow their original predicates. Ambient sound
random consumers and instruction-level interrupt placement remain omitted;
full original-loop random-stream equivalence is not established.

### Bookcase room

Scene three follows the original caller order: input, shared tone expiry,
accepted player update, spider (`3cb1`), plants (`3b42`), broom, then termination.
The spider accepts at least two wrapping BIOS ticks between updates; plants
accept one changed tick and consume at most one target in reverse slot order.
The last plant sets success only while failure is clear. Spider contact reports
an overlap while allowing movement to continue: the original nine-tick lethal
effect and failure flag remain disabled.

Bookcase shelf support (`3c43`) precedes the ordinary furniture table but follows
the descending room-window exit test. It aligns X to four pixels, accepts
164..280 inclusive, and snaps to shelf rows 42, 58, 74, 90, 106, 122, 138 and
154 using the original byte arithmetic. These supports span the bookcase
independently of the randomized book tiles. Spider and plant drawing execution
costs are provisional; no full original-loop pacing equivalence is claimed.

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
damage and the air timeout's sinking/failure path are disabled during development;
the original air-timer warning thresholds still change the background palette.
Swimming, bowl, actor and surface execution costs remain provisional. Controlled
routine comparisons establish these rules, not full original-loop timing or
random-stream equivalence.

### Ordinary-room scoring presentation

Success enters the shared `1bf0`/`38b0` transition. Two `1c67` wipes expand around
`((playerX+12)&fff0, playerY+8)`, first filling black and finally logical color
two. Width starts at one pixel, grows by 32 and is rounded down to eight-pixel
words for drawing; height starts at eight and grows by 16. Each boundary clamps
independently. The wipes have no explicit tick or retrace wait: their pace comes
from the execution-cost allowance for drawing and sound I/O.

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

The final `3a1c` loop samples a BIOS tick before waiting for retrace set. It
sets logical background color zero to hardware green when sampled tick bit 2
is clear, and black otherwise. After 30 wrapping ticks, it restores black and
runs the final color wipe before initializing the alley. Pending retrace samples
and animation phases remain frozen during a desktop pause. Input, player and
ordinary actors stay blocked throughout; host pause and quit remain responsive.
Sound counters advance with the reviewed table-driven tone requests, without
audio synthesis or random draws from these presentation helpers.

The original sets scene zero before presentation. The recreation's scene
diagnostic remains five until the alley has been initialized; a dedicated
completion state blocks all room work meanwhile. With `dev-tools`,
`--scene completion` starts this sequence directly for testing. The pending bonus flag survives alley
return and selects the hearts scene at the next eligible window entry.

### Hearts scene schedule and progression

Scene 7 runs controls, pending-tone expiration, common player movement, arrows
(`6106`), automatic pickup (`2f66`), Alt-key placement (`2e60`) and cats (`4c10`),
then tests success/exit. Action is latched by the same two-tick input poll as
directions. These calls bypass the alley's actor divider. Arrows and pickup
reject unchanged BIOS ticks; the cat scheduler commits its tick only at indices
1, 4 and 0, allowing the other cats within a group to run in that same tick.

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
Ambient sound random consumers, synthesis and sound busy-loop timing remain
incomplete. Controlled helper/frame comparisons do not establish full original
loop timing or random-stream equivalence.

### Reference execution costs

The execution-cost profile is fitted to guest-time measurements from DOSBox-X
normal core at fixed 240 cycles. The number 240 describes emulator throughput;
it is neither a 240 Hz game tick nor the native host CPU's frequency. Costs are
approximate stage allowances, not execution of original instructions or a
cycle-accurate 8088 model.

Representative CGA costs include about 0.35 ms for an accepted idle call without
redraw, 3.05 ms for ordinary walking, and 3.27 ms for ordinary jumping. Post-player
work costs about 0.096 ms on an ordinary pass plus 0.346 ms on a grouped pass.
Active clothes scrolling adds approximately 7.9 ms, or 9.4 ms when generating a
new tile, above that grouped allowance. Original fixed-240 helper measurements
gave about 8 ms for a shift, 9.5 ms with generation, and 0.1 ms for a tick-gated
hold. These costs remain approximate; they do not establish every garment path
or interrupt phase. Active mouse redraw adds about 1.55 ms; catching adds about
7.6 ms, fitted from full original helper calls of about 1.6 ms and 8 ms.
Active bin animation adds a bounded fit by drawn and restored row counts; full
original calls range from about 0.43 ms when hiding to 6.04 ms at full height.
Active window work adds about 0.48 ms, based on fixed-240 row-helper samples
near 0.5 ms; start and contact-hold paths share this approximate allowance.
Room actor gates, cage/bird/broom drawing and blocking-fall polling use
provisional allowances that have not been independently cost-calibrated.
Hearts actor drawing and presentation work also use provisional allowances.
Completion drawing currently uses a provisional allowance per copied byte;
its explicit two-tick reveal waits and 30-tick final hold remain independent
of that fit. No exact fixed-240 transition duration is claimed.
Room tick predicates and blocking waits are retained, but this does not
establish the room's original real-time movement cadence.
The bin cat's first-contact sound still incurs a silent delay: about 81.5 ms for
CGA or 40.9 ms for PCjr, measured with original native calls at fixed240. PCjr
halves each of its three `1800h` delay-loop counts. Subsequent contact with its
latch set skips that effect. The ordinary downward transition uses the existing
support-release cost allowance; its separate support-tone bookkeeping remains
incomplete, and PCjr still shares that general CGA cost fit.
Other omitted actor drawing can cause larger delays that
the allowance does not reproduce. Keyboard decoding, retained support state and redraw paths
also vary. Clipped re-entry costs have not been separately calibrated. PCjr uses
the CGA cost fit with its verified 16-call countdown, pending separate cost fits.

The original fixed-240 samples show two accepts per complete BIOS tick during
CGA/PCjr walking and about one during CGA idle. The model reproduces those sampled
cadences; this does not establish exact timing within each tick. Software
breakpoints perturb the stage measurements, and the profile does not model every
path, interrupt latency, the BIOS midnight reset, or hardware keyboard transport.
Physical-PC timing fidelity is not established.
Actor helpers currently share one BIOS sample within each nonblocking call;
an original interrupt between its internal timer reads can change a boundary
decision, such as a mouse's 53/54-tick respawn. Instruction-level interrupt
placement is outside this execution-cost model.

### Blocking effects and diagnostic modes

Fence fall runs the edge initializer before starting the blocking effect. The
runner clears logical directions, accepted tick and countdown, retaining the
input-poll baseline, divider and last accepted AX. `FenceEffect` retains the
original two-tick initial wait, variable call countdown, wrapping accumulator,
and seven-tick exit before a random call. It advances the shared random state
once per completed effect cycle even with silent output. Ordinary controls,
player and actors remain suspended until the effect returns; then the interrupted
pass resumes with post-player work.

Effect-loop costs use bounded fixed-240 helper measurements and remain approximate,
so exact random-call counts are not established. Audio synthesis and ambient
sound-state random consumers remain absent.

Mouse knockdown (`2567`) clears attachment and horizontal direction, sets a
17-update support delay and downward direction, and preserves vertical step,
animation phase and sprite history. Its temporary image blocks input, player
and scenery work until eight wrapping low BIOS ticks have elapsed. The silent
`5a90` model consumes one random value per changed tick when sound is enabled,
including the final tick check. Completion resumes after the interrupted actor
batch without advancing its divider again. Effect polling has an approximate
execution cost; the elapsed-tick exit and random predicate are retained.

The following diagnostic controls require a build with `dev-tools`.
`--step-hz` explicitly selects the former uniform diagnostic movement scheduler.
Untimed headless execution also advances accepted movement bodies directly.
These accepted-update diagnostic modes do not advance scenery or actor schedules.
`--headless --timed` executes the reference schedule without wall-clock waits;
script rows choose held directions by accepted-update index. `--trace-timing`
records loop attempts and blocking-effect checks, including rejected movement.
On player attempts the BIOS/PIT fields describe the BIOS-sample stage; retrace
is sampled later, after the gate's execution allowance.
The windowed runner uses monotonic host time only to pace virtual execution and
discards excessive pause debt without resetting guest phases. Display refresh
never supplies BIOS ticks.

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
