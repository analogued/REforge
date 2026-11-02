<!-- Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later -->

# Mindwheel DOS data and wordcode

This reference describes the DOS layout implemented by the BTZ crate for the
supported Mindwheel data, interpreter and disk map. It is not a specification
for every BTZ release. Other interpreter revisions are unvalidated.
See the [engine overview](../README.md) for supported inputs and the
[runtime reference](mindwheel-dos-runtime.md) for input, timing and host services.

Story rules reside in supplied wordcode and object data. `GO.EXE` also supplies
text compression tables, character classes, native strings and an RNG adjustment.
The Rust engine reads those resources without executing the DOS program.

The supported file identities are:

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `ADISK.OBJ` | 188,928 | `4572f6fae68fa81be3e723c0a4a90994fd2c9f1219636a334d2db0761db0b091` |
| `GO.EXE` | 33,280 | `268edea9a2ed7df72cd41f2d3c767a57e6ebbe019037b46638c06be5c8512bcb` |
| `SIDE.TXT` | 65 | `40f185dc69d51a5f98769f5d89f10405f29132ebbbdd73290c7f139a12efd1e7` |

These identities scope the format and interpreter rules below. REforge's
application catalog identifies this release; the crate's object loader validates
the supplied container's structure.

## Container and address spaces

The first little-endian word of `ADISK.OBJ` gives the initial variable image's
length in bytes, including that word. The directory starts at the next
512-byte boundary: `ceil(variable_bytes / 512) * 512`.
Directory fields are little-endian block numbers measured from the file start;
each block contains 512 bytes. End blocks are exclusive.

| Store | Contents | Start field | End field |
| --- | --- | --- | --- |
| 0 | Program and immutable object/property tables | `0x04` | `0x10` |
| 1 | Object grammar phrases | `0x0e` | `0x1a` |
| 2 | Message boundaries | `0x0c` | `0x18` |
| 3 | Dictionary tree and compressed lexemes | `0x08` | `0x14` |
| 4 | Message dictionary references | `0x0a` | `0x16` |

These field offsets are relative to the directory. Store 0 starts at the
directory itself, so its offsets include the directory bytes. The primary
`StoryImage` loader checks that the five extents are contiguous, start at the
directory and together reach the file end. It rejects invalid or truncated ranges.

Most stored words are little-endian. Grammar references in store 1 are the
exception: they are big-endian words interspersed with single-byte separators.
An address is meaningful only with its address space:

| Value | Unit and origin |
| --- | --- |
| `ProgramAddr` and wordcode branch targets | Byte offset within store 0 |
| `VarAddr` | Byte offset within the mutable variable image |
| Object ID | Index into object tables; not a byte address |
| Dictionary reference | Byte offset within store 3 |
| Grammar boundary | Byte offset within store 1 |
| Message boundary | Word index within store 4 |
| Container extent | 512-byte block index within the file |

Program words occupy two bytes; advancing one word adds two to its address.
Address arithmetic follows the interpreter's wrapping 16-bit operations, followed
by checked access to the corresponding Rust buffer.

## Initial variables and layout words

`Vm::new` copies the initial variable image, writes `0xffff` at offset zero and
writes the constants 1 through 14 at offsets `0x02` through `0x1c`. The entry
address is read from initial offset `0xe8`. The accumulator occupies `0xce`.
Variable writes require even addresses; offsets `0x02` through `0x08` are
protected, and writing offset zero always produces `0xffff`.

Several initial words describe table locations rather than story scalar values:

| Variable-image offset | Meaning |
| --- | --- |
| `0xe0` | Bias for immutable property pairs in store 0 |
| `0xe2`, `0xe4` | Two property IDs sharing a direct-value table |
| `0xe6` | Wildcard key used by property lookup |
| `0xe8` | Initial program byte address |
| `0xee` | Program address of object property-range records |
| `0xf0` | Program address of object grammar boundary table |
| `0xf6`, `0xf8` | Variable addresses of interval lower/upper tables |
| `0xfa` | Byte extent of those interval tables |
| `0x102` | Maximum object ID |
| `0x10c` | Variable address of direct object values |
| `0x10e` | Bias for mutable property pairs |
| `0x110` | Variable address of grammar interval flags |

The implementation reads this layout from the boot image. Replacing text media
does not replace the program, grammar or layout metadata.

## Instruction encoding

An instruction begins with a little-endian word `w`, followed by zero to three
operand words. The opcode is `(w >> 2) & 0x3f`. The implemented opcode range
is `0x00` through `0x22`; larger values are rejected.

The dispatch form is computed as:

```text
form = (w & 3) + ((w & 0x0800) >> 9) + ((w & 0x8000) >> 12)
```

