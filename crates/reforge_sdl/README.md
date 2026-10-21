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
For these 200-row games, a display multiplier N spreads 200 beam cycles over
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

Both current engines use a 320×240 logical display area. Source images are
stretched into that aspect-corrected area and sampled with nearest neighbours.
Resizing chooses the largest integer display multiple that fits, centred with
black margins. The requested minimum drawable size is one display multiple.
This is integer scaling of the display area: the 320×200 source's vertical
correction and Mach 3's 640×200 monochrome image do not give every source pixel
an identical square block, especially at small or odd display multipliers. At 320 pixels of output
width, monochrome columns must be discarded; odd display multipliers give them
unequal widths. Nearest sampling selects source colours without blending;
linear filtering would blend them.

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
[dependency notices and license texts](docs/dependencies.md) when distributing
the corresponding dependencies. This crate bundles no game data.

## Settings menu

F10 opens the shared Compact Display and CRT & Colour pages in either engine.
Window mode, scale, renderer, backend, VSync, frame ceiling, Hyllian shader
controls and Audio use draft edits with Apply/Cancel. Global and game
preferences are saved through `reforge_settings`; CLI overrides remain temporary
and lock their controls. A dark-blue section notice and pink padlocks after
locked controls identify these restrictions; hover the lock or disabled control
for the argument and active value. Locked labels and controls are gray; controls
show the active launch value without copying it into the saved draft. The icons
sit outside radio groups, sliders, checkboxes and dropdowns without adding
per-setting explanation rows. See [configuration](../../docs/configuration.md)
for the complete persistence, inheritance and fallback contract.

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
See the [dependency and font notices](docs/dependencies.md#settings-ui).

`RuntimeSettings::default` supplies shared Auto/windowed defaults. Engines pass
`VideoCapabilities` to restrict available choices when required; both current
engines permit all initial settings. These restrictions belong at the engine
boundary rather than in the SDL painter. Unsupported changes display an error
and keep the panel usable. The menu does not expose development-only controls.

Presentation pacing lives in `src/pacing.rs`. The shared ceiling uses current
display refresh, 60 fps, 120 fps or Unlimited, independently of engine scheduling.
VSync is counted within each interval and needs no extra timer wait when it
already meets the ceiling. A stall resets presentation debt, while each engine
retains its own simulation catch-up policy. See [configuration](../../docs/configuration.md)
for persistence, defaults and shader parameter ranges.

## Host audio preferences

`FrameCreator::audio_playback()` shares a `Playback` endpoint with the window's
settings application and focus handling. Engines register their mono PCM rate,
then retain control of synthesis, queue recovery and prefill/resume. The shared
endpoint handles output selection and stream gain, without advancing an engine
clock. Device/enable/renderer/category changes increment an endpoint generation; engines
refill and apply their own renderer policy at the current native timestamp.

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
