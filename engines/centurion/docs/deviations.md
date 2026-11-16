# Intentional deviations

These describe the current native frontend. Gameplay reconstruction follows
verified DOS rules; the presentation changes below are deliberate. Known
reconstruction and timing limits are listed in the [engine guide](../README.md#validation-and-limits).

## Menu feedback

Menu decorations now offers Mouse, Keyboard and Dynamic under Centurion UI's
advanced options. Mouse, the Reforged default, uses the original capped
three-line rails; Keyboard uses the original square selectors. Ordinary vertical
menus and fleet/legion selectors share one continuous rail, while side-by-side
Yes/No choices have separate rails. Selected-label feedback and disabled text
remain independent of the decoration. Modern mouse-hover focus and keyboard
navigation remain available together, regardless of the chosen style.
Enabled choices highlight on hover throughout the menus, including Hold Games,
arena/race choices, Game Control and Yes/No confirmations; leaving their hit
areas clears mouse focus. Hover never activates a choice or acknowledges a
message. Confirmation hover and clicks use the same title-dependent button
positions as drawing, correcting the formerly hardcoded areas.
Keyboard menu entry now exposes the first enabled action, or an existing
value/confirmation default, immediately. Keyboard return restores valid parent
focus. Without a visible highlight, any arrow first focuses the first enabled
item instead of advancing from an invisible selection; subsequent Up/Down
navigation skips disabled items and wraps. Left/Right does the same initial
focus step in vertical menus and navigates between choices in confirmations.
Mouse entry remains hover-only, independently of decoration style. These focus
changes do not alter acknowledgment input or store anything in DOS save bytes.

Dynamic switches to keyboard decorations after arrow navigation and to mouse
decorations after actual pointer movement or a click. Stationary pointer polling,
releases, timer updates and unrelated keys do not switch it. This presentation
state is separate from continue-prompt input, is not part of DOS saves and does
not advance RNG. Changes apply to open menus. Original/Reforged preset aliases
both use Mouse, matching DOS's normal mouse startup; Keyboard and Dynamic are
granular alternatives. The native save/load/difficulty menu geometry and their
existing simplified layouts are retained; they now share the decoration policy.

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

Ship quantity editors always show the original mouse-mode **OK** and **Cancel**
choices, with hover and keyboard focus using the selected menu decorations.
DOS shows these choices in mouse mode and substitutes an Enter/ESC instruction
line in keyboard mode. The native editor retains those keys without displaying
that line: Enter buys by default, or activates the focused action; Escape always
cancels. Left/Right selects the actions, while Up/Down edits quantity and returns
focus to OK. Hit testing uses the same descriptor bounds as drawing.

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
Roaming armies at sea use the same Original/Detailed/Both setting. Their added
316 × 32 panel shows the full Marauding Army of <province> name, followed by
separate centered Infantry, Cavalry and Elephants counts, including zeros.
The original message remains below in Both. There are no ship columns: DOS
creates that army's temporary fleet with a random ship type when combat starts.
Inspection reads only the stored army record and does not generate ships, advance
RNG or change campaign bytes. Land-army and empty-region notices are unchanged.
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
information, Popup cursor and Menu decorations: Original/Hidden/Mouse or
Both/Visible/Mouse. Other combinations
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

The campaign map highlights the province under the pointer, including during
keyboard cursor movement. Roman provinces retain a blue outline and other
provinces retain a brown outline, brightened with a frame-local palette entry.
Only the existing province border path changes; terrain, units, seas and map
controls retain their colors. Territory information popups keep the inspected
province highlighted, independent of pointer movement while open. On dismissal,
hover follows the current pointer again. Unrelated menus clear the highlight.
**Map highlight: Off** retains the original static borders; the Reforged
default is On. This presentation choice does not affect campaign saves or RNG.

The software pointer uses absolute host mouse coordinates. The host cursor is
hidden over the window without relative capture or added cursor smoothing.
Keyboard movement synchronizes the host pointer to the game cursor, so resuming
mouse movement or clicking continues at that position. Synchronization events
do not trigger hover or change the selected input decorations.
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
