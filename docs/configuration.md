# Host preferences

F10 opens the Compact Settings menu during graphical play in each supported game.
The Display page offers window mode (windowed or borderless desktop fullscreen),
window scale (Auto or ×1–×16), scaling policy, sampling, aspect ratio, renderer,
backend, VSync and a frame limiter. Mach 3 also offers scene and player craft
movement settings. CRT & Colour offers sharp pixels or Hyllian CRT with adjustable
shader controls. Colour profiles and white points remain future work. Audio
offers enable sound, master/category volumes, per-game sound renderer, output
device and focus muting. Controls offers a shared controller stick dead zone and
Alley Cat's input response and controller layout templates, with diagrams.
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

During graphical play, the mouse cursor hides after two seconds without mouse
activity, in both windowed and fullscreen modes. Moving or clicking the mouse
shows it again. Settings and file dialogs keep it visible; leaving the game
window or returning to the launcher restores it.

The frame limiter controls presentation, independently of game simulation.
It keeps a steady deadline schedule so ordinary polling delays do not accumulate
as a lower average FPS. A missed full interval resets that schedule instead of
building up a burst of overdue frames. Rendering capacity, engine frame updates
and VSync can still limit the achieved rate; Unlimited removes the software cap.

Aspect ratio stores `original` (the default) or `square`. Each engine supplies
its native pixel grid and aspect-corrected base display size; the shared menu and
renderer use that same metadata. Global labels are Original display / Square
pixels; game labels include the resolved ratios. Alley Cat and Mach 3's colour
mode use 320×200 pixels on a 4:3 display, giving square pixels an 8:5 ratio.
Mach 3's monochrome mode and BTZ's text raster use 640×200 pixels, so
Square pixels becomes 16:5 while Original stays 4:3.
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
For an unregistered game launched with `--original`, game overrides apply only
to the current run; add the game to the launcher library to retain them. Global
defaults still save normally. With `--no-config`, all edits are temporary.
Next-launch-only settings such as Mach 3's Digitized samples require a registered
game with configuration enabled and are disabled otherwise.
Cancel, Escape and F10 discard unapplied edits. **Reset section** clears only the
selected global section when editing Global defaults. When editing a game, the
button is **Restore defaults** on Controls, BTZ's Display page and Mach 3's
Display/Audio pages, or **Restore global defaults** on the other pages. It clears
only that game's overrides for the selected section: shared fields inherit global
preferences, and game-only fields return to their built-in defaults. Display,
CRT & Colour, Audio and Controls reset independently. Resets remain drafts until
Apply.

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
records an entry ID, game/release IDs, title, canonical directory and original-data
location. One directory has one entry. The same release in different directories
has separate entries, preferences and a reserved save namespace. Directory aliases,
including symlinks, select the same entry. Re-adding a directory selects its
existing entry.

Games follow their entry order in `library.toml`. Closing the launcher saves the
selected entry ID as `selected_entry` under `[launcher]` in `config.toml`.
The next launch restores that selection, including a game with missing files.
If the entry was removed, the first remaining entry is selected instead.

The launcher fully verifies all entries at startup. Add and Relocate verify the
selected inputs and reuse that result when it matches the saved installation.
If duplicate Add retains a different original-file anchor, that saved anchor is
checked separately. Library, settings and game-return updates retain unchanged
entries without rehashing their originals; new or changed installation records
are verified. Title and order edits do not require content verification.

The displayed verification status records the last check, rather than proving
that files cannot have changed since then. Play always performs a fresh full
check, and a failed check updates that entry without scanning the other games.
Use Locate Files to recheck an unavailable installation after restoring its files.

Detection verifies every required file by size and complete BLAKE3 hash.
Expected relative paths provide the fast path: exact spelling first, then other
ASCII case variants alphabetically. Unresolved files are sought under any name
or extension in the game root and the file-containing subdirectories named in
that release's catalog. Files can move between those locations. Other folders
and unlisted descendants are not searched; traversing an intermediate directory
to reach a catalog folder does not add its files to the search. Directory names
also accept ASCII case variations. Missing catalog folders are harmless when
the required files are found in another permitted location.

