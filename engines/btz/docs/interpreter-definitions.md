<!-- Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later -->
# Interpreter definitions

This reference compares the identified DOS interpreters and selected original
story routines. It preserves recovered behavior independently of whether the
replacement needs the original mechanism. The scope is the four catalogued DOS
releases, not an established chronology of official interpreter versions.

BTZ uses one VM and integrated `Session`. An immutable `Interpreter` definition
selects the behavior that differs between identified implementations. Executable
resource locations live separately in `DosResources`; relocated text tables or
prompts do not by themselves imply different VM semantics.

`MINDWHEEL_DOS`, `ESSEX_DOS`, `BRIMSTONE_DOS` and `BREAKERS_DOS` are descriptive
implementation names. They do not assert official version numbers. The
application verifies the required files and passes its existing detected game ID
to BTZ. The player and preview loaders select the corresponding `DosRelease`
profile, which supplies the interpreter definition and resource layout, without
hashing those files again. The standalone diagnostic frontend retains executable
fingerprint lookup under `dev-tools`.

Existing `new` and `from_dos_executable` constructors retain the Mindwheel DOS
definition and table layout. Explicit `with_interpreter`,
`from_state_with_interpreter` and `from_dos_resources` constructors support
identified alternatives and synthetic tests without putting file detection
inside the VM.

## Shared execution and selected differences

The definitions share address spaces, arithmetic, property traversal, stacks,
RNG arithmetic and BIOS-tick deadline rules. Differences are selected at the
operation that needs them:

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

Essex and Brimstone accept service 17 but perform no operation for that selector.
Their services 7 and 10 retain Mindwheel's unknown-word and completed-input echo
roles, respectively.

The submitted-read conversions clamp at zero. Their scratch word is an explicit
adapter input, separate from the clock and from timeout results. The ordinary
player adapter continues to supply result zero; selecting a definition does not
invent a DOS memory environment or change the BIOS tick rate. For a 16-bit scratch
word, the possible submitted results are 0–254 in Mindwheel and 0–770 in the other
three examined interpreters. A real timeout returns its requested timeout value
separately from these calculations.

Numeric native-service IDs are translated through the definition's service table
into `NativeService` operations. Session code implements those named operations.
The common opcode executor uses a small extension table for slots `0x0d`–`0x0f`,
`0x17` and `0x18`. This permits definitions to share individual behaviors without
ordering all implementations along a single version-number comparison.

### Byte operations

The extended byte load uses a byte offset into the variable image and returns
the low byte as an unsigned word. The original implementation obtains that byte
by reading a word and masking with `0xff`; it does not require an even address.
The replacement reads the checked byte directly instead of reproducing an extra
native memory access beyond the last byte.

The byte store writes only the accumulator's low byte. Odd addresses, including
offset 1, are permitted. Offsets 2 through 9 and offsets at or beyond the variable
extent are rejected. Native code first changes address zero to `0xffff`, which
then fails its bounds check; this differs from the ordinary word setter's
special handling of offset zero. Checked replacement writes reject zero
directly. These rules apply to Essex, Brimstone and Breakers, while the byte
opcode slots have no additional action in Mindwheel.

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
  A zero byte supplies an empty C string, which can still introduce a separator.
  Opcode `0x0d` performs no operation.
- Service 17 captures a screen rectangle, 19 restores it, and 18 draws a
  single-line rectangle when the story bottom is less than row 17;
  otherwise it redraws the full double-line frame. The rectangle uses variables
  `0x11c`–`0x122` and includes the surrounding border. Its character bytes are
  stored in the variable image at the accumulator's byte offset; attributes
  are not stored. These bytes travel in ordinary native bookmarks.
- The full frame ends at column 79 and story text at 78. The editor and painted
  cursor both start on row 19. When input history overflows, only rows 20–22
  scroll; row 19 stays in place. Earlier definitions scroll rows 19–22.
  Command capacity remains 76 characters. Coordinates are zero-based.
- Before a full grammar sweep, both free matching and gap matching are enabled.
  Object-specific flags can change them as matching proceeds. Earlier
  definitions reset only free matching at that boundary.
