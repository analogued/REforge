# Mach 3

Reverse-engineering project for the **DOS version of Mach 3**, released in 1987
by Loriciels. [MobyGames entry](https://www.mobygames.com/game/249/mach-3/).
Status: Rust graphics decoding and a windowed, no-input opening preview,
including terrain changes, four enemy waves, collision responses, and life recovery.
Later gameplay remains under investigation.

The main implementation language is **Rust**. Supporting tools and investigation
scripts may use other languages. Other platform versions are outside the initial
scope; broader hardware support remains to be established.

See [DOS startup](docs/dos-startup.md) for the verified entry path, expanded-image
address conventions, initial graphics setup, and file access.
See [CGA graphics](docs/cga-graphics.md) for screen layouts, gameplay composition,
and decoder usage.
See [DOS opening gameplay](docs/dos-gameplay.md) for update order, encounter
scripts, and repeatability.

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

Requires Rust 1.85 or later. Asset decoding and frame export use the standard
library; the optional windowed viewer uses SDL3. From the repository root:

```sh
cargo run --bin mach3-frame -- /path/to/assets /path/to/first-frame.ppm
```

The renderer reads `DATA2`, `DATA3`, `DATA4`, and `DATA5` from the supplied
directory; names are case-insensitive. It decodes the graphics and composes the
verified initial scene into a 320-by-200 binary PPM. The output must be a new
file. For a 16-KiB CGA framebuffer with unused bank bytes zeroed:

```sh
cargo run --bin mach3-frame -- --format cga /path/to/assets /path/to/frame.bin
```

This implements the fixed initial draw state, masked sprites, and frame export.
The separate opening simulation implements bounded gameplay updates; audio
remains unimplemented.
Asset decoding and frame composition are also available through the `mach3`
library. See [CGA graphics](docs/cga-graphics.md) for the supported formats.

The separate CGA decoder runs with Python 3.11 or later and has no external
dependencies.

## Windowed opening preview

The optional viewer requires SDL3 development and runtime libraries available
to the compiler and system loader:

```sh
cargo run --features viewer --bin mach3-viewer -- /path/to/assets
```

It reads `DATA1` and `DATA6` as the splash and intro. Fresh Space presses advance
to the opening, which then runs 360 no-input game-loop bodies using graphics
from `DATA2` through `DATA5`. Terrain moves, the HUD updates, and the first
four enemy waves and their seeking projectiles appear. The second enemy steers
into the player, triggering hit and explosion animations. Life loss restores
fuel and resumes flight. The third enemy orbits before seeking the player;
the fourth keeps its lateral orbit and fixed height. Ground effects can also
damage the craft.
The preview holds its final frame at the current verification boundary.
Escape or window close exits.

Use `--step` to advance one body per fresh Space press after entering gameplay,
or `--static` to retain the initial scene. Held or repeated Space does not skip
manual steps. The default animation pace is approximately 12.14 bodies per
second, derived from the game's timer logic; `--period-ms 1..5000` overrides
the interval. Original startup timing and exact DOS CPU-dependent pacing are
not reproduced. Keyboard movement/firing, later encounters, and audio remain
deferred.

The default window is 960 by 600 (3x). `--scale 1..6` sets its initial scale:

```sh
cargo run --features viewer --bin mach3-viewer -- --scale 2 /path/to/assets
```

Resizing keeps pixels sharp with integer nearest-neighbor scaling and centered
black borders. The viewer preserves the source's 320-by-200 proportions; CRT
pixel-aspect correction is not applied. See [dependency notices](docs/dependencies.md).
After Cargo has fetched the locked dependencies, these commands can use `--offline`.

For reproducible frame and state export without SDL3:

```sh
cargo run --bin mach3-opening -- /path/to/assets /path/to/new-output-directory
```

This writes frames `000.cga` through `360.cga` plus state, player, and object
TSV tables.
`--steps 0..360` exports a shorter prefix. The directory must be new. Simulation
uses the decoded assets and interpreted update rules; it does not load captured
frames, emulator state, or a replay.

See [CONTRIBUTING.md](CONTRIBUTING.md) for formatting and commit conventions.

## License

The project's license has not been selected; none is granted by this scaffold.
Once chosen, include the full text in `LICENSE`, identify and link it here, and
add short copyright and SPDX license headers to authored code files.
