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
cargo run --locked --release --features dev-tools -- \
  --original /path/to/CAT.EXE --difficulty 6 --original-window-timing
```

These are hold thresholds, not exact total visible durations: animation also
depends on its invocation countdown, retrace gate, and actor contact.

## Pause controls

The original pauses on Esc and resumes on any key press. The recreation toggles
pause only with Esc, P, or Pause/Break. Other keys, repeated key events, and
Alt/Ctrl/Super combinations leave pause unchanged, allowing Alt+Tab without
resuming play. Arrow keys do not resume. Losing or regaining focus does not
change pause state. Ctrl+Q and the window close button quit, including while
paused. There is currently no development override for the original any-key
resume behavior.

## Game over and restart

After the final life is lost, the recreation holds an authored game-over screen
and accepts R to start another run. The original returns to its startup/menu
sequence, which is not yet implemented. Restart retains the high score and
development controls and resets score, lives, initial difficulty and initial
seed. Ctrl+Q and window close remain available. This is the current desktop
replacement for that menu flow; there is no development override.
