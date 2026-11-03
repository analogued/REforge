<!-- Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later -->
# Interpreter definitions

BTZ uses one VM and integrated `Session`. An immutable `Interpreter` definition
selects the behavior that differs between identified implementations. Executable
resource locations live separately in `DosResources`; relocated text tables or
prompts do not by themselves imply different VM semantics.

`MINDWHEEL_DOS`, `ESSEX_DOS`, `BRIMSTONE_DOS` and `BREAKERS_DOS` are descriptive implementation
names. They do not
assert official version numbers. The player loader and diagnostic frontend can
identify their executable hashes before constructing the VM. Existing `new` and
`from_dos_executable` constructors retain the Mindwheel DOS definition and table
layout. Explicit `with_interpreter`, `from_state_with_interpreter` and
`from_dos_resources` constructors support identified alternatives and synthetic
tests without putting file detection inside the VM.

## Shared execution and selected differences

The definitions share address spaces, arithmetic, property traversal, stacks,
RNG arithmetic and BIOS-tick deadline rules. Differences
are selected at the operation that needs them:

| Behavior | Mindwheel DOS | Essex / Brimstone DOS |
| --- | --- | --- |
| Opcodes `0x0d` / `0x0e` / `0x0f` | No operation | Print byte / load variable byte / store variable byte |
| Property keys | Checked against the object limit | May exceed the object limit; the object remains checked |
| Service 7 / 10 | Echo unknown word / echo input | Same |
| Services 15 / 16 / 18 | Unavailable | Get cursor / set cursor / redraw frame |
| Text execution | Direct formatting | Formatting can call wordcode and resume |
| Numeric input | No numeric object slots | Optional range of numeric object slots |
| Story text region | Fixed coordinates | Bounds and cursor exposed through variables |
| Submitted-read conversion | `(scratch_word >> 8) - 1` | `(scratch_word / 85) - 1` |

The submitted-read conversions clamp at zero. Their scratch word is an explicit
adapter input, separate from the clock and from timeout results. The ordinary
player adapter continues to supply result zero; selecting a definition does not
invent a DOS memory environment or change the BIOS tick rate.

Numeric native-service IDs are translated through the definition's service table
into `NativeService` operations. Session code implements those named operations.
The common opcode executor uses a small extension table for slots `0x0d`–`0x0f`,
`0x17` and `0x18`. This permits definitions to share individual behaviors without ordering
all implementations along a single version-number comparison.

## Breakers differences

Breakers retains the extended text callbacks and byte load/store operations,
with these additional selections:

- Operand forms 0, 1, 2, 3, 5 and 7 retain their prior meanings. Form 4 consumes
  no operands and supplies `0xffff`; form 6 consumes none and supplies zero.
  Earlier definitions consume three operands for form 4 and retain the previous
  argument for forms 4, 6 and 7.
- Comparisons `0x05`–`0x0a` save the incoming accumulator in variable `0xEC`.
  Opcode `0x17` restores that value. Opcode `0x18` prints the low argument byte
  through the token formatter, including its case and text-control rules.
  Opcode `0x0d` performs no operation.
- Service 17 captures a screen rectangle, 19 restores it, and 18 draws a
  single-line rectangle when the story bottom is less than row 17;
  otherwise it redraws the full double-line frame. The rectangle uses variables
  `0x11c`–`0x122` and includes the surrounding border. Its character bytes are
  stored in the variable image at the accumulator's byte offset; attributes
  are not stored. These bytes travel in ordinary native bookmarks.
- The full frame ends at column 79, story text at 78, and input history begins
  on row 20. The initial painted cursor remains on row 19. Command capacity
  remains 76 characters. Coordinates are zero-based.
- Before a full grammar sweep, both free matching and gap matching are enabled.
  Object-specific flags can change them as matching proceeds. Earlier
  definitions reset only free matching at that boundary.
- Cancelling or failing to open a transcript clears the accumulator; a successful
  open retains it. The player hosts ask before replacing a Breakers bookmark.

Rectangle operations validate both the physical screen and variable buffer
bounds. They preserve the logical story cursor separately from positioned
screen writes. No x86 execution or alternative save format is needed.

## Numeric input

The numeric-capable parser reads an exclusive range of reserved object IDs from
the initial variable header at `0x114` and `0x116`. Equal endpoints disable the
feature, as in the current Essex and Brimstone data. Breakers reserves four IDs,
607 through 610. Decimal tokens may start with minus;
conversion accepts a decimal prefix and wraps at 16 bits. Values are written to
the corresponding direct-value object slots.

The parser still performs dictionary lookup. A numeric conversion suppresses an
unknown-word miss while its reserved object ID is inserted into grammar output
before the next recognized token, including after the final token. Exhausting
the range restores ordinary dictionary/unknown-word handling. Numeric input does
not require a second grammar matcher or host-side story commands.

## Resumable text and bookmarks

In the callback-capable formatter, pipe controls collect a callback letter and
push individual decimal digits onto the VM value stack. Closing the control
transfers execution through the callback table at variable `0x138`.

A suspended message or object phrase records four ordinary return-stack words:
remaining text position, printing argument, interrupted IP, and a zero sentinel.
Returning through that sentinel restores the printing argument and IP, clears
variable `0x130`, and requests text continuation. Message positions are word
indices; object-phrase positions are byte offsets. Nested callbacks use the same
protocol and formatter state.

These words are included in the existing native bookmark stack arrays. No
additional host-only continuation stack or new save format is introduced. The
bookmark codec remains shared; definitions change only where demonstrated native
behavior requires it.

## Extended output controls

Essex, Brimstone and Breakers expose the story cursor through variables `0x118` (column) and `0x11a`
(row), and its story rectangle through `0x11c`, `0x11e`, `0x120` and `0x122`
(left, top, right, bottom). The physical display remains 80×25 cells. Reflected
extent values at `0x124` and `0x126` do not relocate the physical pause line.

Variable `0x134` enables display and keyboard pauses; zero suppresses those
pauses while ordinary keyboard input continues. With pauses disabled, Ctrl-S
and Escape retain their distinct byte values in input. Variable `0x136` controls
transcript buffering and flushing without closing an already-open transcript.

Successful bookmark restore rebuilds the frame, refreshes the exposed cursor,
and resets the story rectangle and reflected extents. Other saved variable
state remains intact. The bookmark byte format itself is unchanged.

## Validation boundary

Each of the four games has one catalogued DOS release. Brimstone
shares the demonstrated Essex behavior while retaining a distinct definition
and executable resource layout. Its printer-error message moves independently
of the other native messages.

Synthetic tests cover
numeric-range boundaries, nested callbacks, continuation frames, resource
relocation, output controls and both submitted-read conversions. Selected
original-executable comparisons cover title rendering, numeric parsing, callbacks,
byte operations, property keys, cursor services, echoing, pauses and transcripts.
The seven supplied Essex bookmarks also match native load/re-save variable state,
screen output and serialized save bytes through the shared codec. Brimstone
comparisons also cover the title, opening scene and native bookmark exchanges
in both directions. Its 4,240-byte variable image gives a 4,962-byte bookmark;
the codec reads the extent from the data rather than selecting a new format.

Breakers comparisons cover the changed forms, comparison backup, formatted
character output, rectangle capture/restore, numeric input and transcript
cancellation. Its 4,674-byte variable image gives a 5,396-byte bookmark.

These checks and longer gameplay runs do not establish a winning playthrough,
complete behavioral equivalence or support for other BTZ releases. The remaining
pause/input and timing-policy limits in the engine overview still apply.
