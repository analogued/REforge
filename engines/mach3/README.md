# Mach 3

The `mach3` engine crate for [REforge](../../README.md) recreates the **DOS version of Mach 3**,
released in 1987 by Loriciels. [MobyGames entry](https://www.mobygames.com/game/249/mach-3/).
Status: Rust gameplay recreation with steering, shooting, encounter
progression, collision responses, and retained-session restarts.
Live gameplay has no loop cap. The normal animated viewer runs intro/pre-game
samples, procedural sound and gameplay waits on one reconstructed PIT timeline,
with PC speaker or enhanced PCM rendering and queued SDL3 playback. Headless
continuations and paired offline WAV export are also available. Step/static/custom-period previews
retain silent nominal stepping.
Later gameplay paths still need broader comparisons with the original.

The main implementation language is **Rust**. Supporting tools and investigation
scripts may use other languages. Other platform versions are outside the initial
scope; broader hardware support remains to be established.

See [DOS startup](docs/dos-startup.md) for the verified entry path, expanded-image
address conventions, initial graphics setup, and file access.
See [CGA graphics](docs/cga-graphics.md) for screen layouts and gameplay composition.
See [DOS sound assets](docs/sound-assets.md) for sample encoding, speaker
conversion, executable music sequencing, procedural tone/noise routines, and
the bounded sound diagnostic,
including shared PIT execution and the engine's interrupt/foreground boundary.
See [DOS gameplay](docs/dos-gameplay.md) for update order, encounter
scripts, and repeatability.
See [native timing diagnostics](docs/timing-tracing.md) for the optional development
trace of simulation execution, scheduling and presentation.

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

The frame, opening, replay and sound exporter binaries require `dev-tools`.
`mach3-viewer` requires `viewer`; either feature can be enabled independently.

```sh
cargo run -p mach3 --features dev-tools --bin mach3-frame -- /path/to/assets /path/to/first-frame.ppm
```

The renderer reads `DATA2`, `DATA3`, `DATA4`, and `DATA5` from the supplied
directory; names are case-insensitive. It decodes the graphics and composes the
verified initial scene into a 320-by-200 binary PPM. The output must be a new
file. For a 16-KiB CGA framebuffer with unused bank bytes zeroed:

```sh
cargo run -p mach3 --features dev-tools --bin mach3-frame -- --format cga /path/to/assets /path/to/frame.bin
```

This implements the fixed initial draw state, masked sprites, and frame export.
The separate gameplay simulation implements live play and bounded frame exports.
Frame export is silent.
Asset decoding and frame composition are also available through the `mach3`
library. With the `viewer` feature, `mach3::cli::run(args)` accepts command-line
arguments as `OsString` values, excluding the executable name, so REforge can
dispatch the same viewer interface. See [CGA graphics](docs/cga-graphics.md) for
the supported formats.

The library's `gameplay::Gameplay` owns reconstructed game state and updates;
`executable` loads and expands the DOS executable, and `session_replay` consumes
recorded inputs. The private `viewer::realtime` module runs the normal PIT-driven
frontend, including input, lifecycle, video and audio presentation.

## Windowed playback and previews

The optional viewer requires SDL3 development and runtime libraries available
to the compiler and system loader:

```sh
cargo run -p mach3 --features viewer --bin mach3-viewer -- --original /path/to/assets
```

Supply the game resource directory with `--original`, including `MACH3.EXE` and
`DATA1` through `DATAH` for normal animated playback. The executable supplies
decoded encounter scripts, sound calls and resource tables; the recreation does
not execute DOS code. The initial splash advances automatically after the
original DOS-clock wait (about 1.98 seconds with the deterministic starting
phase). A fresh Space press skips that wait as a modern convenience; release
Space before pressing it again in the intro. During
intro audio, Space takes effect after the current sample, its checked group and
the unconditional four-call tail. A zero-result traversal repeats after 500
timer waits with ordinary input polls; it does not automatically start gameplay.
The selected intro image stays visible until return. The first game image is
then drawn before its pre-game sample runs. After game over, the first title
pair completes before Space or another nonzero gameplay input returns to the
selected intro, with retained random state and best score.

Use `--controls` to steer with arrows or S/X/B/N and fire with Space or keypad 5.
It is a flag with no value; the data path belongs to `--original`.
Live gameplay continues until game over, including when controls are disabled.
Each body computes three render passes, with player input sampled separately
at each pass. Normal playback advances the installed timer and keyboard handlers
on the same PIT clock as sound. Pass one adds a timer wait between composition
and publication when the selected background layer remains unchanged; a
layer-change body skips it. Sample playback suspends the guest foreground while
PIT and temporary keyboard handlers continue. See
[gameplay timing](docs/dos-gameplay.md#timer-pacing) for update and display order.
Ordinary simulation catches up due updates and publication events before
presenting the latest published frame. It preserves original pass order and
input-poll sites; slow presentation
does not limit it to one pass per display refresh. Catch-up batches are bounded
to keep processing host events, with remaining deadline debt retained. Ending
frames retain their timer waits. Each title first polls until its input mask is
zero; the mandatory pair skips animation polls but still processes F2/F3 at
title entry. Scene changes display seven expanding regions, each for about
1/60 second. This normalizes the original CPU-dependent retrace polling;
timer/audio and keyboard interrupts continue while gameplay is suspended.
The mandatory Game Over hold plays its guarded sample; the mandatory High Score
hold plays its sample only for a strictly improved best score.
Received gameplay key transitions are applied at their scheduled input polls,
including during catch-up. A press held across a poll remains visible even if
its release has already arrived; a tap completed between polls can still be
missed. Events delivered after an executed poll take effect prospectively.
See [input timing and tracing](docs/input-tracing.md) for clock conversion and
pause/reset boundaries.
Host audio receives synthesized 48 kHz mono samples. An unavailable device or
output failure warns and mutes playback while the same simulation continues.
Buffer recovery discards expired PCM, retaining device and gameplay phase.
Instruction execution cost and precise DOS elapsed time are not reproduced.

Use `--audio-renderer speaker|enhanced-pcm` to select host audio rendering for
normal animation. The default is `speaker`; `enhanced-pcm` renders each full-byte
sample value directly while retaining PC speaker synthesis for procedural
effects. Both use the same PIT simulation timeline, gameplay waits and input
polls. This option leaves step, static and custom-period previews silent.

F1 pauses: release it, then press and release another gameplay key to resume.
Holding F1 at the pause decision ends the game after the current body completes.
F2 toggles the retained sound preference while timer execution continues.
F3 switches between color and the original 640-by-200 one-bit interpretation of
the same video bytes. Interactive viewing defaults to `--f3-mode press`, which
toggles once per press; release F3 before switching again. This is an intentional
modern convenience. Use `--f3-mode original` to retain the original toggle at
every active control poll, including repeated toggles while F3 is held, for
physical-input fidelity comparisons. F1/F2 retain their original polling rules.
Original-game recording replays always keep original per-poll F3 behavior.
Ctrl+Q, Escape or window close exits the viewer.
Pause/Break toggles a separate modern host pause during intro, pre-game, gameplay
and game-over animation, freezing simulation and queued playback. Repeats and
Alt/Ctrl/Super combinations
are ignored; other keys and focus changes do not resume it. Paused time is
excluded from presentation and simulation deadlines, and held game controls
are cleared on each toggle. This does not feed an F1 event to the DOS key table;
the original F1 pause/end-game sequence remains available independently.
Host pause and focus loss preserve the native F1 phase and its pending waits.
Cleared controls count as released when that handshake next samples input;
a fresh press is required after clearing, so OS repeats cannot reactivate a
key still held across pause or focus loss.

Use `--step` for one body or title step per fresh **Enter** press after entering
gameplay, or `--static` to retain the initial scene. `--period-ms 1..5000` selects
a nominal four-unit body interval. These diagnostic previews are silent and need
only `MACH3.EXE` and `DATA1` through `DATA6`; fresh Space presses advance their
startup screens. Space remains Fire during manual gameplay. Losing focus releases
held controls.

Development builds can record input diagnostics with `--trace-input PATH`:

```sh
cargo run -p reforge --features dev-tools -- --original /path/to/assets --controls --trace-input /path/to/new-input-trace.tsv
```

The output must be a new file; existing files are never overwritten. The
option is available only with `dev-tools` and retains normal gameplay input
behavior. For the standalone viewer, enable `--features viewer,dev-tools`.
See [input tracing](docs/input-tracing.md) for the diagnostic format and limits.

The default `--scale auto` requests SDL's reported usable display area as an
ordinary window's client size, without subtracting window decorations. SDL may
report the full monitor area when usable bounds are unavailable; an actual query
error makes auto fall back to scale 4. `--scale 1..16` requests a fixed initial
drawable pixel multiplier; scale 4 gives 1280 by 960 pixels, and scale 8 gives
2560 by 1920. The [shared SDL3 backend](../../crates/reforge_sdl/README.md)
requests high pixel density and converts explicit pixel targets to window
coordinates using drawable density. Desktop constraints and compositor
transformations can still affect the result. It lets SDL select an available
renderer and requests vsync, continuing with a warning if unsupported.
SDL fits the image at integer multiples of 320×240 inside the actual granted
window, preserving 4:3 with black margins, including after resizing.
`--presentation pixels` is the default and uses nearest-neighbour sampling to
keep colours discrete. `--presentation hyllian` enables the shared Hyllian CRT
shader in animated, step and static playback. Shader unavailability warns and
falls back to pixels. This display effect preserves the game framebuffer,
palette rules, timing and frame exports.

Mach 3 retains its 640×200 texture for both colour and monochrome output:

```sh
cargo run -p mach3 --features viewer --bin mach3-viewer -- --scale 2 --original /path/to/assets
```

The 320-by-200 CGA framebuffer is displayed at 4:3, with pixels 20% taller
than wide. Explicit numeric drawable targets use 320-by-240 display units
multiplied by the selected scale. The default pixels presentation resizes with
nearest-neighbor sampling and
centered black borders and preserves 4:3. Source rows can occupy different numbers of screen
rows because of the pixel-aspect correction. See [dependency notices](docs/dependencies.md).
After Cargo has fetched the locked dependencies, these commands can use `--offline`.

For reproducible frame and state export without SDL3:

```sh
cargo run -p mach3 --features dev-tools --bin mach3-opening -- /path/to/assets /path/to/new-output-directory
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
