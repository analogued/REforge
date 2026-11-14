# Centurion: Defender of Rome

`centurion` is a development engine for the DOS version of Centurion: Defender
of Rome. Its standalone `centurion-prototype` research frontend supports playing
a campaign, including province management, diplomacy, land and naval battles,
gladiator shows, chariot races, Cleopatra and the closing presentations.

This frontend is separate from REforge's normal player and release catalog.
The loader currently requires the DOS 1.00A data layout and its original filenames;
other releases and renamed resources are not supported by this research entry
point. The crate currently exposes `run_prototype()`, which reads process
arguments. Application loading and host integration are not implemented.

## Build and run

Use Rust 1.88 or newer and installed SDL3 3.4 or newer development/runtime
libraries. Run these commands from the combined repository root:

```sh
cargo run --locked -p centurion --bin centurion-prototype -- \
  --original /path/to/DOS --state-dir /path/to/saves
```

Supply your own DOS distribution, including `OVL0.OVL` and the `DAT`/`DIR` pairs
for `CENT`, `MINIT`, `MMAP`, `BATL`, `MGLAD`, `MRACE`, `MNAVAL`, `NAVL` and
`MNEGO`. Keep `CENTURIO.EXE` with it for playthrough recording. No original assets,
executable code, extracted tables or saved games are bundled. The frontend reads
original resources as data; it does not run the DOS executable or sound driver.

`--scale auto` is the default: fit a compact 4:3 window to the available desktop.
`--scale 1` through `--scale 16` requests a display multiple. The original 320x200
image is presented at 4:3 with sharp pixels. AdLib music and effects use OPL2
compatibility through shared OPL synthesis; `--mute` disables playback.
Use `--help` to list frontend options.

## Playing

Mouse and keyboard navigation work together. Menus highlight the focused choice;
arrows move selection and Enter activates it. Escape usually returns to the
previous screen. A key or click skips the intro to the orders prologue; another
enters the campaign map.

| Context | Controls |
| --- | --- |
| Campaign map | Click a province; arrows move the map cursor, Space selects a province, Enter selects a legion. Escape or F1 opens the game menu. |
| Legion movement | Choose Move in a province's legion menu, or right-click the map. Select a destination after choosing a ready legion. |
| Fleets | F lists fleets. Space selects a ready local fleet and its destination. Shipbuilding uses Up/Down, Enter to purchase and Escape to cancel. |
| End Turn | E or the map's End Turn button. Battles prompted during processing return to the same turn sequence. |
| Land battle | Space or click pauses. M orders melee, R retreat, D resume; S displays Roman orders. Select a cohort for individual orders using arrows, Space/Enter or dragging. |
| Naval battle | Arrows steer/change speed; Space or Insert fires arrows; Enter or `+` boards; R retreats. |
| Gladiator show | Left/Right moves, Up/Down guards, Left with Up/Down attacks. H opens games from a province; Rome offers gladiator shows. |
| Chariot race | Arrows steer/brake, Up whips, Up with Left/Right strikes. Choose racing from Rome's games menu. |

Province screens include tribute, army management and province lists. Rome also
offers global Tribute Policy; other controlled provinces offer plunder. Tribute
menus mark the applied level with a dim band, independently of selection.
Changing global policy updates controlled provinces; local tribute can then
override it. The global menu continues to show the stored policy.

Save/load uses nine slots, displayed as 1–9 and stored as `0.CSV`–`8.CSV` in
`--state-dir`. Menu saving requires this directory; descriptions accept up to
47 ASCII bytes. Loading first checks that directory, then the corresponding
slot in the original-data directory. Saving does not write to original inputs.
The prompt identifies the destination slot and prefills the current campaign
description. Back up wanted slots before overwriting them.

Some surprising DOS rules are retained. Escape during formation/tactic selection
spends the legion's action without moving it or inflicting battle casualties.
Commander deaths do not immediately end battles, and some tactical positions
can remain in a standoff until further orders or retreat.
See [intentional deviations](docs/deviations.md) for presentation changes.

## Development recording

Recording is compiled only with the opt-in `dev-tools` feature, independently of
debug/release optimization. Install `b3sum` and choose a new output directory:

```sh
cargo run --locked -p centurion --bin centurion-prototype --features dev-tools -- \
  --original /path/to/DOS --state-dir /path/to/saves \
  --record /path/to/new-recording
```

F12 adds a marker without a game action. Recordings preserve logical inputs,
timer advances, RNG and sound requests, frame/palette/state checkpoints, initial
save slots and the executable, with BLAKE3 identities. Existing recording
directories are refused. Recording output contains original-derived data and
should be kept outside source control. These traces support aligned comparisons;
they do not automatically translate an entire playthrough into DOS controls or
establish full equivalence.

## Validation and limits

The synthetic tests require no original game data or desktop. From the repository
root:

```sh
cargo fmt --all -- --check
cargo test --locked -p centurion
cargo test --locked -p centurion --features dev-tools
cargo clippy --locked -p centurion --all-targets -- -D warnings
cargo clippy --locked -p centurion --all-targets --features dev-tools -- -D warnings
```

The playable scope does not imply complete DOS parity. Difficulty presets work;
fine tuning and the shared application's settings interface are not implemented.
Audio currently implements AdLib, without the other original sound hardware
modes. Comparisons cover selected frames, routines and play sequences rather
than every possible campaign.

Logical ticks preserve recovered durations, but CPU drawing cost, VGA retrace,
interrupt timing and cursor refresh are not cycle exact. Naval busy-loop AI uses
a calibrated execution budget, so its RNG cadence can differ with DOS emulated
CPU speed. Original uninitialized stack values in some racing/enemy paths are
not generally reproducible. Invalid exhausted arena offers are bounded and
reported instead of following arbitrary indices. These remain fidelity limits.

Interactive audio runs independently of rendering and uses an estimated consumed
source-sample clock, falling back to wall time when audio is unavailable or muted.
This does not measure DAC latency or make every scene transition sample exact.
Headless stepping remains driven by explicit logical inputs.

Authored source and documentation use [GPL-3.0-or-later](../../LICENSE).
Dependency notices and license texts are maintained in the repository's
[central inventory](../../THIRD_PARTY.md).
