<!-- Copyright (c) 2026 analogued. -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Mindwheel DOS runtime

This reference describes the observed Mindwheel DOS runtime implemented by the
BTZ crate. It covers execution around the wordcode VM: input, parsing, text,
native services, persistence and timing. Binary layouts and address spaces are
specified separately in [Mindwheel DOS formats](mindwheel-dos-format.md).
See the [engine overview](../README.md) for supported use and build entry points.

The story program and its rules come from externally supplied `ADISK.OBJ` data.
`GO.EXE` supplies this release's tables, native strings and RNG adjustment;
its machine code is not executed by the recreation. `SIDE.TXT` supplies disk
mapping. Original game files and extracted text/tables are not bundled.
These observations do not establish compatibility with another BTZ release.

## Execution responsibilities

`Vm` executes wordcode with 16-bit values and typed address spaces. Arithmetic,
properties, predicates and control flow follow the observed DOS rules. Native
operations needing runtime work suspend through `Request`; the VM does not open files,
read a terminal or choose a host random source.

`Session` connects the VM to `Parser`, `TextOutput`, `InputEditor`, `DosRandom`
and `DosReadTiming`. Its results distinguish instruction progress, parsed input,
timeout, key waits, platform requests and process exit. A suspended operation
remains pending until its required work completes.

`HostRequest` and `HostReply` separate filesystem, media and process actions from
interpreter algorithms. The player hosts supply an SDL window or attached
terminal, BIOS-clock inputs and filesystem paths. Presentation and application
detection remain separate from the VM.

`new_with_keyboard` and `advance_with_keyboard` accept a nonblocking source of
currently available typed keys. These entry points service input while executing
operand and output operations as well as explicit reads. `new` and `advance`
assume no host keys at those internal boundaries, allowing deterministic
headless execution with explicitly supplied input and timing.

## Keyboard editor and read service

The implemented screen mode is 80 columns by 25 rows. The lower input pane
accepts 76 ordinary bytes; CR can still be appended as the 77th byte to submit
the line. Input keeps its original case. LF does not submit or move the editor.
Submitting CR clears variable `0xda`, marks the line ready, erases the current
block cursor and advances to the next input row.

Backspace deletes the last buffered byte and moves the block cursor back.
Tab inserts ten literal spaces, subject to the same character limit. Native
Home/F5 erases back to column 1; End/F3 recalls the previous line's suffix from
the current column. Right/F1 recalls one previous character. Recall preserves
case and stops before CR, LF or NUL. Up/down and unused translated extended
keys are ignored by the observed editor.

Escape and Ctrl-S enter the editor's pause state before its submitted-line
check. Repeated pause keys or empty polls leave that pause active. CR/LF dismiss
it without submitting another line; another dismissal key continues through
normal editing. The low-level editor emits a bell action for BEL rather than
painting a glyph, although it still stores the byte and advances its logical
column. Screen updates and audible feedback are separate host actions.

