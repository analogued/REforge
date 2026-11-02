<!-- Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later -->
# Interpreter definitions

BTZ uses one VM and integrated `Session`. An immutable `Interpreter` definition
selects the behavior that differs between identified implementations. Executable
resource locations live separately in `DosResources`; relocated text tables or
prompts do not by themselves imply different VM semantics.

`MINDWHEEL_DOS` and `ESSEX_DOS` are descriptive implementation names. They do not
assert official version numbers. The player loader and diagnostic frontend can
identify their executable hashes before constructing the VM. Existing `new` and
`from_dos_executable` constructors retain the Mindwheel DOS definition and table
layout. Explicit `with_interpreter`, `from_state_with_interpreter` and
`from_dos_resources` constructors support identified alternatives and synthetic
tests without putting file detection inside the VM.

## Shared execution and selected differences

Both definitions use the same eight operand forms, address spaces, arithmetic,
property traversal, stacks, RNG algorithm and BIOS-tick deadline rules. Differences
are selected at the operation that needs them:

| Behavior | Mindwheel DOS | Essex DOS |
| --- | --- | --- |
| Opcodes `0x0d` / `0x0e` / `0x0f` | No operation | Print byte / load variable byte / store variable byte |
| Property keys | Checked against the object limit | May exceed the object limit; the object remains checked |
| Service 7 / 10 | Echo unknown word / echo input | Echo input / echo unknown word |
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
The common opcode executor uses a small extension table for the three changed
slots. This permits definitions to share individual behaviors without ordering
all implementations along a single version-number comparison.

## Numeric input

The numeric-capable parser reads an exclusive range of reserved object IDs from
the initial variable header at `0x114` and `0x116`. Equal endpoints disable the
feature, as in the current Essex data. Decimal tokens may start with minus;
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

## Validation boundary

Mindwheel remains the catalogued player release. Essex behavior is available to
the diagnostic frontend while gameplay integration is being validated. Synthetic
tests cover the selected differences, numeric-range boundaries, nested callbacks,
continuation frames, resource relocation and both submitted-read conversions.
Selected original-executable comparisons cover title rendering, numeric parsing,
callbacks and cursor services. These checks do not establish a winning playthrough,
complete Essex compatibility, or support for other BTZ releases. Essex's extended
pause/transcript control variables still need integration and native comparison;
full native bookmark restore paths also await comparison before player support
is advertised.