Wrong-size files are never hashed. Valid expected paths are reserved before
filling gaps, and each required record needs a distinct file. Duplicate identical
copies use a deterministic selection. Actual verified paths are passed to the
engine without renaming or copying the files.

Launching from the library performs full verification when Play is pressed.
The game process reuses that result through a one-use handoff containing the
release and resolved file paths. It checks file sizes and modification times
before accepting the handoff; an unavailable or invalid handoff falls back to
full verification. Direct command-line launches perform full verification.

Multiple matching releases remain ambiguous, including a renamed release beside
a normally named one. Selecting a file requires that exact file to satisfy one
of the release's records, with the remaining files in the permitted locations.
For a file inside a catalog subdirectory, REforge also checks the corresponding
game root. Multiple complete roots remain ambiguous; select the intended root
directory instead. A directory selection always defines the root and does not
search ancestors. A file whose contents are shared by multiple complete releases
cannot distinguish them. Explicit file selections are retained in library
entries for later launches, while preferences remain attached to the game root.

Entry IDs combine the game ID and six random lowercase letters/digits, such as
`alley-cat-m7k4p9`. They use operating-system randomness with collision checks
against the library, retained entry preferences and save-folder names. There is
no separate UUID or sequence counter. IDs remain unchanged when relocating or
switching to a compatible release. Keep IDs intact when manually editing files;
changing one creates a different profile. The stored title starts with the catalog
name and can be edited without changing identity or save storage.

```toml
schema_version = 2

[[entries]]
entry_id = "alley-cat-m7k4p9"
game_id = "alley-cat"
release_id = "alley-cat-dos-06ff7fe280"
title = "Alley Cat"
directory = "/Games/AlleyCat"
original = "/Games/AlleyCat/CAT.EXE"
```

Relocate verifies the replacement using the detection catalog. It accepts the
same release or another release with the same game ID and explicit
`compatibility_id`, meaning compatible saves and persistent option semantics.
Matching hashes, titles or platforms alone never establish compatibility. The
stored release ID and paths change; entry identity, overrides and its reserved
save namespace remain intact. Saved files are not moved. BTZ games' default
save directory follows the selected game-data directory. A directory already
registered to another entry is rejected.
Files changed to another release in place require explicit Relocate as well.
Unknown/unsupported releases remain rejected.

Remove asks for confirmation and removes the library record and that entry's
preferences. Original files and saves stay on disk. Manually removing a library
record can leave unreferenced preferences/saves; adding the game again allocates
a fresh ID rather than reclaiming them.

