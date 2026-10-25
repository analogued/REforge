# Shared SDL3 presentation

`reforge_sdl` owns SDL3 windows, renderer setup, event pumping and reusable
streaming frame textures and queued audio output for REforge's engines. It also
implements common startup sizing, Ctrl+Q quit and Pause/Break toggle recognition.
Engines choose their default sizing mode; both current engines default to auto.
Ctrl+Q follows the Q letter in the current keyboard layout, rather than the
physical Q position on a US keyboard. Engines decide when pausing is available
and retain their own additional bindings. Pause repeats and desktop modifier
combinations are ignored. The optional host pause timeline freezes deadlines
without synthesizing DOS input or changing an engine's original pause state.
`game_title(game_name, status)` formats the common window title as
`REforge · Game name`, with an optional ` · Status` suffix supplied by the engine.
Each engine retains its input mapping, pixel decoding, palettes and game clock.
For diagnostics, `event_clock_sample()` brackets SDL’s nanosecond event clock
with host `Instant` samples. Engines can record this clock correlation alongside
events and their own deadlines. The helper does not translate timestamps, retain
taps or decide when input becomes effective.

RGB24 and RGBA32 inputs are accepted without changing the engine framebuffer
format.

The backend uses the pinned `sdl3` Rust wrapper and the installed native SDL3
library (SDL 3.4 or later). Both presentation modes prefer SDL's shader-capable
GPU renderer, allowing plain pixels and CRT effects to share one renderer. Vsync is requested by default; an unsupported request prints a
warning and continues without sync. Presentation can block for display refresh;
engines must advance due simulation work independently of presentation count.
The wrapper does not expose a vsync setter or nanosecond tick access; these
calls use generated SDL declarations with isolated safety explanations.

Sharp pixels is the built-in default. Optional `--presentation hyllian` uses
the shared Hyllian CRT shader: horizontal filtering, source-row beam lines,
gamma processing and an RGB aperture mask. It leaves the engine's input pixels,
palettes, graphics modes and simulation untouched. This initial treatment uses
the same initial CGA-style parameters for both engines. The CRT & Colour menu
exposes its scanline, beam, filter, mask, brightness and gamma controls; separate
colour profiles and monitor selection are not implemented. Small output areas can
show interference patterns; display-dependent preset tuning is not implemented.
With Original display selected, a display multiplier N spreads 200 beam cycles over
240*N drawable pixels: each cycle spans 1.2*N pixels on average. The aperture
mask retains its two-drawable-pixel pitch. Changing scale therefore changes the
beam spacing, while the current beam/mask parameters remain fixed.

The GPU setup advertises bundled SPIR-V, Metal source and DXIL, allowing SDL to
choose Vulkan, Metal or Direct3D 12 without tying a presentation mode to an API.
An explicit `SDL_RENDER_DRIVER` override is respected. GPU initialization failure
warns and tries SDL's ordinary renderer selection; if that also fails, the backend
tries software rendering. CRT shader setup failure retains the selected renderer
and falls back to pixels. To force software, set `SDL_RENDER_DRIVER=software`;
Hyllian then warns and falls back to pixels. No shader compiler, Qt or DXC is
needed to build or run REforge. Native Vulkan rendering is validated on Linux;
DXIL compiler validation and reflection are checked, but native D3D12 and Metal
rendering still need Windows and macOS validation.

The app launch session resolves persistent and CLI presentation preferences.
For standalone engine previews, `Window::new_with_presentation` selects the
initial effect and `Window::new` starts with pixels on the same preferred GPU
renderer. `Window::set_presentation` switches
pixels/CRT without recreating the renderer, window or uploaded frame textures.
CRT resources are created on first use and cached when returning to pixels.
`Window::presentation` reports the effective mode, including startup fallback.
An unsupported runtime CRT request returns an error and leaves the active mode
unchanged. Settings Apply retains a requested Hyllian preference on fallback
renderers and restores the shader when a capable renderer is selected.
The generated-binding GPU bridge is isolated in `src/hyllian.rs`; adapted
third-party shader code, its license, provenance and regeneration commands live
in [shaders/](shaders/README.md).
The mask uses drawable pixels relative to the game's presentation rectangle.
Resizing and DPI changes update that rectangle each frame. Modern overlays are
drawn after the effect with ordinary SDL rendering; original in-frame text and
HUD graphics receive the effect along with the game image.

