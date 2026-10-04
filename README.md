# Alley Cat

Game reverse-engineering project. Status: initial reconnaissance; there is no
playable reconstruction yet. The initial goal is an accurate recreation, with
enhancements considered afterward. See [project goals](docs/goals.md).

The target release, supporting libraries, and RE method remain open.

Target game: **Alley Cat**. A public recreation name has not been chosen. When
chosen, use it in this repository's documentation while keeping the target game
clearly identified.

## Implementation language

The authored game implementation will use Rust, initially targeting Linux on
modern desktop systems while keeping the platform layer portable.
This is an Alley Cat decision; other game projects choose their languages
independently. Supporting tools can use other languages where appropriate, such
as Python for analysis utilities and Java or Python for Ghidra scripts.

The compiler version, libraries, and RE method remain open. Retaining C-like
decompiler output as external reference evidence does not require a compiled
C++ implementation. Add authored C++ components when a concrete need justifies
their integration and maintenance.

## Repository structure

Starter directories are `src/`, `tools/`, `re/`, `tests/`, and `docs/`. Adapt them
to the project when its needs become clear. Game-specific tools belong here;
shared tools can have independent repositories elsewhere in the workspace.

## External inputs

Original executables/assets, raw analysis, emulator snapshots, and reference
captures live outside this repository. Use configured paths to access them.
In the DOSRE umbrella, the adjacent local `project.toml` describes those paths;
standalone use must not require that umbrella layout.

Build and development instructions will be added when those choices are made.
No implementation build system or RE workflow has been selected. The initial
[analysis tool](tools/README.md) supports read-only executable reconnaissance.

See [CONTRIBUTING.md](CONTRIBUTING.md) for formatting and commit conventions.

## License

The project's license has not been selected; none is granted by this scaffold.
Once chosen, include the full text in `LICENSE`, identify and link it here, and
add short copyright and SPDX license headers to authored code files.
