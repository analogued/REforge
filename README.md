# REforge

Rust reconstructions of DOS games for modern desktop systems, currently
[Alley Cat](engines/alleycat/README.md) and [Mach 3](engines/mach3/README.md).
Each game requires its original files, supplied separately. REforge reads their
assets and tables and implements the reconstructed behavior natively.
The games are still in development; audio remains unimplemented.

## Build and run

Install Rust 1.85 or newer, Cargo and SDL3 development/runtime libraries.
One Cargo workspace and lockfile cover the application and both engine crates.

```sh
cargo build --locked --release -p reforge
cargo run --locked --release -p reforge -- --list
cargo run --locked --release -p reforge -- alley-cat --original /path/to/CAT.EXE
cargo run --locked --release -p reforge -- mach-3 --controls --original /path/to/mach3-files
```

The built application is `target/release/reforge`, unless `CARGO_TARGET_DIR`
selects another output path. Run `reforge <game> --help` for game-specific options.
See each game's README for supported input versions, controls and current limits.

Use `--features dev-tools` to include Alley Cat's development scene selector,
fixed-seed controls, actor/damage switches and diagnostics. Optimization is
independent: a release build can include these tools, while a normal build omits
them. Mach 3 retains its existing viewer/replay diagnostic interfaces; their
player/development split remains to be defined as its reconstruction progresses.

```sh
cargo run --locked --release -p reforge --features dev-tools -- \
  alley-cat --original /path/to/CAT.EXE --scene cheese --disable-damage all
```

`--no-default-features` omits window support. Alley Cat headless diagnostics also
require `dev-tools`; Mach 3's package provides separate frame/opening/replay
binaries for headless work. These tools continue to be available with
`cargo run --locked -p mach3 --bin mach3-replay -- --help`.

## Source organization

- `app/`: the `reforge` application and game selection.
- `engines/alleycat/`: the `alleycat` engine library and legacy game binary.
- `engines/mach3/`: the `mach3` engine library and its viewer/export/replay binaries.
- `crates/`: shared facilities as demonstrated reuse justifies extraction.

Game-specific gameplay, asset formats, timing and randomness stay in their own
engine crates. Engine IDs, Cargo packages and Rust crate names use `alleycat`
and `mach3`; the player game IDs remain `alley-cat` and `mach-3`. An engine can
support multiple games when their implementation is shared. Shared presentation
and audio can be extracted incrementally.
See [CONTRIBUTING.md](CONTRIBUTING.md) for style, commits and validation.

## License

The repository-wide project license has not been selected; no project license
is granted yet. Existing third-party terms and attribution remain in
[Alley Cat notices](engines/alleycat/THIRD_PARTY.md) and
[Mach 3 notices](engines/mach3/docs/dependencies.md), with their license texts.
Original game data is not included or licensed by this repository.
