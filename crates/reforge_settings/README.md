# Shared preferences

Pure Rust preference model, TOML editing and coordinated atomic persistence.
No SDL types, engine execution state or original game data are used.
See [configuration](../../docs/configuration.md) for the player-facing contract.

The player application installs a scoped main-thread launch context after game
detection. A shared SDL window reads that context into a session. Standalone
engine/SDL diagnostic callers get an in-memory session without filesystem access.
Drafts contain sparse global/game patches; effective runtime settings overlay
temporary CLI values. The menu requests activation first and commits only after
the SDL runtime accepts the request.

Mach 3 also has a per-game native startup preference for digitized samples,
kept outside shared/global host audio keys. The engine reads it once at startup;
later menu commits persist the next-launch choice without changing execution.

## Dependencies

Ordinary library API use; no dependency source was adapted into authored code.
MIT is selected wherever alternatives are offered. The root Cargo.lock pins
these versions. Preserve the full notices below when distributing them.

| Dependency | Version | Selected terms |
| --- | --- | --- |
| [equivalent](https://crates.io/crates/equivalent/1.0.3) | 1.0.3 | [MIT](licenses/equivalent-1.0.3-MIT.txt) |
| [fs2](https://crates.io/crates/fs2/0.4.3) | 0.4.3 | [MIT](licenses/fs2-0.4.3-MIT.txt) |
| [hashbrown](https://crates.io/crates/hashbrown/0.17.1) | 0.17.1 | [MIT](licenses/hashbrown-0.17.1-MIT.txt) |
| [indexmap](https://crates.io/crates/indexmap/2.14.2) | 2.14.2 | [MIT](licenses/indexmap-2.14.2-MIT.txt) |
| [memchr](https://crates.io/crates/memchr/2.8.3) | 2.8.3 | [MIT](licenses/memchr-2.8.3-MIT.txt) |
| [toml_datetime](https://crates.io/crates/toml_datetime/0.6.11) | 0.6.11 | [MIT](licenses/toml_datetime-0.6.11-MIT.txt) |
| [toml_edit](https://crates.io/crates/toml_edit/0.22.27) | 0.22.27 | [MIT](licenses/toml_edit-0.22.27-MIT.txt) |
| [toml_write](https://crates.io/crates/toml_write/0.1.2) | 0.1.2 | [MIT](licenses/toml_write-0.1.2-MIT.txt) |
| [winapi](https://crates.io/crates/winapi/0.3.9) | 0.3.9 | [MIT](licenses/winapi-0.3.9-MIT.txt) |
| [winapi-i686-pc-windows-gnu](https://crates.io/crates/winapi-i686-pc-windows-gnu/0.4.0) | 0.4.0 | [MIT](licenses/winapi-i686-pc-windows-gnu-0.4.0-MIT.txt) |
| [winapi-x86_64-pc-windows-gnu](https://crates.io/crates/winapi-x86_64-pc-windows-gnu/0.4.0) | 0.4.0 | [MIT](licenses/winapi-x86_64-pc-windows-gnu-0.4.0-MIT.txt) |
| [winnow](https://crates.io/crates/winnow/0.7.15) | 0.7.15 | [MIT](licenses/winnow-0.7.15-MIT.txt) |

The two winapi target support crates use the same upstream winapi-rs MIT terms
(the published support archives omit a separate license file); copies are
retained explicitly for them. libc is also used through fs2; its existing
[MIT notice](../reforge_sdl/docs/licenses/libc-0.2.190-MIT.txt) is retained.
These terms do not select a license for REforge's authored source.
