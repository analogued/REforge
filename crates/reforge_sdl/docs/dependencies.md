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
The generated SPIR-V and Metal source are distributed with the same notices.
The import permits the project's candidate MIT/BSD or GPL licensing choices;
it does not select a license for authored project code.

Ordinary builds use bundled assets. Offline regeneration uses glslang,
SPIRV-Tools and Qt Shader Baker as documented beside the shader; these tools
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
