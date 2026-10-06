# Mach 3

Reverse-engineering project for the **DOS version of Mach 3**, released in 1987
by Loriciels. [MobyGames entry](https://www.mobygames.com/game/249/mach-3/).
Status: Rust graphics decoding and a windowed, no-input opening preview,
including terrain changes, nine enemy encounters, collision responses, life recovery,
and the post-game title sequence.
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
to the opening, which then runs 836 no-input game-loop bodies using graphics
from `DATA2` through `DATA5`. Terrain moves, the HUD updates, and the first
nine enemy encounters appear, with seeking projectiles. The fifth wave contains
two craft with separate shadows and a shared projectile. The second enemy steers
into the player, triggering hit and explosion animations. Life loss restores
fuel and resumes flight. The third enemy orbits before seeking the player;
the fourth keeps its lateral orbit and fixed height. Ground effects can also
damage the craft. A later section mixes terrain families and adds procedural
gates with their own projection and crossing rules. The sixth encounter follows
a scripted sweep; the seventh uses staggered sweep phases around a center that
follows the player. It then enters fuel-bearing terrain and starts a three-craft
seeking wave, followed by a three-craft wave that seeks at near depths.
After the final death, the frozen scene displays animated Game Over and High Score
presentations. The first pair completes before Space can return to the intro
path, whose image selection uses the retained random stream. This no-input run
selects the splash artwork. The viewer holds there; another game within that
stream remains unsupported.
Escape or window close exits.

Use `--step` to advance one body or post-game animation step per fresh Space
press after entering gameplay, or `--static` to retain the initial scene. Held or repeated Space does not skip
manual steps. The default animation pace is approximately 12.14 bodies per
second, derived from the game's timer logic; `--period-ms 1..5000` overrides
the interval. Original startup timing and exact DOS CPU-dependent pacing are
not reproduced. Firing, later encounters, and audio remain deferred.

Use `--controls` for a 90-body steering preview. Arrow keys move the craft;
release slows it down, and diagonal inputs combine both axes. Movement includes
camera changes, world displacement, banking, and the HUD indicators. This
controlled prefix holds its final frame; the full no-input mode still reaches
game over and the return intro. `--controls --step` samples held directions
when Space advances a body. Losing focus releases held controls.

The default window is 960 by 600 (3x). `--scale 1..16` sets its initial scale:

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

This writes frames `000.cga` through `836.cga` plus state, player, object, and
drawing-order TSV tables. Add `--ending` to export the first post-game title pair,
its state and drawing order, and the selected return intro image.
`--steps 0..836` exports a shorter prefix. The directory must be new. Simulation
uses the decoded assets and interpreted update rules; it does not load captured
frames, emulator state, or a replay.

For a deterministic steering replay, add `--inputs /path/to/inputs.tsv`.
The default becomes 90 bodies; `--steps 0..90` selects a shorter prefix.
The TSV header is `body`, `update`, `mask`, separated by tabs. Each row changes
the held state at body 1–90 and player update 1–3. Masks combine Up=1, Down=2,
Left=4, Right=8; zero releases all directions. Rows must be strictly ordered,
and the state persists until the next row. Steering export excludes `--ending`.

See [CONTRIBUTING.md](CONTRIBUTING.md) for formatting and commit conventions.

## License

The project's license has not been selected; none is granted by this scaffold.
Once chosen, include the full text in `LICENSE`, identify and link it here, and
add short copyright and SPDX license headers to authored code files.
