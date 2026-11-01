# Contributing to the Mach 3 engine

Follow [REforge's contributing guide](../../CONTRIBUTING.md) for formatting,
commit conventions, licensing and shared validation. The `mach3` engine is a
package in the combined Cargo workspace and uses its root `Cargo.lock`. Rust
1.87 or newer is required. Keep game-specific file formats, gameplay, timing,
randomness and input behavior within this crate. Preserve imported code
attribution and license notices; see the shared
[dependency notices](../../THIRD_PARTY.md).

## Rust validation

For changes to the Rust implementation, run these checks from the combined
repository root. They supplement the workspace checks in REforge's contributing
guide. Describe material validation limitations when relevant.

```sh
cargo fmt --all -- --check
cargo test --locked -p mach3
cargo clippy --locked -p mach3 --all-targets -- -D warnings
cargo test --locked -p mach3 --no-default-features
cargo clippy --locked -p mach3 --all-targets --no-default-features -- -D warnings
cargo test --locked -p mach3 --features dev-tools
cargo clippy --locked -p mach3 --all-targets --features dev-tools -- -D warnings
cargo test --locked -p mach3 --no-default-features --features dev-tools
cargo clippy --locked -p mach3 --all-targets --no-default-features --features dev-tools -- -D warnings
```

Shared presentation tests (`cargo test --locked -p reforge_sdl`) use offscreen
surfaces and require no desktop display. Tests must use authored synthetic data
and must not import original game files, extracted assets or private captures.

## Build features and tools

The default `sdl` feature enables windowed playback and requires SDL3 development
and runtime libraries. Use `--no-default-features` to build without SDL3.
Headless diagnostic runs also require `--features dev-tools`.

With `sdl` enabled, `mach3::cli::run` provides windowed playback for REforge and
the standalone `mach-3` executable. Build both the library and standalone
executable with `cargo build --locked -p mach3`.

Add `--features dev-tools` to build the frame renderer, opening exporter, replay
tool and sound exporter, and enable their CLI integration tests and offline
speaker WAV rendering. In windowed builds, it also enables input/timing
diagnostics and silent step/static previews. Combine it with
`--no-default-features` when only headless tools are needed; `dev-tools` does not
enable SDL3 by itself.

Normal builds exclude `--step`, `--static`, `--trace-input` and `--trace-timing`,
including release builds unless `dev-tools` is explicitly enabled. Check all
four combinations above when changing tracing or windowed input. REforge's
`dev-tools` feature forwards this feature to the engine.

## Original-data validation

Runtime checks require the supported external game files described in the
[engine README](README.md). Keep captures and raw comparison evidence outside
Git, with input hashes and reproduction metadata.

Compare reconstructed behavior against controlled original-code evidence before
claiming fidelity. Distinguish logical replay, synthetic tests, native
presentation checks and timing/audio comparisons; none alone establishes
full-game fidelity.

## Build output and dependencies

Use `CARGO_TARGET_DIR` and, when appropriate, `CARGO_HOME` to direct reproducible
build/cache output to a configured working directory. Do not commit those outputs.
Once Cargo has fetched the locked dependencies, use `--offline` for local checks
and builds.
