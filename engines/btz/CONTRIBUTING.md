# Contributing to BTZ

Follow the repository [contribution guide](../../CONTRIBUTING.md). Commands here
run from the Cargo workspace root. BTZ uses Rust 2024; compiler requirements are
declared in the Cargo manifests.

## Build and validate

The core library has no default third-party dependencies. The `frontend` feature
adds player loading and terminal support; `sdl` adds windowed presentation through
the shared SDL crate. `dev-tools` enables the diagnostic binary and probes.

```sh
cargo build --locked -p btz
cargo test --locked -p btz
cargo test --locked -p btz --features frontend
cargo test --locked -p btz --features sdl
cargo clippy --locked -p btz --all-targets -- -D warnings
cargo clippy --locked -p btz --all-targets --all-features -- -D warnings
cargo test --locked -p btz --features dev-tools
cargo clippy --locked -p btz --all-targets --features dev-tools -- -D warnings
cargo fmt --all -- --check
```

Also run the workspace checks required by the root contribution guide. Tests
use authored synthetic fixtures, including owned Unix pseudoterminals. They do
not require original game files or a desktop.

## Player frontends

REforge dispatches verified Mindwheel files to `cli::run_with_files`. Windowed
play uses the shared SDL display and settings; `--renderer terminal` uses an
attached terminal without initializing an SDL window. Both hosts use `Session`
and the same player loading and file-service adapters. Graphical file prompts
are asynchronous and never read standard input.

## Development frontend

Supply the original inputs explicitly:

```sh
cargo run --locked -p btz --features dev-tools --bin btz-core -- \
  --original /path/to/ADISK.OBJ --interpreter /path/to/GO.EXE \
  --side-map /path/to/SIDE.TXT
```

The diagnostic terminal adapter supports Unix and Windows consoles. This binary is separate
from REforge's application game detection interface. Without `--interpreter`,
it operates as a raw VM trace probe rather than a playable session.

| Option | Meaning |
| --- | --- |
| `--snapshot` | Stop at the first input/key wait |
| `--input file` | Replay DOS key bytes: CR submits, LF is ignored, and each batch ends at CR |
| `--save-dir directory` | Base for relative prompted bookmark/transcript names; defaults beside the supplied game data |
| `--bookmark-path file` | Resolve bookmark write/read requests without prompting |
| `--transcript-path file` | Resolve transcript requests without prompting |
| `--side-map file` | Supply logical-disk mappings |
| `--trace` | Print VM/session diagnostics |
| `--screen-dump file` | Write 4,000 logical DOS character/attribute bytes |
| `--unlimited` | Remove the replay/snapshot execution budget |
| Numeric positional limit | Set an explicit diagnostic VM-step budget |

Interactive play has no default lifetime instruction limit. Replays/snapshots
default to 100,000 steps; `--unlimited` and a numeric limit are mutually
exclusive. `--snapshot` and `--input` are also mutually exclusive. Without a
terminal or replay, the text probe takes a snapshot.

Replay is a diagnostic key source at exposed waits, not an original CPU/input
schedule. Successful replay alone does not establish live timing/typeahead
equivalence. Explicit per-file CLI paths keep their launch-directory meaning;
`--save-dir` controls prompted relative names, including bookmark restore.
Absolute prompted paths override that base. Additional-media folder prompts
and input-file paths remain separate from save-file resolution.

## Implementation and verification

Keep story behavior in the supplied wordcode. Reconstruct native algorithms
rather than adding room/NPC scripts or frontend command aliases. See the
[format](docs/mindwheel-dos-format.md) and [runtime](docs/mindwheel-dos-runtime.md)
references for the current implementation contract.

Use synthetic tests for instruction, parser, text, editor, timing and format
boundaries. Original files, decoded text, memory dumps, captured screens and
comparison recordings remain external evidence. Public builds/tests must not
depend on them or unpublished helpers.

Compatibility claims must identify the supplied game/interpreter, operation
and timing/RNG inputs. Distinguish synthetic tests, static original-code
analysis, selected native comparisons and an actual complete playthrough.
Do not equate the first three with the fourth.

Preserve typed addresses, wrapping arithmetic, exact predicates and pending
operations. Keep filesystem dialogs, clocks and terminal policy outside the
VM. Retain the documented timing/input limitations and the fetch-boundary
contract of `VmState`. Support for another interpreter requires identified
inputs and verified semantics.
