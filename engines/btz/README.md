# BTZ interpreter

Experimental REforge engine for the investigated Mindwheel DOS interpreter.
It executes the story's wordcode and data through reconstructed VM, parser,
text, keyboard, timing and native-service algorithms. Story rules remain in
supplied game data.

The development terminal frontend supports normal commands, bookmarks,
transcripts, restart and exit. A complete winning playthrough has not been
verified. Other BTZ games and interpreter revisions remain unvalidated; game
detection and SDL application presentation are not integrated for this engine.

## External inputs

Supply the corresponding `ADISK.OBJ` and `GO.EXE` files from Mindwheel DOS.
The executable supplies compression/classification tables, native messages and
an RNG adjustment; its x86 code is not executed. `SIDE.TXT` supplies logical-disk
mappings for services such as restart and bookmark restore.

No original game files, extracted vocabulary, story text or interpreter tables
are bundled. The [format reference](docs/mindwheel-dos-format.md) identifies
the investigated inputs and their layout; this is not an automatic revision
selector.

For the experimental frontend, see the [Mindwheel guide](../../docs/games/mindwheel.md).
Build, test and diagnostic commands are in [CONTRIBUTING.md](CONTRIBUTING.md).

## Implemented architecture

The library has no SDL or default third-party dependencies. Mutable session
state is separate from the original data. These are the current implementation
boundaries:

| Module | Responsibility |
| --- | --- |
| [`image`](src/image.rs), [`media`](src/media.rs) | Checked store loading, original data access and replacement text media |
| [`decode`](src/decode.rs), [`vm`](src/vm.rs) | Eight operand forms, addressing, 16-bit operations, properties, stacks and suspended native operations |
| [`parser`](src/parser/mod.rs) | Lexical processing, grammar matching, alternatives and remembered objects |
| [`input`](src/input.rs), [`text`](src/text.rs) | Keyboard editor, text decoding/formatting and logical 80×25 character screen |
| [`random`](src/random.rs), [`timing`](src/timing.rs) | DOS RNG arithmetic, explicit idle advancement and BIOS-tick read deadlines |
| [`bookmark`](src/bookmark.rs), [`services`](src/services.rs) | Native bookmark codec, transcript buffering, disk mappings and host request/reply types |
| [`session`](src/session.rs) | VM plus native-service lifecycle, parser, editor, text and host interactions |
| [`btz-core`](src/bin/core.rs) | Development clock, Unix terminal and filesystem adapters, behind the `dev-tools` feature |

`Session` is the integrated execution interface. It accepts explicit clock and
keyboard input and yields events or `HostRequest` operations. The host completes
filesystem/media requests with `HostReply`. `Vm` is the lower-level wordcode
interface; it does not independently implement the native services.

`new_with_keyboard` and `advance_with_keyboard` service nonblocking keyboard
sources at the implemented native poll sites. `new` and `advance` provide a
deterministic headless path without a host keyboard source. `VmState` restores
logical VM state at an instruction-fetch boundary; it excludes parser, text,
RNG and pending host operations. Game saves use the separate native bookmark
codec.

## Reference and compatibility

- [DOS format and VM](docs/mindwheel-dos-format.md): stores, address units,
  operand forms, instruction semantics, properties and persistence formats.
- [DOS runtime](docs/mindwheel-dos-runtime.md): parser, text, keyboard, native
  services, timing/RNG and host behavior.

All 14 observed native services have implementation paths. That does not imply
complete behavioral equivalence: pause/typeahead, newline polling and bell
feedback have remaining edge cases described in the runtime reference.

The investigated DOS input routine reads an uninitialized stack word. The
development host uses an explicit default-zero adapter rather than modeling the
DOS memory/stack environment. This affects the discarded post-input draw and
subsequent random sequence; later gameplay draws retain their own bounds.
Exact DOS polling schedules and random-sequence fidelity remain unverified.

Synthetic tests exercise the library and terminal host. Original-interpreter
comparisons cover selected states, screens, bookmark bytes and transcript output,
with observed input-return values and idle RNG counts supplied explicitly.
They do not establish a winning playthrough or compatibility with other releases.
Secondary text-media support has original-code analysis and synthetic coverage,
without an original secondary-media sample.

## License

BTZ's authored code and documentation use `GPL-3.0-or-later`; see
[LICENSE](../../LICENSE). Third-party dependencies retain the terms listed in
[THIRD_PARTY.md](../../THIRD_PARTY.md). Original game data is external and is not
licensed by this project.
