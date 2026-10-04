# Mach 3

Reverse-engineering project for the **DOS version of Mach 3**, released in 1987
by Loriciels. [MobyGames entry](https://www.mobygames.com/game/249/mach-3/).
Status: Rust asset decoding and initial CGA frame composition implemented;
reverse engineering of gameplay continues.

The main implementation language is **Rust**. Supporting tools and investigation
scripts may use other languages. Other platform versions are outside the initial
scope; broader hardware support remains to be established.

See [DOS startup](docs/dos-startup.md) for the verified entry path, expanded-image
address conventions, initial graphics setup, and file access.
See [CGA graphics](docs/cga-graphics.md) for screen layouts, gameplay composition,
and decoder usage.

Target game: **Mach 3**. A public recreation name has not been chosen. When
chosen, use it in this repository's documentation while keeping the target game
clearly identified.

## Repository structure

Starter directories are `src/`, `tools/`, `re/`, `tests/`, and `docs/`. Adapt them
to the project when its needs become clear. Game-specific tools belong here;
shared tools can have independent repositories elsewhere in the workspace.
Keep `docs/` for maintained implementation and reference documentation. Internal
discussion, experiments, and planning notes live outside the source repository.

## External inputs

Original executables/assets, raw analysis, emulator snapshots, and reference
captures live outside this repository. Use configured paths to access them.
Input provenance, checksums, comparison reports, and experiment records belong
with the private evidence rather than in repository documentation.

## Rust renderer

Requires Rust 1.85 or later. The crate uses only the standard library.
From the repository root:

```sh
cargo run --offline --bin mach3-frame -- /path/to/assets /path/to/first-frame.ppm
```

The renderer reads `DATA2`, `DATA3`, `DATA4`, and `DATA5` from the supplied
directory; names are case-insensitive. It decodes the graphics and composes the
verified initial scene into a 320-by-200 binary PPM. The output must be a new
file. For a 16-KiB CGA framebuffer with unused bank bytes zeroed:

```sh
cargo run --offline --bin mach3-frame -- --format cga /path/to/assets /path/to/frame.bin
```

This implements the fixed initial draw state, masked sprites, and frame export.
Gameplay updates, window presentation, input, and audio remain unimplemented.
Asset decoding and frame composition are also available through the `mach3`
library. See [CGA graphics](docs/cga-graphics.md) for the supported formats.

The separate CGA decoder runs with Python 3.11 or later and has no external
dependencies.

See [CONTRIBUTING.md](CONTRIBUTING.md) for formatting and commit conventions.

## License

The project's license has not been selected; none is granted by this scaffold.
Once chosen, include the full text in `LICENSE`, identify and link it here, and
add short copyright and SPDX license headers to authored code files.
