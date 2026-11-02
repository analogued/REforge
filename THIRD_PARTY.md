# Third-party software and assets

This inventory covers direct dependencies, imported code and bundled assets.
License texts are shared by license type under [LICENSES/](LICENSES/); the
component-specific copyright notices below accompany those texts. Existing
notices in imported source files remain intact. Third-party material retains
these terms; REforge's authored code and documentation use
[`GPL-3.0-or-later`](LICENSE).

Cargo dependencies are used unmodified through their ordinary APIs. Exact
resolved versions and crate checksums are in [Cargo.lock](Cargo.lock).
Release packaging generates a combined notices document for the selected
target and features, including indirect dependencies; see
[release packaging](docs/releases.md). Generated dependency inventories are
build output rather than manually maintained repository files.

## Direct Cargo dependencies

MIT is selected where an alternative is offered.

| Component | Version | Used by / purpose | Selected license |
| --- | --- | --- | --- |
| [png](https://crates.io/crates/png/0.17.16) | 0.17.16 | Application: local native preview PNG cache | [MIT](LICENSES/MIT.txt) |
| [unicode-segmentation](https://crates.io/crates/unicode-segmentation/1.12.0) | 1.12.0 | Application: Unicode-aware Symbolic initials | [MIT](LICENSES/MIT.txt) |
| [sha2](https://crates.io/crates/sha2/0.11.0) | 0.11.0 | Application, Alley Cat and BTZ: original-file verification | [MIT](LICENSES/MIT.txt) |
| [sdl3](https://crates.io/crates/sdl3/0.20.0) | 0.20.0 | Shared SDL: window, input, rendering and audio APIs | [MIT](LICENSES/MIT.txt) |
| [egui](https://crates.io/crates/egui/0.31.1) | 0.31.1 | Shared SDL: settings and launcher interfaces, with default fonts | [MIT](LICENSES/MIT.txt); font terms below |
| [blip_buf](https://crates.io/crates/blip_buf/0.2.1) | 0.2.1 | Shared audio: optional band-limited speaker renderer | [MIT](LICENSES/MIT.txt) |
| [toml_edit](https://crates.io/crates/toml_edit/0.22.27) | 0.22.27 | Shared settings: TOML parsing and editing | [MIT](LICENSES/MIT.txt) |
| [getrandom](https://crates.io/crates/getrandom/0.3.4) | 0.3.4 | Shared settings: operating-system randomness for library IDs | [MIT](LICENSES/MIT.txt) |
| [libc](https://crates.io/crates/libc/0.2.190) | 0.2.190 | BTZ terminal frontend: terminal readiness polling | [MIT](LICENSES/MIT.txt) |
| [fs2](https://crates.io/crates/fs2/0.4.3) | 0.4.3 | Shared settings and launcher preview cache: file locking | [MIT](LICENSES/MIT.txt) |

Copyright notices, transcribed from the corresponding upstream license files:

- **png:** Copyright (c) 2015 nwin.
- **unicode-segmentation:** Copyright (c) 2015 The Rust Project Developers.
- **sha2:** Copyright (c) 2016-2026 The RustCrypto Project Developers;
  Copyright (c) 2016 Artyom Pavlov; Copyright (c) 2009-2013 Mozilla Foundation;
  Copyright (c) 2006-2009 Graydon Hoare.
- **sdl3:** Copyright (c) 2013 Mozilla Foundation.
- **egui**, including its `ecolor`, `emath`, `epaint` and default-font crate code:
  Copyright (c) 2018-2021 Emil Ernerfeldt <emil.ernerfeldt@gmail.com>.
  These archives omit the root license file; the source is upstream
  [LICENSE-MIT at 0.31.1](https://github.com/emilk/egui/blob/0.31.1/LICENSE-MIT).
- **blip_buf:** Copyright (c) 2015 Mathijs van de Nes. Published upstream revision
  [06c66ea66151a9d7a3d338342165a1b0a0bf7303](https://github.com/mvdnes/blip_buf-rs/tree/06c66ea66151a9d7a3d338342165a1b0a0bf7303).
- **toml_edit:** Copyright (c) Individual contributors.
- **getrandom:** Copyright (c) 2018-2025 The rust-random Project Developers;
  Copyright (c) 2014 The Rust Project Developers.
- **libc:** Copyright (c) The Rust Project Developers.
- **fs2:** Copyright (c) 2015 The Rust Project Developers.

Except for egui's upstream root license, these notices come from `LICENSE` or
`LICENSE-MIT` in each published crate archive. No implementation from these
libraries has been adapted into REforge's authored source.

## SDL native library and bindings

SDL is dynamically linked from the system, with SDL 3.4 or newer required.
The normal build does not bundle the native library, enable static linking,
or build SDL from source. A distributor that bundles SDL must also account for
the actual library and any additional components it ships.

SDL and the generated `sdl3-sys` bindings use [Zlib](LICENSES/Zlib.txt).
SDL: Copyright (C) 1997-2026 Sam Lantinga <slouken@libsdl.org>.
Bindings: (C) 2024-2025 Maia S Ravn <maia.noreply@runbox.com>.
The bindings version is recorded in Cargo.lock; its SDL suffix identifies the
binding API version, not the installed native library version.

The generated HIDAPI bindings additionally retain this source notice:

> HIDAPI - Multi-Platform library for communication with HID devices.
>
> Copyright 2009, Alan Ott, Signal 11 Software. All Rights Reserved.
>
> This software may be used by anyone for any reason so long as the copyright
> notice in the source files remains intact.

Source: `sdl3-sys/src/generated/hidapi.rs` in the published bindings archive.

## Bundled fonts

The settings interface embeds unmodified font data supplied by
[`epaint_default_fonts` 0.31.1](https://crates.io/crates/epaint_default_fonts/0.31.1).
Its `fonts/` directory supplies the notices and license texts below. These
fonts are independent of the original game data.

| Font | Copyright / attribution | License |
| --- | --- | --- |
| Hack | Copyright 2018 Source Foundry Authors; Bitstream Vera Sans Mono Copyright 2003 Bitstream Inc.; DejaVu contributions committed to the public domain | [MIT](LICENSES/MIT.txt) and [Bitstream Vera](LICENSES/Bitstream-Vera.txt); reserved names "Bitstream" and "Vera" |
| Noto Emoji | Copyright 2013 Google Inc. All Rights Reserved. | [OFL-1.1](LICENSES/OFL-1.1.txt) |
| Ubuntu Light | Copyright 2011 Canonical Ltd. | [Ubuntu-font-1.0](LICENSES/Ubuntu-font-1.0.txt) |
| Emoji Icon Font | Copyright (c) 2014 John Slegers | [MIT](LICENSES/MIT.txt) |

Noto Emoji and Ubuntu Light copyright notices are embedded in their font files;
the license texts come from `fonts/OFL.txt` and `fonts/UFL.txt`. Hack's notices
come from `fonts/Hack-Regular.txt`; Emoji Icon Font's from
`fonts/emoji-icon-font-mit-license.txt`.

### DOS bitmap text fonts

[`engines/alleycat/src/font.rs`](engines/alleycat/src/font.rs) adapts printable
ASCII glyphs from SDL's
[`src/render/SDL_render_debug_font.h` at release-3.4.16](https://github.com/libsdl-org/SDL/blob/release-3.4.16/src/render/SDL_render_debug_font.h).
The upstream header credits Marcel Sondaar and International Business Machines
(public domain VGA fonts) and marks the font definition Public Domain. The
SDL copyright and Zlib notice are retained in the adapted source.

The adaptation selects characters 32–126, adds a blank space glyph and converts
the byte rows to a Rust table. REforge's CGA packing helper is authored code;
no font rows were copied from a game or BIOS dump. Upstream header SHA-256:
`0c96157b6397485698ac07d20d33858e086c2752403f4d2f57a70acc26fa45df`.

[`engines/btz/src/display/ascii.rs`](engines/btz/src/display/ascii.rs) reuses
that adapted ASCII row table with the same notices. BTZ's line-drawing, block
and shade glyphs in [`display/mod.rs`](engines/btz/src/display/mod.rs) are
authored code under `GPL-3.0-or-later`.

## Hyllian CRT shader

The source and generated assets under
[`crates/reforge_sdl/shaders/`](crates/reforge_sdl/shaders/README.md) adapt
DOSBox Staging's
[`resources/shaders/crt/crt-hyllian.glsl`](https://github.com/dosbox-staging/dosbox-staging/blob/7b40053b7ac580843d0461eba8c36a47a990e66c/resources/shaders/crt/crt-hyllian.glsl)
at v0.83.0, commit `7b40053b7ac580843d0461eba8c36a47a990e66c`, under
[MIT](LICENSES/MIT.txt).

- Copyright 2020-2026 The DOSBox Staging Team
- Copyright 2021-2020 Hyllian <sergiogdb@gmail.com>
- Copyright 2020-2020 Tyrells

The unusual upstream year ranges are preserved exactly. The aperture mask is
by hunterk, released into the public domain.

REforge replaces the DOSBox/OpenGL interface with SDL GPU uniforms and vertex
inputs, exposes runtime parameters and retains the selected aperture mask.
SPIR-V, DXIL, HLSL and Metal variants are compiled or translated forms of this
adaptation and carry the same attribution. Source hashes, preset provenance,
technical adaptations and regeneration commands are in the shader README.
Shader compilers are separate developer tools, not shipped with REforge.

## Original game data and external tools

Original executables, sprites, samples and tables remain external user inputs.
They are not redistributed or licensed by this repository. No implementation
code was imported from another Alley Cat or Mach 3 recreation.

Separately installed emulators and investigation tools are not REforge
dependencies or release contents. In particular, DOSBox-X Observer retains its
own GPL and upstream notices in its separate repository; no emulator executable
or source is bundled here.
