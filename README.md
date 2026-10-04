# Mach 3

Reverse-engineering project for the **DOS version of Mach 3**, released in 1987
by Loriciels. [MobyGames entry](https://www.mobygames.com/game/249/mach-3/).
Status: initial scaffold; no original game files have been imported or analyzed.

The exact DOS release, tools, implementation stack, and RE method remain
undecided. Other platform versions are outside the initial scope. Hardware
support must be verified against the selected DOS build.

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
In the DOSRE umbrella, the adjacent local `project.toml` describes those paths;
standalone use must not require that umbrella layout.

Build and development instructions will be added when those choices are made.
No build system or RE workflow is supplied by this scaffold.

See [CONTRIBUTING.md](CONTRIBUTING.md) for formatting and commit conventions.

## License

The project's license has not been selected; none is granted by this scaffold.
Once chosen, include the full text in `LICENSE`, identify and link it here, and
add short copyright and SPDX license headers to authored code files.
