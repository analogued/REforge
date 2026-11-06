# Contributing

## Formatting and code style

Use UTF-8 text, LF line endings, a final newline, and no trailing whitespace in
authored text files. The repository's `.editorconfig` records these settings.

The repository's `.gitattributes` normalizes detected text files to LF in Git
and keeps LF on checkout, including on Windows. Add explicit `-text` rules for
any fixtures or other files whose bytes must be preserved exactly. If a file
needs another line-ending convention, keep its Git and editor rules consistent.

The implementation uses Rust, standard Rust naming and rustfmt.
Format the workspace with `cargo fmt --all`. Supporting tools may use
other languages with documented formatters. Keep CI formatting checks in step
with those tools.

Keep broad formatting changes separate from functional changes. Preserve
third-party style and notices. Do not reformat generated files or fixtures whose
exact contents matter.

## Features and deviations

Document intentional departures from original game behavior or presentation in
the engine's deviations documentation, including corrections to original bugs
and changes to quirks. Keep these records current when behavior changes. Use a
format and level of detail appropriate to the engine; no fixed entry template
is required.

Features and deviations describe different aspects of a change. Features describe
player capabilities or improvements; deviations describe differences from the
original. Their documentation can overlap:

- A feature that is also a deviation belongs in both the user-facing guide and
  deviations documentation. BTZ's save/load menu and Mach 3's interpolation are
  examples.
- A feature that retains original behavior or compatibility needs feature
  documentation. BTZ's compatibility with original save files is an example;
  that compatibility is not itself a deviation.
- Original bug corrections and changed quirks each deserve a brief mention in
  deviations documentation. They usually need little space in user guides; a
  short collective summary can be enough when there are many. An unusually
  significant correction may also warrant coverage as a feature.

User-facing descriptions should explain the experience and how to use relevant
options. Deviations documentation can explain the original and changed behavior
more technically. Cross-link related descriptions where useful. Record known
reconstruction bugs, missing features and unresolved fidelity questions in
compatibility or limitations documentation.

## Commit messages

Start the description after any scope prefix with an imperative verb and
describe the change clearly.
Examples of suitable verbs include Add, Fix, Remove, Document, Set, Include,
Tweak, and Make; this is not a restricted vocabulary.

Use `<scope>: <Message>` when a change primarily belongs to one engine or
shared area. Engine scopes match the source crate IDs: `alleycat:` and `mach3:`.
Shared scopes such as `sdl:`, `app:` or `docs:` identify the relevant facility.
Keep scope names and capitalization consistent. Broad changes spanning several
areas can omit the prefix when no single scope usefully describes them.

Illustrative subjects:

```text
alleycat: Fix bin collision boundaries
mach3: Add input timing traces
sdl: Handle drawable size changes
docs: Clarify commit scope prefixes
Update workspace dependencies
```

Keep the subject concise, preferably 50 characters or fewer, including any scope
prefix. Prioritize clarity over exact length. For substantive changes, leave a
blank line and explain the reason, relevant behavior changes, and validation in
the body. Wrap body text around 72 characters where practical.

The default branch is `main`. Keep each commit focused on a coherent change,
avoiding unrelated changes across areas. Broader structural commits are acceptable
during initial setup; use tighter scopes as the project matures. Coordinated
refactors may legitimately span several areas. Conventional Commits syntax is
not required.

## Release notes

Add notable user-facing changes to `Unreleased` in [CHANGELOG.md](CHANGELOG.md).
Group entries by New games, General, individual engines or platforms; include
only sections that have entries. Summarize the effect for users rather than
copying commit subjects. Under New games, use one "Added support for" entry per
game, including the platform where relevant.

Wrap `CHANGELOG.md` source lines at roughly 80 columns, indenting continued list
items by two spaces. The single-line bullet formatting used for GitHub release
notes does not apply to this file.

Describe the difference from the last public release, not each development step.
Consolidate related work into a few meaningful entries. Routine refactoring,
tests and documentation formatting generally do not need changelog entries.

When introducing a game in its first public release of support, do not list the
bug fixes made while developing that support. They are part of preparing the
initial implementation, not fixes to behavior users received in a prior release.
Use the new-game announcement and, where useful, a brief support/limitations
summary. Fixes affecting already released games or shared features can still
warrant their own entries, even when discovered during new-game development.

[Release packaging](docs/releases.md) documents calendar versioning and building
distributable packages.

## Third-party code

Code from other projects may be reused when its license is compatible with this
repository's license and intended distribution. Check the exact license/version
before adopting copied, adapted, or translated code. Record the upstream source,
revision, affected files, and modifications; retain required copyright notices,
attribution, and license text inside this repository. Attribution alone does not
replace the license requirements.

Keep copied, adapted, or translated third-party code and embedded third-party
data in dedicated files or modules wherever practical. Avoid scattering imported
material through authored implementation files. Retain its notices and provenance
with it, and identify local adaptations and any authored helpers in those files.
Ordinary library API calls do not require upstream source notices at each call
site; keep applicable dependency notices centrally in the repository.

Maintain direct dependency, imported-code and asset attribution in the root
`THIRD_PARTY.md`, with one text per distinct license under `LICENSES/`. Keep
component-specific copyright notices in the inventory and existing notices in
imported source. Do not maintain per-version license directories or manually
duplicate indirect Cargo notices; [release packaging](docs/releases.md) generates
one combined notices document for the selected target and features.

Contributions to authored code and documentation use
[`GPL-3.0-or-later`](LICENSE). Review proposed imports for compatibility with
that license before adoption, retaining their original terms and attribution.

## Validation

GitHub Actions runs Linux checks on pushes and pull requests to `main`, and can
also be started manually. It checks formatting, workspace tests and Clippy with
normal, headless and development feature combinations, selected software-rendered
launcher tests, and the minimum supported Rust version. SDL3 is built from a pinned
release; tests use synthetic fixtures and need no original game files or desktop.

Run `cargo fmt --all -- --check`,
`cargo test --locked --workspace`, and
`cargo clippy --locked --workspace --all-targets -- -D warnings`, plus the
feature/build combinations required by the affected crates. Package-scoped
checks can aid development; shared changes must validate affected games.
See [Alley Cat checks](engines/alleycat/CONTRIBUTING.md),
[Mach 3 checks](engines/mach3/CONTRIBUTING.md), and
[BTZ checks](engines/btz/CONTRIBUTING.md) for their feature matrices and tools.
For application CLI changes, test and lint `-p reforge` with default features,
`--no-default-features`, `--features dev-tools`, and both options combined.
Release optimization (`--release`) is independent of the opt-in `dev-tools`
feature; normal builds exclude engine testing and diagnostic switches.

When changing dependencies or using new language/library features, also check
the minimum supported Rust version, currently 1.88. With that toolchain installed:

```sh
cargo +1.88.0 check --locked --workspace --all-targets --all-features
```

Tests must use authored synthetic fixtures, with original-data comparisons
stored externally. Document concrete commands and limitations as crates are added.

Use separate worktrees and configured build/cache paths for concurrent sessions.
Commit on the worktree's branch, then merge completed changes into `main`.
Do not stage or commit another session's unfinished changes. Before removing a
worktree, preserve its uncommitted/untracked work and verify its commits are
integrated or otherwise deliberately retained. No force-removal or branch
deletion is part of the ordinary workflow.
