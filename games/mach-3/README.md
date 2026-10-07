# Mach 3

Reverse-engineering project for the **DOS version of Mach 3**, released in 1987
by Loriciels. [MobyGames entry](https://www.mobygames.com/game/249/mach-3/).
Status: Rust gameplay recreation with steering, shooting, encounter
progression, collision responses, and retained-session restarts.
Live gameplay has no loop cap. Audio remains deferred; later gameplay paths
still need broader comparisons with the original.

The main implementation language is **Rust**. Supporting tools and investigation
scripts may use other languages. Other platform versions are outside the initial
scope; broader hardware support remains to be established.

See [DOS startup](docs/dos-startup.md) for the verified entry path, expanded-image
address conventions, initial graphics setup, and file access.
See [CGA graphics](docs/cga-graphics.md) for screen layouts, gameplay composition,
and decoder usage.
See [DOS opening gameplay](docs/dos-gameplay.md) for update order, encounter
scripts, and repeatability.
See [gameplay recordings](docs/gameplay-recordings.md) to record a manually played
original game and compare its rendered pixels and modeled state with a Rust replay.

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
The separate gameplay simulation implements live play and bounded frame exports;
audio remains unimplemented.
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

Supply the game resource directory, including `MACH3.EXE` and `DATA1` through
`DATA6`. The executable supplies externally decoded encounter scripts and
resource tables; the recreation does not execute DOS code. Fresh Space presses
advance the splash and intro. After game over, the title pair completes before
Space returns to the selected intro; another fresh press starts the next game
with retained random state and best score.

Use `--controls` to steer with arrows or S/X/B/N and fire with Space or keypad 5.
Live gameplay continues until game over, including when controls are disabled.
Each body displays three passes, with player input sampled separately at each
pass. The default pace is approximately 36.41
passes per second; `--period-ms 1..5000` overrides the whole-body interval.
Original CPU-dependent pacing and audio are not reproduced.

F1 pauses: release it, then press and release another gameplay key to resume.
Holding F1 at the pause decision ends the game after the current body completes.
F2 toggles the retained sound preference; synthesis is deferred. F3 switches
between color and the original 640-by-200 one-bit interpretation of the same
video bytes. Held function keys follow the original polling behavior and can
repeat their action. Escape or window close exits the viewer.

Use `--step` for one body or title step per fresh **Enter** press after entering
gameplay, or `--static` to retain the initial scene. Space remains Fire during
manual gameplay. Losing focus releases held controls.
The default window is 960 by 720 (3x). `--scale 1..16` sets its initial scale;
scale 8 gives 2560 by 1920:

```sh
cargo run --features viewer --bin mach3-viewer -- --scale 2 /path/to/assets
```

The 320-by-200 CGA framebuffer is displayed at 4:3, with pixels 20% taller
than wide. Window dimensions use 320-by-240 display units multiplied by the
selected scale. Resizing uses nearest-neighbor sampling with centered black
borders and preserves 4:3. Source rows can occupy different numbers of screen
rows because of the pixel-aspect correction. See [dependency notices](docs/dependencies.md).
After Cargo has fetched the locked dependencies, these commands can use `--offline`.

For reproducible frame and state export without SDL3:

```sh
cargo run --bin mach3-opening -- /path/to/assets /path/to/new-output-directory
```

This writes frames `000.cga` through `836.cga` plus state, player, weapon, object, and
drawing-order TSV tables. Add `--subframes` to also write each body's three
displayed passes as `NNN-1.cga` through `NNN-3.cga`. The numbered boundary frames
and TSV tables retain their whole-body meaning. Add `--ending` to export the first
post-game title pair, its state and drawing order, and the selected return intro image.
`--steps 0..836` exports a shorter prefix. The directory must be new. Simulation
uses the decoded assets and interpreted update rules; it does not load captured
frames, emulator state, or a replay.

For a deterministic steering replay, add `--inputs /path/to/inputs.tsv`.
The default becomes 90 bodies; explicit `--steps 0..836` permits longer bounded investigations.
The TSV header is `body`, `update`, `mask`, separated by tabs. Each row changes
the held state at body 1–836 and player update 1–3. Masks combine Up=1, Down=2,
Left=4, Right=8, Fire=16; zero releases all actions. Rows must be strictly ordered,
and the state persists until the next row. Input export excludes `--ending`.

See [CONTRIBUTING.md](CONTRIBUTING.md) for formatting and commit conventions.

## License

The project's license has not been selected; none is granted by this scaffold.
Once chosen, include the full text in `LICENSE`, identify and link it here, and
add short copyright and SPDX license headers to authored code files.
