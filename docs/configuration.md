# Host preferences

F10 opens the Compact Settings menu in either game. The Display page offers
window mode (windowed or borderless desktop fullscreen), window scale (Auto or
×1–×16), scaling policy, sampling, aspect ratio, renderer, backend, VSync and a
frame limiter. CRT & Colour offers sharp
pixels or Hyllian CRT with adjustable shader controls. Colour profiles and white
points remain future work. Audio offers enable sound, master/category volumes, per-game
sound renderer, output device and focus muting.
Controls offers a shared controller stick dead zone and Alley Cat's game-specific
input response.
Navigate with Tab/Shift+Tab or the arrow keys; Enter/Space activates a control.
Arrow keys adjust sliders and numeric fields. The controller's View/Back button
opens settings, D-pad/left stick navigates, A (bottom face button) confirms,
B (right face button) goes back, and the shoulders move to the previous/next
control. Held directions repeat. Escape/B closes a dropdown first, then cancels
the menu on a fresh press. Focus is outlined and scrolls into view; disabled
settings are skipped. Down from the last option goes to Apply when it is enabled;
otherwise it goes to Reset. Tab retains the footer's visual order. Menu input
does not reach gameplay.
With Auto selected, the disabled scale slider shows the resolved windowed
multiplier. Turning Auto off starts manual scaling at that value.

The frame limiter controls presentation, independently of game simulation.
It keeps a steady deadline schedule so ordinary polling delays do not accumulate
as a lower average FPS. A missed full interval resets that schedule instead of
building up a burst of overdue frames. Rendering capacity, engine frame updates
and VSync can still limit the achieved rate; Unlimited removes the software cap.

Aspect ratio stores `original` (the default) or `square`. Each engine supplies
its native pixel grid and aspect-corrected base display size; the shared menu and
renderer use that same metadata. Global labels are Original display / Square
pixels; game labels include the resolved ratios. Both current games use 320×200
pixels on a 4:3 display, giving square pixels an 8:5 ratio. Mach 3's F3 monochrome
mode uses 640×200 pixels, so Square pixels becomes 16:5 while Original stays 4:3.
An expanded upload texture does not change the engine's native pixel geometry.
Future engines may supply other ratios without changing the shared menu.

Applying an aspect change fits the new proportions inside the existing window;
it does not resize the window or alter gameplay. A later window-scale request
uses the selected aspect's base dimensions, including fullscreen's deferred
window-size request.

Scaling policy controls the image inside that window or fullscreen area:

| Policy | Behaviour | Stored value |
| --- | --- | --- |
| Integer fit (default) | Largest whole display multiple, preserving aspect ratio with black borders | `integer` |
| Fit to window (fractional) | Largest aspect-preserving image, including fractional multiples | `fit` |
| Fill window (stretch) | Fill the entire area, ignoring aspect ratio | `stretch` |

Stretch grays out Aspect ratio and shows a pink lock with an explanatory tooltip.
Its retained value becomes effective again with either fitting policy. Stretch
uses the original display base for window-scale sizing, independently of that
inactive aspect preference. Policy changes do not request a new window size;
Auto still chooses the initial window's largest whole display multiple. Resizing,
maximizing or fullscreen provides additional space for fractional fitting/stretch.

Sampling selects Nearest (the default, source colours without blending) or Linear
(interpolated colours, softer pixel boundaries) for Sharp pixels presentation.
Hyllian CRT retains its own reconstruction filter, so Sampling is locked while
that treatment is selected; the preference is retained for Sharp pixels and its
runtime fallback. Neither fitting nor sampling changes engine frames or timing.

Settings use the desktop's UI scale independently of the game scale. A normal
window below 569 × 420 logical UI units temporarily grows towards 630 × 470,
within the display's available client area. The threshold uses rounded logical
dimensions, so 1280 × 960 pixels at 225% remains large enough. Windows already
meeting the threshold are unchanged. Closing settings restores the previous
size and, where the compositor permits, position. Applying a different scale
or window mode supersedes that restoration; temporary sizing is never saved.

