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
Active dog and projectile drawing costs remain provisional allowances.

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

Active mouse and transfer drawing costs are provisional allowances, separate
from these original gates. Sound routines now consume their native random draws,
but provisional execution timing still prevents claiming that a matching seed
alone synchronizes an original full-game run.

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

Room tones now emit ordered speaker writes. A pending tone expires once per
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
contact sets failure before its thirteen-tick effect and stops the contact scan,
unless success or sinking suppresses harm. Air expiry at the external `0589`
threshold forces sinking, clears horizontal motion and draws rising bubbles
every six accepted updates. `0599` fails the room 72 ticks later. Disabling air
damage retains warning colors but suppresses sinking and failure.
Swimming, bowl, actor and surface execution costs remain provisional. Controlled
routine comparisons establish these rules, not full original-loop timing or
random-stream equivalence.

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
the alternate eight-frame, four-tick dog presentation (`5be0`). The color wipe
returns the cat through a ground-level alley entry. Success takes precedence
when both success and failure flags are present. Zero lives returns to the later
animated title, described below. Headless runs stop at this last-life boundary.

`--disable-damage` bypasses each selected response while leaving ordinary actor
motion and rendering active. `--disable-actors` skips the corresponding actor;
`milk-dogs` controls sleeping attention and its pursuit separately from other
room dogs. `--infinite-lives` preserves life counts without skipping hit effects
or room-failure presentation. These controls require the `dev-tools` feature.
The ten-, nine- and thirteen-tick hit effects suspend other gameplay and resume
at their original caller stage; host pause/quit remain responsive. Audio and
instruction-level interrupt placement remain outside this implementation.

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

Milk-room active-drawing costs are provisional allowances, not independently
calibrated CPU timings. Warning audio now blocks ordinary game work for its
original two-tick interval; the internal polling cadence remains provisional.
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
Speaker synthesis and native sound random consumers are implemented. Busy-loop
execution rates remain provisional. Controlled helper/frame comparisons do not
establish full original-loop timing or random-stream equivalence.

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

The host uses the virtual BIOS/retrace clock and provisional loop/drawing costs
from the XT profile. Splash-specific costs and exact playback cadence are not established.
The first presentation has a separate data copy and does not advance the gameplay
seed or clock; integrating the full original shared startup state remains deferred.
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
The development CLI's initial difficulty override is retained for subsequent
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
advance the reconstruction's gameplay clock or random stream.

The host also accepts Ctrl+R to restart and Ctrl+M to select a new run, including
while paused. Returning to play resets host pacing and pause state, preserving
the high score and development controls. The two updated instruction rows and
explicit CLI difficulty bypass are described in [deviations](deviations.md).

### Reference execution costs

The default profile targets one stock 4.77 MHz 8088 PC/XT baseline for both
CGA and PCjr, preserving the game's PCjr branches. Bounded MartyPC 0.4.1 captures
use the IBM 5160 configuration, turbo off, at `157500000/33 Hz`. They measure
executed guest cycles, independently of host display refresh and tracing speed.
This remains a fitted execution-cost model, not an instruction-level CPU.

Accepted original movement bodies measure about 0.360 ms without redraw,
2.480 ms for idle redraw, 2.717 ms walking, 2.961 ms rising, and 3.244 ms descending.
Lower-support landing/release allowances include their one short beep, about
26.514/23.923 ms in total. Runtime subtracts the committed beep duration from
that allowance before advancing the sound clock, so it is counted once.
Two-beep forced-release paths and other unmeasured contact combinations remain
approximate. Upper-support noise and the bin-contact trio remain separate work.

A valid running-dog helper measures about 8.489 ms, including background restore
and draw; its active increment is above the roughly 0.094 ms quiet allowance.
Moving projectiles add about 2.644 ms above the inactive helper. Clothes shifts
add about 7.804 ms, or 8.770 ms with tile generation; normal mouse redraw adds
about 1.362 ms above equal-tick work. These are scoped conditions, not universal
costs for every actor state. Earlier malformed dog and partial-state release
inputs are excluded from calibration.

Control hold/poll helpers measure about 0.084/0.306 ms. Gate and BIOS sampling
splits remain approximate; whole-helper totals do not identify each instruction
boundary. Quiet grouped-call work uses a fitted 0.800 ms allowance. Active work
is additional to that baseline rather than charged as another entire helper.
Consecutive original idle/walking batches of 64 and 256 passes check the combined
fit. Their counter instrumentation adds measurable overhead, and their exact
initial BIOS/video phases are unknown. Phase/overhead sweeps compare duration,
final position and BIOS tick together. Agreement within this uncertainty range
is not proof of exact intra-tick scheduling or random-stream synchronization.

`ReferenceCosts::dosbox_x_240` retains the earlier table for explicit comparisons;
ordinary play uses `ibm_xt`. Unmeasured bin row fits, mouse catches, other effect
loops, room actors, transitions, clipped re-entry and startup-specific work retain
provisional allowances. Some of those derive from DOSBox-X normal core at fixed
240 cycles/ms, an emulator throughput setting rather than a physical CPU clock.
Changing measured costs is not a global speed multiplier and does not complete
physical-PC timing calibration. Instruction/interrupt placement, midnight clock
reset and original keyboard transport remain outside this model.

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
so exact random-call counts are not established. Its port-61 toggles are now
synthesized, and ambient sound-state random consumers are included.

