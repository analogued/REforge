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
Each engine retains its input mapping, pixel decoding, palettes and game clock.
For diagnostics, `event_clock_sample()` brackets SDL’s nanosecond event clock
with host `Instant` samples. Engines can record this clock correlation alongside
events and their own deadlines. The helper does not translate timestamps, retain
taps or decide when input becomes effective.

RGB24 and RGBA32 inputs are accepted without changing the engine framebuffer
format.

The backend uses the pinned `sdl3` Rust wrapper and the installed native SDL3
library (SDL 3.4 or later). Plain renderer selection is delegated to SDL, allowing hardware acceleration
where available. Vsync is requested by default; an unsupported request prints a
warning and continues without sync. Presentation can block for display refresh;
engines must advance due simulation work independently of presentation count.
The wrapper does not expose a vsync setter or nanosecond tick access; these
calls use generated SDL declarations with isolated safety explanations.

`--presentation pixels` is the default. Optional `--presentation hyllian` uses
the shared Hyllian CRT shader: horizontal filtering, source-row beam lines,
gamma processing and an RGB aperture mask. It leaves the engine's input pixels,
palettes, graphics modes and simulation untouched. This initial treatment uses
one fixed CGA-style parameter set for both engines; it does not yet provide
colour profiles, monitor selection or Custom controls. Small output areas can
show interference patterns; display-dependent preset tuning is not implemented.
For these 200-row games, a display multiplier N spreads 200 beam cycles over
240*N drawable pixels: each cycle spans 1.2*N pixels on average. The aperture
mask retains its two-drawable-pixel pitch. Changing scale therefore changes the
beam spacing, while the current beam/mask parameters remain fixed.

Hyllian requests SDL's shader-capable GPU renderer, with bundled SPIR-V for
Vulkan and Metal source for Metal. SDL chooses a compatible device/backend;
an explicit `SDL_RENDER_DRIVER` override is respected. No shader compiler or Qt
library is needed to build or run REforge. D3D shader binaries are not bundled;
Windows needs a working Vulkan backend for this effect. GPU/shader setup failures
warn and continue with plain pixels. If ordinary renderer creation fails, the
backend explicitly tries SDL's software renderer. To force the software path,
set `SDL_RENDER_DRIVER=software`; Hyllian then warns and falls back to pixels.

`Window::new_with_presentation` selects the effect before texture creation;
`Window::new` retains plain presentation. The generated-binding GPU bridge is
isolated in `src/hyllian.rs`; adapted third-party shader code, its license,
provenance and regeneration commands live in [shaders/](shaders/README.md).
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
Windows and macOS have not yet been validated.

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
