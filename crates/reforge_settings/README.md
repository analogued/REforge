# Shared preferences

Pure Rust preference model, TOML editing and coordinated atomic persistence.
The `library` module stores launcher game locations in a separate `library.toml`,
using the same locked atomic writer. It keeps no game saves or asset copies.
No SDL types, engine execution state or original game data are used.
See [configuration](../../docs/configuration.md) for the player-facing contract.

The player application installs a scoped main-thread launch context after game
detection. A shared SDL window reads that context into a session. Standalone
engine/SDL diagnostic callers get an in-memory session without filesystem access.
Drafts contain sparse global/game patches; effective runtime settings overlay
temporary CLI values. The menu requests activation first and commits only after
the SDL runtime accepts the request. A present game key is an explicit override,
even if it matches global. `Draft::edit_game` preserves the original key on an
edit round trip while saving new differences sparsely; pass no original draft
after an explicit section reset. Equality with global does not trigger cleanup
of untouched overrides.

Mach 3 also has a per-game native startup preference for digitized samples,
kept outside shared/global host audio keys. The engine reads it once at startup;
later menu commits persist the next-launch choice without changing execution.

## Dependencies

Direct dependencies and their attribution are maintained in the repository's
[third-party inventory](../../THIRD_PARTY.md). Cargo.lock records the resolved
graph; release packaging generates the complete dependency notices.
