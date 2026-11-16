# Intentional deviations

These describe the current native frontend. Gameplay reconstruction follows
verified DOS rules; the presentation changes below are deliberate. Known
reconstruction and timing limits are listed in the [engine guide](../README.md#validation-and-limits).

## Menu feedback

The native menus combine DOS keyboard-style selection boxes and highlighted text
with mouse-hover selection. DOS mouse mode normally uses a decorative vertical
column without highlighting the option under the pointer. Native keyboard and
mouse input remain available together.

Tribute and Tribute Policy add a subdued band and a current-level legend for the
stored choice, separately from the brighter focus highlight. The local menu
reads that province's tribute; the global menu reads stored policy, even after
local overrides. The display changes no tribute value, income, activation rule
or random draw. `--original-tribute-menus` removes this indicator for comparison;
it does not restore all DOS mouse-mode presentation.

The save-name prompt identifies the destination slot. This is an additional
clarity aid; descriptions continue to prefill from the current campaign rather
than the selected slot's previous name.

## Fleet information

DOS enemy inspection displays a fleet-name/location notice, including in the
controlled captures with Roman fleets present. **Fleet information: Original**
uses that notice alone. **Detailed** shows a centered ship-count and embarked
Troops table instead. **Both**, the Reforged default, shows the table above the
original notice. All three choices use one key/click dismissal and leave campaign
state unchanged.

The enemy table omits Status: the permanent enemy fleets do not use the Roman
Ready/Moved/Forming turn cycle. Its cargo byte indexes the enemy army table;
Troops sums that army's infantry, cavalry and elephants. The former native table
incorrectly treated it as a Roman legion index, producing misleading numerals.
Enemy army records include eleven roaming slots and three dedicated permanent
fleet slots; all fourteen are valid cargo references.
An empty transport displays None. Roman fleet tables now use complete legion
identifiers rather than just numerals, so I Ital, I Aeg and I Narb are distinct.
Enemy columns keep fixed centered positions. Roman panels start closer to DOS
and measure the headings and every visible row with the game font. Fleet and
Status stay fixed; Legion and the three ship columns shift left only as far as
needed, working from right to left. Galleons/Legion retain an eight-pixel gap;
other text pairs retain at least four. The positions are shared by the headings
and all rows in that panel, so a wide name elsewhere cannot affect a single-fleet
movement panel. The fleet-name column has no heading, matching the original
table. These layout and cargo corrections apply to the fleet tables independently
of the enemy inspection choice.

The granular setting is under Settings → Centurion overrides → Enhancements →
Centurion UI → Advanced options. Original/Reforged are aliases over Fleet
information and Popup cursor: Original/Hidden or Both/Visible. Other combinations
show Custom. Only granular choices are saved. Applying a change can switch an
already open inspection. Movement and naval mechanics are unchanged.

The three permanent enemy fleets normally occupy distinct fixed seas. The
original inspection routine reports only the first active matching permanent
fleet, rather than listing several; artificial co-location overlays their map
symbols at the same sea coordinate. Roaming armies at sea are separate temporary
Marauding Fleet encounters, and the original information routine checks those
before permanent fleets. Native Roman/enemy co-location selection still differs
from the controlled DOS captures: the native selection path prioritizes local
Roman fleets. This presentation change does not alter that selection path.

## Acknowledgment cursor and continue prompts

DOS's shared acknowledgment routine hides and restores the software pointer;
complete fleet/legion lists and province lists do likewise. Interactive Yes/No
and selection dialogs show it while accepting a choice. **Popup cursor: Hidden**
preserves the acknowledgment behavior. **Visible**, the Reforged default, keeps
the pointer visible on those messages and read-only lists, including game/battle
reports. Interactive choices remain visible under either setting. The added
Detailed enemy fleet table follows the same read-only policy.

DOS chooses its continue-prompt string once at startup according to mouse
availability. REforge instead follows the last deliberate gameplay input:
mouse click selects “Click to continue”, keyboard input selects “Hit any key to
continue”. Motion, releases and polling do not change it. This policy is shared
by messages and read-only lists, independent of the cursor option or UI preset.
It changes wording rather than the accepted dismissal controls. The input mode
is transient presentation state and does not enter campaign saves or alter RNG.

## Host presentation and sound

The software pointer uses absolute host mouse coordinates. The host cursor is
hidden over the window without relative capture or added cursor smoothing.
The player uses the shared renderer and display preferences, defaulting to the
original 4:3 display aspect of the 320x200 image. Square pixels and stretch are
explicit shared presentation choices. The standalone research window retains
its constrained 4:3 geometry.

F10 opens shared Settings; Pause freezes host simulation and callback playback,
and Ctrl+Q exits. Escape retains original game controls. Shared volume, semantic
music/effects levels, output-device selection and focus muting affect host audio
without changing the game's original single sequencer or sound requests.

AdLib output uses software OPL2-compatible synthesis and host PCM playback.
Audio generation runs independently of window rendering. Interactive pacing
follows estimated source-sample consumption with a wall-clock fallback; late
sound requests apply at the next unrendered sample. These host scheduling choices
retain logical scene timers and deterministic headless stepping, but do not
reproduce DOS CPU/VGA or physical audio-device timing exactly.

## Bounded undefined states

The DOS program can use uninitialized entries when its arena offer roster is
exhausted. The native engine bounds that search and reports an error instead of
following an arbitrary fighter index. Ordinary cancellation/availability effects
are retained. Other unresolved DOS stack/interrupt dependencies remain explicit
fidelity limits rather than broadly corrected game rules.