Controls edit a draft. Apply activates and saves pending edits across both
Global defaults and the current game's overrides, leaving the menu open.
Cancel, Escape and F10 discard unapplied edits. Reset section clears only the
selected global section when editing Global defaults. Restore global defaults
clears only the current game's overrides for the selected section when editing
that game. Display, CRT & Colour, Audio and Controls reset independently.
Both operations remain drafts until Apply.

Game fields with an override in the draft use the theme accent text, including
explicit values matching global defaults. An amber dot marks a change compared with
saved preferences, including removal of an old override.
Hover labels/dots for global or saved values. No override badges or individual
reset buttons are displayed. Checking **Override global defaults**
enables game editing without copying the global section. The checkbox explains
that only different values are saved. Unchecking it clears that section's game
overrides in the draft; Apply saves the reset and Cancel discards it.

New game edits save only differences from the current global draft (or built-in
value for game-only options). Existing overrides are preserved when editing
other fields or global defaults, even when their values match global. Changing
a field away and back to its starting value preserves its original override
state. Explicit section resets clear that state; subsequent edits do not restore
cleared overrides. Apply never prunes unrelated overrides from the same section.
Renderer/treatment compatibility adjustments still apply. Hover details
distinguish saved overrides from inherited values.

## File and precedence

The launcher opens only when REforge receives no arguments. **Add** selects a
folder and identifies a supported game/release. Entries are saved immediately in
`library.toml`, beside the default `config.toml` described below. The library
records stable entry IDs, game/release identities, names and absolute original-data
locations; it does not copy game data or store gameplay progress. Re-adding the
same location selects its existing entry. Removing an entry never deletes files.
If files move or fail detection, the entry remains and **Locate files** lets you
select the matching game/release again. A different release should be added as a
separate entry. Missing files are checked when playing, not silently discarded.

The launcher uses the same Compact UI and supports mouse, keyboard and controller
navigation. Games run through the ordinary `--original` path in a child REforge
process while the launcher is hidden; exiting the game restores the launcher.
Existing global/game preferences apply. Launching with command-line arguments
retains the CLI behaviour and does not add library entries. `--config` and
`--no-config` continue to affect that command-line game launch; they do not
redirect the no-argument launcher's library.

Library saves use their own lock and atomic replacement, merging edits from
other launcher instances. Malformed files and unsupported schema versions are
preserved with an error, rather than overwritten by an empty library.

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
aspect_ratio = "original"
scaling_policy = "integer"
sampling = "nearest"
renderer = "auto"
backend = "auto"
vsync = true
frame_limit = "display"

[global.crt]
presentation = "hyllian"
mask_intensity = 0.35

[global.controls]
stick_dead_zone = 35

[games.alley-cat.display]
scale = 4

[games.alley-cat.controls]
control_mode = "original"

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
selects its original speaker renderer. Development-only step/static previews remain
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

## Alley Cat controls

**Alley Cat overrides → Controls → Input response** offers **Responsive** (the
built-in default) and **Original**. Responsive refreshes direction and action
requests more often, preserving movement cadence and blocking effects. Original
retains the original game's input polling. Apply changes the running game and
saves the preference; Cancel discards pending edits.

This option exists only for Alley Cat, without a Global defaults or Mach 3
counterpart. It is stored as `[games.alley-cat.controls] control_mode = "original"`;
selecting Responsive removes that override. Check **Override global defaults**
to edit it. Restoring section defaults removes only the Alley Cat
Controls override on Apply; global resets leave it intact. Reference/headless
diagnostics retain original input polling.

## Mach 3 digitized samples

Mach 3 overrides on the Audio page include **Digitized samples** immediately
below Sound renderer. It defaults to On and is stored only as
`[games.mach-3.gameplay] digitized_samples = true|false`. There is no global or
Alley Cat counterpart. On this page, unchecking **Override global defaults**
makes shared audio inherit global defaults and restores Digitized
samples to its built-in On value. Section preference controls are grayed out
until that checkbox is checked. A saved samples Off value counts as an Audio
override, so the section reopens enabled. **Restore defaults** has the same reset behavior; Apply saves
the reset and Cancel discards it. Resetting global audio leaves game overrides
intact. Host mute remains separate from the native sample policy.

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

