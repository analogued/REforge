# Intentional deviations

These are deliberate differences from DOS 1.00A in the native frontend. For
controls and configuration, see the [player guide](../../../docs/games/centurion.md).
Recovered original behavior is kept in the [DOS reference](original-engine.md);
unsupported features and unresolved fidelity issues belong under
[validation and limits](../README.md#validation-and-limits).

## Presentation settings

Centurion UI's Original/Reforged presets are aliases over four granular choices:

| Setting | Original | Reforged (default) |
| --- | --- | --- |
| Fleet information | Original | Both |
| Popup cursor | Hidden | Visible |
| Menu decorations | Mouse | Mouse |
| Map highlight | Off | On |

Nonmatching combinations display Custom. Only the individual choices are saved.
They apply to open views as well as newly entered ones, without modifying DOS
campaign bytes or advancing RNG. The preset affects these four settings; the
input feedback and host adaptations described below also apply with Original.

### Acknowledgments

DOS hides the pointer on acknowledgment messages and read-only lists. Popup
cursor: Hidden retains that behavior; Visible keeps it on these views, including
battle/game reports and added fleet tables. Interactive choices keep their
pointer under either setting.

DOS fixes continue wording at startup. Native messages and read-only lists
instead follow the last deliberate gameplay input: a click selects “Click to
continue”; a key selects “Hit any key to continue”. Pointer motion, releases,
polling and timer updates do not change it. This is independent of cursor
visibility and decorations, and changes wording rather than dismissal controls.

### Menu decorations

Mouse uses DOS's capped three-line rails; Keyboard uses its square selectors.
Both preset aliases use Mouse, corresponding to normal DOS mouse startup.
Dynamic switches to Keyboard after arrow navigation and to Mouse after actual
pointer movement or a click. Stationary polling, releases and unrelated keys do
not switch it. All styles retain simultaneous mouse/keyboard input and the same
selected-label and disabled-text feedback. The resolved style is transient host
state, separate from continue wording and DOS saves.

### Map highlight

The campaign map brightens the border under the pointer, including keyboard
cursor movement: blue for Roman provinces and brown for other provinces. Only
the existing province border path changes, using a frame-local palette entry;
terrain, units, seas and map controls retain their colors. Territory information
popups keep the inspected province highlighted until dismissal, then resume
hover at the current pointer. Unrelated menus clear it. Off retains DOS's static
borders.

## Menu feedback and editing

Enabled choices highlight on hover throughout menus and confirmations, including
Hold Games, arena/race choices and Game Control. Leaving a hit area clears mouse
focus; motion never activates a choice or acknowledges a message. Confirmation
drawing, hover and clicks share their title-dependent button positions.

Keyboard entry immediately exposes the first enabled action or an existing
value/confirmation default. Returning to a parent restores valid focus. If no
highlight is visible, any arrow first focuses the first enabled item rather than
advancing from an invisible selection. Subsequent navigation skips disabled
items and wraps. Left/Right establishes initial focus in vertical menus and
navigates side-by-side confirmations. Mouse entry otherwise waits for hover.
These rules are independent of decoration style and acknowledgment input.

Tribute and Tribute Policy add a subdued band and current-level legend for the
stored value, separate from focus. Local tribute reads that province's value;
global policy continues to show stored policy after local overrides. This does
not change tribute, income, activation rules or RNG. `--original-tribute-menus`
removes the indicator independently of the preset.

The save-name prompt additionally identifies the destination slot. Descriptions
still prefill from the current campaign, not the selected slot's previous name.

Ship quantity editors always show OK and Cancel, whereas DOS substitutes an
Enter/ESC instruction line in keyboard mode. Hover and keyboard focus use the
selected decorations; hit testing uses the same descriptor bounds as drawing.
The original keys remain available: Enter buys by default or activates the
focused action; Escape always cancels. Left/Right selects actions, while Up/Down
edits quantity and returns focus to OK. Space activates the focused action;
O buys and C cancels without requiring those shortcuts to be displayed.

## Fleet and army inspection

DOS enemy inspection shows a name/location notice. Original uses that notice
alone; Detailed substitutes a centered ship-count/Troops table; Both adds the
table above the notice. All views share one key/click dismissal and leave campaign
state unchanged. The table omits Status because permanent enemy fleets do not
have the Roman readiness cycle. Troops includes infantry, cavalry and elephants
from the enemy army cargo record; an empty transport displays None.

Roaming armies at sea use the same setting. Their added 316 × 32 panel shows
`Marauding Army of <province>`, then separate centered Infantry, Cavalry and
Elephants counts, including zeros. Both keeps the original notice below. There
are no ship columns: ships are only generated when combat starts. Inspection
reads stored army data without generating ships, advancing RNG or changing
campaign bytes. Land-army and empty-region notices retain their original content.

Roman tables show complete legion identifiers instead of numerals alone, making
I Ital, I Aeg and I Narb distinct. Roman panels begin close to DOS's positions
and measure headings and every visible row with the game font. Fleet and Status
stay fixed; Legion and the three ship columns shift left only as needed, working
from right to left. Galleons/Legion retain an eight-pixel gap; other text pairs
retain at least four. Headings and all rows share the resulting positions within
that panel. A name elsewhere cannot affect a single-fleet movement panel. The
fleet-name column has no heading, as in DOS. Enemy columns have fixed centered
positions; counts use plain decimal digits. These layouts apply independently
of the enemy inspection setting and do not change movement or naval mechanics.

## Host presentation and sound

The software pointer uses absolute host mouse coordinates. The host cursor is
hidden over the window without relative capture or added smoothing. Keyboard
movement synchronizes it to the game cursor, so mouse movement/clicks continue
from that position. Synchronization events do not trigger hover or switch
Dynamic decorations.

The normal player uses the shared renderer, defaulting to the original 4:3
display aspect for the 320 × 200 image. Square pixels and stretch are shared
presentation choices; the standalone research window keeps constrained 4:3
geometry. F10 opens shared Settings, Pause freezes simulation and callback
playback, and Ctrl+Q exits. Escape retains its original game actions.

Shared volume, music/effects levels, output-device selection and focus muting
control host audio without changing the original single sequencer or sound
requests. AdLib output uses software OPL2-compatible synthesis and host PCM
playback. Callback generation is independent of window rendering; scheduling
uses estimated source-sample consumption with a wall-clock fallback. These host
adaptations retain logical scene timers and deterministic headless stepping,
but do not reproduce CPU/VGA or physical device timing exactly. See the
[technical limits](../README.md#validation-and-limits).

## Invalid arena offers

DOS can use uninitialized entries when its arena offer roster is exhausted.
The native engine bounds that search and reports an error instead of following
an arbitrary fighter index. Ordinary cancellation and availability effects are
retained. Other unresolved stack/interrupt dependencies are fidelity limits,
not broadly corrected game rules.
