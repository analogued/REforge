# REforge

Rust reconstructions of DOS games for modern desktop systems, currently
[Alley Cat](engines/alleycat/README.md) and [Mach 3](engines/mach3/README.md).
Each game requires its original files, supplied separately. REforge reads their
assets and tables and implements the reconstructed behavior natively.
The games are still in development. Mach 3's normal animated viewer includes
PC speaker audio; Alley Cat audio remains unimplemented.

## Build and run

Install Rust 1.85 or newer, Cargo and SDL3 3.4 or newer development/runtime libraries.
One Cargo workspace and lockfile cover the application and both engine crates.

```sh
cargo build --locked --release -p reforge
cargo run --locked --release -p reforge -- --list
cargo run --locked --release -p reforge -- --original /path/to/CAT.EXE
cargo run --locked --release -p reforge -- --controls --original /path/to/mach3-files
```

The built application is `target/release/reforge`, unless `CARGO_TARGET_DIR`
selects another output path. Run `reforge --original <path> --help` for game-specific options.
Both games default to `--scale auto`, opening a normal window at the largest
integer multiple of the aspect-corrected 320×240 display area that fits the
compositor's maximum client area. Auto briefly measures that area through
maximization, accounting for desktop panels and window decorations, then restores
a compact window. If maximization is unavailable, auto falls back to scale 4.
`--scale 1..16` requests a fixed drawable pixel multiplier; `--scale 4` requests
1280×960 pixels. SDL retains integer presentation and adds black margins when
later user resizing leaves extra space.

Presentation defaults to sharp nearest-neighbour pixels. Add
`--presentation hyllian` for the shared Hyllian CRT shader, or
`--presentation pixels` to select the default explicitly. The initial CRT
setting is the same for both engines; colour profiles and adjustable monitor
presets are not yet exposed. GPU/shader setup failures warn and retain plain
pixels, including with `SDL_RENDER_DRIVER=software`. See the
[shared presentation documentation](crates/reforge_sdl/README.md) for backend
requirements and the bundled shader's attribution.
Ctrl+Q quits either game, and Pause/Break toggles pause during gameplay.
Each game retains its additional bindings.
See each game's README for supported input versions, controls and current limits.

`--original` accepts a game directory or a required original file. For Alley Cat,
it locates `CAT.EXE`; for Mach 3, detection checks `MACH3.EXE` and `DATA1` through
`DATA6` in the same directory. Normal animated Mach 3 also loads sound resources
`DATA7` through `DATAH`; supply the complete original directory. Step, static and
custom-period previews remain silent. Detection checks case-insensitive DOS
filenames, then
exact file sizes, then SHA-256 hashes. The catalog currently contains one verified
reference per game; its reference IDs do not claim a historical version number.
Additional releases can have separate file records and select the same engine.

A directory matching both games is ambiguous; select a required original file
inside it instead. Missing, incomplete, unknown or mismatched data stops startup.
Unrelated extra files are allowed. Renamed required files are not detected.

Unknown options, options for another game, unavailable development options and
invalid/missing values produce warnings and are ignored. A bad repeated value
retains the previous valid value or engine default. Recognized options consume
only their own values; a missing value does not swallow the next option. Supported
options can appear before or after `--original`, including `--option=value`.
Conflicting modes produce warnings and are reduced to a valid configuration.
Original-data and non-audio runtime I/O failures remain errors. Engine-specific standalone
diagnostic binaries keep their own stricter interfaces.

Use `--features dev-tools` to include Alley Cat's development scene selector,
fixed-seed controls, actor/damage switches and diagnostics. Optimization is
independent: a release build can include these tools, while a normal build omits
them. Mach 3's `--trace-input` and `--trace-timing` also require `dev-tools`.
Its package provides separate frame, opening, replay and sound exporters, all
requiring `dev-tools`.

```sh
cargo run --locked --release -p reforge --features dev-tools -- \
  --original /path/to/CAT.EXE --scene cheese --disable-damage all
```

`--no-default-features` omits window support. Alley Cat headless diagnostics also
require `dev-tools`; Mach 3's package provides separate exporters for headless
work under the same feature. For example:
`cargo run --locked -p mach3 --features dev-tools --bin mach3-replay -- --help`.

## Source organization

- `app/`: the `reforge` application, release detection and option catalog.
- `engines/alleycat/`: the `alleycat` engine library and legacy game binary.
- `engines/mach3/`: the `mach3` engine library and its viewer/export/replay binaries.
- `crates/reforge_sdl/`: shared SDL3 windows, events, frame presentation and
  queued audio output.
- `crates/reforge_audio/`: shared deterministic PIT and PC speaker devices,
  with signal rendering independent of host playback.

Game-specific gameplay, asset formats, timing and randomness stay in their own
engine crates. Engine IDs, Cargo packages and Rust crate names use `alleycat`
and `mach3`; game catalog IDs remain `alley-cat` and `mach-3`. An engine can
support multiple games when their implementation is shared. Both engines use
the shared SDL3 backend; their pixel decoding and palettes remain engine-specific.
See [CONTRIBUTING.md](CONTRIBUTING.md) for style, commits and validation.

## License

The repository-wide project license has not been selected; no project license
is granted yet. Existing third-party terms and attribution remain in
[Alley Cat notices](engines/alleycat/THIRD_PARTY.md),
[Mach 3 notices](engines/mach3/docs/dependencies.md), and
[shared SDL3 notices](crates/reforge_sdl/docs/dependencies.md), with their license texts.
Original game data is not included or licensed by this repository.
