# Release packaging

The repository maintains direct dependency and imported-code attribution in
[THIRD_PARTY.md](../THIRD_PARTY.md), with one text per license type under
[LICENSES/](../LICENSES/). Indirect Cargo dependencies are discovered during
packaging rather than copied into per-crate notice directories.

## Build a package

Install Python 3.11 or newer and the release-only
[`cargo-about`](https://embarkstudios.github.io/cargo-about/) tool:

```sh
cargo install --locked --version 0.9.2 --features cli cargo-about
python3 tools/package_release.py
```

The command builds the `reforge` application in release mode using Cargo.lock
and writes `out/releases/reforge-<target>.tar.gz` (ZIP for Windows targets).
The archive contains the executable, the root GPLv3 `LICENSE` and one
self-contained `THIRD_PARTY.html`. Packaging fails if `LICENSE` is missing.
This command creates a local artifact; it does not publish a release. It does
not bundle original game data, SDL or other system libraries. Players need the
SDL3 runtime for the normal build; bundling native libraries requires reviewing
their actual versions, dependencies and applicable notices separately.

Options select the target, features and destination for both the build and its
notices. Cross-compilation still requires the corresponding Rust target, linker
and native development libraries:

```sh
python3 tools/package_release.py --target x86_64-unknown-linux-gnu
python3 tools/package_release.py --features dev-tools --output-dir /path/to/packages
python3 tools/package_release.py --no-default-features
```

`CARGO_HOME` and `CARGO_TARGET_DIR` are honored. `--notices-only` generates
`THIRD_PARTY-<target>.html` without compiling or creating an archive. `--offline`
requires cached Cargo inputs and cannot fetch license clarifications; generation
fails if this leaves placeholder MIT copyright notices. Use an online run when
the required upstream texts are not available locally.

## Notice generation and maintenance

The generator scans the application's enabled Cargo graph for the specified
target, including transitive dependencies. It retains build dependencies
conservatively because generated output can incorporate their code; development
dependencies are excluded. It adds the source inventory, shader/font attribution
and shared license texts to the same HTML file. The source inventory can cover
assets omitted by a headless build; the Cargo listing is target/feature filtered.
The result records the target, feature choices and Cargo.lock hash.

[about.toml](../about.toml) prefers MIT when alternatives are offered, accepts
the other currently needed terms, and verifies upstream license files missing
from some published crate archives against recorded SHA-256 hashes. A missing
or unrecognized license, failed clarification leaving a placeholder copyright,
or failed build stops packaging. Review generator warnings and the final notices
when dependencies change; automated discovery does not replace reviewing copied
code, assets or additional native libraries.

When adding a direct dependency or importing code/assets, update THIRD_PARTY.md,
add a license text only if its terms are not already present, and retain upstream
notices in source. Do not add per-version directories or manually copy every
indirect crate's licenses. Changes to indirect dependencies normally need only
the Cargo.lock update and regeneration; add a verified clarification if an
archive omits necessary notices. Generated HTML and archives stay in ignored
build output, outside Git.

Warnings about clarifications for crates absent from the selected graph are
expected when changing targets or disabling SDL features.

## Source for binary releases

REforge's authored code and documentation use `GPL-3.0-or-later`. For binary
releases, publish the complete corresponding source for the exact released
revision alongside the binary downloads, following [GPLv3 section 6](https://www.gnu.org/licenses/gpl-3.0.html#section6).
The source must include the build scripts, Cargo.lock and shader sources needed
to rebuild that version. Original game data remains an external player input
and must not be included.

Create the source archive from the same clean release commit used for the
binary build. The packaging command creates the binary archive and notices;
publishing the matching source is a separate release step.
