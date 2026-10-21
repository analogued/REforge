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

## Pause controls

The original pauses on Esc and resumes on any key press. The recreation toggles
pause only with Esc, P, or Pause/Break. Other keys, repeated key events, and
Alt/Ctrl/Super combinations leave pause unchanged, allowing Alt+Tab without
resuming play. Arrow keys do not resume. Losing or regaining focus does not
change pause state. Ctrl+Q and the window close button quit, including while
paused. Ctrl+Q follows the Q letter in the current keyboard layout.
There is currently no development override for the original any-key
resume behavior.

## Startup and keyboard menu

The initial animated splash leads to the skill menu and controls. Its data and
virtual clock are separate from gameplay, so time spent watching it does not
advance the fixed development gameplay seed.
Original shared startup state integration is deferred. Desktop shortcut
combinations, modifier keys alone and held-key repeats do not dismiss the splash;
a fresh ordinary key press does. Explicit development scene selection bypasses
startup, while `--scene splash` previews only that presentation.

The joystick question is omitted; keyboard controls are always enabled.
Controller support remains unimplemented. The menu retains the original skill
choices, accumulating text layout and Ctrl+S sound-toggle instruction. Its
pause instruction describes the host toggle keys; Ctrl+Q additionally quits. Ctrl+R restarts and Ctrl+M opens the skill menu directly, without
replaying the splash. An explicit `--difficulty` skips the skill question and
shows controls with a separate key-to-start wait. Menu waits ignore desktop
shortcuts, modifier keys alone and held-key repeats.

## Manual restart state

Ctrl+R and selecting a new run through Ctrl+M reset the initial development seed
and ordinary-room selection history for reproducible testing. The original
seeds and clears this history only at first startup. Automatic last-life returns
to the title retain the shared random state and room history, as in the original.
The title waits for a fresh key press and keeps the selected skill; it does not
repeat the menu questions. Desktop shortcut filtering applies to this later
title as well. Ctrl+Q and window close remain available throughout.
