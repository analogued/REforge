# Host preferences

F10 opens the Compact Settings menu in either game. The Display page offers
window mode (windowed or borderless desktop fullscreen), window scale (Auto or
×1–×16), renderer, backend, VSync and a frame limiter. CRT & Colour offers sharp
pixels or Hyllian CRT with adjustable shader controls. Colour profiles and white
points remain future work. Audio offers enable sound, master/category volumes, per-game
sound renderer, output device and focus muting.
With Auto selected, the disabled scale slider shows the resolved windowed
multiplier. Turning Auto off starts manual scaling at that value.

Controls edit a draft. Apply activates and saves pending edits across both
Global defaults and the current game's overrides, leaving the menu open.
Cancel, Escape and F10 discard unapplied edits. Reset section clears only the
selected global section when editing Global defaults. Restore global defaults
clears only the current game's overrides for the selected section when editing
that game. Display, CRT & Colour and Audio reset independently.
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
frame_limit = "display"

[global.crt]
presentation = "hyllian"
mask_intensity = 0.35

[games.alley-cat.display]
scale = 4

[games.mach-3.display]
vsync = false

[global.audio]
master_volume = 80
mute_unfocused = true

[games.mach-3.audio]
audio_renderer = "enhanced-pcm"

[games.mach-3.gameplay]
digitized_samples = true
```

Defaults are windowed, Auto scale, automatic GPU-preferred rendering, automatic
backend, VSync enabled when supported, a display-refresh frame ceiling, and sharp
pixels. Missing keys inherit: shared defaults,
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
| frame_limit | `"display"`, `"60"`, `"120"`, `"unlimited"` |

The menu offers platform choices and compiled SDL renderer drivers. Direct3D
and Metal are not offered on Linux. A renderer change resets an unlocked,
incompatible backend to Automatic. Software disables backend selection.
Vulkan uses the GPU renderer. The separate standalone SDL Vulkan renderer is
excluded from the menu and ordinary fallback selection because repeated live
switches failed native validation; explicit requests are rejected with guidance
to select GPU/Vulkan.

`--scale auto|1..16`, `--window-mode windowed|fullscreen`,
`--renderer auto|gpu|sdl|software`, `--backend <name>`, `--vsync on|off`,
`--frame-limit display|60|120|unlimited` and `--presentation pixels|hyllian` override
only this run. Those controls are disabled in both editing scopes. Each scope
shows its saved/draft value, with a separate line identifying the active CLI
value. Apply never copies CLI values into the file.

Preferences record requests rather than temporary runtime outcomes. Auto stays
Auto; unavailable backend/VSync requests are not replaced in the file by their
fallbacks. The active renderer/backend and VSync are reported separately.
Unavailable startup rendering warns and tries automatic rendering. Rejected
Apply reconstructs working resources and does not save its changes.

## Frame limiter

The frame limiter caps shared presentation, including menu rendering. Display
refresh rate queries the window's current display and falls back to 60 fps if
unknown. It is a ceiling: engine redraw cadence can be lower, and Unlimited does
not force extra game frames. VSync remains independent; disabling the cap does
not disable VSync. When VSync already satisfies the cap, no additional timer wait
is added. Rendering and VSync waits count toward the interval; stalls do not
accumulate presentation debt. Native simulation clocks, catch-up and deliberate
game waits retain their existing behavior. VSync and timer granularity can make
the observed frame rate lower than the chosen ceiling.

## CRT & Colour

CRT preferences use `[global.crt]` and `[games.<game-id>.crt]`, with the same
sparse inheritance and Apply/Cancel rules as Display. `presentation` accepts
`"pixels"` (default) or `"hyllian"`. Fine adjustments is a shared collapsible
section for the selected treatment's controls, closed by default. Hyllian exposes
its horizontal filter and shader sliders there. Sharp pixels keeps the section
visible but disabled. Switching treatments preserves saved parameter values;
expanding/collapsing changes only visibility, while parameter edits use Apply.
Hyllian remains visible but disabled for incompatible renderer/backend choices.
Applying such a combination resets its treatment to Sharp pixels and saves that
choice. Switching back to a compatible renderer requires selecting Hyllian again.
Global edits reconcile global defaults and this game's inherited choices; game
edits leave global defaults unchanged. Explicit compatible game overrides and
other games' saved overrides are preserved. Temporary CLI restrictions do not
rewrite preferences. Automatic GPU initialization failure still falls back to
pixels without changing the saved treatment. Modern menu overlays are always
drawn after the shader.

| CRT key | Range / choices | Initial value |
| --- | --- | --- |
| horizontal_filter | `"hermite"`, `"catmull-rom"` | `"hermite"` |
| scanline_strength | 0.00–1.00 | 0.65 |
| beam_min | 0.10–2.00, ≤ beam_max | 0.70 |
| beam_max | 0.10–2.00, ≥ beam_min | 1.00 |
| colour_boost | 1.00–3.00 | 2.00 |
| sharpness | 1.00–5.00 | 1.00 |
| mask_intensity | 0.00–1.00 | 0.35 |
| anti_ringing | 0.00–1.00 | 1.00 |
| input_gamma | 1.00–4.00 | 2.40 |
| output_gamma | 1.00–4.00 | 2.40 |

Numeric controls use increments of 0.01. Initial values preserve the earlier fixed
Hyllian treatment. Beam widths control brightness-dependent scanline thickness;
they do not change the 200 source-row beam cycles. The RGB aperture mask retains
its physical two-pixel pitch. Gamma controls belong to the shader, not a separate
monitor profile or white-point conversion. Invalid fields inherit; invalid beam
pairs discard the pair's overrides at that scope and warn.

## Audio

Audio preferences use `[global.audio]` and `[games.<game-id>.audio]` with the
same draft, sparse overrides and section reset rules. Sound renderer selection
is enabled in game overrides for engines/runs that provide audio. Shared levels,
devices and focus defaults can be edited globally even from a silent game.

| Key | Values | Built-in default |
| --- | --- | --- |
| sound_enabled | `true`, `false` | `true` |
| master_volume | Integer 0–100 | 80 |
| music_volume | Integer 0–100 | 100 |
| speech_volume | Integer 0–100 | 100 |
| effects_volume | Integer 0–100 | 100 |
| audio_renderer | `"default"`, `"speaker"`, `"enhanced-pcm"` | `"default"` |
| output_device | `"default"` or exact detected device name | `"default"` |
| mute_unfocused | `true`, `false` | `true` |

Mach 3's normal timed playback supports PC speaker and Enhanced PCM; Game default
selects its original speaker renderer. Step/static/custom-period previews remain
silent. Alley Cat supports PC Speaker playback in its normal timed runner and
animated title, with Music and Effects categories; its uniform-step diagnostic
runner remains silent. Engines declare supported Music, Speech and Sound effects groups;
unsupported game groups remain visible but disabled. Global category defaults
can still be edited. Latency controls and stereo processing are not implemented.

Category levels are applied in synthesis, then Master controls the final host
stream. All category levels at 100% preserve the original PCM exactly. Master
defaults to 80% to leave upward adjustment to the existing full level at 100%;
existing explicitly saved values are retained. These controls are independent of
desktop/system volume: neither is read back into preferences or modified by them.
Native single-speaker interruptions/priorities are preserved; lowering an effect
does not resume music that the original game interrupted. Later category changes
use a five-millisecond gain ramp, without resetting synthesis/filter history.

Enable sound and Master volume operate on host output. Native sound routines,
device counters, random behavior and the engine's timeline continue unchanged;
Mach 3's original F2 and Alley Cat's original Ctrl+S sound controls remain independent. Focus muting sets stream
gain to zero without pausing simulation or retaining a backlog to replay later.
Native/host/menu pauses retain their existing scheduling behavior.

The device selector refreshes when Audio is opened in a new settings session and
on SDL device notifications. Preferences retain names rather than session-only
SDL IDs. System default follows SDL's default playback route; selecting another
device affects only REforge. Identical device names are ambiguous: select System
default in that case. A named device missing at startup warns and tries System
default while preserving the preference. A failed live device request retains
the previous endpoint and leaves the draft unsaved for correction/retry.

Master/focus changes use SDL stream gain. Enable/device/renderer/category changes discard
old queued presentation samples and refill from continuing engine execution.
Audio already handed to the host device cannot be recalled. Changing renderer
preserves observed sample/speaker phase and the existing resampling state.
Unavailable audio does not prevent unrelated Display/CRT changes.

`--audio-renderer speaker|enhanced-pcm` remains a temporary Mach 3 override and
locks its menu control for that run. It is never saved into preferences.
Latency remains the existing buffering policy, and output remains mono; the
mockup's Latency and Keep original mono controls are intentionally absent.

## Mach 3 digitized samples

Mach 3 overrides on the Audio page include **Digitized samples** immediately
below Sound renderer. It defaults to On and is stored only as
`[games.mach-3.gameplay] digitized_samples = true|false`. There is no global or
Alley Cat counterpart. It remains editable when host sound is disabled or shared
audio inherits global defaults. Restoring shared audio defaults leaves it alone;
select On to restore its built-in value.

This is a native game option, separate from host mute, volumes and rendering.
Off skips the original digitized playback calls and their time/input effects:
intro speech/music and tail, pre-game, completion, and guarded Game Over/High
Score samples. Procedural effects and the independent native F2 switch remain.
The chosen value persists across new games and returned intros in the same run.
Apply saves it for the **next application launch**; it never interrupts playback
or changes the running game's sample policy. Cancel discards pending edits.

`--digitized-samples on|off` temporarily overrides the preference and locks its
menu control for that run, without saving the override. It applies only to
Mach 3; other detected games warn and ignore it. Host output settings still apply
live. The native option works with either speaker or Enhanced PCM rendering and
is distinct from setting Music or Speech volume to zero.

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
