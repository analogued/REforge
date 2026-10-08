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
other languages with documented formatters. Add formatting checks when CI is
introduced.

Keep broad formatting changes separate from functional changes. Preserve
third-party style and notices. Do not reformat generated files or fixtures whose
exact contents matter.

## Commit messages

Start the subject with an imperative verb and describe the change clearly.
Examples of suitable verbs include Add, Fix, Remove, Document, Set, Include,
Tweak, and Make; this is not a restricted vocabulary.

An optional `<scope>: <Message>` prefix can identify a subsystem or module when
it helps readers scan the history. Use consistent scope names and capitalization
within this repository. A game prefix such as `alley-cat:` or `mach-3:` is useful
for changes confined to that game; use shared-area prefixes when appropriate.

Illustrative subjects:

```text
Add command-line help for the asset inspector
tools: Include the input hash in inspection reports
audio: Fix timer wraparound handling
```

Aim for around 50 characters in the subject, including a prefix, without
sacrificing clarity. For substantive changes, leave a blank line and explain
the reason, relevant behavior changes, and validation in the body. Wrap body
text around 72 characters where practical.

The default branch is `main`. Keep each commit focused on a coherent change,
avoiding unrelated changes across areas. Broader structural commits are acceptable
during initial setup; use tighter scopes as the project matures. Coordinated
refactors may legitimately span several areas. Conventional Commits syntax is
not required.

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

The project license remains undecided. Assess proposed reuse against the candidate
licenses and record any constraints before adoption; do not silently narrow the
choice.

## Validation

Run `cargo fmt --all -- --check`,
`cargo test --locked --workspace`, and
`cargo clippy --locked --workspace --all-targets -- -D warnings`, plus the
feature/build combinations required by the affected crates. Package-scoped
checks can aid development; shared changes must validate affected games.
See [Alley Cat checks](engines/alleycat/CONTRIBUTING.md) and
[Mach 3 checks](engines/mach3/CONTRIBUTING.md) for their feature matrices and tools.
For application CLI changes, test and lint `-p reforge` with default features,
`--no-default-features`, `--features dev-tools`, and both options combined.
Release optimization (`--release`) is independent of the opt-in `dev-tools`
feature; normal builds exclude engine testing and diagnostic switches.

Tests must use authored synthetic fixtures, with original-data comparisons
stored externally. Document concrete commands and limitations as crates are added.

Use separate worktrees and configured build/cache paths for concurrent sessions.
Commit on the worktree's branch, then merge completed changes into `main`.
Do not stage or commit another session's unfinished changes. Before removing a
worktree, preserve its uncommitted/untracked work and verify its commits are
integrated or otherwise deliberately retained. No force-removal or branch
deletion is part of the ordinary workflow.
