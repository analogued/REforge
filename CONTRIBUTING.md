# Contributing

## Formatting and code style

Use UTF-8 text, LF line endings, a final newline, and no trailing whitespace in
authored text files. The repository's `.editorconfig` records these settings.

The repository's `.gitattributes` normalizes detected text files to LF in Git
and keeps LF on checkout, including on Windows. Add explicit `-text` rules for
any fixtures or other files whose bytes must be preserved exactly. If a file
needs another line-ending convention, keep its Git and editor rules consistent.

The authored game implementation uses Rust. Use `cargo fmt` to format and
`cargo fmt --check` to verify formatting. Use standard Rust naming conventions
and the default rustfmt configuration. Supporting tools can use
other languages; select their formatters when needed. Let established formatters
govern indentation, spacing, and wrapping. Add formatting checks when CI is
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
within this repository. Do not repeat the game's name on every commit.

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

The project license remains undecided. Assess proposed reuse against the candidate
licenses and record any constraints before adoption; do not silently narrow the
choice.

## Validation

Run checks appropriate to the change and describe material limitations in
validation when relevant. For changes to the Rust implementation:

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --no-default-features
cargo clippy --locked --all-targets --no-default-features -- -D warnings
```

The default build requires SDL3; the headless build does not. Unit tests use
synthetic data and do not require original game files. Runtime checks need the
supported external executable described in the README. Keep captures and raw
comparison evidence outside Git, with input hashes and reproduction metadata.
Compare reconstructed routines and accepted movement updates against the
original before claiming behavior fidelity; a window smoke test alone does not
establish that fidelity.

Use `CARGO_TARGET_DIR` and, when appropriate, `CARGO_HOME` to direct reproducible
build/cache output to a configured working directory. Do not commit those outputs.
