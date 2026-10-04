# Viewer dependencies and notices

The optional `viewer` feature uses `sdl2 = "=0.38.0"` with default features
disabled. `Cargo.lock` fixes the Rust dependencies below. The default library
and frame renderer do not require these dependencies. No third-party runtime
source is copied into this repository; compiled Rust dependencies still require
their notices to accompany distributed viewer binaries.

| Dependency | Locked version | Purpose | Upstream license; terms used here |
| --- | --- | --- | --- |
| [sdl2](https://crates.io/crates/sdl2/0.38.0) | 0.38.0 | Window, events, texture presentation | MIT; [MIT notice](licenses/rust-sdl2-0.38.0-MIT.txt) |
| [sdl2-sys](https://crates.io/crates/sdl2-sys/0.38.0) | 0.38.0 | Raw SDL bindings and native linking | MIT AND Zlib; [shared MIT notice](licenses/rust-sdl2-0.38.0-MIT.txt), [SDL Zlib notice](licenses/SDL-2.26.4-Zlib.txt) |
| [libc](https://crates.io/crates/libc/0.2.190) | 0.2.190 | C ABI types and functions | MIT OR Apache-2.0; [MIT selected](licenses/libc-0.2.190-MIT.txt) |
| [bitflags](https://crates.io/crates/bitflags/1.3.2) | 1.3.2 | SDL flag types | MIT/Apache-2.0 (upstream's alternative-license spelling); [MIT selected](licenses/bitflags-1.3.2-MIT.txt) |
| [lazy_static](https://crates.io/crates/lazy_static/1.5.1) | 1.5.1 | SDL shared state initialization | MIT OR Apache-2.0; [MIT selected](licenses/lazy_static-1.5.1-MIT.txt), [copyright notice](licenses/lazy_static-1.5.1-NOTICE.txt) |
| [cfg-if](https://crates.io/crates/cfg-if/1.0.5) | 1.0.5 | Platform configuration in the bindings build | MIT OR Apache-2.0; [MIT selected](licenses/cfg-if-1.0.5-MIT.txt) |
| [version-compare](https://crates.io/crates/version-compare/0.1.1) | 0.1.1 | Version comparison in the bindings build | MIT; [MIT notice](licenses/version-compare-0.1.1-MIT.txt) |

MIT is selected wherever upstream offers a choice. Retain the applicable copyright
and permission notices with distributed viewer binaries. These dependency terms
do not select a license for this project's authored code.

## Native runtime

The viewer dynamically links a system-provided SDL2 ABI library. Neither
`bundled` nor `static-link` is enabled. The SDL source shipped inside the
`sdl2-sys` crate is therefore not built or vendored here. SDL2 extension libraries
(image, mixer, ttf, gfx) are not enabled.

A system may provide native SDL2 or an SDL2 compatibility layer over SDL3.
Unchanged [sdl2-compat](licenses/sdl2-compat-Zlib.txt) and
[SDL3](licenses/SDL3-Zlib.txt) license notices are retained for those runtime
implementations as well. The runtime version is controlled by the system
installation, not `Cargo.lock`.

SDL's Zlib terms require accurate attribution of origin, marking altered source
versions, and preserving the notice in source distributions. These terms do not
require publishing application source. If distributing a native runtime with the
viewer, retain the notices for that exact runtime and its included components;
the current system libraries and their other dependencies are not redistributed
by this repository. Bundling SDL or enabling other features requires reviewing
the additional components and notices actually included.

## Notice provenance

Files under `licenses/` reproduce upstream text without modifications, including
upstream whitespace. Paths below are relative to the named crate archive unless
an installed system path is given. Crate versions and checksums are recorded in
`Cargo.lock`; source archives are available through the package links above.

| Retained file | Original file and source |
| --- | --- |
| `rust-sdl2-0.38.0-MIT.txt` | `sdl2-0.38.0/LICENSE`; [source](https://docs.rs/crate/sdl2/0.38.0/source/LICENSE) |
| `libc-0.2.190-MIT.txt` | `libc-0.2.190/LICENSE-MIT`; [source](https://docs.rs/crate/libc/0.2.190/source/LICENSE-MIT) |
| `bitflags-1.3.2-MIT.txt` | `bitflags-1.3.2/LICENSE-MIT`; [source](https://docs.rs/crate/bitflags/1.3.2/source/LICENSE-MIT) |
| `lazy_static-1.5.1-MIT.txt` | `lazy_static-1.5.1/LICENSE-MIT`; [source](https://docs.rs/crate/lazy_static/1.5.1/source/LICENSE-MIT) |
| `lazy_static-1.5.1-NOTICE.txt` | First six lines of `lazy_static-1.5.1/src/lib.rs`; [source](https://docs.rs/crate/lazy_static/1.5.1/source/src/lib.rs) |
| `cfg-if-1.0.5-MIT.txt` | `cfg-if-1.0.5/LICENSE-MIT`; [source](https://docs.rs/crate/cfg-if/1.0.5/source/LICENSE-MIT) |
| `version-compare-0.1.1-MIT.txt` | `version-compare-0.1.1/LICENSE`; [source](https://docs.rs/crate/version-compare/0.1.1/source/LICENSE) |
| `SDL-2.26.4-Zlib.txt` | `sdl2-sys-0.38.0/SDL/LICENSE.txt`; [source](https://docs.rs/crate/sdl2-sys/0.38.0/source/SDL/LICENSE.txt) |
| `sdl2-compat-Zlib.txt` | Installed `/usr/share/licenses/sdl2-compat/LICENSE.txt`; [upstream](https://github.com/libsdl-org/sdl2-compat) |
| `SDL3-Zlib.txt` | Installed `/usr/share/licenses/SDL3/LICENSE.txt`; [upstream](https://github.com/libsdl-org/SDL) |

The `sdl2-sys` archive declares `MIT AND Zlib` but omits a standalone MIT license
file. Its Rust bindings share the rust-sdl2 repository's root MIT license,
retained from `sdl2` at the same source revision
`52de59121c087d43b555f6d9e1aaae35a0cb0ed6` (both archives' `.cargo_vcs_info.json`).
The separate SDL license is retained without copying the unused bundled SDL
source or its third-party source trees. `lazy_static`'s MIT file has no copyright
line; its exact source header supplies the additional copyright notice.
