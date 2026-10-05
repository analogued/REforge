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
clothesline routine's call-count, retrace, suppression and BIOS-tick gates.
Damaging actors are absent; their post-player work retains fitted time allowances.

Clothes scrolling (`04a0`) decrements byte `0531` before other tests. On expiry
it reloads 1, requires retrace clear, no support-disable flag or active window
projectile, and a changed low BIOS tick relative to `0544`. It then reloads a
difficulty-dependent call interval, quartered when player Y is at most 96.
An airborne cat in the selected row can inhibit movement. A completed garment
advances its packed occupancy by two support cells before its four successive
four-pixel columns finish entering; player support checks compensate for this
phase. Attached cats move with the selected line.

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
or interrupt phase. Other omitted actor drawing can cause larger delays that
the allowance does not reproduce. Keyboard decoding, retained support state and redraw paths
also vary. Clipped re-entry costs have not been separately calibrated. PCjr uses
the CGA cost fit with its verified 16-call countdown, pending separate cost fits.

The original fixed-240 samples show two accepts per complete BIOS tick during
CGA/PCjr walking and about one during CGA idle. The model reproduces those sampled
cadences; this does not establish exact timing within each tick. Software
breakpoints perturb the stage measurements, and the profile does not model every
path, interrupt latency, the BIOS midnight reset, or hardware keyboard transport.
Physical-PC timing fidelity is not established.

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
so exact random-call counts are not established. Audio synthesis and ordinary
sound-state random consumers remain absent.

`--step-hz` explicitly selects the former uniform diagnostic movement scheduler.
Untimed headless execution also advances accepted movement bodies directly.
These accepted-update diagnostic modes do not advance the clothesline schedule.
`--headless --timed` executes the reference schedule without wall-clock waits;
script rows choose held directions by accepted-update index. `--trace-timing`
records loop attempts and blocking-effect checks, including rejected movement.
On player attempts the BIOS/PIT fields describe the BIOS-sample stage; retrace
is sampled later, after the gate's execution allowance.
The windowed runner uses monotonic host time only to pace virtual execution and
discards excessive pause debt without resetting guest phases. Display refresh
never supplies BIOS ticks.
