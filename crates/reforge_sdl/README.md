# Shared SDL3 presentation

`reforge_sdl` owns SDL3 windows, renderer setup, event pumping and reusable
streaming frame textures for REforge's engines. Each engine retains its input
mapping, pixel decoding, palettes and game clock. RGB24 and RGBA32 inputs are
accepted without changing the engine framebuffer format.

The backend uses the pinned `sdl3` Rust wrapper and the installed native SDL3
library. Renderer selection is delegated to SDL, allowing hardware acceleration
where available. Vsync is requested by default; an unsupported request prints a
warning and continues without sync. This controls presentation, not simulation
frequency. The wrapper does not expose a vsync setter, so that single call uses
its generated SDL declarations with an isolated safety explanation.

Both current engines use a 320×240 logical display area. Source images are
stretched into that aspect-corrected area and sampled with nearest neighbours.
Resizing chooses the largest integer display multiple that fits, centred with
black margins. The minimum window size is one display multiple. This is integer
scaling of the display area: the 320×200 source's vertical correction and Mach 3's
640×200 monochrome image do not give every source pixel an identical square
block, especially at small or odd display multipliers. Nearest sampling selects
source colours without blending; linear filtering would blend them.

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

Authored project code has no selected license yet. Preserve the
[dependency notices and license texts](docs/dependencies.md) when distributing
the corresponding dependencies. This crate bundles no game data.
