# Release packaging

The repository maintains direct dependency and imported-code attribution in
[THIRD_PARTY.md](../THIRD_PARTY.md), with one text per license type under
[LICENSES/](../LICENSES/). Indirect Cargo dependencies are discovered during
packaging rather than copied into per-crate notice directories.

## Versioning and release notes

REforge uses `YEAR.RELEASE.PATCH` calendar versions. `RELEASE` counts main
releases within that year, starting at 1; it is not a month or quarter.
`PATCH` starts at 0 and increases for maintenance updates to that release.
For example, `2026.1.1` updates `2026.1.0`, while `2026.2.0` is the next main
release. The first main release in a new year is `YEAR.1.0`.

The single version in the root `[workspace.package]` table is inherited by all
application, engine and shared crates. `reforge --version` (or `-V`) reports it.
The `-dev` suffix marks unreleased development builds; it is independent of
the `dev-tools` feature and Cargo's `--release` optimization flag.

[CHANGELOG.md](../CHANGELOG.md) contains curated user-facing notes grouped by
New games, General, individual engines and platforms where relevant. `Unreleased`
contains changes intended for the next public release; a local build or package
does not count as one.
Git history records the detailed development work separately.

Calendar versions do not encode compatibility guarantees. Check release notes
for changes requiring user action, including new system/build requirements or
incompatible command-line options.

### Temporary compatibility cleanup

Review these transitional adapters for removal after a few published releases
have carried each migration. Development builds do not count toward that window;
no removal version has been selected yet.

- **BLAKE3 identities (first shipped in 2026.2.1):**
  `app/src/detection/migration.rs` retains six old-to-new release-ID aliases and
  ten preview-cache-key aliases. Retire their library/profile and preview lookup
  wiring and migration-only tests together. No SHA-256 verification algorithm,
  `sha2` dependency or per-file SHA-256 fingerprint catalog is retained for this
  transition. SHA-256 checks for release-packaging dependency sources are separate.
- **Enhancements preferences (currently Unreleased):**
  `migrate_enhancements` in `crates/reforge_settings/src/lib.rs` moves the old
  Display/Controls fields into per-entry Enhancements tables. Retire the helper,
  its call, the legacy-section read fallback and migration-only tests together.
  Keep ordinary Enhancements loading, saving and reset coverage.
- **Centurion sound preferences (currently Unreleased):**
  `migrate_centurion_sound` in `crates/reforge_settings/src/lib.rs` moves the
  former per-entry `launcher.sound` choice into shared `audio.audio_renderer`.
  Retire its call and migration-only tests together; retain shared audio
  inheritance, startup selection and reset coverage.

## Build a package

Install Python 3.11 or newer and the release-only
[`cargo-about`](https://embarkstudios.github.io/cargo-about/) tool:

```sh
cargo install --locked --version 0.9.2 --features cli cargo-about
python3 tools/package_release.py
```

The command builds the `reforge` application in release mode using Cargo.lock
and writes `out/releases/reforge-<version>-<platform>-<architecture>.tar.gz`
(ZIP for Windows targets), for example `reforge-2026.1.0-linux-x86_64.tar.gz`.
Common targets use Linux, macOS or Windows names, with a `-musl` suffix for Linux
musl builds. Other targets retain their full Rust target name. The full Rust
target is always recorded in the notices. The version is read from Cargo's
resolved application metadata.
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
`THIRD_PARTY-<version>-<target>.html` without compiling or creating an archive.
`--offline` requires cached Cargo inputs and cannot fetch license clarifications.
Generation fails if this leaves placeholder MIT copyright notices. Use an online
run when the required upstream texts are not available locally.

## Windows CI artifacts

The single Windows/stable job builds natively on Windows Server 2022 using the
x64 MSVC toolchain. After its checks pass, the workflow uploads
`reforge-windows-x86_64`, retained for seven days. Download it from the workflow
run's Artifacts section and extract it before running `reforge.exe`.

The test bundle includes `SDL3.dll`, REforge's license, generated dependency
notices, SDL's license and a `BUILD.txt` identifying the exact source commit.
Keep the DLL beside the executable. The Microsoft Visual C++ x64 runtime is a
prerequisite. Original game files are external, as with other builds.

These unsigned CI builds support testing; they are not published releases.
Passing CI covers compilation, automated tests and a DLL-loading/version smoke
check, not validation of native GPU rendering or physical devices.

## Notice generation and maintenance

The generator scans the application's enabled Cargo graph for the specified
target, including transitive dependencies. It retains build dependencies
conservatively because generated output can incorporate their code; development
dependencies are excluded. It adds the source inventory, shader/font attribution
and shared license texts to the same HTML file. The source inventory can cover
assets omitted by a headless build; the Cargo listing is target/feature filtered.
The result records the REforge version, target, features and Cargo.lock hash.

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