Mouse knockdown (`2567`) clears attachment and horizontal direction, sets a
17-update support delay and downward direction, and preserves vertical step,
animation phase and sprite history. Its temporary image blocks input, player
and scenery work until eight wrapping low BIOS ticks have elapsed. The
`5a90` model emits a tone and consumes one random value per changed tick when native sound is enabled,
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
The windowed runner uses monotonic host time only to pace virtual execution.
Each host sample adds at most 100 ms to the pacing target, discarding longer
stalls without resetting guest phases. Catch-up yields after 256 complete
passes or a 4 ms host time slice, whichever comes first; a single pass can
overrun that slice. Pending work remains due. The runner polls events again
before continuing and skips intermediate presentation and sleep while behind.
These bounds affect host responsiveness, not guest execution costs or BIOS
ticks. Display refresh never supplies BIOS ticks.

Gameplay directions and Alt currently use the latest host held-state snapshot
for each batch, sampled only when the original control gate permits. A completed
tap between guest polls can legitimately be missed. During host stalls, however,
the latest snapshot can lose a press held at an overdue poll or apply a later
press too early. Catch-up retains this input-timing limitation; it does not latch
every tap or replay timestamped transitions.

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
on ports 43h, 42h and 61h. Note/divisor, pattern and waveform tables remain in
external executable data. The device layer is the shared `reforge_audio` PIT,
PC Speaker and band-limited renderer; host output uses SDL's queued mono stream
at 48 kHz. No independent host oscillator or melody scheduler advances game state.

The title, asynchronous first/continuation notes, ambient/dog/broom patterns,
collection tones, damage programs, milk warning, hearts sweep/melodies, failure
messages, falling cage, hearts failure drop/landing and presentation wipes have
sound paths. Raw spider/eel pulses and fence changes use speaker-control bits
rather than substituting pitched notes.
Ambient and upper-landing random calls use the same generator as gameplay.
Host enable, volume, device selection and focus mute only affect playback;
Ctrl+S changes the original sound flag and therefore its native predicates.

Counted beep and sweep delays use helper-specific effective XT cycle fits;
PCjr branches halve the original counts. The hearts sweep waits even with native
sound disabled, unlike the short beeps. PIT-counter bursts use their mode-three
counter-difference threshold. Milk warning and upper landing end at the second
BIOS boundary. Their inner polling costs now use the same stock-XT reference:
milk-warning pulses average about 612 CPU cycles (128.229 microseconds), while
upper-landing note programs average about 1044 cycles (218.74 microseconds).
With native sound disabled, upper-landing polls average about 677 cycles
(141.8 microseconds), preserving the original shorter path without random draws.
The modeled intervals are 153, 261 and 169 PIT clocks respectively. Captures
check pulse/program counts against the original phase/pitch fields. These
replace the older DOSBox-X fixed-240 19/49-work-slot allowances.
The blocking spider tail uses a measured whole-loop allowance of about
151.8 microseconds per pulse. Its nine-tick exit and original phase/bit pattern
remain separate from that allowance. Eel contact retains the original
thirteen-tick wait, but tests its exit only after the retrace-dependent sequence:
poll/pulse until retrace is set, emit an extra pulse, apply the saved outer BIOS
sample's blue/white background, emit another pulse, then sample time and restart.
The measured pulse intervals are about 84.53 microseconds on the retrace-poll
path, 224.13 around the palette call, and 148.57 around the outer time check.
Native-muted paths still advance the same waveform counters. These replace the
shared provisional 50-microsecond polling allowance for spider/eel effects.

Fence-reset pulse spacing is fitted as about 462 XT cycles per counted BIOS
check plus 380 cycles per random/reload cycle. The existing accumulator/countdown
algorithm and two-/seven-tick gates are unchanged, including random consumption
when native sound masks the output. The fit charges the whole counted-check
cost at the BIOS sampling boundary rather than inventing instruction-level
subdivisions. Entry/exit costs, timer-read and I/O placement remain approximate;
the broader shared room-effect allowance is unchanged for other effects.
These are explicit timing approximations, not measurements of original waveform
frequency or exact random-call counts. Interrupt placement and per-instruction
I/O overhead remain unmodeled. Actor writes are placed at their execution stage,
with within-stage instruction timing unresolved.

The beep and hearts sweep retain the original divisor/write sequence. Their
XT fits use 18 effective CPU cycles per counted iteration, plus 112 cycles for
the beep gate window or 360 for the sweep caller/helper cadence. Four CPU cycles
correspond to one PIT input clock on this baseline. These are calibrated costs,
not universal 8088 opcode timings. The measured CGA beep gate is about 23.195 ms;
the sweep note interval is about 15.523 ms. PCjr retains its halved counts on the
same baseline, without claiming a separately measured physical PCjr fit. Silent
sweep helper overhead remains approximate. The earlier 6% discrepancy against
DOSBox-X fixed240 is not a correction factor for real 8088 execution. Warning
and upper-landing fits also exclude startup/exit instructions and IRQ outliers;
hardware refresh and prefetch produce several nearby steady interval lengths.
The milk-warning PCjr shift branch retains its waveform change but uses the
shared XT pulse allowance, without a separate PCjr cadence measurement. Exact
silent warning loop cadence is not modeled: only its two-tick wait and unchanged
phase are observable in the current implementation.

Selected controlled DOSBox-X calls compare complete data-segment results and
ordered speaker-port values; they do not establish full-game port timestamps
or audible waveform equality. Port-61 comparisons cover the modeled low two bits.
Pure synthetic checks cover note/rest gates, counter wrap, native versus host
muting, blocking work, category gains and PCM continuity across chunk boundaries.
The first startup still has its separate data/clock copy described above.

The host keeps at most 100 ms in SDL's queue, feeding longer committed blocking
programs from a bounded two-second staging buffer. Pauses preserve that pending
sequence; run/title resets clear it. Device failures warn and leave game logic
running. Uniform accepted-update diagnostics do not have a meaningful speaker
timeline and remain silent; the normal timed headless runner executes the sound
logic without retaining output writes.