Each engine supplies `DisplayGeometry`: its native pixel grid and original
aspect-corrected base display size. Settings persist `original` or `square`,
never a particular game's numeric ratio. The menu derives contextual ratio
labels from the same metadata used for rendering. Engines may update it at a
video-mode change; texture dimensions alone do not establish native geometry.

Both current engines default to a 320×240 logical display area. Source images are
stretched into that aspect-corrected area and sampled with nearest neighbours.
Resizing chooses the largest integer display multiple that fits, centred with
black margins. The requested minimum drawable size is one display multiple.
This is integer scaling of the display area: the 320×200 source's vertical
correction and Mach 3's 640×200 monochrome image do not give every source pixel
an identical square block, especially at small or odd display multipliers. At 320 pixels of output
width, monochrome columns must be discarded; odd display multipliers give them
unequal widths. Nearest sampling selects source colours without blending;
linear filtering would blend them.
Square pixels instead uses the native grid as the logical display area:
320×200 for both colour paths, or 640×200 for Mach 3 monochrome. Aspect changes
preserve the existing window size and continue using integer fitting.

Windows request high pixel density. Auto measures the compositor's maximum
client area by briefly requesting maximization from a modest bootstrap window,
then restores a normal window sized to the largest integer display multiple
that fits. This accounts for panels and decorations even when SDL's usable-bounds
query reports the full monitor (as on some Wayland desktops). The finished window
has only the few border pixels caused by fractional density rounding, rather
than maximized letterbox margins. If maximization is unavailable or denied,
startup retains a density-compensated multiplier of 4. Placement remains a
compositor decision. Native Wayland auto windows are remapped at their final
normal size so the compositor applies its initial placement policy again,
instead of retaining the maximized window's upper-left origin. Centering is
requested on backends that support positioning; Wayland does not guarantee
coordinates. This measurement, sizing and remapping run only at startup.

An explicit multiplier targets drawable pixels: 4 requests a 1280×960 rendering
area, regardless of the desktop's content scale. SDL's window pixel density
converts that target to platform window coordinates; rounding upward can leave
a few extra border pixels. A hidden window is sized before mapping, then startup
sizing is checked once more after its first presentation for the selected display
and density. Desktop size constraints can still reduce the available area. The
rendering area is not a guarantee about final physical monitor pixels: the
compositor can apply further transformations.

SDL renders the original-resolution texture directly into the drawable area;
engines do not upload an enlarged image. SDL tracks drawable changes during
resizing and monitor moves. Pixel-size/display events update the minimum window
dimensions for the current density, but never restore the initial multiplier or
resize the window merely because the game changes graphics modes. User resizing
remains allowed, including a 320-pixel-wide game area that loses Mach 3 monochrome
columns. No high-DPI-specific dependency or game-specific size restriction is used.

SDL's GPU renderer updates its backing texture during presentation. Before the
first game frame and after a drawable-size change, the shared backend submits
the retained image once to settle that backing size, then refreshes integer
geometry before drawing the requested frame. This prevents a static screen's
only resize redraw from leaving a clipped image visible. The caller's overlay
runs once; software and ordinary renderers retain their existing path. Auto uses the compositor-negotiated client area for startup sizing.

Construct and use windows, texture creators and textures on the main thread.
Keep a texture's creator alive until the texture is dropped, and present it only
with the window that created it. The separate creator lifetime avoids
self-referential ownership. Engine overlays are drawn after the frame, in logical
display coordinates, without modifying its pixels; an overlay that changes SDL
scale or draw state must restore it before returning.

Run `cargo test --locked -p reforge_sdl` from the workspace root. Synthetic
RGB/RGBA pixel tests cover nearest sampling, aspect correction and integer
letterboxing with offscreen software rendering. Engine checks cover input
semantics. Native builds and runtime checks have been exercised on Linux;
Windows and macOS have not yet been validated. The ignored native
`gpu_presentation_switch_retains_renderer_textures_and_plain_pixels` check starts
in pixels with the default GPU renderer, repeatedly toggles Hyllian using both
engine texture formats, verifies unchanged window/renderer identity, and compares
the restored plain image byte for byte. It also checks live uniform edits,
parameter validation, and retention across software fallback and GPU return.
Run mixed-renderer native checks without a forced `SDL_RENDER_DRIVER`: that
process-wide SDL hint can also affect the software window surface's internal
blit renderer. On the tested Wayland/NVIDIA setup, forcing `gpu` there crashes
in native presentation; ordinary REforge auto-selection passes. Software fallback
checks also verify that unsupported direct effect changes preserve pixels mode.

