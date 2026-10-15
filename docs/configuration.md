# Display preferences

F10 opens the Compact Settings menu in either game. The Display page offers
window mode (windowed or borderless desktop fullscreen), window scale (Auto or
×1–×16), renderer, backend and VSync. Other sections remain future work.
With Auto selected, the disabled scale slider shows the resolved windowed
multiplier. Turning Auto off starts manual scaling at that value.

Controls edit a draft. Apply activates and saves pending edits across both
Global defaults and the current game's overrides, leaving the menu open.
Cancel, Escape and F10 discard unapplied edits. Reset section clears only the
global Display preferences when editing Global defaults. Restore global defaults
clears only the current game's Display overrides when editing that game.
Both operations remain drafts until Apply.

Game fields differing from the global draft use cyan text. An amber dot marks
a change compared with saved preferences, including removal of an old override.
Hover labels/dots for global or saved values. No override badges or individual
reset buttons are displayed. Unchecking Use global defaults enables game editing
without copying the global section. Only edited differences become overrides.
Values equal to global inherit. Applying global edits does not rewrite game
entries; redundant game entries are removed when game settings are edited/applied.

## File and precedence

Linux uses `$XDG_CONFIG_HOME/reforge/config.toml`, falling back to
`~/.config/reforge/config.toml`. Relative XDG_CONFIG_HOME is ignored.
Windows uses `%APPDATA%/REforge/config.toml`; macOS uses
`~/Library/Application Support/REforge/config.toml`. Windows/macOS runtime
behavior has not yet been validated. `--config <path>` selects an alternative.
`--no-config` disables reads/writes, retaining in-memory menu use.

```toml
schema_version = 1

[global.display]
window_mode = "windowed"
scale = "auto"
renderer = "auto"
backend = "auto"
vsync = true

[games.alley-cat.display]
scale = 4

[games.mach-3.display]
vsync = false
```

Defaults are windowed, Auto scale, automatic GPU-preferred rendering, automatic
backend and VSync enabled when supported. Missing keys inherit: shared defaults,
then global preferences, then game preferences. Game identities are the catalog
IDs `alley-cat` and `mach-3`, independent of engines or original data paths.
Explicit CLI values form the final temporary layer.

| Key | Values |
| --- | --- |
| window_mode | `"windowed"`, `"fullscreen"` |
| scale | `"auto"` or integer 1–16 |
| renderer | `"auto"`, `"gpu"`, `"sdl"`, `"software"` |
| backend | `"auto"`, `"vulkan"`, `"opengl"`, `"opengles2"`, `"direct3d11"`, `"direct3d12"`, `"metal"` |
| vsync | `true`, `false` |

The menu offers platform choices and compiled SDL renderer drivers. Direct3D
and Metal are not offered on Linux. A renderer change resets an unlocked,
incompatible backend to Automatic. Software disables backend selection.
Vulkan uses the GPU renderer. The separate standalone SDL Vulkan renderer is
excluded from the menu and ordinary fallback selection because repeated live
switches failed native validation; explicit requests are rejected with guidance
to select GPU/Vulkan.

`--scale auto|1..16`, `--window-mode windowed|fullscreen`,
`--renderer auto|gpu|sdl|software`, `--backend <name>` and `--vsync on|off` override
only this run. Those controls are disabled in both editing scopes. Each scope
shows its saved/draft value, with a separate line identifying the active CLI
value. Apply never copies CLI values into the file.

Preferences record requests rather than temporary runtime outcomes. Auto stays
Auto; unavailable backend/VSync requests are not replaced in the file by their
fallbacks. The active renderer/backend and VSync are reported separately.
Unavailable startup rendering warns and tries automatic rendering. Rejected
Apply reconstructs working resources and does not save its changes.

## Activation and persistence

Window scale targets the windowed drawable area with high-DPI compensation.
Fullscreen retains aspect-correct integer fitting and remembers the windowed
scale. Exclusive fullscreen and fractional scaling are not offered yet.
Renderer/backend changes rebuild the native rendering window and resources while
retaining simulation, pause/input policy and the latest CPU image. Old textures
are invalidated before renderer teardown; game textures and the font atlas are
reuploaded for the replacement. The desktop may renegotiate window placement.
VSync and ordinary mode/scale changes retain the renderer.
Compatible GPU devices remain alive across these renderer changes, including
while an ordinary or software renderer is selected. Drawing uses the selected
renderer; returning to GPU reuses its device rather than repeatedly selecting
and initializing hardware. Devices and their loader are released after their
borrowing renderers/textures. Switch requests and active results are logged.

Fallback renderers without the GPU shader interface use sharp pixels if Hyllian
was requested. Returning to a capable GPU renderer restores the requested CRT
presentation. RGB24 and RGBA32 remain supported; engine framebuffers and clocks
are unchanged.

Saving locks the file, rereads it and updates only changed keys, preserving
other games' concurrent edits, unrecognized entries and comments. Contention
reports an error for retry. A synced temporary file in the same directory is
renamed into place atomically. Opening Settings alone creates no file. Write
failure keeps the live change and dirty draft for retry, with a visible error.
The lock file may remain; its OS lock is released on process exit or crash.

Malformed TOML and unsupported schema versions warn and run with defaults while
protecting the file from replacement. Invalid individual fields warn and
inherit; valid fields still load. Original game data failures retain their
existing fatal-error behavior.

`reforge_settings` owns resolution and persistence; `reforge_sdl` owns activation
and UI. Engine adapters preserve pause/input boundaries and acknowledge runtime
success before saving. Standalone diagnostic engine binaries retain in-memory
preferences; the REforge app supplies persistent settings and the common CLI.