The first two terms select forms 0 through 7. Setting bit 15 produces an
unsupported form, which the decoder rejects. Operand counts for forms 0 through
7 are `[1, 2, 2, 0, 3, 2, 0, 0]`.

Each operand has two addressing bits. For operand index `i` from zero through
two, bit `8+i` contributes 1 and bit `12+i` contributes 2:

| Mode | Operand value from encoded word `r` |
| --- | --- |
| 0: literal | `r` |
| 1: variable | Read variable word at byte address `r` |
| 2: indirect variable | Read variable word at the variable address stored at `r` |
| 3: program at variable | Read program word at the program address stored in variable `r` |

After resolving operands, the form selects the persistent argument used by
opcode dispatch:

| Form | Operands | Argument selection |
| --- | --- | --- |
| 0 | 1 | First operand |
| 1 | 2 | Property value: first operand is object, second is property |
| 2 | 2 | Next related object after the second operand, matching the first |
| 3 | 0 | Current accumulator |
| 4 | 3 | Preserve the previous argument |
| 5 | 2 | Writable property address: first is object, second is property |
| 6 | 0 | Preserve the previous argument |
| 7 | 0 | Preserve the previous argument |

Form 1 provides a nonblocking keyboard-service boundary after operand evaluation
and before property lookup. Forms 4, 6 and 7 do not assign a new argument;
form 4 still consumes and evaluates all three encoded operands.
The instruction pointer advances past the encoded operands before dispatch.

## Opcode behavior

Let `A` be the accumulator and `X` the resolved argument. Arithmetic operates
on unsigned 16-bit words. Addition, subtraction and multiplication wrap modulo
65536; division and remainder are unsigned and reject a zero divisor.
Predicates produce exactly `0xffff` for true and zero for false.

| Opcode | Behavior |
| --- | --- |
| `00` | Pop return address into IP; set `A = X` |
| `01` | Call `X`, pushing the next instruction address |
| `02` | Jump to `X` |
| `03` | Jump to `X` when `A == 0` |
| `04` | Jump to `X` when `A == 0xffff` |
| `05`, `06` | Set predicate `A != X`, `A == X` |
| `07`, `08` | Set predicate `A >= X`, `A <= X` |
| `09`, `0a` | Set predicate `A > X`, `A < X` |
| `0b` | Set `A = X` |
| `0c` | Store `A` at variable byte address `X` |
| `0d`–`0f` | No additional opcode action |
| `10` | Set `A = A + X` |
| `11` | Set `A = A - X` |
| `12` | Set `A = X - A` |
| `13`, `14` | Set `A = A * X`, `A = A / X` |
| `15` | Request an inclusive bounded random value from 0 through `X` |
| `16` | Output message `X` |
| `17`, `18` | No additional opcode action |
| `19` | Dispatch native service `X` |
| `1a` | Output the first grammar phrase of object `X` |
| `1b` | Push `X` on the value stack |
| `1c` | Pop the value stack into variable byte address `X` |
| `1d` | Set `A` to the byte offset of the first interval containing `X` |
| `1e` | Jump to `X` when `A` is neither zero nor `0xffff` |
| `1f` | Output `X` as signed 16-bit decimal |
| `20` | Remember object `X` in parser context |
| `21`, `22` | Set `A = A % X`, `A = A & X` |

“No additional opcode action” still permits operand/form effects. Opcode `1e`
does not push a return address. Calls and jumps with target `0xffff` do nothing.
Both the return stack and value stack have 128 slots; overflow and underflow
are checked. Native service dispatch accepts IDs 1 through 14. Service 12 clears
both active stack depths; service 13 has no additional action.

Interval lookup scans paired inclusive lower/upper bounds in two-byte steps.
If none contains the argument, it returns the configured byte extent. Form 2
scans objects beginning at `second + 1`, compares each direct value with the
first operand and returns the first matching object or `0xffff`.

## Object properties

For object `o`, the property-range record begins at `records + 4*o` in store 0.
Its two words identify mutable and immutable starts; the next record provides
the corresponding ends. Adding the configured biases gives a variable-image
range and a program range. Each range contains four-byte `(key, value)` pairs.

For either direct-property ID, lookup first checks the object's direct-value
slot at `direct_values + 2*o`. A value other than the configured wildcard resolves
the lookup immediately. Otherwise, lookup scans mutable pairs first, then
immutable pairs, returning the first exact key match.

A pair whose key is the wildcard records a fallback object. If neither range
contains the requested key, lookup repeats on that fallback object. A later
wildcard pair replaces an earlier fallback, including one in the immutable
range. Missing fallback is represented by `0xffff`; cyclic fallbacks are rejected.

