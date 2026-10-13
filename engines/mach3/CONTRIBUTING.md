# Contributing to Mach 3

Follow [REforge's contributing guide](../../CONTRIBUTING.md) for formatting,
commit conventions, licensing and shared validation. Mach 3 is the `mach3`
package within the combined Cargo workspace, with one lockfile at its root.
Rust requires version 1.85 or later and uses the repository's rustfmt settings.
Keep game-specific formats, gameplay, timing, randomness and input behavior
within this crate.

Tests use authored synthetic data and must not import original game material.
Preserve third-party style and notices, including the complete dependency terms
linked from [dependency notices](docs/dependencies.md).

## Rust validation

From the combined repository root, run:

```sh
cargo fmt --all -- --check
cargo test --locked -p mach3
cargo clippy --locked -p mach3 --all-targets -- -D warnings
cargo test --locked -p mach3 --features dev-tools
cargo clippy --locked -p mach3 --all-targets --features dev-tools -- -D warnings
```

With SDL3 development and runtime libraries installed, also check the viewer:

```sh
cargo test --locked -p mach3 --features viewer
cargo clippy --locked -p mach3 --all-targets --features viewer -- -D warnings
cargo test --locked -p mach3 --features viewer,dev-tools
cargo clippy --locked -p mach3 --all-targets --features viewer,dev-tools -- -D warnings
```

Build the library with `cargo build --locked -p mach3`; add `--features dev-tools`
to build the frame renderer, opening exporter, replay tool and sound exporter.
Use `--features viewer` to build the windowed viewer, or `--features viewer,dev-tools`
for both. The `viewer` feature exposes `mach3::cli::run` for REforge dispatch and
the standalone `mach3-viewer` binary. Dependencies are recorded in the root
`Cargo.lock` and [dependency notices](docs/dependencies.md). Once Cargo has
fetched them, use `--offline` for local checks and builds.

The opt-in `dev-tools` feature enables all four diagnostic/export binaries and
their CLI integration tests, optional offline speaker WAV rendering, and input
and timing diagnostics in viewer builds; it does not enable SDL3 by itself.
Normal builds exclude `--trace-input` and `--trace-timing`, including
release builds unless `dev-tools` is explicitly enabled. Test all
four combinations above when changing tracing or viewer input. REforge's
`dev-tools` feature forwards this feature to Mach 3.

Shared presentation tests (`cargo test --locked -p reforge_sdl`) use offscreen
surfaces and require no desktop display. Tests use synthetic resources; private captures and original
assets must not become test fixtures. Set `CARGO_TARGET_DIR` when build output
belongs in a separately configured working directory. Shared application changes
must also validate affected games as described in the root contributing guide.

Runtime captures and comparisons with original-derived images remain external
evidence. Describe material validation limitations when relevant.