`audio::AudioOutput` owns a default playback stream and its SDL/audio lifetimes.
Construct, use and drop it on the main thread. It accepts mono native-endian
signed 16-bit samples at a caller-selected rate, leaving negotiated host format
conversion to SDL. Startup is paused so the caller can prefill before resuming.
`queued_samples()` counts remaining unconverted input samples, excluding audio
already handed to the device. `set_paused()` retains the queue; `clear()` discards
pending stream data while retaining pause state.

The queue has a hard 250 ms input limit. Overflow rejects new data and preserves
the existing queue; the caller explicitly clears/replaces expired audio. Queueing
does not wait for playback, invoke simulation callbacks or know an engine clock.
Initialization/output errors are returned to the caller. Audio tests use fresh
dummy-driver and unavailable-driver child processes, so ordinary tests never
open host audio or modify the renderer tests' process environment.

Pixel-target tests cover whole and fractional densities. An opt-in window check
exercises high-density flags, startup sizing, both texture formats and resizing:

```sh
cargo test --locked -p reforge_sdl high_density_window_pixel_size_and_user_resize -- --ignored --nocapture --test-threads=1
```

Run it on an otherwise idle desktop or with `SDL_VIDEODRIVER=dummy`. The native
check expects space for a 1280×960 rendering area. Dummy/software checks do not
establish the compositor's final monitor-pixel mapping.

Authored project code has no selected license yet. Preserve the
[dependency notices and license texts](../../THIRD_PARTY.md) when distributing
the corresponding dependencies. This crate bundles no game data.

## Settings menu

F10 opens the Compact Display, CRT & Colour, Audio and Controls pages in either engine.
Controls offers a shared stick dead zone, with Alley Cat's additional input-response
choice under its game overrides.
Selectors switch live between dropdowns for mouse input and horizontal carousels
for keyboard/controller navigation. They retain the same Compact dimensions,
identity, focus and value. Left/Right cycles available choices; Enter/Space or
controller A cycles forward. Up/Down and Tab/Shift+Tab (controller shoulders)
navigate controls. Disabled or command-line-locked choices remain unavailable.
A mouse button press switches back before that press is handled, so one click
opens a carousel as a dropdown. Nonzero wheel input also selects mouse mode;
hover and cursor movement do not. Navigation dismisses an open dropdown without
committing its highlighted choice and returns to its owning field. Escape or
controller B dismisses a dropdown before closing the menu on a fresh press.
Held-stick repeat is suppressed after switching to mouse/keyboard input until
fresh controller input takes over. These are host UI controls, independent of
reconstructed gameplay input and clocks.
Window mode, scale, aspect ratio, renderer, backend, VSync, frame ceiling, Hyllian shader
controls and Audio use draft edits with Apply/Cancel. Global and game
preferences are saved through `reforge_settings`; CLI overrides remain temporary
and lock their controls. A dark-blue section notice and pink padlocks after
locked controls identify these restrictions; hover the lock or disabled control
for the argument and active value. Locked labels and controls are gray; controls
show the active launch value without copying it into the saved draft. The icons
sit outside radio groups, sliders, checkboxes and dropdowns without adding
per-setting explanation rows. See [configuration](../../docs/configuration.md)
for the complete persistence, inheritance and fallback contract.

`Window::presentation_due()` offers a nonblocking check of the same ceiling used
by presentation. Engines can service input and simulation between redraws;
VSync can still block the eventual Present call. Explicit presentation calls
retain the shared limiter, including for engines that do not use this check.

The engine adapters freeze game time while the menu is open, preserve existing
host/native pause, and consume menu keys before native input. Escape/F10 cancel;
Ctrl+Q and window close remain global. Controls held across a menu boundary need
a fresh press before game use. No pause-on-focus-loss policy is introduced.

