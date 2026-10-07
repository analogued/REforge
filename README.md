# REforge

Combined game reconstruction project. Status: source-layout scaffold only.
The main implementation uses Rust and SDL3. Each game requires external original
game data at runtime; do not bundle original or extracted assets.

## Source structure

The intended Cargo workspace contains one application under `app/`, separate
library crates under `games/<game-id>/`, and shared crates under `crates/`.
The workspace root will own `Cargo.toml` and a single `Cargo.lock`. The application
depends on game crates; games depend on shared facilities; shared crates do not
depend on game implementations.

The scaffold reserves these directories without generating Cargo manifests,
placeholder game code or a shared engine. Add actual crates and workspace
membership as implementation begins. Extract shared code when demonstrated use
justifies it. Game-specific assets, gameplay, timing and random algorithms remain
owned by each game.

Keep game implementation/reference documentation beside its crate under
`games/<game-id>/docs/` when needed. Root documentation covers the application and
shared development; shared crates can document their own APIs. Discussion,
experiments, audits and private analysis stay outside the source repository.

## Development

Each development worktree contains this entire repository, on its own branch.
Make focused commits and merge completed branches into `main`. One checkout can
be used for sequential work; concurrent sessions should use distinct worktrees.
See [CONTRIBUTING.md](CONTRIBUTING.md) for conventions and validation.

## External inputs

Original executables/assets, raw analysis, emulator state and reference captures
remain outside this repository. Accept their paths through configuration without
requiring any particular parent-directory layout or local manifest.

## License

The combined project's license has not been selected; none is granted by this
scaffold. Once chosen, put its full text in root `LICENSE`, identify it here, and
add short copyright/SPDX headers to authored source. Preserve third-party notices
and record compatible dependency and imported-code licenses within this repository.
