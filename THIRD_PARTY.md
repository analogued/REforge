# Third-party software and assets

This inventory covers direct dependencies, imported code and bundled assets.
License texts are shared by license type under [LICENSES/](LICENSES/); the
component-specific copyright notices below accompany those texts. Existing
notices in imported source files remain intact. Third-party material retains
these terms; REforge's authored code and documentation use
[`GPL-3.0-or-later`](LICENSE).

Registry Cargo dependencies are used unmodified through their ordinary APIs.
Exact resolved versions and registry crate checksums are in [Cargo.lock](Cargo.lock).
Release packaging generates a combined notices document for the selected
target and features, including indirect dependencies; see
[release packaging](docs/releases.md). Generated dependency inventories are
build output rather than manually maintained repository files.

## Direct Cargo dependencies

MIT is selected where it is offered.

| Component | Version | Used by / purpose | Selected license |
| --- | --- | --- | --- |
| [bzip2](https://crates.io/crates/bzip2/0.5.2) | 0.5.2 | `alleycat`: decompress external restoration patches | [MIT](LICENSES/MIT.txt); bundled libbzip2 terms below |
| [png](https://crates.io/crates/png/0.17.16) | 0.17.16 | Application preview cache and shared SDL controller artwork | [MIT](LICENSES/MIT.txt) |
| [unicode-segmentation](https://crates.io/crates/unicode-segmentation/1.12.0) | 1.12.0 | Application: Unicode-aware Symbolic initials | [MIT](LICENSES/MIT.txt) |
| [blake3](https://crates.io/crates/blake3/1.8.7) | 1.8.7 | Application: original-file verification, release and cache identities; parallel memory-mapped hashing | [CC0-1.0](LICENSES/CC0-1.0.txt) |
| [sdl3](https://crates.io/crates/sdl3/0.20.0) | 0.20.0 | Shared SDL: window, input, rendering and audio APIs | [MIT](LICENSES/MIT.txt) |
| [egui](https://crates.io/crates/egui/0.31.1) | 0.31.1 | Shared SDL: settings and launcher interfaces, with default fonts | [MIT](LICENSES/MIT.txt); font terms below |
| [nuked-opl3](https://crates.io/crates/nuked-opl3/0.1.0) | 0.1.0 | Shared audio: optional pure Rust OPL2/OPL3 synthesis | [LGPL-2.1-or-later](LICENSES/LGPL-2.1.txt) |
| [blip_buf](https://crates.io/crates/blip_buf/0.2.1) | 0.2.1 | Shared audio: optional band-limited speaker renderer | [MIT](LICENSES/MIT.txt) |
| [toml_edit](https://crates.io/crates/toml_edit/0.22.27) | 0.22.27 | Shared settings: TOML parsing and editing | [MIT](LICENSES/MIT.txt) |
| [getrandom](https://crates.io/crates/getrandom/0.3.4) | 0.3.4 | Shared settings and application: operating-system randomness for library IDs and launch handoff names | [MIT](LICENSES/MIT.txt) |
| [libc](https://crates.io/crates/libc/0.2.190) | 0.2.190 | `btz` terminal frontend: terminal readiness polling | [MIT](LICENSES/MIT.txt) |
| [fs2](https://crates.io/crates/fs2/0.4.3) | 0.4.3 | Shared settings and launcher preview cache: file locking | [MIT](LICENSES/MIT.txt) |

BLAKE3 uses the CC0 public-domain dedication offered in its upstream
`LICENSE_CC0`; that license option is selected here.

Copyright notices, transcribed from the corresponding upstream license files:

- **bzip2:** Copyright (c) 2014-2025 Alex Crichton and Contributors.
- **png:** Copyright (c) 2015 nwin.
- **unicode-segmentation:** Copyright (c) 2015 The Rust Project Developers.
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

## Optional OPL synthesis

`reforge_audio`'s `opl` feature uses the unmodified pure Rust `nuked-opl3` 0.1.0
registry crate through its ordinary API. No upstream source is copied into the
adapter. Upstream: [Tony Gies, nuked-opl3-rs](https://github.com/tgies/nuked-opl3-rs).
The package names Tony Gies, Nuke.YKT and Daniel Balsom as authors. Its core
retains Copyright (C) 2013-2020 Nuke.YKT and Copyright (C) 2026 Tony Gies
(Rust port), under LGPL-2.1-or-later. The [license text](LICENSES/LGPL-2.1.txt)
is included; Cargo.lock records the exact registry checksum.
Neither `c-reference-tests` nor `stereo-ext` is enabled, so this dependency
requires no C/C++ compiler. Centurion enables `opl` for its AdLib output, so
default application builds include this dependency.

## NihAV QuickTime support

`reforge_quicktime` uses [NihAV](https://nihav.org/) by Kostya Shishkov
and upstream contributors under [AGPL-3.0-only](LICENSES/AGPL-3.0.txt).
The five crates in [third_party/nihav](third_party/nihav) come from
revision `13f31633cf325b0036d697af7bf05c611b4b8967` of the official
[source bundle](https://nihav.org/nihav.gitbundle). The adapter enables MOV,
Photo JPEG, Cinepak, Apple Video (RPZA), QuickTime IMA ADPCM and QDesign Music
(QDMC).

REforge adds Cargo license metadata and omits the vendored crates' test-only
dependencies. In `nihav-core/src/refs.rs` it replaces manual shared mutable
references with copy-on-write buffers. It adds baseline JPEG restart handling
in the two `src/codecs/jpeg.rs` files, and corrects MOV sample durations,
repeated movie metadata, and packet interleaving in
`nihav-commonfmt/src/demuxers/mov/`. The IMA ADPCM and QDMC decoders in
`nihav-qt/src/codecs/` write directly to their returned PCM buffers. The
RPZA decoder retains the completed frame as the next decode reference. The
[import instructions](third_party/nihav/README.md) describe how to reapply
the [patch series](third_party/nihav/patches). The vendored code retains its
AGPL terms. The adapter is authored under GPL-3.0-or-later and the code is
combined under the permission in section 13 of the GPLv3 and AGPLv3.

## PC speaker response design credit

The PC speaker response in
[`speaker_filter.rs`](crates/reforge_audio/src/render/speaker_filter.rs)
credits DOSBox Staging for its choice of third-order Butterworth high-pass
filter at 120 Hz and third-order Butterworth low-pass filter at 4,300 Hz.
The reference is `PcSpeakerImpulse::SetFilterState` in
[`pcspeaker_impulse.cpp` at v0.83.0](https://github.com/dosbox-staging/dosbox-staging/blob/7b40053b7ac580843d0461eba8c36a47a990e66c/src/hardware/audio/pcspeaker_impulse.cpp),
commit `7b40053b7ac580843d0461eba8c36a47a990e66c`.

REforge independently implements the mathematical filter design and coefficient
calculation in Rust. No Staging or upstream filter-library implementation code
is copied, adapted or translated. This is a design credit, separate from the
imported shader below; the authored implementation uses GPL-3.0-or-later.
Waveform reconstruction continues to use the `blip_buf` dependency listed above.

## Statically linked libbzip2

Alley Cat's bzip2 dependency enables `static`, compiling the unmodified libbzip2
1.0.8 supplied by `bzip2-sys` into the executable. It does not require a system
libbz2 at runtime. Its copyright is (C) 1996-2019 Julian R Seward; the retained
[bzip2 license](LICENSES/bzip2-1.0.8.txt) applies to that native component.
The Rust wrapper and bindings retain their separate Cargo license terms.

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

[`crates/reforge_graphics/src/font8x8.rs`](crates/reforge_graphics/src/font8x8.rs)
adapts printable ASCII glyphs from SDL's
[`src/render/SDL_render_debug_font.h` at release-3.4.16](https://github.com/libsdl-org/SDL/blob/release-3.4.16/src/render/SDL_render_debug_font.h).
The upstream header credits Marcel Sondaar and International Business Machines
(public domain VGA fonts) and marks the font definition Public Domain. The
SDL copyright and Zlib notice are retained in the adapted source.

The adaptation selects characters 32–126, adds a blank space glyph and converts
the byte rows to a Rust table shared by `alleycat` and `btz`. No font rows were
copied from a game or BIOS dump. Upstream header SHA-256:
`0c96157b6397485698ac07d20d33858e086c2752403f4d2f57a70acc26fa45df`.

`alleycat`'s CGA packing helper in
[`font.rs`](engines/alleycat/src/font.rs) and `btz`'s line-drawing, block and shade
glyphs in [`display/mod.rs`](engines/btz/src/display/mod.rs) are authored code
under `GPL-3.0-or-later`.

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

## Controller artwork

[`crates/reforge_sdl/assets/controller/`](crates/reforge_sdl/assets/controller/README.md)
contains selected drawings from Gidedin's
[Gamepad Line Art Pack](https://gidedin.itch.io/gamepad-line-art-pack) and button
glyphs from Kenney's [Input Prompts 1.5A](https://kenney.nl/assets/input-prompts).
Both are dedicated to the public domain under
[CC0 1.0 Universal](LICENSES/CC0-1.0.txt). These UI assets are independent of
original game data. Controller names and marks belong to their respective owners;
no affiliation or endorsement is implied.

Unmodified SVG masters, upstream notices, archive/file hashes and regeneration
instructions accompany the generated lossless masks. The generation labels
existing paths by physical control and normalizes glyph viewports. Runtime code
tints the masks for each theme and selected template; the authored generator and
UI code use the repository license.

## Original game data and external tools

Original executables, sprites, samples and tables remain external user inputs.
They are not redistributed or licensed by this repository. No implementation
code was imported from another Alley Cat or Mach 3 recreation.

Separately installed emulators and investigation tools are not REforge
dependencies or release contents. In particular, DOSBox-X Observer retains its
own GPL and upstream notices in its separate repository; no emulator executable
or source is bundled here.