Renderer/backend changes invalidate texture residents and tear down the native
renderer before creating its replacement. The stable `FrameCreator` coordinates
this transition. Leases retain native renderer ownership while resident; empty
leases cannot be rendered. `FrameTexture` retains the latest CPU image and
reuploads it lazily. The menu recreates its font atlas for the new generation
while preserving egui layout memory, so its position, scroll offsets and expanded
sections survive renderer changes without a visible layout jump.
The GPU device is independently owned and retained by texture creators, including
across ordinary/software rendering. Compatible GPU selections reuse it; a
different GPU backend replaces it after old window-bound resources are released.
This avoids repeated device selection/initialization during live switching.
The standalone SDL `vulkan` renderer is excluded; use GPU/Vulkan. Native
repeated-switch checks reproduced buffer-import failure and driver crashes with
that separate path. Ordinary fallback enumeration also excludes it, including
explicit `SDL_RENDER_DRIVER=vulkan` requests (which warn and fall back).
Failed creation reconstructs the previous renderer, with software as the final
recovery fallback. Window creation/positioning may briefly occur during Apply;
engine state and clocks are retained. Ordinary scale/mode and VSync changes keep
the renderer. Only pixels are available on fallback renderers without the custom
GPU interface.
The Treatment dropdown keeps Hyllian visible but disables its selection for
incompatible renderer/backend choices in the current settings scope, including
pending Display edits and temporary command-line overrides. Applying an
incompatible saved combination resets its treatment to Sharp pixels; restoring
a compatible renderer requires selecting Hyllian again. CLI restrictions and
automatic GPU initialization failures remain temporary runtime fallbacks.

