# Deliberate deviations from the original

This document records intentional corrections and usability changes. Missing
features and current accuracy limitations are described in the README and
[timing reference](timing.md).

## Window hold at difficulty six

The original uses a one-BIOS-tick open-hold threshold at internal difficulty 6,
between 18-tick thresholds at difficulties 5 and 7. This isolated short hold is
treated as a bug: the recreation defaults to 18 ticks (about 0.99 seconds) at
difficulty 6. The historical intended value has not been established; 18 ticks
is our correction based on the neighboring entries.

The correction applies to ordinary windows and female-cat windows leading to
hearts, which share the timing table. Opening and closing animation, contact
holds, entry conditions, and other difficulty entries retain their existing
behavior. The external executable and its table are left unchanged.

Development builds can restore the original one-tick threshold with
`--original-window-timing`. The flag requires the `dev-tools` Cargo feature and
is unavailable in normal builds. Development builds also use the correction
unless that flag is supplied. For example:

```sh
cargo run --locked -p reforge --release --features dev-tools -- \
  --original /path/to/CAT.EXE --difficulty 6 --original-window-timing
```

These are hold thresholds, not exact total visible durations: animation also
depends on its invocation countdown, retrace gate, and actor contact.

## Responsive controls

Interactive play defaults to Responsive input, which refreshes direction and
action requests on each ordinary simulation pass instead of waiting for the
original control poll. Movement gates, blocking effects and the modeled
control-poll cost remain unchanged; movement updates do not become more frequent.
Earlier input can change which action is selected at an existing update.

