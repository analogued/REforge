# Mach 3

The `mach3` engine crate for [REforge](../../README.md) recreates the **DOS version of Mach 3**,
released in 1987 by Loriciels. [MobyGames entry](https://www.mobygames.com/game/249/mach-3/).
Status: Rust gameplay recreation with steering, shooting, encounter
progression, collision responses, and retained-session restarts.
Live gameplay has no loop cap. Audio remains deferred; later gameplay paths
still need broader comparisons with the original.

The main implementation language is **Rust**. Supporting tools and investigation
scripts may use other languages. Other platform versions are outside the initial
scope; broader hardware support remains to be established.

See [DOS startup](docs/dos-startup.md) for the verified entry path, expanded-image
address conventions, initial graphics setup, and file access.
See [CGA graphics](docs/cga-graphics.md) for screen layouts and gameplay composition.
See [DOS opening gameplay](docs/dos-gameplay.md) for update order, encounter
scripts, and repeatability.
See [gameplay recordings](docs/gameplay-recordings.md) for the Rust replay interface,
recording format and synchronization rules.

## Engine crate structure

This crate lives at `engines/mach3/` in REforge. Its implementation is in `src/`,
Rust integration tests in `tests/`, and technical reference documentation in `docs/`.
Shared application facilities belong in the combined repository's `crates/`.
Keep `docs/` for maintained implementation and reference documentation. Internal
discussion, experiments, and planning notes live outside the source repository.

## External inputs

Original executables/assets, raw analysis, emulator snapshots, and reference
captures live outside this repository. Use configured paths to access them.
Input provenance, checksums, comparison reports, and experiment records belong
with the private evidence rather than in repository documentation.

## Rust renderer

Requires Rust 1.85 or later. Asset decoding and frame export use the standard
library; the optional windowed viewer uses SDL3. Run Cargo commands below from
the combined repository root; `-p mach3` selects this game package:

```sh
cargo run -p mach3 --bin mach3-frame -- /path/to/assets /path/to/first-frame.ppm
```

The renderer reads `DATA2`, `DATA3`, `DATA4`, and `DATA5` from the supplied
directory; names are case-insensitive. It decodes the graphics and composes the
verified initial scene into a 320-by-200 binary PPM. The output must be a new
file. For a 16-KiB CGA framebuffer with unused bank bytes zeroed:

```sh
cargo run -p mach3 --bin mach3-frame -- --format cga /path/to/assets /path/to/frame.bin
```

This implements the fixed initial draw state, masked sprites, and frame export.
The separate gameplay simulation implements live play and bounded frame exports;
audio remains unimplemented.
Asset decoding and frame composition are also available through the `mach3`
library. With the `viewer` feature, `mach3::cli::run(args)` accepts command-line
arguments as `OsString` values, excluding the executable name, so REforge can
dispatch the same viewer interface. See [CGA graphics](docs/cga-graphics.md) for
the supported formats.

## Windowed opening preview

The optional viewer requires SDL3 development and runtime libraries available
to the compiler and system loader:

```sh
cargo run -p mach3 --features viewer --bin mach3-viewer -- --original /path/to/assets
```

Supply the game resource directory with `--original`, including `MACH3.EXE` and
`DATA1` through `DATA6`. The executable supplies externally decoded encounter scripts and
resource tables; the recreation does not execute DOS code. Fresh Space presses
advance the splash and intro. After game over, the title pair completes before
Space returns to the selected intro; another fresh press starts the next game
with retained random state and best score.

Use `--controls` to steer with arrows or S/X/B/N and fire with Space or keypad 5.
It is a flag with no value; the data path belongs to `--original`.
Live gameplay continues until game over, including when controls are disabled.
Each body computes three render passes, with player input sampled separately
at each pass. The default pace is approximately 36.41
passes per second; `--period-ms 1..5000` overrides the whole-body interval.
Ordinary simulation catches up due render passes before presenting the latest
frame. It preserves original pass order and input-poll sites; slow presentation
does not limit it to one pass per display refresh. Catch-up batches are bounded
to keep processing host events, with remaining deadline debt retained. Blocking
transition frames and ending frames retain their separate visible waits.
Original CPU-dependent pacing and audio are not reproduced.

F1 pauses: release it, then press and release another gameplay key to resume.
Holding F1 at the pause decision ends the game after the current body completes.
F2 toggles the retained sound preference; synthesis is deferred. F3 switches
between color and the original 640-by-200 one-bit interpretation of the same
video bytes. Held function keys follow the original polling behavior and can
repeat their action. Ctrl+Q, Escape or window close exits the viewer.
Pause/Break toggles a separate modern host pause during gameplay, including
transitions and game-over animation. Repeats and Alt/Ctrl/Super combinations
are ignored; other keys and focus changes do not resume it. Paused time is
excluded from presentation and simulation deadlines, and held game controls
are cleared on each toggle. This does not feed an F1 event to the DOS key table;
the original F1 pause/end-game sequence remains available independently.
Host pause and focus loss preserve the native F1 phase and its pending waits.
Cleared controls count as released when that handshake next samples input;
a fresh press is required after clearing, so OS repeats cannot reactivate a
key still held across pause or focus loss.

Use `--step` for one body or title step per fresh **Enter** press after entering
gameplay, or `--static` to retain the initial scene. Space remains Fire during
manual gameplay. Losing focus releases held controls.
The default requested window size is 1280 by 960 (4x). `--scale 1..16` sets its initial scale;
scale 8 gives 2560 by 1920. The [shared SDL3 backend](../../crates/reforge_sdl/README.md)
lets SDL select an available renderer and requests vsync, continuing with a
warning if unsupported. Resizing preserves 4:3 at integer multiples of 320×240
with black margins. Nearest-neighbour sampling keeps colours discrete;
Mach 3 retains its 640×200 texture for both colour and monochrome output:

```sh
cargo run -p mach3 --features viewer --bin mach3-viewer -- --scale 2 --original /path/to/assets
```

The 320-by-200 CGA framebuffer is displayed at 4:3, with pixels 20% taller
than wide. Window dimensions use 320-by-240 display units multiplied by the
selected scale. Resizing uses nearest-neighbor sampling with centered black
borders and preserves 4:3. Source rows can occupy different numbers of screen
rows because of the pixel-aspect correction. See [dependency notices](docs/dependencies.md).
After Cargo has fetched the locked dependencies, these commands can use `--offline`.

For reproducible frame and state export without SDL3:

```sh
cargo run -p mach3 --bin mach3-opening -- /path/to/assets /path/to/new-output-directory
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

Authored code follows [REforge's repository license decision](../../README.md#license),
which remains undecided. Preserve the complete third-party notices in
the [dependency notices](docs/dependencies.md).