The `settings` module uses egui core and its bundled fonts, without eframe, winit,
wgpu or a separate CPU rasterizer. One SDL geometry adapter handles GPU, ordinary
hardware and software rendering. UI coordinates follow SDL display content
scale independently of game scaling, and the panel is drawn after CRT processing.
Textures, clipping, font filtering and renderer coordinate-state restoration
are handled by the adapter. Hardware uses premultiplied alpha; the tested SDL
software triangle path requires conversion to standard alpha for correct
transparency. Software glyph/edge rasterization can differ from GPU output.
See the [dependency and font notices](../../THIRD_PARTY.md#bundled-fonts).

`RuntimeSettings::default` supplies shared Auto/windowed defaults. Engines pass
`VideoCapabilities` to restrict available choices when required; both current
engines permit all initial settings. These restrictions belong at the engine
boundary rather than in the SDL painter. Unsupported changes display an error
and keep the panel usable. The menu does not expose development-only controls.

Engines call `Window::prepare_settings(&mut menu)` before painting. This shared
entry point applies pending edits, prepares the actual accepted geometry and
temporarily enlarges undersized normal windows. Below 569 × 420 rounded logical
UI units it requests at least 630 × 470, bounded by the display's usable client
area. Closing restores the earlier geometry unless an applied scale/window-mode
change superseded it. Renderer changes retain this restoration state. The raw
`SettingsMenu::prepare` remains available for offscreen layout previews.

Presentation pacing lives in `src/pacing.rs`. The shared ceiling uses current
display refresh, 60 fps, 120 fps or Unlimited, independently of engine scheduling.
VSync is counted within each interval and needs no extra timer wait when it
already meets the ceiling. A stall resets presentation debt, while each engine
retains its own simulation catch-up policy. See [configuration](../../docs/configuration.md)
for persistence, defaults and shader parameter ranges.

## Gamepads

`gamepad::Gamepads` owns one SDL-recognized gamepad, handles hot-plugging and
normalizes the left stick/D-pad and bottom face button into digital `Actions`.
`PadEvent` retains independent direction/button press edges and host pause/settings
shortcuts. Adapters call `handle` in SDL queue order and retain the source event's
timestamp using their existing clock correlation/history. This module neither
executes gameplay nor synthesizes DOS keyboard interrupts.

The stick uses independent axis thresholds and hysteresis; no combination window,
pulse modulation or analog movement is added. `set_dead_zone` applies the shared
preference, and `reset` rejects stale events and suppresses held controls until
neutral. Keyboard and controller sources must be merged without cancelling each
other's held actions. Engines decide their own opposing-direction rules and map
Start/Back to host pause/settings. The manager survives renderer recreation because
it belongs to the engine adapter, independently of the presentation window.

Unit tests cover conversion and boundary rules. An isolated SDL virtual-gamepad
test exercises events, device identity and disconnects without physical hardware.
See [controller preferences](../../docs/configuration.md#controllers) for bindings,
configuration and current scope.

## Host audio preferences

`FrameCreator::audio_playback()` shares a `Playback` endpoint with the window's
settings application and focus handling. Both engines feed timestamped mono PCM
to `audio::queue::Queue`. Positions are input-sample indices on the engine's
presentation timeline; engines convert from their own virtual clocks. Generated
PCM can extend ahead of that position, as with Alley Cat's blocking programs.
Wall-clock timestamps must not be passed as a substitute.

The shared `Policy` defaults to 50 ms prefill, a 100 ms device-input queue target,
a 100 ms additional-lateness budget, and at most ten seconds pending. Engines
can supply an explicit policy override. `Policy::for_output` honors the
endpoint's `set_prefill(Duration)` override, including enough queue capacity.
Pending overflow or noncontiguous input returns an error without silently
truncating future sound. `phase`, `pending_samples` and `stats` expose diagnostics.

Priming waits without blocking the engine. Before the first resume after startup
or reset, it bounds the oldest sample's delay by normal prefill plus the
additional-lateness budget. If that bound is exceeded, it discards the expired
prefix and waits for current prefill, preserving future PCM. The default bound
is 150 ms; startup recovery then targets a 50 ms prefill offset. Running playback
preserves submitted order while buffering absorbs interruptions. It estimates nominal playback
progress from the presentation position at resume, then compares that progress
to the oldest unconverted submitted sample. An empty SDL input queue alone is
not an underrun: converted audio may still be held by the device. During running
playback, excessive lateness enters recovery, pauses/clears the device once, and
removes expired PCM. Startup recovery has no submitted device audio to clear.
Recovery retains future data and waits for a fresh prefill before resuming. Its
cutoff may advance while waiting for simulation catch-up; normal submissions
have no unconditional age trimming. The budget is conservative and does not
promise exact physical speaker latency.

Engines retain sound sequencing, device execution and the mapping of their
presentation target, including discarded host-stall time. Pause retains queued
and pending PCM; the engine freezes its supplied position. If an explicit pause
boundary discards simulation catch-up debt, `rebase` updates the presentation
observation without consuming PCM or changing its generated epoch. Restart and
explicit audio-setting boundaries reset presentation state at the next generated-sample
epoch, independently of native sound execution. Device/enable/renderer/category
changes increment an endpoint generation; engines finish old synthesis and then
reset presentation at that boundary. The shared layer never advances a game
clock. The lower-level `resume_when_ready`/`set_paused` operations remain available
for simple direct playback, but engine streaming uses the timestamped queue.

`RuntimeSettings` and `Window::apply_settings` resolve/activate both video and
audio preferences in one menu transaction. A failed explicit device open retains
the old endpoint; an audio startup failure permits silent simulation and unrelated
video edits. Focus gain changes never alter engine input or pause state. The
engine declares available sound renderers through `SettingsMenu::with_audio_renderers`;
without that capability, game audio controls are disabled.
`with_audio_categories` declares supported Music/Speech/Effects volume groups.
These levels belong in synthesis; the SDL endpoint keeps Master (80% default)
and focus gain separate, and never changes the desktop mixer volume. Category
changes discard old queued mixed audio and let the engine update its gains at
the current committed time. Global levels remain editable from silent games.

Output names are enumerated through the Rust SDL wrapper. The output stream
uses SDL's per-stream gain and a stream-owned logical device. Only device names,
not transient instance IDs, enter the configuration. No user-adjustable latency
or stereo conversion is provided. See the shared
[configuration reference](../../docs/configuration.md#audio) for defaults and
fallback behavior.

The ignored `audio_endpoint_survives_renderer_changes_and_failed_device_apply`
check requires a native desktop and `SDL_AUDIO_DRIVER=dummy`. It verifies that
audio preferences/endpoints survive renderer replacement and that invalid device
requests or invalid video settings preserve the previous configuration. Ordinary
audio tests run in isolated dummy-driver child processes and play no host sound.
