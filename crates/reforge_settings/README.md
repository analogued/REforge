# Shared preferences

Pure Rust preference model, TOML editing and coordinated atomic persistence.
The `library` module stores launcher game locations in a separate `library.toml`,
using the same locked atomic writer. Entry IDs are stable game-prefixed random tokens; entry preferences live under
`[entries.<entry-id>]`, independent of detected releases and original paths.
Old library identities can still upgrade; old global/game preference tables are
ignored.
Removing an entry clears its preferences but retains saves and originals.
The module reserves the future per-entry save path without creating saves.
No SDL types, engine execution state or original game data are used.
See [configuration](../../docs/configuration.md) for the player-facing contract.

The player application installs a scoped main-thread launch context after game
detection. A shared SDL window reads that context into a session. Standalone
engine/SDL diagnostic callers get an in-memory session without filesystem access.
Drafts contain one entry patch over built-in game defaults; effective runtime
settings overlay temporary CLI values. The menu requests activation first and
commits only after the SDL runtime accepts the request. `Draft::edit_game`
preserves original key presence on an edit round trip while new values matching
built-in defaults can remain absent. Pass no original draft after a section reset.
Global game preferences and old section layouts are ignored, with no read-time
conversion or rewrite. Application-only launcher preferences remain separate.

Mach 3 also has a per-entry native startup preference for digitized samples,
kept separately from host audio keys. The engine reads it once at startup;
later menu commits persist the next-launch choice without changing execution.

Alley Cat's per-entry Enhancements preference `motion` selects Reforged (the default)
or Original scene movement, independently of its input response. Mach 3 keeps
its existing motion choices. The flat runtime patch uses distinct internal keys
for these typed preferences; both are stored in their own entry's `enhancements.motion`.
Alley Cat input response uses `enhancements.control_mode`.

Alley Cat's Controls preferences are per-entry too. Its movement mode and the
independent button choices for directional and button jumping use physical
positions, so changing controller models does not change the bindings. Defaults
retain directional jumping and the south face button for Action.

## Dependencies

Direct dependencies and their attribution are maintained in the repository's
[third-party inventory](../../THIRD_PARTY.md). Cargo.lock records the resolved
graph; release packaging generates the complete dependency notices.
