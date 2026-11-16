# Centurion: Defender of Rome

`centurion` recreates the DOS version of Centurion: Defender of Rome, including
province management, diplomacy, land and naval battles, gladiator shows, chariot
races, Cleopatra and the closing presentations. REforge selects supported DOS
1.00A resources through its release catalog. The engine reads the exact selected
paths, including renamed files; it does not execute the DOS program or driver.

The separate `centurion-prototype` research frontend accepts a raw DOS directory
with original filenames and explicit diagnostic controls. It does not perform
normal application detection and should be used only for research.

## Build and run

Use Rust 1.88 or newer and installed SDL3 3.4 or newer development/runtime
libraries. Run these commands from the combined repository root:

```sh
cargo run --locked -p reforge -- \
  --original /path/to/DOS --state-dir /path/to/saves
```

Supply your own DOS distribution, including `OVL0.OVL` and the `DAT`/`DIR` pairs
for `CENT`, `MINIT`, `MMAP`, `BATL`, `MGLAD`, `MRACE`, `MNAVAL`, `NAVL` and
`MNEGO`. Normal playback needs these nineteen resource files; the DOS executable
is needed only by the original game and raw research recorder. No original assets,
executable code, extracted tables or saved games are bundled.

`--scale auto` is the default: fit a compact 4:3 window to the available desktop.
`--scale 1` through `--scale 16` requests a display multiple. The original 320x200
image is presented at 4:3 with sharp pixels. AdLib music and effects use OPL2
compatibility through shared OPL synthesis; `--mute` silences playback.
F10 opens shared Settings, including renderer, display, volume, output device
and focus muting. Settings and Pause freeze simulation and the audio timeline;
Ctrl+Q exits to the launcher. Escape retains the original back/cancel/game-menu
behavior. Gamepads can operate Settings and host pause; gameplay uses the mouse
and keyboard controls below. Use `--help` to list player options.

The launcher derives its colour mosaic from the selected original resources
while preparing the title preview. It counts each stored visible artwork pixel
once, including both raw backgrounds, then includes cached bitmap expansion,
province uniform remaps and procedural drawing pens. Colours found only through
those runtime rules receive weight one. The palette keeps CMAP order and RGB
values; these artwork weights do not measure time on screen or prove that every
stored image is reachable or unobscured during play.

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
`--state-dir`, defaulting to the original installation directory for sharing
with DOS. Descriptions accept up to 47 ASCII bytes. Loading first checks that
directory, then the corresponding slot in the original-data directory. Save writes go only to the selected state
directory. To exchange saves with the DOS game, point `--state-dir` to the same
writable game installation that contains `CENTURIO.EXE`; both versions then use
the same slot files. Use a different directory to keep native writes separate.
Load a slot again when switching versions to pick up its changed contents.
The prompt identifies the destination slot and prefills the current campaign
description. Back up wanted slots before overwriting them.

Some surprising DOS rules are retained. Escape during formation/tactic selection
spends the legion's action without moving it or inflicting battle casualties.
Commander deaths do not immediately end battles, and some tactical positions
can remain in a standoff until further orders or retreat.
Enemy-fleet information defaults to **Both**: a detailed table above the original
fleet-name/location notice. In Settings, Centurion overrides → Enhancements →
Centurion UI offers Original/Reforged presets and expandable Fleet information
choices: Original, Detailed and Both. Reforged selects Both; Detailed alone shows
Custom. Roman tables show complete legion identifiers and readiness; enemy tables
show embarked troop totals without the uninformative readiness column. Counts
use plain decimal digits. See the
[player guide](../../docs/games/centurion.md#presentation-and-original-rules) and
[intentional deviations](docs/deviations.md) for presentation changes.

## Development recording

Recording is available through the standalone research frontend, compiled only
with the opt-in `dev-tools` feature, independently of debug/release optimization.
Its retained binary supports deterministic research replay; the normal player
does not expose recording yet. Install `b3sum` and choose a new output directory:

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
cargo test --locked -p centurion --no-default-features
cargo test --locked -p centurion --features dev-tools
cargo clippy --locked -p centurion --all-targets -- -D warnings
cargo clippy --locked -p centurion --all-targets --features dev-tools -- -D warnings
cargo clippy --locked -p centurion --all-targets --no-default-features -- -D warnings
```

The playable scope does not imply complete DOS parity. Difficulty presets work;
original fine tuning is not implemented.
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
source-sample clock, falling back to wall time when audio is unavailable. Muting
the normal player retains synthesis and its clock.
This does not measure DAC latency or make every scene transition sample exact.
Headless research stepping remains driven by explicit logical inputs. Building
with `--no-default-features` excludes SDL and retains those raw research tools;
interactive playback and launcher previews require the default `sdl` feature.

Authored source and documentation use [GPL-3.0-or-later](../../LICENSE).
Dependency notices and license texts are maintained in the repository's
[central inventory](../../THIRD_PARTY.md).