Original polling remains selectable in the Alley Cat settings. See the
[Responsive controls feature](../README.md#responsive-controls) for how to switch.
Headless reference and uniform diagnostic runs retain original input unless
explicitly selected by a runtime caller.

Controller templates can separately replace land Up/Down requests with a
Jump/Drop face button. Down is sampled when that button is pressed; it selects
Drop unless the cat is grounded on a solid floor, where the button selects Jump.
Hearts has no bottom floor and retains Drop. Changing directions or landing
while holding the button does not switch between jumping and dropping.
Swimming retains four-way direction input. Keyboard input, menu confirmation,
movement gates and the reconstructed physics are unchanged. The original
directional controller layout remains the default.

## Shared ordinary-action timing

The default XT profile uses common execution allowances for comparable ordinary
cat actions across rooms. Extra furniture-search and floor-mark drawing work
does not slow those actions, while native search results, movement, animation,
tick/retrace gates and explicit scene-specific rules remain intact. Swimming and
the bookcase broom's distinct gate are preserved separately.

These are approved modern pacing choices based on the XT measurements, not
exact reproductions of every measured processing cost or a global speed change.
The [execution-cost reference](timing.md#reference-execution-costs) records the
current allowances, eligibility guards and measurement limits. The older DOSBox
profile retains its existing costs for comparisons.

## Bin-cat animation cadence

Once the bin cat starts emerging, its animation advances at most once per
changed BIOS tick (about 55 ms), without the original retrace requirement.
The approximate loop schedule could repeatedly miss that narrow window and
hold an image for several hundred milliseconds. Spawning retains its original
tick/retrace and random-selection rules. The original frame sequence, repeated
full-height image, contact holds and dog-alert response are preserved.

An uninterrupted 26-interval cycle is nominally about 1.43 seconds. This is a
deliberate smoothing choice, slightly faster than the measured 1.69-second XT
example; blocking game work and contact can still delay it. No frames are
skipped to catch up after a delay. Earlier animation steps can change when
contact occurs and subsequent gameplay history. Drawing costs remain charged
once by the existing runtime. The native helper retains its original gates for
direct reference comparisons; the gameplay caller applies this adjustment.

## Hearts pairing presentation

The default XT profile omits simulated sprite-drawing latency during the three
pairing groups in the Hearts winning presentation. This avoids slowing larger
frames to reproduce historical drawing throughput. All images, frame counts,
positions and movement updates remain intact, together with the original
one-, two- and three-BIOS-tick gates and movement/bookkeeping allowances.
The melody logic and ordered sound commands are unchanged; their timing follows
the resulting presentation pace.

This policy applies only to pairing frames. Other presentation drawing costs,
including the shared large cat's approach animation, retain their measured
allowances. The older DOSBox timing profile retains its byte-based drawing costs.

## Speaker presentation

PC speaker output uses the
[shared small-speaker response](../../../crates/reforge_audio/README.md#replaceable-signal-rendering)
after digital waveform reconstruction. This approximates speaker bandwidth
without changing sound commands, PIT execution or category gains. Tandy PSG
output retains its separate digital rendering path.

## Scene and sprite composition

The original draws into display memory in stages and restores saved sprite
backgrounds. REforge constructs complete destination scenery before presenting
it and composes the current scenery, player and actor layers without reproducing
stale saved-background fragments or intermediate CGA-bank/tile writes. This is
a deliberate presentation choice to avoid incidental drawing artifacts. The
stepped transition wipes remain visible; see the
[timing reference](timing.md#ordinary-room-scoring-presentation) for their schedule.

## Pause controls

The original pauses on Esc and resumes on any key press. In graphical play,
Esc opens Settings, which pauses gameplay until the menu closes. F10 remains
an alternative. The recreation toggles dedicated pause only with P or Pause/Break.
Other keys, repeated key events, and Alt/Ctrl/Super combinations leave pause
unchanged, allowing Alt+Tab without resuming play. Arrow keys do not resume.
Losing or regaining focus does not change pause state. Ctrl+Q and the window
close button quit, including while
paused. Ctrl+Q follows the Q letter in the current keyboard layout.
There is currently no development override for the original any-key
resume behavior.

Pause also freezes the complete virtual clock, including PIT and retrace phases.
The original restores the BIOS tick count on resume without restoring those
hardware phases. REforge excludes the paused interval from host pacing, so
resuming does not catch up that elapsed time. See the
[host pacing and pause details](timing.md#blocking-effects-and-diagnostic-modes).

## Startup and keyboard menu

The initial animated splash leads to the skill menu and controls. Its advanced
shared state carries into the first game without reseeding. The host starts a
fresh gameplay/audio presentation epoch after the menu rather than emulating
absolute BIOS time across the questions. Desktop shortcut
combinations, modifier keys alone and held-key repeats do not dismiss the splash;
a fresh ordinary key press does. Explicit development scene selection bypasses
startup, while `--scene splash` previews only that presentation.

The joystick question is omitted; keyboard controls are always enabled.
SDL controller support maps the stick/D-pad to digital directions and its bottom
button to the contextual action. The skill screen adds a white selection cursor
for controller and keyboard Up/Down navigation. A controller-advanced title shows
it immediately; otherwise the first direction or confirmation only reveals
Kitten. Subsequent controller A or keyboard Enter/Space confirms the visible
choice. K/H/T/A remain direct shortcuts and restore the original question pixels.
The menu retains the original skill choices, accumulating text layout and Ctrl+S
sound-toggle instruction. Its pause instruction describes the host toggle keys;
Ctrl+Q additionally quits. Ctrl+R restarts and Ctrl+M opens the skill menu
directly, without replaying the splash. An explicit `--difficulty` skips the skill
question and shows controls with a separate key-to-start wait. Menu waits ignore
desktop shortcuts, modifier keys alone and held-key repeats.

## Disk and full-build input integration

The DOS hard-disk image is a container of the same conversion executable.
REforge reads CAT.EXE through its FAT16 directory and cluster chain instead of
executing the image's boot records. It then uses the standalone conversion's
asset parser and native behavior; no emulated DOS or disk hardware is involved.

The supported booter and both restored DOS input forms use the same native Rust
gameplay and coordinate contracts as the DOS conversion. The loader translates
full-build asset pointers and retains the conversion's completion-image row convention and
menu placement, rather than reproducing those small full-build layout changes.
The external full-build score remains separate and supplies PCjr title
music. DOS interrupt hooks, hardware detection and the external-speaker question
are replaced by the Machine choice; no DOS adapter code executes. The booter is
read and decoded in memory without running its BIOS loader or disk-check path.
The restored title's distinct timer rate and native sound-toggle sequencing remain. The former
separate Tandy selection used the same native presentation as PCjr; it is now a
compatibility alias rather than another machine mode. The two NRS input variants
retain their catalog label independently of the machine choice.

## Startup seed and original bad-state guard

Normal play uses the original `2e10` counter-byte swap and zero-to-`0xfa59`
fallback. A modeled channel-zero count with a host-time phase supplies the sample;
physical DOS PIT access and its initial phase are not reproduced. Explicit
`--seed` values bypass sampling. The shared game generator is unchanged.

The original can produce seed 1, whose next state is zero and prevents scenery
initialization from completing. Automatic startup resamples this one invalid
result; explicit seeds 0 and 1 are rejected. Headless, explicit-scene and uniform
step diagnostics keep their historical default seed for comparisons. An explicit
seed takes precedence in every mode.

## Manual restart state

Ctrl+R and selecting a new run through Ctrl+M reset the chosen startup seed
and ordinary-room selection history for reproducible testing. The original
seeds and clears this history only at first startup. Automatic last-life returns
to the title retain the shared random state and room history, as in the original.
The title waits for a fresh key press and keeps the selected skill; it does not
repeat the menu questions. Desktop shortcut filtering applies to this later
title as well. Ctrl+Q and window close remain available throughout.

## Scene movement

Reforged scene movement is the interactive default. Original retains the native
packed framebuffer, sprite placement and stepped updates. The per-game Enhancements
setting and temporary `--motion reforged|original` override are independent of
Responsive/Original input response. Neither presentation policy changes native
movement, collisions, support checks, random consumption, progression or sound
execution. Visual positions can differ from the authoritative collision positions.

Reforged separates safely recoverable sprite images from their native backgrounds
and composes them at finer visual positions. Ordinary player movement and the
released bird interpolate actual available observations on a 25 ms presentation
clock. Missing future observations hold the last known position rather than
extrapolating a later correction. Scene, lifetime, direction and large relocation
boundaries cut history instead of drawing a path across unrelated positions.
Walking poses use a shared front anchor while retaining the original artwork,
leg changes and bobbing; Original keeps their native placement.

Hearts rivals use 40 ms observation history, while the player and scene remain
on the 25 ms clock. Each rival's artwork and position come from the same selected
observation. The main timeline owns visibility and sprite lifetimes; missing or
unsafe older images fall back to it. Boundary turns retain their native final
outward displacement with the new facing artwork. Drawn origins stay within the
native raster field limits after pose registration.

Swimming retains continuous visual velocity derived from decoded native steps
and observed update cadence. Fish, eels, dogs, brooms and the bookcase spider
also retain their actor-specific velocity smoothing and bounded stop/turn rules.
These observers preserve the PCjr aquarium scheduling differences, the bookcase
broom's slower gate and native acceleration. The spider checks possible nearby
broom blocks on independent scratch state; it never advances the live simulation.
Opaque dog pursuit/death images, sleeping dogs' eyes, cheese-hole transfers and
drinking images retain native placement. Bird/broom overlap stays native until a
clean draw makes their saved backgrounds separable again.

Clothes use completed native band scrolls, including newly entering edge pixels.
A riding cat shares the garment's visual offset once. Attachment changes rebase
that offset; unrelated writes invalidate the affected strip. Clothesline mice use
their own observed cadence and native directions, stops and catch/respawn images.
Window opening and closing retain native one-pixel steps, keeping projectile
appearance and room entry aligned with the native window edge.

The non-success Hearts return uses a 60 ms presentation clock for its complete
wipe, falling head, landing hold and fill sequence. Only the falling head's
position is interpolated, between its actual downward steps. Its different landing
picture is never moved. The first ending frame is held briefly when extending
the ordinary 25 ms delay; the delayed ending finishes before resuming ordinary
play. Native sound timing is unchanged. Successful Hearts approach/pairing
presentations, the cage and other unsplit effects retain native images and steps.

The aquarium composes complete actors over a separate water background with
native surface animation and accumulated bubbles, avoiding fragments from older
actor restores. Exact saved-image checks guard other layer separation. An unsafe
local overlap can leave just that actor native; a broader composition mismatch
falls back to the native frame instead of reusing stale background pixels.

See the [player guide](../../../docs/games/alleycat.md#game-specific-options)
for selection and the [timing reference](timing.md) for native update schedules.
