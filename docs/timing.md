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
also calls the sound-state routine, projectiles, windows, other alley animation,
dumpster cat, mice, and another alley helper in their original order. During
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

`Player::update` advances an already accepted movement update. The windowed
runner uses evenly spaced diagnostic updates, defaulting to 36.413 per second,
and latches host directions approximately every two BIOS-tick periods. This
mean active rate matches the measured DOSBox-X normal-core/fixed-2000-cycle
CGA walking/jumping and PCjr walking samples. Those observations used a naturally
advancing guest clock, with controlled initial state and held keys. Debugger
pauses were used to inspect acceptances; these samples do not establish
uninterrupted physical-hardware pacing.

The runner does not implement the original countdown/retrace gate or outer-loop
schedule. BIOS/PIT phase, elapsed costs between calls, hardware keyboard timing,
and idle/retrace cadence are not established by that average. The reference
configuration is not a validated 8088 or 286 hardware profile.

The fence effect uses an approximate seven-tick pause in the windowed runner;
its sound and random-state schedule remain unimplemented. Headless execution
counts accepted updates and imposes no wall-clock delays. Rendering and host
display refresh do not define original BIOS ticks.
