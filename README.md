# Alley Cat

Game reverse-engineering project. Status: scaffold only. Project goals, target
release, supporting tools and libraries, and RE method will be decided for this game.

Target game: **Alley Cat**. A public recreation name has not been chosen. When
chosen, use it in this repository's documentation while keeping the target game
clearly identified.

## Implementation language

The authored game implementation will use Rust and target modern desktop systems.
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
No build system or RE workflow is supplied by this scaffold.

See [CONTRIBUTING.md](CONTRIBUTING.md) for formatting and commit conventions.

## License

The project's license has not been selected; none is granted by this scaffold.
Once chosen, include the full text in `LICENSE`, identify and link it here, and
add short copyright and SPDX license headers to authored code files.
