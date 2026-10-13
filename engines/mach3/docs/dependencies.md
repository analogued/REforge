# Dependencies and notices

The optional `viewer` feature uses REforge's
[shared SDL3 backend](../../../crates/reforge_sdl/README.md). Its pinned Rust
binding dependencies and native SDL3 terms are retained in the shared crate's
[dependency notices](../../../crates/reforge_sdl/docs/dependencies.md) and
[license texts](../../../crates/reforge_sdl/docs/licenses/).
The default Mach 3 library and headless tools do not require SDL3.

## Audio renderer

The `viewer` and `dev-tools` features enable the shared audio crate's optional
`blip` renderer for live playback and `mach3-sound --device --wav`, respectively.
Its pinned Rust dependency and MIT
terms are described in the [shared audio dependency notices](../../../crates/reforge_audio/docs/dependencies.md).
The default headless library does not require this renderer or open an audio device.

## Capture emulator

The optional original-game recorder uses the separately configured DOSBox-X
Observer component. Its generic observer and emulator adapter are maintained
outside this repository, together with the game-specific capture profile.
The component's C++ observer and adapter are GPL-2.0-or-later; its Python tools have
no selected license yet. Its builder retains DOSBox-X's complete GPL version 2
`COPYING` and original per-file notices in an external checkout and records the
tested revision. The patched upstream files allow GPL version 2 or later;
the permitted license versions for the combined executable depend on all
included components. The observer adds no GPLv2-only restriction.
No emulator executable or upstream source is redistributed here. The upstream
emulator uses SDL2; the Rust recreation continues to use SDL3. Redistribution of
an integrated emulator must follow upstream and applicable observer terms.
