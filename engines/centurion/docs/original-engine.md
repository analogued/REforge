# Original DOS engine reference

This partial reference describes the supported English DOS releases of
Centurion: Defender of Rome. Findings come from recovered program/data behavior
and controlled execution of original routines. They do not specify other releases
or establish exhaustive behavior on historical hardware. Original files and raw
research evidence are external and are not included here. Unless stated
otherwise, addresses refer to the 1.00A layout; the supported 1.00B layout is
adapted during native loading.

## Release and video layouts

The examined 1.00B release retains the same save block table, font and VGA
artwork as 1.00A. Its naval re-engagement question moves from `DS:935A` to
`DS:8EEE`, the outcome strings move forward by `4C`, and five footer pointers at
`DS:970A` through `DS:9712` refer six bytes later into the naval scratch block.
Native loading restores its internal arrangement using the supplied text and
pointers. Version/credits and release-specific new-campaign data are retained.

The 1.00B package also contains EINIT, EMAP, EGLAD, ERACE, ENAVAL and ENEGO
banks for EGA. Their nibble pixels use direct 16-colour attributes; VGA uses
the palette tags and maps at `DS:60C0`. EINIT supplies packed `WORLD.CBM` in
place of VGA's raw `WORLD.LZW`; ENAVAL supplies `GALLEYS.CBM` in place of
NAVL's raw `GALLEYS.LZW`. EGA uses CENT's EMENU sprites. Initialized drawing
pens `DS:6344`, `DS:6346`, `DS:6348` are 3, 7, 7; VGA startup replaces them
with `74`, `1A`, `10C`. VGA DAC cycling does not apply to EGA artwork.

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

### Save and load panels

The slot selector descriptor at `DS:5BCC` uses bounds `(30,40)` to `(290,159)`
over the Game Control menu. Its nine 12-byte row records start at `DS:5B60`;
absolute row bounds are `(36,63+10n)` to `(284,73+10n)` for zero-based row `n`.
The generic menu renderer draws the static `1 -` through `9 -` prefixes at x=47;
the save/load handlers draw descriptions separately at x=65 on the same rows.
The title is centered at `(160,46)`.

Resident Save (`7784`) installs title `DS:5B18`, “Select a Save Position,” in the
shared descriptor. Load (`78F0`) installs `DS:5B4A`, “Select a Game to Load.”
Descriptions are drawn black separately from the descriptor labels; keyboard
selection can whiten the number prefix without repainting the description.
The original description editor instead uses title `DS:5B30`, “Enter a Game
Description,” and edits at the selected row's description position. The native
prompt and focus improvements are described in
[deviations](deviations.md#menu-feedback-and-editing).

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

## Campaign map lookup

The map uses 320 by 200 logical pixels. `DS:3A96` holds one little-endian run-list
pointer per scanline. Each run is a region byte followed by a nonzero pixel-count
byte. Region IDs 0–26 identify provinces and 27–36 identify seas; special values
254 and 255 do not identify a named region. These gaps include both separators
and unnamed water, so a missing region alone does not identify a border.

`DS:5222` holds region-name string pointers, while `DS:1AB8` holds pairs of
little-endian x/y words used as map-symbol anchors. The cached `MMAP` `MAP.CBM`
background contains the original artwork. The 27 province border seeds at
`DS:3C52` are x/y word pairs selecting isolated four-connected artwork paths;
their brown pen also occurs in terrain. The replacement's optional hover panel
is described separately in [deviations](deviations.md#map-hover-info).

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

In DOS 1.00A's enemy-invasion capture path, ownership is cleared before applying
the reputation adjustment, so the status-dependent multiplier becomes `5 / 4`.
The examined 1.00B build calculates `floor(reputation * (5 - previous_status) / 4)`
before clearing ownership instead. For reputation 80 and prior statuses 1–4,
1.00A produces 100 in each case; 1.00B produces 80, 60, 40 and 20. This release
comparison was verified with bounded execution of the original instructions;
it does not establish exhaustive 1.00B support. The native correction is recorded
in [deviations](deviations.md#province-loss-reputation).

The scouting report indexes four wealth descriptions at `DS:5F52` using the
province's plunder reserve divided by 40: small, unremarkable, immense and
legendary. In 1.00A a reserve of 160 or more oversteps this table and reads the
adjacent army descriptions at `DS:5F5A`, producing insignificant, ordinary or
strong. The examined 1.00B build caps the index at 3, so reserves of 120–255
remain legendary. The native engine adopts this
[correction](deviations.md#scouting-wealth-descriptions).

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

Canceling naval flagship selection returns result 0 in 1.00A and result bit 4
(retreat) in the examined 1.00B build. An attacking fleet has already spent its
action; either result aborts entry and leaves it at its origin. During enemy-turn
defense, bit 4 invokes a forced retreat to the first adjacent sea whose stored
distance is no greater than the current sea's and which contains no active
permanent enemy fleet. Cargo moves with the fleet, retaining the legion's action
state. The 1.00B retreat helper additionally marks the fleet's action spent.
With no eligible escape, the fleet and its cargo legion are destroyed.

The legion-removal routine is used for battle annihilation, province/revolt
losses and destroyed transports. In 1.00A it clears the legion's active
type/location but retains fleet/cargo links. The examined 1.00B routine clears
both links. The surviving-transport case occurs when its embarked legion is
annihilated attacking land: stale cargo can prevent boarding another legion or
move an unrelated legion after the slot is reused. Native corrections are
described in [deviations](deviations.md#defensive-fleet-retreat).

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
