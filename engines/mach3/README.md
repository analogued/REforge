# Mach 3 engine

The `mach3` crate reconstructs the DOS version of Mach 3 (1987, Loriciels) for
[REforge](../../README.md). For installation, controls and player options, see
the [player guide](../../docs/games/mach3.md). This README covers development
interfaces and diagnostic tools.

> [!IMPORTANT]
> This engine does not include original Mach 3 assets or data. You must provide
> the original files from a supported release. See the
> [player guide](../../docs/games/mach3.md) for details.

Live play includes steering, shooting, encounter progression, collision responses,
endings and retained-session restarts, with no gameplay loop cap. Samples,
procedural sound and foreground waits share one reconstructed PIT timeline.
Reforged scene movement is the interactive default; craft/shadow-only
interpolation and Original presentation remain selectable without changing native
execution or recording/replay output. Mountains retain their original artwork
and updates.
PC Speaker and Enhanced PCM output use that same execution. Validation includes
controlled later encounters and difficulty cases without establishing exhaustive
gameplay coverage; other platform versions and additional hardware modes remain
outside the supported set.

## Technical references

- [DOS startup](docs/dos-startup.md): unpacking, address conventions, initialization
  and the original sample-disable argument.
- [CGA graphics](docs/cga-graphics.md): asset layouts, palette, composition and
  startup presentation.
- [DOS gameplay](docs/dos-gameplay.md): update order, encounter algorithms,
  timing, controls, session state and intentional compatibility corrections.
- [DOS sound assets](docs/sound-assets.md): sample encoding, sound programs,
  PIT execution, rendering policies and offline WAV export.
- [Gameplay recordings](docs/gameplay-recordings.md): replay API, recording
  formats, synchronization and supported boundaries.
- [Input tracing](docs/input-tracing.md) and [timing diagnostics](docs/timing-tracing.md):
  optional development traces and their clock domains.

## Crate interfaces

`assets`, `frame` and `scene` decode graphics, draw packed frames and compose
the fixed initial scene. `executable` expands the external program without
executing DOS code. `gameplay::Gameplay` owns reconstructed state,
`session_replay` consumes recorded inputs, and `platform::run_live` runs the
normal PIT-driven frontend. `platform::run_preview` provides silent nominal
stepping; its scheduler is separate from live execution and the host-audio
adapter. The library exposes headless continuations independently of host
audio output.

With the `sdl` feature, `mach3::cli::run(args)` accepts `OsString` arguments
excluding the executable name, allowing REforge to dispatch the same interface.
See [CONTRIBUTING.md](CONTRIBUTING.md) for feature combinations and validation.
Run the commands below from the combined repository root. Once dependencies are
cached, they also accept `--offline`; configured Cargo output directories apply.

## Rust renderer

The frame, opening, replay and sound exporter binaries require `dev-tools`.
They use authored algorithms and external game resources, not captured frames or
emulator state. Supplied filenames are case-insensitive, and output files or
directories must be new.

```sh
cargo run --locked -p mach3 --no-default-features --features dev-tools --bin mach3-frame -- /path/to/assets /path/to/first-frame.ppm
cargo run --locked -p mach3 --no-default-features --features dev-tools --bin mach3-frame -- --format cga /path/to/assets /path/to/frame.bin
```

`mach3-frame` needs `DATA2` through `DATA5` and exports the fixed initial scene
as a 320×200 binary PPM or a 16-KiB CGA framebuffer with unused bank bytes zeroed.
Asset decoding and frame composition are also available through the library.
The animated simulation is separate from this initial-scene exporter.

## Windowed playback and previews

Windowed playback requires SDL3 development/runtime libraries. The `sdl` feature
is enabled by default; run the standalone `mach-3` executable with:

```sh
cargo run --locked -p mach3 --bin mach-3 -- --original /path/to/assets
```

Normal animation needs `MACH3.EXE` and `DATA1` through `DATAH`. Steering
and firing are enabled by default. Startup, ending and original F1/F2/F3
processing retain their separate input rules.

The windowed frontend uses a 640×200 texture for both colour and the original
monochrome interpretation. Original display presents both at 4:3 by default;
Square pixels and Fill window (stretch) can change those proportions. Shared
window sizing, fullscreen, renderer/backend selection, CRT treatment and host
audio settings are documented in [configuration](../../docs/configuration.md) and
the [shared SDL backend](../../crates/reforge_sdl/README.md). F10 is available during
startup, gameplay and diagnostic previews; closing Settings restores the
preceding host-pause state.

Mach 3-specific options include:

- `--motion reforged|enhanced|original`: temporarily select Reforged scene
  movement (default `reforged`), craft/shadow-only interpolation (`enhanced`), or
  native presentation (`original`). Existing saved values keep their meanings.
  The saved per-game Display setting applies when this argument is absent.
  F10 offers Scene movement and, for Original scene movement, Player craft movement.

- `--video cga|mono`: select the initial native colour or monochrome display.
  F3 can still switch modes during play.
