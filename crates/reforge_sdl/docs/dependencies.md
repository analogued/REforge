# Dependencies and notices

Both engines use this shared presentation crate, which depends on
`sdl3 = "=0.20.0"` with default features disabled. The [root workspace lockfile](../../../Cargo.lock) fixes the Rust
dependencies below. Headless engine builds do not require these dependencies.
SDL runtime source is not copied into this repository.

| Dependency | Locked version | Purpose | Upstream license; retained terms |
| --- | --- | --- | --- |
| [sdl3](https://crates.io/crates/sdl3/0.20.0) | 0.20.0 | Window, events, texture presentation | MIT; [MIT notice](licenses/sdl3-0.20.0-MIT.txt) |
| [sdl3-sys](https://crates.io/crates/sdl3-sys/0.7.2+SDL-3.4.18) | 0.7.2+SDL-3.4.18 | Raw SDL3 bindings and native linking | Zlib; [Zlib notice](licenses/sdl3-sys-0.7.2+SDL-3.4.18-Zlib.txt), [HIDAPI notice](licenses/sdl3-sys-0.7.2+SDL-3.4.18-HIDAPI-NOTICE.txt) |
| [libc](https://crates.io/crates/libc/0.2.190) | 0.2.190 | C ABI types and functions | MIT OR Apache-2.0; [MIT selected](licenses/libc-0.2.190-MIT.txt) |
| [bitflags](https://crates.io/crates/bitflags/2.13.2) | 2.13.2 | SDL flag types | MIT OR Apache-2.0; [MIT selected](licenses/bitflags-2.13.2-MIT.txt) |
| [pkg-config](https://crates.io/crates/pkg-config/0.3.34) | 0.3.34 | Native library discovery during the bindings build | MIT OR Apache-2.0; [MIT selected](licenses/pkg-config-0.3.34-MIT.txt) |
| [vcpkg](https://crates.io/crates/vcpkg/0.2.15) | 0.2.15 | Native library discovery through vcpkg | MIT/Apache-2.0 (upstream's alternative-license spelling); [MIT selected](licenses/vcpkg-0.2.15-MIT.txt) |

MIT is selected wherever upstream offers a choice. The full copyright and
permission notices are retained below for inclusion with distributed copies of
the corresponding dependencies. These terms do not select a license for this
project's authored code.

## Native runtime

The shared backend dynamically links a system-provided SDL3 library directly. It does
not use SDL2 or an SDL2 compatibility layer. `build-from-source`,
`build-from-source-static`, and `static-link` are not enabled; no SDL source
crate is built. SDL extension features (image, mixer, ttf, net, gfx) are not
enabled. The bindings retain their default pkg-config and vcpkg discovery
features.

The unchanged [SDL3 Zlib notice](licenses/SDL3-Zlib.txt) is retained for the
native runtime. The installed runtime version is controlled by the system,
not the root `Cargo.lock`; `SDL-3.4.18` in the bindings crate version identifies the
bindings' SDL version. The repository does not redistribute the system library
or its dependencies.

## Bundled shader

The optional Hyllian CRT effect adapts DOSBox Staging's MIT-licensed Hyllian
shader. Unlike the SDL API calls, this is imported source and is retained in
dedicated [shader files](../shaders/README.md), including the immutable upstream
revision, modifications and [complete MIT terms](../shaders/LICENSE-MIT.txt).
The generated SPIR-V, DXIL, HLSL and Metal source are distributed with the same
notices.
The import permits the project's candidate MIT/BSD or GPL licensing choices;
it does not select a license for authored project code.

Ordinary builds use bundled assets. Offline regeneration uses glslang,
SPIRV-Tools, Qt Shader Baker and Microsoft DXC as documented beside the shader;
these tools
are not linked into or shipped with REforge. Native presentation now requires
SDL 3.4 or newer for the custom GPU render-state API.

## Notice provenance

Files under `licenses/` reproduce complete upstream license files and the
complete HIDAPI source header without modifications, including upstream
whitespace. Paths below are relative to the named crate archive unless an
installed system path is given. Crate versions and checksums are recorded in
the root `Cargo.lock`.

| Retained file | Original file and source |
| --- | --- |
| `sdl3-0.20.0-MIT.txt` | `sdl3-0.20.0/LICENSE`; [source](https://docs.rs/crate/sdl3/0.20.0/source/LICENSE) |
| `sdl3-sys-0.7.2+SDL-3.4.18-Zlib.txt` | `sdl3-sys-0.7.2+SDL-3.4.18/LICENSE.md`; [source](https://docs.rs/crate/sdl3-sys/0.7.2+SDL-3.4.18/source/LICENSE.md) |
| `sdl3-sys-0.7.2+SDL-3.4.18-HIDAPI-NOTICE.txt` | First 24 lines of `sdl3-sys-0.7.2+SDL-3.4.18/src/generated/hidapi.rs`; [source](https://docs.rs/crate/sdl3-sys/0.7.2+SDL-3.4.18/source/src/generated/hidapi.rs) |
| `libc-0.2.190-MIT.txt` | `libc-0.2.190/LICENSE-MIT`; [source](https://docs.rs/crate/libc/0.2.190/source/LICENSE-MIT) |
| `bitflags-2.13.2-MIT.txt` | `bitflags-2.13.2/LICENSE-MIT`; [source](https://docs.rs/crate/bitflags/2.13.2/source/LICENSE-MIT) |
| `pkg-config-0.3.34-MIT.txt` | `pkg-config-0.3.34/LICENSE-MIT`; [source](https://docs.rs/crate/pkg-config/0.3.34/source/LICENSE-MIT) |
| `vcpkg-0.2.15-MIT.txt` | `vcpkg-0.2.15/LICENSE-MIT`; [source](https://docs.rs/crate/vcpkg/0.2.15/source/LICENSE-MIT) |
| `SDL3-Zlib.txt` | Installed `/usr/share/licenses/SDL3/LICENSE.txt`; [upstream](https://github.com/libsdl-org/SDL) |

The generated HIDAPI bindings retain Alan Ott's permissive notice in addition
to the bindings crate's Zlib license. The complete source header is retained
with its original comment markers.

## Settings UI

The shared settings menu uses `egui = "=0.31.1"`, with default features disabled
and only `default_fonts` enabled. It renders through our SDL geometry adapter;
`eframe`, `winit`, `wgpu`, extra platform backends, and a separate CPU UI
rasterizer are not dependencies. Headless engine builds omit this facility.
The locked additions, including target-specific and optional packages recorded
by Cargo, are listed below. Crate archive checksums remain in `Cargo.lock`.
MIT is selected for alternative-license code; Apache-only dependencies retain
their Apache terms. Fonts retain their own licenses independently of REforge.

| Dependency | Locked version | Upstream license expression | Retained terms | Notice source |
| --- | --- | --- | --- | --- |
| [ab_glyph](https://crates.io/crates/ab_glyph/0.2.32) | 0.2.32 | Apache-2.0 | [ab_glyph-0.2.32-Apache-2.0.txt](licenses/ab_glyph-0.2.32-Apache-2.0.txt) | `ab_glyph-0.2.32/LICENSE` |
| [ab_glyph_rasterizer](https://crates.io/crates/ab_glyph_rasterizer/0.1.10) | 0.1.10 | Apache-2.0 | [ab_glyph_rasterizer-0.1.10-Apache-2.0.txt](licenses/ab_glyph_rasterizer-0.1.10-Apache-2.0.txt) | `ab_glyph_rasterizer-0.1.10/LICENSE` |
| [ahash](https://crates.io/crates/ahash/0.8.12) | 0.8.12 | MIT OR Apache-2.0 | [ahash-0.8.12-MIT.txt](licenses/ahash-0.8.12-MIT.txt) | `ahash-0.8.12/LICENSE-MIT` |
| [ecolor](https://crates.io/crates/ecolor/0.31.1) | 0.31.1 | MIT OR Apache-2.0 | [egui-0.31.1-MIT.txt](licenses/egui-0.31.1-MIT.txt) | `egui tag 0.31.1, root LICENSE-MIT` |
| [egui](https://crates.io/crates/egui/0.31.1) | 0.31.1 | MIT OR Apache-2.0 | [egui-0.31.1-MIT.txt](licenses/egui-0.31.1-MIT.txt) | `egui tag 0.31.1, root LICENSE-MIT` |
| [emath](https://crates.io/crates/emath/0.31.1) | 0.31.1 | MIT OR Apache-2.0 | [egui-0.31.1-MIT.txt](licenses/egui-0.31.1-MIT.txt) | `egui tag 0.31.1, root LICENSE-MIT` |
| [epaint](https://crates.io/crates/epaint/0.31.1) | 0.31.1 | MIT OR Apache-2.0 | [egui-0.31.1-MIT.txt](licenses/egui-0.31.1-MIT.txt) | `egui tag 0.31.1, root LICENSE-MIT` |
| [epaint_default_fonts](https://crates.io/crates/epaint_default_fonts/0.31.1) | 0.31.1 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 | [egui-0.31.1-MIT.txt](licenses/egui-0.31.1-MIT.txt); font terms below | `egui tag 0.31.1, root LICENSE-MIT` |
| [lock_api](https://crates.io/crates/lock_api/0.4.14) | 0.4.14 | MIT OR Apache-2.0 | [lock_api-0.4.14-MIT.txt](licenses/lock_api-0.4.14-MIT.txt) | `lock_api-0.4.14/LICENSE-MIT` |
| [nohash-hasher](https://crates.io/crates/nohash-hasher/0.2.0) | 0.2.0 | Apache-2.0 OR MIT | [nohash-hasher-0.2.0-MIT.txt](licenses/nohash-hasher-0.2.0-MIT.txt) | `nohash-hasher-0.2.0/LICENSE-MIT` |
| [once_cell](https://crates.io/crates/once_cell/1.21.4) | 1.21.4 | MIT OR Apache-2.0 | [once_cell-1.21.4-MIT.txt](licenses/once_cell-1.21.4-MIT.txt) | `once_cell-1.21.4/LICENSE-MIT` |
| [owned_ttf_parser](https://crates.io/crates/owned_ttf_parser/0.25.1) | 0.25.1 | Apache-2.0 | [owned_ttf_parser-0.25.1-Apache-2.0.txt](licenses/owned_ttf_parser-0.25.1-Apache-2.0.txt) | `owned_ttf_parser-0.25.1/LICENSE` |
| [parking_lot](https://crates.io/crates/parking_lot/0.12.5) | 0.12.5 | MIT OR Apache-2.0 | [parking_lot-0.12.5-MIT.txt](licenses/parking_lot-0.12.5-MIT.txt) | `parking_lot-0.12.5/LICENSE-MIT` |
| [parking_lot_core](https://crates.io/crates/parking_lot_core/0.9.12) | 0.9.12 | MIT OR Apache-2.0 | [parking_lot_core-0.9.12-MIT.txt](licenses/parking_lot_core-0.9.12-MIT.txt) | `parking_lot_core-0.9.12/LICENSE-MIT` |
| [proc-macro2](https://crates.io/crates/proc-macro2/1.0.107) | 1.0.107 | MIT OR Apache-2.0 | [proc-macro2-1.0.107-MIT.txt](licenses/proc-macro2-1.0.107-MIT.txt) | `proc-macro2-1.0.107/LICENSE-MIT` |
| [profiling](https://crates.io/crates/profiling/1.0.18) | 1.0.18 | MIT OR Apache-2.0 | [profiling-1.0.18-MIT.txt](licenses/profiling-1.0.18-MIT.txt) | `upstream commit 8271551172eb6fa4cba47369aedd93790c623df9, root LICENSE-MIT` |
| [quote](https://crates.io/crates/quote/1.0.47) | 1.0.47 | MIT OR Apache-2.0 | [quote-1.0.47-MIT.txt](licenses/quote-1.0.47-MIT.txt) | `quote-1.0.47/LICENSE-MIT` |
| [redox_syscall](https://crates.io/crates/redox_syscall/0.5.18) | 0.5.18 | MIT | [redox_syscall-0.5.18-MIT.txt](licenses/redox_syscall-0.5.18-MIT.txt) | `redox_syscall-0.5.18/LICENSE` |
| [scopeguard](https://crates.io/crates/scopeguard/1.2.0) | 1.2.0 | MIT OR Apache-2.0 | [scopeguard-1.2.0-MIT.txt](licenses/scopeguard-1.2.0-MIT.txt) | `scopeguard-1.2.0/LICENSE-MIT` |
| [smallvec](https://crates.io/crates/smallvec/1.16.3) | 1.16.3 | MIT OR Apache-2.0 | [smallvec-1.16.3-MIT.txt](licenses/smallvec-1.16.3-MIT.txt) | `smallvec-1.16.3/LICENSE-MIT` |
| [syn](https://crates.io/crates/syn/2.0.119) | 2.0.119 | MIT OR Apache-2.0 | [syn-2.0.119-MIT.txt](licenses/syn-2.0.119-MIT.txt) | `syn-2.0.119/LICENSE-MIT` |
| [ttf-parser](https://crates.io/crates/ttf-parser/0.25.1) | 0.25.1 | MIT OR Apache-2.0 | [ttf-parser-0.25.1-MIT.txt](licenses/ttf-parser-0.25.1-MIT.txt) | `ttf-parser-0.25.1/LICENSE-MIT` |
| [unicode-ident](https://crates.io/crates/unicode-ident/1.0.27) | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | [unicode-ident-1.0.27-MIT.txt](licenses/unicode-ident-1.0.27-MIT.txt); [Unicode terms](licenses/unicode-ident-1.0.27-Unicode-3.0.txt) | `unicode-ident-1.0.27/LICENSE-MIT` |
| [version_check](https://crates.io/crates/version_check/0.9.5) | 0.9.5 | MIT/Apache-2.0 | [version_check-0.9.5-MIT.txt](licenses/version_check-0.9.5-MIT.txt) | `version_check-0.9.5/LICENSE-MIT` |
| [windows-link](https://crates.io/crates/windows-link/0.2.1) | 0.2.1 | MIT OR Apache-2.0 | [windows-link-0.2.1-MIT.txt](licenses/windows-link-0.2.1-MIT.txt) | `windows-link-0.2.1/license-mit` |
| [zerocopy](https://crates.io/crates/zerocopy/0.8.63) | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | [zerocopy-0.8.63-MIT.txt](licenses/zerocopy-0.8.63-MIT.txt) | `zerocopy-0.8.63/LICENSE-MIT` |
| [zerocopy-derive](https://crates.io/crates/zerocopy-derive/0.8.63) | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | [zerocopy-derive-0.8.63-MIT.txt](licenses/zerocopy-derive-0.8.63-MIT.txt) | `zerocopy-derive-0.8.63/LICENSE-MIT` |

The shared egui MIT notice is from upstream tag
[`0.31.1`](https://github.com/emilk/egui/blob/0.31.1/LICENSE-MIT);
these egui crate archives declare the same alternative licenses but do not
include root license files. The profiling MIT notice is from its crate's
recorded [upstream revision](https://github.com/aclysma/profiling/blob/8271551172eb6fa4cba47369aedd93790c623df9/LICENSE-MIT).
Other retained code notices are copied without modification from the listed
crate archives. No upstream application or renderer implementation is copied.

Default font bytes are supplied unmodified by `epaint_default_fonts`, rather
than extracted from game data. Retained font terms are copied from that archive:

- Hack: [MIT/Bitstream Vera notices](licenses/epaint-default-fonts-0.31.1-Hack-Regular.txt),
  including the public-domain DejaVu contributions.
- Noto Emoji: [SIL Open Font License 1.1](licenses/epaint-default-fonts-0.31.1-OFL.txt);
  copyright 2013 Google Inc., All Rights Reserved (embedded font notice).
- Ubuntu Light: [Ubuntu Font Licence 1.0](licenses/epaint-default-fonts-0.31.1-UFL.txt);
  copyright 2011 Canonical Ltd. (embedded font notice).
- Emoji Icon Font: [MIT notice](licenses/epaint-default-fonts-0.31.1-emoji-icon-font-mit-license.txt).

These dependencies permit their use in this combined application subject to
their retained terms; they do not select the project's authored-code license.