Value lookup returns `0xffff` for a missing property, or when either input is
`0xffff`. Address lookup returns the variable address of a mutable value;
an immutable or absent value returns zero. The found flag distinguishes an
immutable match from absence even though both have no writable address.
These sentinel roles must not be collapsed into a single host Boolean.

## Dictionary, messages and grammar

For this release's MZ executable, the resource-data origin is the header size
in paragraphs plus the initial stack-segment value, multiplied by 16. This is
the observed executable convention, not a general method for locating data in
arbitrary DOS programs. The engine reads these offsets relative to that origin:

| Offset | Resource |
| --- | --- |
| `0x07f` | 128 input character-class bytes |
| `0x36d` | 512 pair-table bytes |
| `0x56d` | 128 third-character bytes |
| `0x6c7` | Little-endian 32-bit RNG adjustment |

Store 3 begins with a little-endian dictionary cutoff word. Its search root is
at byte offset 2. Each node contains two little-endian child addresses followed
by a NUL-terminated compressed lexeme at node offset 4. Child address zero
ends a search. Lookup compares complete decoded byte strings.

For encoded byte `b`, a zero entry in the first pair table means literal `b`.
Otherwise, decoding emits `pairs[b]` and `pairs[b+256]`, followed by `thirds[b]`
when `b < 128` and that entry is nonzero. Decoded words must remain shorter
than the 40-byte DOS word buffer. The executable supplies the 512-byte pair
tables and 128-byte third-character table, rather than the engine embedding them.

Message `0xffff` is empty. Other supported IDs are below `0xc180`. Their
store-2 boundary address is `2 * (id - 0x8000)`, with 16-bit wrapping. Adjacent
little-endian words give exclusive word-index bounds in store 4. Each store-4
word is a dictionary byte address; `0xffff` terminates the message early.

For object `o`, the words at `phrases + 2*o - 2` and `phrases + 2*o` in store 0
bound its store-1 grammar bytes. Each dictionary reference is big-endian;
an initial byte `0xff` is a one-byte alternate-phrase separator. Object-name
output decodes only the first phrase. Message references and grammar references
therefore differ in both byte order and surrounding record format.

Grammar flags are selected by the same inclusive interval tables used by
opcode `1d`. Bit 0 permits free alternatives; bit 1 permits gaps between
phrase words. Parsed object IDs are written as little-endian words beginning
at variable offset `0x2e`, followed by `0xffff`. The output has room for 79
object IDs and its sentinel. Parser matching behavior is described in the
[runtime reference](mindwheel-dos-runtime.md).

## Bookmarks and secondary media

`SIDE.TXT` is a text map for ten logical disk slots, numbered 0 through 9.
Each slot occupies two successive fields: a decimal availability/disk number,
then a drive character. Zero marks the slot's data as present; it is an
availability marker, not physical disk number 0. A nonzero number requests
that physical disk in the specified drive. Multiple logical slots can refer
to the same drive and medium.
For example, an entry `2` followed by `b` means that selecting this slot needs
physical disk 2 in drive B; an entry `0` followed by `b` marks it ready there.

The ten pairs are followed by another decimal value. The parser retains it as
`trailing_value`, but its gameplay purpose is not established and the current
runtime does not use it. Native fields are read in chunks of at most five raw
bytes; CR is discarded while counting toward that limit, and LF ends a chunk.
This detail matters to parsing existing DOS files, not to the logical map's
availability semantics. See the runtime reference for selection and mounting.

DOS bookmarks are little-endian streams with a 722-byte fixed portion:

| Byte offset | Field |
| --- | --- |
| `0` | Variable-image byte length |
| `2` | 128 value-stack words, including inactive slots |
| `258` | Value-stack depth |
| `260` | 128 return-stack words, including inactive slots |
| `516` | Return-stack depth |
| `518` | Parser history head |
| `520` | 100 parser-history words |
| `720` | Selected physical disk |
| `722` | Variable-image bytes |

The file has no game ID, revision field or checksum. Decoding requires the
expected variable extent and exact total length. IP, decoded operands, RNG,
screen and input buffers are absent; bookmark restore is not a complete
`Session` snapshot. See the runtime reference for continuation semantics.

Physical disk 1 uses the primary directory placement described above. The
observed loader places the directory at byte zero for other physical disks.
`TextStores::from_secondary_dos_obj` reads only stores 2, 3 and 4 using the same
directory fields and absolute block units. Those ranges are checked independently;
the primary container's five-store coverage rule is not imposed. No actual
secondary-media image has been validated, so this path has synthetic coverage
and does not establish compatibility with another release.
