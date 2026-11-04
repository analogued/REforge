# Contributing to the Alley Cat engine

Follow [REforge's contributing guide](../../CONTRIBUTING.md) for formatting,
commit conventions, licensing and shared validation. The `alleycat` engine is a
package in the combined Cargo workspace and uses its root `Cargo.lock`. Rust
1.88 or newer is required. Keep game-specific file formats, gameplay, timing,
randomness and input behavior within this crate. Preserve imported code
attribution and license notices; see the shared
[dependency notices](../../THIRD_PARTY.md).

## Rust validation

For changes to the Rust implementation, run these checks from the combined
repository root. They supplement the workspace checks in REforge's contributing
guide. Describe material validation limitations when relevant.

```sh
cargo fmt --all -- --check
cargo test --locked -p alleycat
cargo clippy --locked -p alleycat --all-targets -- -D warnings
cargo test --locked -p alleycat --no-default-features
cargo clippy --locked -p alleycat --all-targets --no-default-features -- -D warnings
cargo test --locked -p alleycat --features dev-tools
cargo clippy --locked -p alleycat --all-targets --features dev-tools -- -D warnings
cargo test --locked -p alleycat --no-default-features --features dev-tools
cargo clippy --locked -p alleycat --all-targets --no-default-features --features dev-tools -- -D warnings
```

Shared presentation tests (`cargo test --locked -p reforge_sdl`) use offscreen
surfaces and require no desktop display. Tests must use authored synthetic data
and must not import original game files, extracted assets or private captures.

## Build features and tools

The default `sdl` feature enables windowed playback and requires SDL3 development
and runtime libraries. Use `--no-default-features` to build without SDL3.
Headless diagnostic runs also require `--features dev-tools`.

With `sdl` enabled, `alleycat::cli::run` provides windowed playback for REforge and
the standalone `alley-cat` executable. Build both the library and standalone
executable with `cargo build --locked -p alleycat`.

The opt-in `dev-tools` feature enables diagnostic CLI options. Normal builds
exclude these options, including release builds unless `dev-tools` is explicitly
enabled. Check all four combinations above when changing CLI or diagnostic
switches. REforge's `dev-tools` feature forwards this feature to the engine.
Development help is available
with `cargo run --locked -p reforge --features dev-tools -- --original /path/to/CAT.EXE --help`.

## Original-data validation

Runtime checks require the supported external game files described in the
[engine README](README.md). Keep captures and raw comparison evidence outside
Git, with input hashes and reproduction metadata.

Compare reconstructed routines and accepted movement updates against the
original before claiming behavior fidelity; a window smoke test alone does not
establish that fidelity.

## Build output and dependencies

Use `CARGO_TARGET_DIR` and, when appropriate, `CARGO_HOME` to direct reproducible
build/cache output to a configured working directory. Do not commit those outputs.
Once Cargo has fetched the locked dependencies, use `--offline` for local checks
and builds.