Ctrl-G supplies BEL (`07`) in both player frontends. The SDL host presents it
using the shared PIT/speaker model and band-limited renderer, with master and
Effects gains, focus mute and the selected output device. Its reference bell
uses a channel-2 mode-3 divisor of 1320 (about 904 Hz) for 333 ms, following
[DOSBox-X's BIOS teletype bell](https://dosbox-x.com/doxygen/html/int10__char_8cpp_source.html#l00787).
A short synthesis tail settles the output. Playback is asynchronous; Session
does not emulate the BIOS's blocking delay. This is a presentation reference,
not a claim that every original PC BIOS used an identical bell.

The terminal host flushes BEL immediately, including during read callbacks.
It suppresses BEL when sound is disabled or master/Effects volume is zero;
otherwise terminal preferences control the audible/visual response and its
volume. It does not initialize SDL audio or play a second sound alongside BEL.

The Unix terminal adapter must decode key sequences before supplying these
editor keys. A bare Escape byte is also the prefix of terminal arrow/function
sequences, such as Left's `ESC [ D`. The adapter allows up to 30 ms for the
remaining bytes to arrive, then treats a lone Escape as the pause key. This is
key identification by the host, not a delay intended for the player. The
original DOS BIOS interface supplies distinct key/scan codes and has no such
30 ms disambiguation rule. Ctrl-S has no terminal-sequence ambiguity.

The input pane scrolls independently of story output. When its cursor reaches
the lower separator, rows 19 through 22, columns 1 through 77, scroll upward.
These coordinates, and the other screen coordinates below, are zero-based.
Rebuilding the frame clears the current editor and submitted flag while keeping
previous-input recall history.

Service 1 saves the incoming accumulator as its timeout, clears the accumulator
and token count, and flushes the transcript remainder. A read iteration checks
already-submitted input first, then the BIOS deadline, then polls the keyboard.
It advances the raw RNG once after that editor batch, including a batch ending
in CR. A buffer submitted earlier during output or operand evaluation therefore
has priority over a deadline that is already expired when the read begins.

On submission, the line copier retains bytes through the first CR in both the
original-case completed-input buffer and an ASCII-lowercase parser buffer.
A buffer without CR is discarded. NUL ends later C-string consumers. The
editor acknowledges the submitted line only after the parser consumes it.
On timeout, completed-input strings are empty, but partially edited raw input
and its cursor survive for a subsequent read.

## Tokenization and grammar

Character classification comes from the executable's 128-byte table. Indexing
uses `byte & 0x7f` while retaining the original byte. Mask `0x02` permits a token
to start; mask `0x04` marks a delimiter. A start byte with both masks becomes a
single-byte token. Otherwise copying continues until a delimiter, which remains
available to the next tokenizer call. Host whitespace or Unicode splitting
would change this behavior.

Dictionary lookup is an exact byte-string comparison through the supplied tree.
A successful lookup above the input-address cutoff is ignored without setting
the unknown flag. A miss sets that flag and replaces the retained unknown-word
buffer. Later known lines do not clear the retained buffer. Numeric text follows
the same table and dictionary path; the tokenizer invents no numeric token type.

Variable `0xd8` counts tokens considered, including unknown and ineligible words.
At most 39 eligible dictionary IDs enter grammar matching; reaching that limit
stops further tokenization. Grammar matching receives only those recognized IDs,
while the lexical unknown flag remains independent of any successful match.
After parsing, that flag can push the interrupted VM return address and transfer
control to the story's own unknown-word handler.

The matcher first tries remembered objects from newest to oldest, then scans
objects in the original order. A phrase can require contiguous words or allow
gaps. Interval-selected object flags also distinguish consuming matches from
free alternatives. Sticky match flags change at the original successful-word
comparison points, rather than merely when an object is visited.

Overlapping matches are arbitrated using their covered spans and contiguous
prefixes. A containing contiguous match can replace an earlier candidate;
objects inside an allowed gap can coexist. Free alternatives are emitted at
their recognized-token positions, preserving encounter order within each
position without consuming those positions. These rules
preserve the data-defined grammar instead of choosing a host-language parser.

The remembered-object ring has 100 words and a terminator, allowing at most 99
remembered objects during traversal. Startup initializes its first two words to
`0xffff`; inactive words are retained. Remembering `0xffff` does nothing.
The phrase staging capacity is 399 decoded words plus its final sentinel.
There are checked limits of 40 span candidates and 70 free alternatives.

Parsed object output begins at variable offset `0x2e` and ends with `0xffff`.
Its 80-word capacity includes that sentinel. Service 2 returns the sentinel's
variable-image address. The service wrapper preserves accumulator and argument
around grammar matching; input timing determines the read result independently.

## Text and logical screen

Messages and object phrases decode into dictionary words and control tokens.
The formatter keeps capitalization, uppercase-next-word, quote, concatenation
and sentence-spacing state across message calls. Commas and closing punctuation
attach to the preceding word; opening brackets and quotes affect the following
separator. A sentence boundary normally introduces two spaces before the next
word, with line-start handling preventing an extra indent after wrapping.

| Token | Formatting effect |
| --- | --- |
| `0x02` | Emit one explicit space and suppress automatic separation. |
| `0x03` | Capitalize the next eligible word. |
| `0x06` | Emit the wide-mode 20-space indent. |
| `0x0c` | Wait for a key, then clear the upper pane. |
| `0x0e` | Consume a formatting token without visible output. |
| `0x10` | Start a paragraph with a newline, two spaces and capitalization. |
| `0x14` | Emit nine spaces. |
| `0x15` | Clear initial-capitalization state. |
| `0x17` | Uppercase the next eligible word. |
| `@` | Start a line without creating a sentence-capitalization boundary. |

The screen stores CP437 bytes and character attributes independently of its
terminal or graphical presentation. The frame occupies rows 0, 18 and
23, with vertical edges at columns 0 and 78. Story output scrolls inside rows
1 through 17 and columns 1 through 77. Input uses rows 19 through 22; row 24
holds pause/status text. Ordinary output cannot scroll the input pane.

Wrapping uses the native logical line length and current-word buffer. A word
that crosses the boundary is erased, a newline is emitted, and the word is
replayed in the next row. Separator spaces and sentence state follow the same
character path. Paginated line breaks decrement the counter in variable `0xdc`;
exhaustion reloads it from `0xde` before waiting. The upper-pane clear control
also reloads the counter after its pause. `0xffff` disables pagination counting.

The property-value operand form polls the keyboard before resolving its
property. The native upper-character writer also polls after output, including
wrapping erasures and replayed characters. These polls allow typeahead and
editor pauses during story output; they do not make idle RNG calls. Pausing
keeps the interrupted operation pending until the editor resumes.

Explicit key waits consume a dismissal key. CR/LF only dismiss the wait;
other keys are forwarded to the editor. The native key-wait loop advances RNG
on empty polls. The editor's internal pause loop polls without RNG advancement.
Current boundary-ordering limitations are described at the end of this reference.

## Native service dispatch

The wordcode native-service operation accepts the following observed selectors.
A selector number identifies an interpreter operation, not a command alias;
story wordcode decides when it is used.

| ID | Operation | Runtime effect |
| --- | --- | --- |
| 1 | Read and parse | Wait for input or deadline, tokenize and match grammar, then follow the story's unknown-word handler when required. |
| 2 | Parsed-output end | Put the trailing parsed-output sentinel address in the accumulator. |
| 3 | Wait for a key | Pause without clearing either pane after dismissal. |
| 4 | Exit | Clear the screen, close printing when active and request process exit with code 0. |
| 5 | Write bookmark | Request host persistence of the current native bookmark record. |
| 6 | Read bookmark | Load and validate a record, restore its saved state and complete any disk transition. |
| 7 | Print unknown word | Format the retained unknown word, then suppress subsequent case conversion. |
| 8 | Open printing | Request a transcript/device name and host open, then rebuild the frame. |
| 9 | Close printing | Close the transcript handle without flushing its unfinished logical line. |
| 10 | Print completed input | Format original-case completed input, then suppress subsequent case conversion. |
| 11 | Reload initial variables | Clear the accumulator, select logical slot 0 and initialize variables from the original image. |
| 12 | Reset stacks | Clear active return/value depths while retaining their backing words. |
| 13 | No operation | Return without changing interpreter state. |
| 14 | Select disk | Use the accumulator as logical slot and perform mapped media selection. |

Decimal output, object-word output and remembered-object operations also have
native implementations, reached through their own wordcode operations rather
than additional selectors. Decimal output uses signed 16-bit presentation.
Service 4 terminates the session; treating it as a resumable clear-screen call
would execute subsequent story instructions incorrectly.

## Host requests, bookmarks and media

The engine queues explicit requests for saving/loading bookmarks, transcript
name/open/write/close operations, side-map reads, media mounting and exit.
`complete_host` accepts only a reply matching the outstanding request. The host
controls paths and file handles; the engine controls serialized bytes and the
pending native continuation. Cancellation completes the relevant file service
without inventing an accumulator success/failure value.

Bookmark restore replaces variables, active and inactive stack slots, remembered
objects, ring position and selected physical disk. It retains the load
invocation's IP, argument and operands, and leaves RNG state unchanged. Screen,
input strings and sticky grammar-match flags are outside the bookmark record.
The format has no game/revision identifier or checksum; the expected variable
extent is checked before replacing live state.

Restore uses a raw variable writer, preserving even offset 0 and the constants
protected from ordinary wordcode writes. A same-logical-disk restore retains the saved accumulator.
A required disk transition can use that accumulator for the loaded slot.
Rebuilding the frame resets cursors and current input while retaining recall.

Service 11 reloads initialized variables while keeping current IP, argument,
stacks, parser history and input-pane state. The story can subsequently reset
its stacks through service 12. This service sequence is distinct from restoring
a bookmark or constructing a fetch-boundary `VmState`.

Logical disk slots are separate from physical data-disk numbers and drive bytes.
The story's selected-slot variable is at `0x1e`. Services 11 and 14, and bookmark
restore, select through the ten-entry `SideMap`. A zero number in the target
entry means ready; any other number requests that physical disk with the
entry's drive character. An unavailable mapping requests media and rereads
`SIDE.TXT` until the selected entry is marked ready.

The modern host represents that request as a folder selection: it reads
`ADISK.OBJ` and `SIDE.TXT` from the chosen folder and returns the new text stores
and map. DOS drive characters are map labels here, not host filesystem paths.
If the ready target's drive differs from the previous slot's drive, the engine
replaces stores 2, 3 and 4, retaining program store 0 and grammar store 1.
The secondary-media path is implemented with checked bounds; compatibility with
original secondary media remains unverified.

Both player hosts start bookmark and transcript selection beside the supplied
`ADISK.OBJ`. `--save-dir directory` overrides that starting directory for both
writing and resuming bookmarks and for transcripts. The windowed browser resolves
relative paths from its currently displayed folder; terminal prompts use the
starting directory. An absolute name keeps its chosen location. Explicit
`--bookmark-path` and `--transcript-path` arguments override prompt selection;
relative command-line file paths resolve from the launch directory. Bookmark
writes use atomic replacement, and active original input paths are protected
from output selection.

## Transcript behavior

The formatter records ordinary bytes and marks spaces even while printing is
disabled. A normal line break flushes through the last marked space; an explicit
LF first moves the mark to the current length. Wrapped words are replayed after
the buffer is cleared. A remainder flush emits everything still buffered and
adds a newline even when the remainder is empty.

Host write requests already contain DOS CRLF line endings. The checked logical
buffer reserves room for newline and C termination within its native 160 bytes.
Opening or closing a transcript does not flush or clear an unfinished line;
service 1 explicitly flushes the remainder before beginning its read.
Successful open rebuilds the frame while retaining formatter and recall state.
Modern filename prompts and I/O-error presentation belong to the host adapter.

## BIOS timing and random state

`DosReadTiming` takes an original 16-bit timeout, a BIOS tick count and an explicit
native counter word. The timeout is sign-extended before addition with 32-bit
wrap. Clock/deadline comparison is signed. `0xffff` disables timeout; deadlines
are not normalized across the BIOS midnight reset.

The submitted-read calculation in the observed executable reads an uninitialized
native stack word. Its result is `max((word >> 8) - 1, 0)`, not measured elapsed
time. DOS load layout and prior cache/stdio calls can change that word. Original
game files alone do not determine it. The player hosts use an explicit
submitted-read return of zero by default. `set_native_read_counter_word` or the
observed-result adapter supplies another value for a chosen environment; valid
submitted results range from 0 through 254. Exact DOS memory/stack reconstruction
and complete host pacing identity are not established.

The terminal host converts a monotonic clock to BIOS ticks for deadlines and
wakes read waits at the next tick or earlier keyboard activity. Execution speed
does not scale the tick rate. Its poll schedule is not an emulated DOS CPU loop:
in particular, explicit key waits do not reproduce the native empty-poll RNG
advancement. Matching deadline arithmetic is separate from matching an exact
random sequence.

The story makes an additional bounded random draw using the input return, then
discards that draw's value. The draw still advances generator state, including
rejection attempts. A different input return can therefore change later random
results. Later story draws retain their own literal, property-derived or computed
bounds; the input return does not replace those bounds. It also participates in
the story's comparison with its timeout value.

`DosRandom` preserves the observed 32-bit fold/multiply arithmetic, signed
comparison, executable-supplied adjustment and wrapping 16-bit counter. A bounded
draw XORs the raw result with that counter, applies the selected mask and rejects
values above the requested inclusive maximum. Raw idle calls leave the bounded
counter unchanged. Substituting a host RNG would change both state progression
and rejection behavior.

## Compatibility boundaries

Three narrow input/presentation behaviors remain incomplete in the current host:

- A pause key dismissing a title or pagination wait can release pending text
  before the editor enters its own pause. The upper pane may clear or advance
  while the player intended to keep it paused.
- An already-submitted line can make an operand/output callback stop before
  consuming its returned key batch, losing the remaining keys. A queued pause
  and resume key can require an additional keypress to resume.
- A logical newline currently exposes only the LF keyboard boundary. The native
  writer polls separately around CR and LF, so a pause at the missing boundary
  cannot stop the row advance even though ordinary rendered text is unchanged.

Malformed pointers, stores, buffers, stacks and bookmarks return checked errors.
The original interpreter's diagnostic pause/recovery and unsafe corruption on
such faults are not reproduced. Existing logical-state construction is not a
full suspended-session checkpoint API. Broader gameplay progression, original
secondary media and other BTZ releases still require separate validation.
A complete winning playthrough of Mindwheel has not been verified. Selected
state, screen, bookmark and transcript comparisons use explicitly supplied native
input-return and idle-RNG observations; they do not establish complete live
scheduling equivalence.
