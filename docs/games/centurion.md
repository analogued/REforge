# Centurion: Defender of Rome

Govern provinces, negotiate alliances and expand Rome through land and naval
battles. The DOS campaign includes gladiator shows, chariot races, Cleopatra and
the closing victory presentations. Music and effects use AdLib synthesis.

> [!IMPORTANT]
> REforge does not include original Centurion assets or data. Supply your own
> files from the supported DOS 1.00A distribution.

## What you need

Keep `OVL0.OVL` and the `DAT`/`DIR` pairs for **CENT, MINIT, MMAP, BATL, MGLAD,
MRACE, MNAVAL, NAVL and MNEGO** together in the game directory. All nineteen
runtime files are required. The original DOS executable is not required by the
REforge player. Other distributions and platforms are not automatically compatible.

The application verifies the complete required files. Renamed files work when
their contents match the supported distribution; the engine reads those exact
verified paths. Existing `0.CSV` through `8.CSV` saves are optional.

## Start playing

After [building REforge](../../README.md#getting-started), add the game directory
in the launcher or run:

```sh
./target/release/reforge --original /path/to/centurion
```

A key or click skips the introduction to the orders prologue; another enters the
campaign map. Menus accept keyboard navigation, shortcuts and mouse input.

## Controls

| Context | Controls |
| --- | --- |
| Shared Settings | F10; Escape closes Settings while it is open |
| Host pause / resume | Pause/Break, or controller Start/Menu |
| Quit / return to launcher | Ctrl+Q or the window close button |
| Campaign map | Click a province; arrows move the cursor and Space selects it. Enter selects a legion. Escape or F1 opens Game Control. |
| Province menus | Arrows select, Enter activates, Escape goes back; letters activate labeled shortcuts. |
| Move a legion | Use the province's Move command, or right-click the map; choose a legion and destination. |
| Fleets | F lists fleets. Space selects a ready local fleet and its destination. During construction, Up/Down adjusts quantity, Enter purchases and Escape cancels. |
| End Turn | E or the map's End Turn button |
| Land battle | Space or click pauses. M orders melee, R retreat and D resume; S displays orders. Select a cohort for individual orders or drag a destination. |
| Naval battle | Arrows steer/change speed, Space or Insert fires arrows, Enter or + boards, R retreats. |
| Gladiator show | Left approaches, Right backs away, Up/Down guards; Left with Up/Down attacks high/low. Up spares and Down kills at judgment. |
| Chariot race | Arrows steer/brake; Up whips, Up with Left/Right strikes. |

Escape keeps its original game action, including canceling formation/tactic
selection. Use F10 for shared Settings. Controller navigation is available in
Settings; campaign and combat gameplay uses the keyboard and mouse.

Settings provides the shared fullscreen, scaling, CRT presentation and audio
controls. Music and effects levels adjust their host output without changing
original sequencing. The audio output continues independently of window drawing;
host pause and Settings pause game time and audio together. See the
[Settings reference](../configuration.md).

## Saves and DOS comparison

The nine slots displayed as 1–9 use DOS-compatible files `0.CSV`–`8.CSV`. By
default, saves are read and written beside the selected game installation, so
DOS and REforge can share progress. Select a slot again after switching versions
to load its latest contents. Save prompts identify the destination slot; back up
wanted files before replacing them.

Choose another save directory with `--state-dir /path/to/saves`. Loading first
checks that directory, then the original installation for a missing slot. To
start from a particular campaign file, add `--load /path/to/7.CSV`.
Original game resources are only read; campaign writes target the save directory.

## Presentation and original rules

Click an uncontrolled province or empty sea to see its name. If a roaming army
is there, the notice also identifies its origin. Either mouse button, Enter,
Space or Escape dismisses the popup.
Controlled and allied provinces open their normal management panels.

Menus combine keyboard-style selection boxes with mouse-hover highlighting.
Tribute and Tribute Policy also mark the currently applied level with a subdued
band. `--original-tribute-menus` hides that added indicator for comparisons.
In Settings, select **Centurion overrides → Enhancements** to edit **Centurion UI**.
**UI preset** selects Original or Reforged (the default) for the options in that
section. Expand **Advanced options** to adjust the individual choices.
**Fleet information** offers Original (the DOS name/location notice), Detailed
(the ship and troop table), and Both (table and notice together). **Both** is the
Reforged default. One key or click dismisses either view or their combination.
**Popup cursor** offers Visible (the Reforged default) and Hidden (DOS behavior).
It applies to acknowledgment messages and read-only lists, including reports in
battles and games. Interactive choices and Yes/No buttons retain their cursor.
**Menu decorations** offers Mouse (the default rails), Keyboard (square
selectors), and Dynamic. Dynamic switches to Keyboard after arrow navigation
and to Mouse after pointer movement or a click. Both input methods remain usable
in every style. This does not change the separate continue-prompt rule below.
Enabled menu and confirmation choices highlight under the mouse in every style.
Move outside a choice to clear its hover highlight; a click or keyboard
activation is still required to select it.
Menus opened by keyboard show focus immediately, and returning to a parent menu
restores its previous choice. If nothing is highlighted, any arrow establishes
focus on the first enabled choice; later presses navigate normally, skipping
disabled items and wrapping. Existing values and confirmation defaults remain
visible when entering their selectors. Mouse-opened menus wait for hover.

The presets apply all three choices: Original selects Original fleet
information, Hidden and Mouse; Reforged selects Both, Visible and Mouse. Other
combinations show Custom. Only the individual choices are saved. Apply takes effect
immediately, including on an open popup; Cancel discards pending edits.

Continue prompts follow the last deliberate gameplay input: a mouse click gives
“Click to continue”; a key gives “Hit any key to continue”. Mouse motion, button
releases and timer updates do not switch the wording. Either input method can
still dismiss an acknowledgment regardless of its current wording.

The Fleet information choice affects enemy inspection. Roman tables use centered
columns with complete legion names (such as I Ital and I Aeg) and fleet readiness.
They start with spacing close to DOS and make room for longer names by moving
only the necessary ship/Legion columns left. All rows in a panel stay aligned.
Enemy tables keep fixed positions and show centered ship counts and an embarked
Troops total, without a Status column. Troops includes infantry, cavalry and
elephants; no embarked army
shows None. Counts use plain decimal digits, as in the original numeric tables.
Marauding armies travelling by sea use the same information setting. Their panel
shows the full army name and separate Infantry, Cavalry and Elephants counts.
Both also keeps the original name/location notice. Ship counts are omitted:
the game generates their temporary ships only when a naval battle begins.
Fleet movement, selection rules and naval mechanics are unchanged. Other menu
presentation changes above are independent of this preset. These choices are
described in the engine's
[deviations reference](../../engines/centurion/docs/deviations.md).

Several surprising original rules are retained. Canceling formation/tactic
selection spends that legion's action. A commander death does not immediately
end a battle; a replacement can be assigned afterward. Some formations/orders
can reach a standoff until further orders or retreat. Saves preserve these
original mechanics.

## Development recording and limits

Recording is available in the standalone `centurion-prototype` research frontend
with the opt-in `dev-tools` feature; it is separate from normal launcher play.
From the repository root:

```sh
cargo run -p centurion --features dev-tools --bin centurion-prototype -- \
  --original /path/to/centurion --state-dir /path/to/centurion \
  --record /path/to/new-recording
```

F12 adds a marker. Recording directories must be new; separate
sessions neither overwrite nor append to an existing recording. Recordings
preserve original-derived state and images and should remain outside source control.
Normal builds exclude recording support.

Support covers the verified DOS distribution and AdLib output. Other sound
hardware modes and the original fine-tuning settings are not implemented.
Selected screens, mechanics and complete play sequences have been compared;
this does not establish exhaustive DOS parity. CPU-dependent naval polling,
interrupt timing and original uninitialized racing state remain fidelity limits.
See the [engine documentation](../../engines/centurion/README.md) for the research
frontend, validation and technical limits.