## Controllers

SDL-recognized gamepads work in both games without Steam. Drivers, pairing and
device permissions are operating-system setup; SDL reports available devices.
The first available recognized controller is used, with a replacement selected
if it disconnects. Device-open failures warn and leave keyboard play available.

| Controller input   | Alley Cat                        | Mach 3          |
| ------------------ | -------------------------------- | --------------- |
| Left stick / D-pad | Move, jump and swim              | Steer           |
| Bottom face button | Contextual action (keyboard Alt) | Fire            |
| Start/Menu         | Host pause                       | Host pause      |
| Back/View          | Toggle settings                  | Toggle settings |

The bottom face button advances title/start screens. Alley Cat's skill question
accepts Up/Down to choose among its four initial skills and the bottom button to
confirm; another fresh press starts play after the instructions. Settings editing
currently uses keyboard/mouse. Bindings are fixed; the right stick, triggers and
remaining buttons are unassigned.

**Stick dead zone** is an integer percentage from 5 to 90, default 35. Set it in
Global defaults → Controls, or uncheck the defaults checkbox in a game's Controls
page to override it. TOML uses `[global.controls] stick_dead_zone = 35` and
`[games.<game-id>.controls] stick_dead_zone = 35`. Apply activates and saves the
value; Cancel discards edits. There is no corresponding command-line option.

Each left-stick axis activates a digital direction at that displacement and
releases below five sevenths of that threshold (approximately 25% at the default
35%). D-pad directions bypass the dead zone. Diagonals combine directions;
displacement beyond the threshold does not change movement speed. Neither engine
gets jump assistance, buffered taps or altered movement physics. Keyboard and
controller holds remain independent; conflicts follow the engine's existing
direction rules.

Focus, pause and settings boundaries discard pending host controller input.
Controls held across these boundaries, remapping or reconnection must return to
neutral before controlling the game again. Unplugging releases controller actions.
Engine adapters retain timestamped transitions on their own simulation timelines;
host catch-up does not substitute the latest stick position for earlier polls.

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

## Launcher appearance and quick options

The no-argument launcher supports Classic Dark, Soft Tonal, Warm Amber and Crisp
Light. Its palette button (Ctrl+T) previews colours; Keep, clicking elsewhere,
Escape or Ctrl+T saves and closes. Revert restores the saved theme. Settings uses
the chosen colours while retaining its own control geometry.

F2 selects Split, F3 Gallery and F4 Compact. Appearance choices are independent:

```toml
[launcher]
theme = "classic-dark"
layout = "split"
```

The first launch uses Classic Dark and Split. Unknown stored appearance values
fall back for display without rewriting them until the player changes that choice.

Quick options are declared by each engine. Choosing a value saves it immediately,
even when it currently equals its default. Its full highlight means an override
exists; dim highlighting means the option follows its default. Clicking an
explicit selection removes that override. Global edits and unrelated Settings
Apply operations preserve explicit selections, including matching defaults.

Shared options use the ordinary per-game Settings keys. Native startup choices
use the per-game launcher table, for example:

```toml
[games.alley-cat.launcher]
machine = "pcjr"

[games.mach-3.launcher]
video = "mono"
```

Native choices are passed through the ordinary engine argument parser on launcher
starts; command-line launches retain their explicit startup arguments. Engine
metadata supplies available choices, badge labels and optional palette colours.

The launcher draws Symbolic placeholders without original assets. Initials use
the first letters/numbers of the first two Unicode words, or the first two
characters of a single word, uppercased; empty/non-text names use `?`.
Engine-generated scene previews and their disk cache are not implemented yet.

Add verifies the supported release before saving its location. Relocate validates
the replacement against the same game/release. Missing entries retain their
identity and offer Locate Files. Remove asks for confirmation and removes only
the library entry. Success notices expire; errors can be dismissed. The original
files are never changed by these library operations.