Saving is engine-specific. BTZ implements native bookmarks and text
transcripts. Relative filenames default to the directory containing `ADISK.OBJ`;
`--save-dir` changes that base, and absolute filenames select their own location.
See the [BTZ guide](games/btz.md#bookmarks-and-transcripts) for prompts,
overwrite behavior and fixed-file options. Alley Cat and Mach 3 currently do not
provide gameplay save/restore.

The application reserves a layout of `saves/<entry-id>/` under
`$XDG_DATA_HOME/reforge` (default `~/.local/share/reforge`) on Linux,
`%APPDATA%/REforge` on Windows and
`~/Library/Application Support/REforge` on macOS. No save folders are created by
adding a game. BTZ uses its native format and game-data-relative default,
not this reserved namespace. Future REforge-owned save formats should record
format version and compatibility group rather than require an exact release hash.

Schema-1 numeric library IDs migrate automatically to stable string IDs. Old
`[games.<game-id>]` preferences, including native launcher choices and explicit
pins equal to globals, are copied to every existing entry for that game. Those
legacy sections are then removed; unmapped game sections are preserved until
an entry for that game exists. The migration is restartable if interrupted
between the library and preferences writes. Unknown entry fields and comments
are retained. Repair malformed preferences before completing a pending migration.

The launcher uses the same Compact UI and supports mouse, keyboard and controller
navigation. Games run through the ordinary `--original` path in a child REforge
process while the launcher is hidden; exiting the game restores the launcher.
Existing global/entry preferences apply. Launching with command-line arguments
does not add library entries. A detected directory matching an existing entry
uses that entry's preferences and launcher choices; otherwise it uses global
and built-in defaults. Explicit CLI values take precedence. Native launcher
choices are applied by windowed (SDL-enabled) builds. `--config` and
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
schema_version = 2

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

[entries.alley-cat-m7k4p9.display]
scale = 4

[entries.alley-cat-m7k4p9.controls]
control_mode = "original"

[entries.mach-3-q8n2v5.display]
vsync = false

[global.audio]
master_volume = 80
mute_unfocused = true

[entries.mach-3-q8n2v5.audio]
audio_renderer = "enhanced-pcm"

[entries.mach-3-q8n2v5.gameplay]
digitized_samples = true
```

Defaults are windowed, Auto scale, automatic GPU-preferred rendering,
automatic backend, VSync enabled when supported, a display-refresh frame
ceiling, and sharp pixels. Missing keys inherit: shared defaults, then
global preferences, then entry preferences. Catalog game IDs (`alley-cat`,
`mach-3`, `mindwheel`, `essex`, `brimstone`, `breakers`) identify games; entry IDs
identify individual configured copies.
The example suffixes above are illustrative; use the IDs from your library.
Explicit CLI values form the final temporary layer.

| Key | Values |
| --- | --- |
| window_mode | `"windowed"`, `"fullscreen"` |
| scale | `"auto"` or integer 1–16 |
| aspect_ratio | `"original"`, `"square"` |
| scaling_policy | `"integer"`, `"fit"`, `"stretch"` |
| sampling | `"nearest"`, `"linear"` |
| renderer | `"auto"`, `"gpu"`, `"sdl"`, `"software"`; `"terminal"` for BTZ entries only |
| backend | `"auto"`, `"vulkan"`, `"opengl"`, `"opengles2"`, `"direct3d11"`, `"direct3d12"`, `"metal"` |
| vsync | `true`, `false` |
| frame_limit | `"display"`, `"60"`, `"120"`, `"unlimited"` |
| file_cursor | `"pulsing-underline"` (default), `"blinking-underline"`, `"steady-block"`; BTZ entries only |

BTZ's `file_cursor` belongs in `[entries.<entry-id>.display]` and cannot be set
globally. In Settings it appears under **Text interface**, at the end of
the game's Display page as **Cursor style**. It affects command input and file
dialogs in both frontends. Terminal play uses a steady underline for Pulsing
underline and delegates the other shapes and blink rate to the terminal. See the
[BTZ controls](games/btz.md#commands-and-controls).

The menu offers platform choices and compiled SDL renderer drivers. Direct3D
and Metal are not offered on Linux. A renderer change resets an unlocked,
incompatible backend to Automatic. Software disables backend selection.
Vulkan uses the GPU renderer. The separate standalone SDL Vulkan renderer is
excluded from the menu and ordinary fallback selection because repeated live
switches failed native validation; explicit requests are rejected with guidance
to select GPU/Vulkan.

`--scale auto|1..16`, `--window-mode windowed|fullscreen`,
`--renderer auto|gpu|sdl|software|terminal`, `--backend <name>`, `--vsync on|off`,
`--frame-limit display|60|120|unlimited` and `--presentation pixels|hyllian` override
only this run. Those controls are disabled in both editing scopes. Each scope
shows the active CLI value in the locked control. Apply never copies CLI values
into the file; the saved preferences remain available on the next ordinary run.

Terminal is a BTZ-only startup renderer. It uses the process's attached
input/output terminal and does not open a terminal application or an SDL game
window. It is available through `--renderer terminal`, including builds without
SDL, or in per-entry Settings when a terminal is attached. It cannot be a global
default or replace a running SDL game window. Other games warn and ignore the
Terminal CLI choice.

Window, backend and CRT preferences are retained and ignored during terminal
play. Their controls are disabled while Terminal is selected; explicit CLI
graphics overrides warn and are ignored for that run. Selecting Graphical in
the BTZ launch options restores the graphical renderer preference. See
[BTZ launch instructions](games/btz.md#start-playing).

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

CRT preferences use `[global.crt]` and `[entries.<entry-id>.crt]`, with the same
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

Audio preferences use `[global.audio]` and `[entries.<entry-id>.audio]` with the
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

BTZ's Ctrl+G keyboard bell uses PC-speaker synthesis in graphical play and
belongs to Sound effects. Master volume, Effects volume, output device and focus
mute apply. Its terminal frontend sends BEL and respects sound-off or zero
Master/Effects volume; the terminal controls the actual sound, flash and volume.

Category levels are applied in synthesis, then Master controls the final host
stream. All category levels at 100% preserve the original PCM exactly. Master
defaults to 80% to leave upward adjustment to the existing full level at 100%;
existing explicitly saved values are retained. These controls are independent of
desktop/system volume: neither is read back into preferences or modified by them.
Native single-speaker interruptions/priorities are preserved; lowering an effect
does not resume music that the original game interrupted. In Alley Cat and
Mach 3's continuous synthesis, later category changes use a five-millisecond gain
ramp without resetting synthesis/filter history. BTZ applies Effects
changes to subsequent bells and discards pending bell presentation on a change.

Enable sound and Master volume operate on host output. Native sound routines,
device counters, random behavior and the engine's timeline continue unchanged;
Mach 3's original F2 and Alley Cat's original Ctrl+S sound controls remain
independent. Focus muting sets stream gain to zero without pausing simulation
or retaining a backlog to replay later.
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
Audio buffering uses the existing latency policy, and output remains mono.

## Alley Cat controls

**Alley Cat overrides → Controls → Input response** offers **Responsive** (the
built-in default) and **Original**. Responsive refreshes direction and action
requests more often, preserving movement cadence and blocking effects. Original
retains the original game's input polling. Apply changes the running game and
saves the preference; Cancel discards pending edits.

This option exists only for Alley Cat, without a Global defaults or Mach 3
counterpart. It is stored as
`[entries.alley-cat-m7k4p9.controls] control_mode = "original"`; selecting Responsive
removes that override. Check **Override global defaults** to edit it.
**Restore defaults** resets this entry's whole Controls section on Apply,
including input response, controller layouts and the stick dead zone override.
Global resets leave game overrides intact. Reference/headless diagnostics retain
original input polling.

## Mach 3 motion

**Mach 3 overrides → Display → Scene movement** offers **Reforged** (the
built-in default) and **Original**. Reforged smooths scene movement while
keeping the original mountain artwork and updates. Original scene movement shows
**Player craft movement**, with **Enhanced** for craft/shadow-only smoothing
and **Original** for native placements and publication cadence. Gameplay,
collisions, randomness, PIT execution and audio timing are unchanged. Apply
changes the running presentation and saves the preference; Cancel discards draft
changes.

This option is game-specific and has no global or other-game counterpart. Its
storage is `[entries.mach-3-q8n2v5.display] motion = "reforged"|"enhanced"|"original"`.
`reforged` is the built-in default, so selecting it normally removes the override.
An existing saved `scene` value is automatically renamed to `reforged` when its
Mach 3 entry loads, preserving the selected behavior.
Existing explicit `enhanced` values keep craft/shadow-only interpolation;
`original` keeps native presentation. The menu remembers the last player craft
choice while switching scene movement during a session.

Check **Override global defaults** to edit the section. Resetting the game Display
section restores Reforged scene movement on Apply; global Display resets and
other game sections leave the motion override intact.
`--motion reforged|enhanced|original` is a temporary, locked override and does not
change the saved value.

## Mach 3 digitized samples

Mach 3 overrides on the Audio page include **Digitized samples** immediately
below Sound renderer. It defaults to On and is stored only as
`[entries.mach-3-q8n2v5.gameplay] digitized_samples = true|false`. There is no global or
Alley Cat counterpart. On this page, unchecking **Override global defaults**
makes shared audio inherit global defaults and restores Digitized
samples to its built-in On value. Section preference controls are grayed out
until that checkbox is checked. A saved samples Off value counts as an Audio
override, so the section reopens enabled. **Restore defaults** has the same reset
behavior; Apply saves the reset and Cancel discards it. Resetting global audio
leaves game overrides intact. Host mute remains separate from the native sample
policy.

This is a native game option, separate from host mute, volumes and rendering.
Off skips the original digitized playback calls and their time/input effects:
intro speech/music and tail, pre-game, completion, and guarded Game Over/High
Score samples. Procedural effects and the independent native F2 switch remain.
The chosen value persists across new games and returned intros in the same run.
Apply saves it for the **next application launch**; it never interrupts playback
or changes the running game's sample policy. Cancel discards pending edits.
Without a library entry, or with `--no-config`, this control is disabled because
the choice cannot be retained for the next launch. The CLI override below can
still choose the sample policy for the current run.

`--digitized-samples on|off` temporarily overrides the preference and locks its
menu control for that run, without saving the override. It applies only to
Mach 3; other detected games warn and ignore it. Host output settings still apply
live. The native option works with either speaker or Enhanced PCM rendering and
is distinct from setting Music or Speech volume to zero.

## Controllers

SDL-recognized gamepads work during graphical play without Steam. Drivers,
pairing and device permissions are operating-system setup; SDL reports available
devices.
The first available recognized controller is used, with a replacement selected
if it disconnects. Device-open failures warn and leave keyboard play available.
The table shows the default bindings.

| Controller input   | Alley Cat                        | Mach 3          | BTZ games |
| ------------------ | -------------------------------- | --------------- | --------- |
| Left stick / D-pad | Move, jump and swim              | Steer           | Navigate menus |
| Bottom face button | Contextual action (keyboard Alt) | Fire            | Confirm menu control |
| Start/Menu         | Host pause                       | Host pause      | Unassigned |
| Back/View          | Toggle settings                  | Toggle settings | Toggle settings |

The bottom face button advances Alley Cat and Mach 3's title/start screens.
BTZ games use controllers for Settings and file-dialog navigation; game commands
and filename text still use the keyboard. B cancels a filename prompt and the
shoulders change focus. Terminal play uses its terminal's keyboard input.

Alley Cat's skill question accepts Up/Down to choose among its four initial
skills and the bottom button to confirm; another fresh press starts play after
the instructions. Settings supports
keyboard, mouse and controller navigation. Menu bindings are fixed; the right
stick and triggers are unassigned.

Alley Cat's **Controls → Controller layout** offers two movement modes:
**Directional jump** retains Up to jump and Down to drop, with one configurable
Action button. **Button jump** keeps land movement on left/right and assigns
Jump/Drop and Action to one of four button pairs. Hold Down before pressing Jump
to drop; changing the stick direction during a held press does not switch that
press between jumping and dropping. Swimming always uses all four directions.
The diagram follows the connected controller family; without a recognized family
it shows Xbox button positions. These templates do not change keyboard, title or
menu controls, or Mach 3's bindings.

Enable **Override global defaults** on Alley Cat's Controls page to edit the
templates. Each movement mode remembers its button layout. Apply saves them
under `[entries.<entry-id>.controls]` as `controller_movement` (`directional` or
`button`), `controller_action` (a physical face-button position), and
`controller_buttons` (a pair such as `west-jump-south-action`). They have built-in
defaults rather than global counterparts. Restore defaults resets this entry's
Controls section on Apply; Cancel discards unapplied changes. There are no
corresponding command-line arguments.

**Stick dead zone** is an integer percentage from 5 to 90, default 35. Set it in
Global defaults → Controls, or check **Override global defaults** in a game's
Controls page to override it. TOML uses `[global.controls] stick_dead_zone = 35` and
`[entries.<entry-id>.controls] stick_dead_zone = 35`. Apply activates and saves the
value; Cancel discards edits. There is no corresponding command-line option.
Launcher navigation uses the global value, including after Apply; per-game
overrides affect that game's controls.

Each left-stick axis activates a digital direction at that displacement and
releases below five sevenths of that threshold (approximately 25% at the default
35%). D-pad directions bypass the dead zone. Diagonals combine directions;
In Alley Cat and Mach 3, displacement beyond the threshold does not change
movement speed. Neither gets jump assistance, buffered taps or altered movement
physics. Keyboard and controller holds remain independent; conflicts follow
the engine's existing direction rules.

Focus, pause and settings boundaries discard pending host controller input.
Controls held across these boundaries, remapping or reconnection must return to
neutral before controlling the game again. Unplugging releases controller actions.
Alley Cat and Mach 3 retain timestamped transitions on their own simulation
timelines; host catch-up does not substitute the latest stick position for
earlier polls.

## Activation and persistence

Window scale targets the windowed drawable area with high-DPI compensation.
Fullscreen uses the selected scaling policy and remembers the windowed scale.
Integer fitting is the default; fractional fitting and stretching are also
available. Exclusive fullscreen is not offered.
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

Shared options use the ordinary per-entry Settings keys. Native startup choices
use the entry's launcher table, for example:

```toml
[entries.alley-cat-m7k4p9.launcher]
machine = "pcjr"

[entries.mach-3-q8n2v5.launcher]
video = "mono"
```

Native choices are passed through the ordinary engine argument parser on launcher
starts; command-line launches retain their explicit startup arguments. Engine
metadata supplies available choices, badge labels and optional palette colours.

Mach 3 quick options are ordered Video, Display, Window, then Audio. Digitized
samples are configured through the full Audio settings or the command line;
editing quick options preserves the per-entry sample preference.
Preview badges follow the same option order, omitting choices without badge text.
PC speaker contributes no badge; Enhanced PCM contributes `PCM`, following `CRT`
when CRT treatment is enabled.

BTZ's quick options are Renderer (Graphical/Terminal), Display and Window.
Terminal disables Display and Window controls while retaining their values;
its preview badge replaces graphical option badges for that mode.

The player-facing data has two owners:

| Owner | Data |
| --- | --- |
| [Detection catalog](../app/src/detection/catalog.rs) | Shared game/editorial data, edition facts, reusable specs profiles, detection fingerprints and compatibility groups. |
| Engine `launcher` module | Available launch options, their defaults and storage keys, labels and preview badges, mode palettes, display geometry, preview variants, recipe revision and native image generation. |

The catalog has four linked record types:

| Record | Data |
| --- | --- |
| `GameMetadata` | Typed game identity, canonical title, genres and tagline. |
| `Edition` | Internal edition ID, game reference, platform, optional title replacement, year, developers, publishers, optional display label and specs profile reference. |
| `SpecsProfile` | Internal profile ID and video/audio feature lists. |
| `Release` | Existing detected-release ID, edition reference, compatibility group, and filename/size/BLAKE3 verification records with SHA-256 identity metadata. |

Multiple detected distributions can reference one edition, for example when
only an executable changes. Separate editions can reference the same game and
specs profile while keeping their dates and credits explicit. Relationships use
direct Rust references; edition and profile IDs identify the named catalog
records without requiring string lookups. Game IDs and existing detected-release
IDs remain unchanged in libraries, configuration, saves and preview identities.

The current chains are:

| Detected release ID | Edition ID | Game ID | Specs profile ID |
| --- | --- | --- | --- |
| `alley-cat-dos-06ff7fe280` | `alley-cat-dos` | `alley-cat` | `alley-cat-dos` |
| `mach-3-dos-5f6ec0fc8b` | `mach-3-dos` | `mach-3` | `mach-3-dos` |
| `mindwheel-dos-2f53a47931` | `mindwheel-dos` | `mindwheel` | `btz-dos-text` |
| `essex-dos-2ba69b4250` | `essex-dos` | `essex` | `btz-dos-text` |
| `brimstone-dos-0020ff40a4` | `brimstone-dos` | `brimstone` | `btz-dos-text` |
| `breakers-dos-acb87c72b9` | `breakers-dos` | `breakers` | `btz-dos-text` |

Required file records use canonical relative paths with `/` separators, such as
`DATA/ROOMS.DAT`. Their parent directories define the permitted search locations
for that release, alongside the game root. All current releases use root files.

All current editions have platform `Platform::Dos`. Edition IDs name catalog
groupings; the precise commercial edition/version designations remain unverified.
Edition, specs and compatibility IDs have separate namespaces. Each current
detected release uses its game's `*-dos` compatibility ID, matching the edition
ID; the four BTZ editions share the `btz-dos-text` specs profile. Sharing edition
or specs records does not establish save/configuration compatibility: each
detected release retains its explicit compatibility group.

Detected-release IDs use `<game-id>-<platform>-<10 hex digits>`. The suffix is
derived from a stable catalogued identity manifest, normally all required files.
An existing release can retain its original identity-file selection when required
verification expands: Mach 3 retains its original seven identity files while all
eighteen playback files are verified. DOS filenames use
ASCII case-insensitive normalization of the canonical catalog names; non-ASCII
characters remain exact. Physical aliases found during detection do not enter
this fingerprint, so renaming files does not change the detected-release ID.
File-list order and hexadecimal SHA-256 letter casing do not affect the identity.
Other platforms must select their own catalog filename case semantics.
Original file contents remain byte-exact and are verified with the full BLAKE3 hash.
Absolute installation paths and editorial/edition/specs metadata are excluded.

SHA-256 file fingerprints remain catalog metadata for stable release and preview
identities. Record both hashes from the same original bytes; normal detection
reads game files only for BLAKE3 verification. Changing the verification algorithm
requires no migration of library entries, preferences, saves or cached previews.

The version-one fingerprint encoding is fixed: the ASCII marker
`reforge-detected-release-v1` followed by a NUL byte, the file count as an unsigned
64-bit little-endian integer, then records sorted by normalized UTF-8 filename.
Each record contains its filename byte length as the same integer type, filename
bytes, file size as the same integer type, and 64 lowercase ASCII SHA-256 hex
characters. Hash this encoding with SHA-256 and take the first ten hex digits.
The game ID and platform form the readable prefix outside the digest.

IDs are assigned in the catalog. [Authoring checks](../app/src/detection/identity.rs)
validate normalization and reject duplicate IDs, including shortened-fingerprint
collisions. Runtime detection uses the assigned IDs and full file hashes.

Years are optional numbers. Credits, genres and feature summaries are separate
lists; unknown facts use `None` or empty lists. An edition uses its game's title
unless it explicitly supplies a replacement. Its optional `label` is display text,
independent of its ID. `tagline` is the short shared editorial line, such as
“A small cat, a very busy neighbourhood.” There is no separate description field.
Split details keep the original single-line tagline spacing, including when the
tagline is absent. A second line uses the existing gaps above and below without
moving the credits, specs or Play button. Taglines have no ellipsis or tooltip.
Keep catalog taglines short enough to fit within two lines at the launcher's
630 × 470 logical UI unit minimum. A rendering regression check enforces this
across themes and representative display densities; tagline length does not
raise the runtime window minimum.

Split sidebar entries and Gallery cards show year and developer only. Split
details and Compact entries can include publisher and genres; identical
developer/publisher lists appear once. The launcher formats lists and separators,
omitting absent facts. Video/audio feature summaries describe the catalogued
release as supported by REforge, including explicitly named enhancements; active
option values and palette swatches come from the engine instead.

Split sidebar, Gallery and Compact titles use an ellipsis when they exceed the
available width. Split details show the full title, wrapping when needed.

A library entry has its own display title, initially copied from the detected
edition's resolved title and editable independently in the library file. Other
metadata is read through the catalog links and is not duplicated in user
configuration. Metadata edits leave native preview cache identities unchanged.
The local development `project.toml` inventories are not runtime catalog inputs.

The launcher loads cached static engine scene previews in a background worker
and generates missing or corrupt images from verified originals. It warms the
available visual variants (CGA/PCjr for Alley Cat, colour/monochrome for Mach
3). Mach 3 uses its first splash screen. Alley Cat uses its intro, advanced
with fixed seed `0xcafe` and the normal virtual startup loop until the cat sits
on the fence with the SynSoft footer visible. BTZ uses the game's original
title screen, with the loading label, continue prompt and input cursor removed
from the preview copy. Its single monochrome image is shared by graphical and
terminal launch choices. Preview generation has no effect on the player's
startup seed or session. Native RGBA pixels, engine-supplied display geometry
and colour model are cached together in PNG files under:

- Linux: `$XDG_CACHE_HOME/reforge/previews`, or `~/.cache/reforge/previews`.
- macOS: `~/Library/Caches/REforge/previews`.
- Windows: `%LOCALAPPDATA%/REforge/Cache/previews`.

The cache identifies the release's complete SHA-256 catalog manifest, visual
variant and engine preview recipe revision. This metadata key remains stable
when file verification switches to BLAKE3. Separate library entries can share those
source images. Valid cached images load without checking or opening original
game files, including when those originals are unavailable. Only missing or
corrupt variants are regenerated. For generation, the worker reuses the recent
library verification if file sizes and modification times still match; otherwise
it verifies the originals once for the job. Valid cached variants remain usable
if another variant cannot be generated. Successful verification after originals
recover allows missing variants to be retried without discarding good images.
Library status verification is separate.

Each completed PNG is a checkpoint, published only after its write finishes.
Preview work continues independently while a launched game runs. After a
launcher crash or an interrupted close, restart reuses valid completed variants
and regenerates missing or corrupt ones; an unfinished image starts over.
Workers serialize cache access across launcher instances and remove abandoned
staging files under that lock. Preview errors and Rust panics leave the
launcher usable and do not stop later queued games. No queue/progress manifest
is needed. Deleting the cache is safe; verified entries regenerate it. When a
usable cached image is unavailable, missing originals, unsupported providers
and preview errors retain Symbolic placeholders. Their initials use the first
letters/numbers of the first two Unicode words, or the first two characters of
one word, uppercased; empty/non-text names use `?`.

Native images are fitted to the preview cell using their intended display aspect.
Each axis uses Catmull–Rom bicubic filtering when shrinking and pixel coverage
when enlarging; exact integer enlargement preserves whole source pixels. Both
passes retain fractional values until final colour conversion. This applies to
both Sharp pixels and CRT previews.

Selecting Hyllian CRT adds an illustrative launcher treatment: scanline strength
**0.30**, spacing **3 logical UI units** and RGB mask strength **20%**, with slight
horizontal softening. Monochrome engine previews retain grayscale and omit the
RGB grille. The cue is generated at the final physical preview size, after
resampling, and is independent of gameplay's actual Hyllian shader and its fine
adjustments. There are no launcher filter tuning controls.

Previews and frames use bounded in-memory texture and frame-mask caches,
regenerated when their actual source, physical size, desktop density, treatment or
appearance changes. Their joined antialiased boundary preserves the neutral frame
appearance on all renderers, including software. Presentation changes reuse clean
source images; they never regenerate engine scenes or advance gameplay.

Add verifies the supported release before saving its location. Relocate validates
the replacement against the game and compatibility group. Missing entries retain their
identity and offer Locate Files. Remove asks for confirmation and removes only
the library entry and its preferences. Success notices expire; errors can be
dismissed. Floating notifications wrap long messages within the window and scroll
lengthy diagnostics without moving the launcher layout or hiding Dismiss.
The original files are never changed by these library operations.