- `--f3-mode original`: retain the original toggle on every active poll instead
  of the interactive default's one toggle per press. Recorded original inputs
  always use original polling. See [session controls](docs/dos-gameplay.md#session-and-gameplay-controls).
- `--audio-renderer speaker|enhanced-pcm`: choose host rendering without changing
  execution. Speaker is the built-in default; see
  [rendering policies](docs/sound-assets.md#enhanced-pcm-playback).
- `--digitized-samples on|off`: temporarily override the saved game
  preference. Off skips native sample calls and their timing/input effects;
  host mute and category levels leave those calls intact. See
  [sample-disable behavior](docs/dos-startup.md#original-sample-disable-argument).

`--step` and `--static` require `dev-tools` in addition to the default `sdl`
feature, for either the standalone executable or REforge. `--step` advances
one body or title step per fresh Enter press; `--static` retains the initial
scene. These previews use a fixed nominal period, are silent and need only
`MACH3.EXE` and `DATA1` through `DATA6`. Fresh Space presses advance startup;
Space remains Fire during manual gameplay. They do not reproduce the live PIT
scheduler's elapsed times.

`--trace-input PATH` and `--trace-timing PATH` have the same feature requirements.
Their respective technical references describe formats, output preservation and
limitations.

## Headless gameplay export

```sh
cargo run --locked -p mach3 --no-default-features --features dev-tools --bin mach3-opening -- /path/to/assets /path/to/new-output-directory
```

This exports the neutral-input opening as `000.cga` through `836.cga`, plus
state, player, weapon, object and drawing-order TSV tables. `--steps 0..836`
selects a shorter prefix. `--subframes` also writes each body's three passes as
`NNN-1.cga` through `NNN-3.cga`; numbered boundary frames and tables retain their
whole-body meaning. `--ending` adds the first post-game title pair, its state and
drawing order, and the selected returned intro image.

For deterministic steering, add `--inputs /path/to/inputs.tsv`. Its header is
`body`, `update`, `mask`, separated by tabs. Each row changes the held state at
body 1–836 and player update 1–3. Masks combine Up=1, Down=2, Left=4, Right=8 and
Fire=16; zero releases all actions. Rows must be strictly ordered, and the state
persists until the next row. The default controlled export is 90 bodies;
`--steps 0..836` permits a longer bounded run. Input export excludes `--ending`.

For replay of original recordings, use `mach3-replay` as described in
[gameplay recordings](docs/gameplay-recordings.md). For paired Speaker/Enhanced
PCM WAVs, use `mach3-sound` as described in
[sound diagnostics](docs/sound-assets.md#bounded-diagnostic).

## License

Authored code and documentation use REforge's
[`GPL-3.0-or-later` license](../../LICENSE). Third-party attribution and retained
terms are in [THIRD_PARTY.md](../../THIRD_PARTY.md). Original game data is not
included or licensed by this repository.

## Visual motion presenter

`motion` consumes immutable native observations. `gameplay::raster` retains the
actual restore/draw transactions. Reforged mode interpolates scene movement; legacy
Enhanced mode composes complete craft/shadow sprites while preserving retained
world fragments, CGA mask operations and foreground order. Original uses native
publications. Presentation neither mutates projection caches nor advances
simulation. Mountains keep their original artwork and updates.

The scene presenter separates intrinsic object coordinates from one shared
camera. Player shots, straight-moving enemies, terrain and gates use their
reconstructed movement rates; other enemies use observed full-body rates, and
the homing projectile follows its observed target, accounting for camera
displacement before clamping its homing correction. Camera compensation follows
that camera's bounded trajectory independently of the projectile's depth and
genuine homing forecasts. Stationary terrain effects stop forecasting forward.
Source timestamps determine cadence, while receipt delay consumes the prediction
allowance. Lifetimes and projected-lifetime tags
prevent joining reused object slots or moving an old cached image as a new actor.

Reforged mode composes complete original sprites and procedural gate geometry in
native paint order, retaining two-bit CGA mask operations and discrete artwork.
Moving terrain and gates stop their forecasts at the native retirement plane;
deactivated gates are removed at the observed event rather than retaining a
frozen cached drawing. A coherent first pass is required after a scene reset.
Damage artwork retains native craft placement while the world continues to
move; actual native waits suspend the applicable visual history.

The craft forecast uses accepted steering and one reconstructed movement step, with
measured pass/body cadence. Target motion is limited to 50 ms, 12 projected
pixels and the native movement limits independently per axis. An analytical
20 ms first-order follower integrates the target and its stopping times.
These are target limits, not a guarantee of a 12-pixel displayed error or fixed
input latency. Banking changes artwork discretely around continuous anchors.

Native pause freezes the visual pose. Damage, blocking samples, transitions,
endings and new games suspend/reset the relevant history; host pause freezes its
clock. Delayed catch-up observations are consumed in order, with obsolete output
omitted rather than traversed rapidly. Interpolated redraws use the shared VSync and
frame limiter. Original keeps native publications; headless exports/replay remain
native. A long host stall can require a visible catch-up correction.

`--trace-motion PATH` is an optional `dev-tools` diagnostic. Creation/write/flush
errors warn and disable logging without affecting the presenter. It retains a
bounded transaction/observation trace, including the selected motion mode and
scene-adjusted craft anchors, not an image series. The historical
`--motion-experiment PATH` development alias selects Enhanced and requests that
trace. Diagnostic paths must be new; no logging is required by normal builds.
