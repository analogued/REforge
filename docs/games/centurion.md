# Centurion: Defender of Rome

Govern provinces, negotiate alliances and expand Rome through land and naval
battles. The DOS campaign includes gladiator shows, chariot races, Cleopatra and
the closing victory presentations. Music and effects offer AdLib synthesis or
the original PC speaker arrangement.

> [!IMPORTANT]
> REforge does not include original Centurion assets or data. Supply your own
> files from a supported DOS 1.00A or 1.00B distribution.

## What you need

Keep `OVL0.OVL` and the `DAT`/`DIR` pairs for **CENT, MINIT, MMAP, BATL, MGLAD,
MRACE, MNAVAL, NAVL and MNEGO** together in the game directory. All nineteen
runtime files are required for either DOS version. EGA additionally needs all
`DAT`/`DIR` pairs for **EINIT, EMAP, EGLAD, ERACE, ENAVAL and ENEGO**. REforge
offers EGA when the complete set matches its verified file fingerprints;
missing or mismatched EGA files leave VGA available. The original DOS executable
is not required by the REforge player. Other distributions and platforms are not
automatically compatible.

Renamed files work when their contents match the supported distribution. After
adding or removing EGA banks from a library installation, use **Relocate…** to
select its directory again and update the detected video choices.

## Video mode

With complete EGA banks, either DOS version offers **VGA** (default) and **EGA**
in the launcher's **Video** choice. It is saved separately for each library entry
and takes effect at launch, like Mach 3's video choice. Both use 320×200 game
pixels with a 4:3 display aspect. EGA uses the supplied 16-colour artwork.
The supported 1.00A EGA combination uses the same bank fingerprints as 1.00B;
it does not identify a separately examined historical 1.00A EGA distribution.

Direct launches accept `--video vga` or `--video ega`. Unsupported EGA selection
without complete EGA banks warns and uses VGA. The two modes share the selected
installation's save slots and game rules. Each release retains its own
new-campaign data, including 1.00B's higher initial plunder reserve in Pontus.

## Sound

The launcher's **Sound** choice and **Settings → Audio →
Sound device** offer **AdLib** (default) and **PC speaker** for either DOS version
and either video mode. They share the standard per-entry audio selection and built-in defaults.
Settings changes take effect on the next launch; they do not switch the running
sound arrangement. PC speaker plays the original game's single-voice arrangement.

Direct launches accept `--sound adlib` or `--sound pc-speaker`. Volume,
music/effects levels, output-device selection and focus muting remain in shared
Settings. The Sound choice selects the emulated game hardware; output-device
selection chooses the host speakers or headphones.

The examined originals also contain Roland MT-32/LAPC-I and Tandy sound drivers;
those two modes are not implemented in REforge.

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
| Fleets | F lists fleets. Space or left click selects a local fleet and its destination. When several fleets share a sea, hover or use arrows to highlight a row, then click or press Enter/Space; 1–3 selects a row directly. |
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

The applied preset or category level stays marked while you browse, using the
Applied value indicator described under
[Game-specific options](#game-specific-options). If the categories have different
levels, Fine Tuning is the applied row in the main menu.

## Saves

Save Game and Load Game open the original nine-row panels over the Game Control
menu. Hover or use arrows to select a slot, then click or press Enter.

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

Open **F10 → Enhancements → Centurion UI**. Edit the
settings, then Apply.
**UI preset** selects Original or Reforged (the default). Expand **Advanced
options** to adjust the six choices separately:

| Option | Original preset | Reforged preset |
| --- | --- | --- |
| Fleet information | Original | Both |
| Popup cursor | Hidden | Visible |
| Menu decorations | Mouse | Mouse |
| Map highlight | Off | On |
| Map hover info | Off | Off |
| Applied value indicator | Original | Subdued band |

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

**Map hover info** optionally shows names in the upper-left corner of the map.
Hover a symbol for unit names, with one row per fleet and any
embarked legion on that row; the territory or sea name appears underneath.
Hover elsewhere for the place name alone. Arrow-key cursor movement works too.
Black separator lines preserve an existing place name and never introduce a
panel. Colored province borders update the name immediately, like the territory
highlight. Unnamed water, map controls, leaving the map and opening
menus hide it. This option is Off in both presets; enable it under Advanced
options when wanted.

**Applied value indicator** identifies the stored choice in Tribute, Tribute
Policy, Difficulty Levels and each Fine Tuning category's level menu. Original
adds no tribute indicator and keeps the applied difficulty white. Hover and
keyboard focus are also white, so a different focused difficulty gives two white
rows; focusing the applied difficulty gives one. Subdued band, the Reforged
default, marks the applied row with gray shading. Side marker and Underline mark
it without shading the row. Focus remains white in each style. No style adds a
footer notice.

Other combinations display Custom. The individual choices are saved. Apply
updates presentation immediately, including open menus and popups; Cancel
discards pending settings edits. These presets control the six options above.

Continue prompts independently follow the last deliberate gameplay input:
“Click to continue” after a mouse click, or “Hit any key to continue” after a key.
Pointer motion does not switch the wording. Either input method still dismisses
the acknowledgment.

Tribute reads the selected province's level; Tribute Policy reads the stored
global policy. Changing global policy updates controlled provinces, and local
tribute can then override it. The global menu keeps showing the stored policy.
For comparisons, `--original-tribute-menus` hides added tribute indicators
regardless of the selected style.

## Original rules and current limits

A legion's move covers the expedition, even if it ends without a fight. Once
the legion enters an encounter, withdrawing from negotiations or canceling
formation/tactic selection leaves its move spent for that turn.

A commander death does not immediately end a battle; a replacement can be
assigned afterward. Some formations/orders can reach a standoff until further
orders or retreat.

Enemy conquest adjusts reputation using the province's former status, adopting
the 1.00B correction to a 1.00A bug that increased reputation when land was lost.
Scouting also retains "legendary" for the richest treasuries instead of reading
army-strength labels past the wealth-description table.

Canceling flagship selection when defending counts as retreat. A fleet that
escapes moves with its cargo and spends its action; the carried legion retains
its own action state. If no escape sea is available, the fleet and carried
legion are lost. Removing a destroyed legion also releases its transport so it
can carry another legion. These 1.00B corrections apply with every UI preset.

Support covers DOS 1.00A and 1.00B with VGA, optional matching EGA banks and
AdLib or PC speaker output. Roland and Tandy sound modes are not implemented.
Comparisons cover selected screens, mechanics and complete play sequences, rather than every possible campaign. Timing and
some original undefined-state behavior can still differ from DOS.

For implementation, development tools and technical limits, see the
[engine documentation](../../engines/centurion/README.md). Original behavior and
deliberate changes are described separately in the
[DOS reference](../../engines/centurion/docs/original-engine.md) and
[deviations reference](../../engines/centurion/docs/deviations.md).
