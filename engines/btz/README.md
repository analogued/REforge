# BTZ engine

The `btz` crate implements **BTZ (Better Than Zork)**, a text-adventure
interpreter, for [REforge](../../README.md). It executes original game wordcode
and data through reconstructed VM, parser, text, keyboard, timing and native
services. Story rules remain in the supplied game data.

Mindwheel and Essex for DOS are supported. Brimstone and Breakers will be added
as their game data and interpreter revisions are implemented and validated.
The games share the VM and player hosts, with identified interpreter differences
selected through immutable definitions.

> [!IMPORTANT]
> This engine does not include original BTZ game assets or data. You must
> provide the original files from a supported release. See
> [External inputs](#external-inputs) for requirements.

The windowed and terminal frontends support commands, bookmarks, transcripts,
restart and exit. Supported games use REforge's game detection and graphical
launcher; see the [BTZ player guide](../../docs/games/btz.md) for
controls and launch options.

> [!NOTE]
> BTZ support is experimental. One identified DOS release each of Mindwheel
> and Essex is supported. Complete winning playthroughs have not been verified.
> Other BTZ games, platform versions and interpreter revisions are not yet
> supported. See [Reference and compatibility](#reference-and-compatibility)
> for the current limits.

## External inputs

For the supported DOS games, supply the corresponding `ADISK.OBJ`, `GO.EXE` and
`SIDE.TXT` files. `GO.EXE` supplies compression/classification tables, native
messages and an RNG adjustment; its x86 code is not executed. `SIDE.TXT` supplies
logical-disk mappings for services such as restart and bookmark restore.

No original game files, extracted vocabulary, story text or interpreter tables
are bundled. The [format reference](docs/mindwheel-dos-format.md) identifies
the supported Mindwheel release and its layout. Its format and runtime rules
are documented separately from engine-wide interfaces.

Build, test and diagnostic commands are in [CONTRIBUTING.md](CONTRIBUTING.md).

## Implemented architecture

The core library has no default third-party dependencies and runs without SDL.
The `frontend` feature adds player loading and terminal support; `sdl` adds
windowed presentation. Mutable session state is separate from the original
data. These are the current implementation boundaries:

| Module | Responsibility |
| --- | --- |
| [`image`](src/image.rs), [`media`](src/media.rs) | Checked store loading, original data access and replacement text media |
| [`interpreter`](src/interpreter.rs) | Immutable behavior definitions, named services and separate executable resource layouts |
| [`decode`](src/decode.rs), [`vm`](src/vm.rs) | Eight operand forms, addressing, 16-bit operations, properties, stacks and suspended native operations |
| [`parser`](src/parser/mod.rs) | Lexical processing, grammar matching, alternatives and remembered objects |
| [`input`](src/input.rs), [`text`](src/text.rs) | Keyboard editor, text decoding/formatting and logical 80×25 character screen |
| [`random`](src/random.rs), [`timing`](src/timing.rs) | DOS RNG arithmetic, explicit idle advancement and BIOS-tick read deadlines |
| [`bookmark`](src/bookmark.rs), [`services`](src/services.rs) | Native bookmark codec, transcript buffering, disk mappings and host request/reply types |
| [`session`](src/session.rs) | VM plus native-service lifecycle, parser, editor, text and host interactions |
| [`player`](src/player.rs), [`cli`](src/cli.rs) | Shared player loading, file operations, terminal and normal application entry point |
| [`windowed`](src/windowed.rs), [`display`](src/display/mod.rs) | SDL event loop, windowed prompts and character-screen rasterization |
| [`launcher`](src/launcher.rs) | Presentation choices and title-screen previews generated from the supplied data |
| [`sound`](src/sound.rs) | BIOS bell presentation through the shared speaker, synthesis and SDL audio facilities |
| [`btz-core`](src/bin/core.rs) | Raw VM trace, snapshots and key replays behind `dev-tools` |

The ASCII bitmap font comes from the shared, SDL-independent
[`reforge_graphics`](../../crates/reforge_graphics/README.md) crate. Character-screen
layout, colours and border/block glyphs remain in BTZ's display module.

Launcher previews retain the original title and credits, with the loading label,
continue prompt and input cursor removed from the preview copy. They use the
same text rasterizer as gameplay, without advancing the story or opening a host.

[Interpreter definitions](docs/interpreter-definitions.md) describe the shared
execution model and the verified differences between DOS implementations.

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
- [Interpreter definitions](docs/interpreter-definitions.md): shared behavior,
  Essex extensions, resource layouts and compatibility boundaries.
- [DOS runtime](docs/mindwheel-dos-runtime.md): parser, text, keyboard, native
  services, timing/RNG and host behavior.

The observed Mindwheel and Essex native-service tables have implementation paths. That does not imply
complete behavioral equivalence: pause/typeahead and newline polling have
remaining edge cases described in the runtime reference.

Both supported DOS input routines read an uninitialized stack word.
The player hosts use an explicit default-zero adapter rather than modeling the
DOS memory/stack environment. The conversion and its use differ between the games: Mindwheel uses it in a
discarded post-input draw; Essex also compares it with a story timeout value.
Later gameplay draws retain their own bounds.
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