- Cancelling or failing to open a transcript clears the accumulator; a successful
  open retains it. The original Breakers file interface asks before replacing a
  bookmark; the earlier three do not. Modern host confirmation policy is
  documented under [file dialogs](deviations.md#file-dialogs).

Rectangle operations validate both the physical screen and variable buffer
bounds. They preserve the logical story cursor separately from positioned
screen writes. No x86 execution or alternative save format is needed.

## Numeric input

The numeric-capable parser reads an exclusive range of reserved object IDs from
the initial variable header at `0x114` and `0x116`. Equal endpoints disable the
feature, as in the current Essex and Brimstone data. Breakers reserves four IDs,
607 through 610. Decimal tokens may start with minus; conversion accepts a
decimal prefix and wraps at 16 bits. Values are written to the corresponding
direct-value object slots.

The parser still performs dictionary lookup. A numeric conversion suppresses an
unknown-word miss while its reserved object ID is inserted into grammar output
before the next recognized token, including after the final token. Exhausting
the range restores ordinary dictionary/unknown-word handling. Numeric input does
not require a second grammar matcher or host-side story commands.

## Resumable text and bookmarks

In the callback-capable formatter, pipe controls collect a callback letter and
push individual decimal digits onto the VM value stack. Closing a control during
message or object-phrase output suspends its iterator and transfers execution
through the callback table whose variable-image address is stored at `0x138`.
Standalone formatted tokens, including Breakers' opcode `0x18`, can collect the
same controls without dispatching a callback.

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

Essex, Brimstone and Breakers expose the story cursor through variables `0x118`
(column) and `0x11a` (row), and its story rectangle through `0x11c`, `0x11e`,
`0x120` and `0x122` (left, top, right, bottom). The physical display remains
80×25 cells. Reflected extent values at `0x124` and `0x126` do not relocate the
physical pause line.

Variable `0x134` enables display and keyboard pauses; zero suppresses those
pauses while ordinary keyboard input continues. With pauses disabled, Ctrl-S
and Escape retain their distinct byte values in input. Variable `0x136` controls
transcript buffering and flushing without closing an already-open transcript.

Successful bookmark restore rebuilds the frame, refreshes the exposed cursor,
and resets the story rectangle and reflected extents. Other saved variable
state remains intact. The bookmark byte format itself is unchanged.

## Original story read wrappers

Native input-return conversion and its story-level consumers are separate
mechanisms. Let `R` be the native read result. Essex's ordinary wrapper compares
`R` directly with the timeout in variable `0x226`, initially 500 in the supplied
story. The later two stories additionally store a scaled result:

| Story | Stored value after the read | Timeout comparison |
| --- | --- | --- |
| Brimstone | `((R * 30) mod 65536) / V[0x150]` in `V[0x22a]` | Original `R` against `V[0x152]` |
| Breakers | `((R * 30) mod 65536) / V[0x182]` in `V[0x260]` | Original `R` against `V[0x184]` |

Here `V` denotes variable words at byte offsets; division is unsigned. Brimstone
also scales its requested timeout before the read as
`((V[0x22c] * V[0x150]) mod 65536) / 30`, saving that timeout in `V[0x152]`.
An observed initial ordinary wait has `V[0x150] = 30` and a 450-tick timeout;
these are story values rather than native interpreter constants.

These computations are established in the original wordcode. Their complete
downstream gameplay effects remain unverified, and the scaled fields have not
been established as global story clocks. In particular, a different submitted
result cannot be characterized only as an RNG-sequence change for these games.
The replacement executes these wrappers from the original data; they are not
host-side timing formulas or reasons to invent another interpreter version.

## Executable resource layouts

These are data offsets within the identified interpreter executables, relative
to the resource origin described in the
[format reference](mindwheel-dos-format.md#dictionary-messages-and-grammar).
Locations and contents must be considered separately: moving a table or string
does not establish a changed VM operation.

| Resource | Mindwheel | Essex | Brimstone | Breakers |
| --- | --- | --- | --- | --- |
| Input classes, 128 bytes | `0x07f` | `0x022` | `0x022` | `0x022` |
| Key-wait prompt | `0x0ff` | `0x105` | `0x105` | `0x114` |
| Compression pairs, 512 bytes | `0x36d` | `0x312` | `0x322` | `0x38b` |
| Third-character table, 128 bytes | `0x56d` | `0x512` | `0x522` | `0x58b` |
| RNG adjustment, 4 bytes | `0x6c7` | `0x6d5` | `0x6e5` | `0x73e` |
| Printing-device prompt | `0x23f` | `0x213` | `0x223` | `0x246` |
| Printing-device error | `0x18f` | `0x163` | `0x18c` | `0x19b` |
| Program-disk prompt | `0x270` | `0x244` | `0x254` | `0x277` |
| Wrong-disk prompt | `0x297` | `0x268` | `0x278` | `0x29b` |

Breakers also has the same pause-prompt text at `0x0de` for editor pauses.
Brimstone's printing-device error relocation is not the uniform `+0x10` movement
seen in its other listed strings relative to Essex.

The examined later three interpreters have identical compression-table bytes.
Relative to Mindwheel, the significant change is encoded byte `0x7c`: Mindwheel
expands it to `ell`, whereas the later tables leave it as literal `|`, available
to the callback-capable formatter. This is a resource-content difference as well
as a change in text execution. It does not establish that every story exercises
the callback mechanism.

## Breakers title program

The supplied `TITLE.EXE` is a separate DOS presentation program preceding
`RUN.EXE`, rather than wordcode in `ADISK.OBJ`. It presents three timed stages:
title artwork, that title with copyright, and credits. Each stage waits for 16
changes of the BIOS tick count; neither artwork page waits for Enter. The
original also performs short wipe transitions between its presentations.

The replacement reads the original presentation data without executing x86,
uses elapsed monotonic time for the stages, and substitutes immediate page
changes for those wipes. The title program's timing is separate from the
interpreter's story input deadlines.

## Validation boundary

Each of the four games has one catalogued DOS release. Brimstone shares the
demonstrated Essex behavior while retaining a distinct definition and executable
resource layout. Its printer-error message moves independently of the other
native messages.

Synthetic tests cover numeric-range boundaries, nested callbacks, continuation
frames, resource relocation, output controls and both submitted-read conversions.
Selected original-executable comparisons cover title rendering, numeric parsing,
callbacks, byte operations, property keys, cursor services, echoing, pauses and
transcripts. Tested Essex bookmarks also match native load/re-save variable state,
screen output and serialized save bytes through the shared codec. Brimstone
comparisons also cover the title, opening scene and native bookmark exchanges
in both directions. Its 4,240-byte variable image gives a 4,962-byte bookmark;
the codec reads the extent from the data rather than selecting a new format.

Breakers comparisons cover the changed forms, comparison backup, formatted
character output, rectangle capture/restore, numeric input and transcript
cancellation. Its 4,674-byte variable image gives a 5,396-byte bookmark.

Command replays of Mindwheel, Essex and Breakers reach their winning endings,
including native bookmark save/restore. They submit commands at input waits with
the default-zero submitted-read adapter. Mindwheel and Breakers use fixed host
ticks; Essex advances the clock at selected waits for time-dependent events.
Complete live playthroughs, a winning Brimstone route, behavioral equivalence and
support for other releases remain unverified. The remaining pause/input and
timing-policy limits in the engine overview still apply.
