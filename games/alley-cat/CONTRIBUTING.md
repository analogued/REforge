# Contributing to Alley Cat

Follow [REforge's contributing guide](../../CONTRIBUTING.md) for formatting,
commit conventions, licensing and shared validation. This game is the `alley-cat`
package in the combined Cargo workspace, using its root lockfile. Rust requires
version 1.85 or newer. Keep game-specific assets, behavior, timing and randomness
within this crate. Preserve imported code attribution and license notices.

## Validation

Run checks appropriate to the change and describe material limitations in
validation when relevant. For changes to the Rust implementation, run from the combined repository root:

```sh
cargo fmt --all -- --check
cargo test --locked -p alley-cat
cargo clippy --locked -p alley-cat --all-targets -- -D warnings
cargo test --locked -p alley-cat --no-default-features
cargo clippy --locked -p alley-cat --all-targets --no-default-features -- -D warnings
cargo test --locked -p alley-cat --features dev-tools
cargo clippy --locked -p alley-cat --all-targets --features dev-tools -- -D warnings
cargo test --locked -p alley-cat --no-default-features --features dev-tools
cargo clippy --locked -p alley-cat --all-targets --no-default-features --features dev-tools -- -D warnings
```

The default build requires SDL3. Headless diagnostic runs require
`--no-default-features --features dev-tools` and do not need SDL3. The normal
CLI excludes testing options; verify both the normal and development feature
combinations when changing CLI or test switches. Development help is available
with `cargo run --locked -p reforge --features dev-tools -- alley-cat --help`.

Unit tests use synthetic data and do not require original game files. Runtime checks need the
supported external executable described in the README. Keep captures and raw
comparison evidence outside Git, with input hashes and reproduction metadata.
Compare reconstructed routines and accepted movement updates against the
original before claiming behavior fidelity; a window smoke test alone does not
establish that fidelity.

Use `CARGO_TARGET_DIR` and, when appropriate, `CARGO_HOME` to direct reproducible
build/cache output to a configured working directory. Do not commit those outputs.
