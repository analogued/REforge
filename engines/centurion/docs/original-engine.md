# Original DOS engine reference

This partial reference describes the supported English DOS 1.00A release of
Centurion: Defender of Rome. Findings come from recovered program/data behavior
and controlled execution of original routines. They do not specify other releases
or establish exhaustive behavior on historical hardware. Original files and raw
research evidence are external and are not included here.

`DS:` addresses below are hexadecimal offsets in the original data segment, not
file offsets or fixed DOS load addresses. Save offsets and pixel dimensions are
decimal. The [engine README](../README.md) describes replacement interfaces and
fidelity limits; [deviations](deviations.md) records deliberate changes.

## Campaign saves

The DOS save image is 1,780 bytes and begins with `10A` followed by a zero byte.
Nine slot files are named `0.CSV` through `8.CSV`; the displayed slot numbers are
1–9. The extension does not indicate a comma-separated text format.

The image concatenates blocks selected by the address/length table at
`DS:5BDE` through `DS:5C41`. Each four-byte entry holds a little-endian address
and size; zero-size entries contribute no bytes. It is not a flat dump of the
whole data segment. The replacement's [save codec](../src/state.rs) reads that
table from the supplied resources and requires its block sizes to total 1,780.
The initial campaign image comes from `CENT`'s `NEWGAME.LZW` resource.

| Save offset | Contents |
| --- | --- |
| 0–3 | `10A` and zero terminator |
| 4–51 | Description field; the native editor accepts up to 47 ASCII bytes plus termination |
| 52 | Land-battle difficulty |
| 53 | Chariot-racing difficulty |
| 54 | Gladiatorial-combat difficulty |
| 55 | Naval-battle difficulty |

This is a partial field map, not a complete save specification. Presentation
preferences such as cursor visibility, menu decorations and continue-prompt
input are native host state and do not add fields to the DOS save image.

### Difficulty menus

The four difficulty bytes at `DS:048E` through `DS:0491` use levels 0–3 for
Galley Slave, Legionnaire, Senator and Emperor. A global preset sets all four
and returns to the map. Fine Tuning changes one category at a time, immediately;
leaving a child or parent menu does not roll back earlier selections.

The global, Fine Tuning and category-level menu descriptors start at `DS:05B8`,
`DS:05CA` and `DS:05DC`. They share panel bounds `(81,79)` to `(251,197)`, with
row origins starting at y=102 and advancing ten pixels. The global menu has
four presets, Fine Tuning and Continue. Fine Tuning has the four categories and
Continue; a category menu has four levels and Continue. The reconstruction uses
these descriptors in [difficulty](../src/difficulty.rs) and the session renderer.

The difficulty handler paints the applied preset (or Fine Tuning for mixed
levels) white before entering its input loop. In mouse mode, pointer movement
does not repaint the labels, so that applied label stays white while browsing.
Keyboard mode instead clears the previous white label and paints the new focused
row white during arrow navigation. The stored level changes only on activation.
The native frontend's selectable applied-value styles, including its persistent
white applied label for both input methods with Original, are documented in
[deviations](deviations.md#applied-value-indicator).

## Mouse and keyboard presentation

DOS selects its menu presentation according to its startup input mode. Mouse
menus use capped three-line rails; keyboard menus use square selectors. Vertical
menus and fleet/legion selectors use a continuous rail, while side-by-side
Yes/No choices have separate rails.

The shared acknowledgment routine hides and restores the software pointer.
Complete fleet/legion lists and province lists do likewise; interactive selection
and Yes/No dialogs show the pointer while accepting a choice. Continue-prompt
wording is chosen once at startup according to mouse availability.

Ship quantity editors show OK and Cancel in mouse mode. Keyboard mode instead
shows an Enter/ESC instruction line. The native editor's always-visible actions
and independently selectable decorations are described in [deviations](deviations.md).

About Centurion combines the author/copyright block at `DS:05EE` with the
version string at `DS:046E`, using the centered message renderer. The body is
black and the continue footer white. Native acknowledgment preferences apply
without replacing the original credits.

## Enemy fleets and transported armies

Enemy inspection displays a fleet-name/location notice. Permanent enemy fleets
do not use the Roman Ready/Moved/Forming turn cycle. Their cargo byte indexes
an enemy army record, not a Roman legion. There are fourteen valid enemy army
records: eleven roaming slots and three dedicated permanent-fleet slots. Each
army record stores infantry, cavalry and elephant counts; a fleet with no cargo
has no such army. The added native inspection table's Troops total is described
in [deviations](deviations.md#fleet-and-army-inspection).

The three permanent fleets normally occupy distinct fixed seas. Original
inspection reports the first active matching permanent fleet rather than
listing several. Artificially placing several in one sea overlays their map
symbols at the same coordinate. The information routine checks roaming armies
at sea before permanent fleets. The replacement's different Roman/enemy
co-location priority is recorded in [technical limits](../README.md#validation-and-limits).

A roaming army at sea becomes a temporary Marauding Fleet encounter. Its ships
are generated with a random ship type when combat starts; there is no persistent
ship count to display before that point. Inspection can read its stored infantry,
cavalry and elephants without generating ships or advancing RNG. These record
meanings underpin the added native detail panels; DOS shows the original notice.

## Movement and battle behavior

The map's Enter action selects a legion for movement, while Space selects a
province or a ready local fleet. The two actions intentionally lead to different
flows. When a land destination requires an encounter, resident code at `0x398A`
marks the legion Moved before calling the encounter handler at `0x4B3F`.
Withdrawing from negotiations or canceling formation/tactic selection does not
restore readiness: the attempted expedition has already spent the move. In an
encounter canceled before any combat, the legion's stored location remains at
its origin and no battle casualties are applied. The replacement retains this
behavior.

A commander's death does not immediately end a battle; replacement handling
follows. Some tactical positions can remain in a standoff until further orders
or retreat. The replacement retains these behaviors.

Map music continues through legion/fleet selection, destination selection,
readiness messages and cancellation. Returning from a province's Move command
to the destination map requests the map score. Committing a valid land destination
stops it before friendly movement, landing or encounter handling. Boarding into
sea and ordinary fleet sailing do not stop it. A map redraw requests the same
score without restarting an already selected score.

The reconstruction preserves these sound requests separately from host playback.
CPU-dependent polling, interrupt timing and uninitialized stack values remain
explicit [fidelity limits](../README.md#validation-and-limits), rather than a
claim that every original execution is deterministic from a save alone.
