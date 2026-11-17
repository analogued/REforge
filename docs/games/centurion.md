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

Renamed files work when their contents match the supported distribution.
Existing `0.CSV` through `8.CSV` saves are optional.

## Start playing

After [building REforge](../../README.md#getting-started), add the game directory
in the launcher or run from the repository root:

```sh
./target/release/reforge --original /path/to/centurion
```

A key or click skips the introduction to the orders prologue; another enters the
campaign map. Click a controlled or allied province to open its management
panel. Clicking an uncontrolled province or empty sea shows an information
popup, including any army or fleet there. Enter, Space, Escape or either mouse
button dismisses it.

## Controls

| Context | Controls |
| --- | --- |
| Shared Settings | F10; Escape closes Settings while it is open |
| Host pause / resume | Pause/Break, or controller Start/Menu |
| Quit / return to launcher | Ctrl+Q or the window close button |
| Campaign map | Click a province; arrows move the cursor and Space selects it. Enter selects a legion. Escape or F1 opens Game Control. |
| Menus | Arrows select, Enter activates, Escape goes back; letters activate labeled shortcuts. Mouse movement highlights choices; click to activate. |
| Move a legion | Use the province's Move command, Enter on the map, or right-click the map; choose a legion and destination. |
| Fleets | F lists fleets. Space selects a ready local fleet and its destination. |
| Ship purchases | Up/Down adjusts quantity; Left/Right selects OK or Cancel. Enter or Space activates the selected action (OK by default); Escape cancels. Both actions also accept mouse clicks. |
| End Turn | E or the map's End Turn button |
| Land battle | Space or click pauses. M orders melee, R retreat and D resume; S displays orders. Select a cohort for individual orders using arrows and Space/Enter, or drag a destination. |
| Naval battle | Arrows steer/change speed, Space or Insert fires arrows, Enter or + boards, R retreats. |
| Gladiator show | Left approaches, Right backs away, Up/Down guards; Left with Up/Down attacks high/low. Up spares and Down kills at judgment. |
| Chariot race | Arrows steer/brake; Up whips, Up with Left/Right strikes. |

Mouse and keyboard navigation work together. Menus opened by keyboard show
focus immediately; returning to a parent restores its previous choice. If
nothing is highlighted, any arrow first highlights the first enabled choice.
Further presses navigate normally, skipping disabled items and wrapping.
Selectors also show their current value or confirmation default. Mouse-opened
menus otherwise wait for hover. After moving the map pointer with arrows,
mouse movement and clicks continue from that position.

Escape keeps its original game action, including canceling formation/tactic
selection. Use F10 for shared Settings. Controller navigation is available in
Settings; campaign and combat gameplay uses the keyboard and mouse.

For fullscreen, scaling, CRT presentation, audio levels and output-device
selection, see the shared [Settings reference](../configuration.md).
Settings and host pause suspend game time and audio together.

## Difficulty

Game Control → Difficulty Levels offers Galley Slave, Legionnaire, Senator and
Emperor presets. Each preset sets all four categories and returns to the map.
Fine Tuning lets you choose separate levels for land battles, chariot races,
gladiatorial combat and naval battles; the current levels appear beside their
categories. A level takes effect when selected. Continue or Escape goes back
without undoing previous selections. These values are preserved in DOS saves.

## Saves

The nine slots displayed as 1–9 use DOS-compatible files `0.CSV`–`8.CSV`. By
default, saves are read and written beside the selected game installation, so
DOS and REforge can share progress. Load a slot again after switching versions
to pick up its latest contents. Save prompts identify the destination slot and
prefill the current campaign description, with room for 47 ASCII characters.
Back up wanted files before replacing them.

Choose another save directory with `--state-dir /path/to/saves`. Loading first
checks that directory, then the original installation for a missing slot. To
start from a particular campaign file, add `--load /path/to/7.CSV`.
Original game resources are only read; campaign writes target the save directory.

## Game-specific options

Open **F10 → Centurion overrides → Enhancements → Centurion UI**. Enable
**Override global defaults** to edit the game settings, then Apply.
**UI preset** selects Original or Reforged (the default). Expand **Advanced
options** to adjust the four choices separately:

| Option | Original preset | Reforged preset |
| --- | --- | --- |
| Fleet information | Original | Both |
| Popup cursor | Hidden | Visible |
| Menu decorations | Mouse | Mouse |
| Map highlight | Off | On |

**Fleet information** affects enemy inspection. Original shows the DOS
name/location notice; Detailed shows ship counts and an embarked Troops total;
Both shows the table above the notice. Troops includes infantry, cavalry and
elephants; an empty transport shows None. Marauding armies at sea show their full
name and separate infantry, cavalry and elephant counts instead of ship counts.
One key or click dismisses the whole view. Roman fleet tables always show full
legion names and fleet readiness, with columns that make room for longer names.

**Popup cursor** controls the pointer on acknowledgment messages and read-only
lists, including reports. Interactive choices keep their pointer in either mode.

**Menu decorations** selects Mouse rails, Keyboard square selectors, or Dynamic.
Dynamic switches to Keyboard after arrow navigation and to Mouse after pointer
movement or a click. Both input methods and choice highlighting work in every
style.

**Map highlight** brightens the territory border under the pointer: blue for
Roman territory and brown for other territory. Arrow-key movement gives the same
feedback. Information popups keep their territory highlighted until dismissed;
unrelated menus clear it.

Other combinations display Custom. The individual choices are saved. Apply
updates presentation immediately, including open popups; Cancel discards pending
settings edits. These presets control the four options above.

Continue prompts independently follow the last deliberate gameplay input:
“Click to continue” after a mouse click, or “Hit any key to continue” after a key.
Pointer motion does not switch the wording. Either input method still dismisses
the acknowledgment.

Tribute and Tribute Policy mark the currently applied level with a subdued band,
separate from the brighter selection highlight. Changing global policy updates
controlled provinces; local tribute can then override it. The global menu keeps
showing the stored policy. `--original-tribute-menus` hides the added indicator.

## Original rules and current limits

Several surprising original rules are retained. Canceling formation/tactic
selection spends that legion's action. A commander death does not immediately
end a battle; a replacement can be assigned afterward. Some formations/orders
can reach a standoff until further orders or retreat.

Support covers the verified DOS 1.00A distribution and AdLib output. Other sound
hardware modes are not implemented. Comparisons cover selected screens, mechanics
and complete play sequences, rather than every possible campaign. Timing and
some original undefined-state behavior can still differ from DOS.

For implementation, development tools and technical limits, see the
[engine documentation](../../engines/centurion/README.md). Original behavior and
deliberate changes are described separately in the
[DOS reference](../../engines/centurion/docs/original-engine.md) and
[deviations reference](../../engines/centurion/docs/deviations.md).
