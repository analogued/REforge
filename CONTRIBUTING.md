# Contributing

## Formatting and code style

Use UTF-8 text, LF line endings, a final newline, and no trailing whitespace in
authored text files. The repository's `.editorconfig` records these settings.

The repository's `.gitattributes` normalizes detected text files to LF in Git
and keeps LF on checkout, including on Windows. Add explicit `-text` rules for
any fixtures or other files whose bytes must be preserved exactly. If a file
needs another line-ending convention, keep its Git and editor rules consistent.

Use Rust 1.85 or later for the main implementation, with rustfmt and the
checked-in `rustfmt.toml`. Format with `cargo fmt --all`.
Tools and investigation scripts may use other languages.

The maintained Python tools use Python 3.11 or later. Format with
`ruff format --config ruff.toml tools tests` and check with
`ruff check --config ruff.toml tools tests`. The decoder itself uses only the
Python standard library. Keep language-specific naming idiomatic and let the
formatter govern spacing and wrapping. Add formatting checks when CI is introduced.

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

Run the Python tool checks with `python3 -m unittest discover -s tests -v`.
Tests use authored synthetic data and must not import original game material.
Check the Rust implementation with:

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

With SDL2 development and runtime libraries installed, also check the viewer:

```sh
cargo test --locked --features viewer
cargo clippy --locked --all-targets --features viewer -- -D warnings
```

Build the library and frame renderer with `cargo build --locked`; add
`--features viewer` to build the windowed viewer. The viewer feature enables
the dependencies recorded in `Cargo.lock` and [dependency notices](docs/dependencies.md).
Once Cargo has fetched them, use `--offline` for local checks and builds.
The viewer's software-rendering test uses an offscreen surface and requires no
desktop display. Tests use synthetic resources; private captures and original
assets must not become test fixtures. Set `CARGO_TARGET_DIR` when build output
belongs in a separately configured working directory.

Run checks appropriate to the change and describe material limitations in
validation when relevant. Runtime captures and comparisons with original-derived
images remain external evidence rather than repository fixtures.
