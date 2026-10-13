# Dependencies and notices

The optional `blip` feature uses the pure Rust
[`blip_buf` 0.2.1](https://docs.rs/crate/blip_buf/0.2.1), pinned exactly in
`Cargo.toml` and the workspace lockfile. The published crate records upstream
revision [`06c66ea66151a9d7a3d338342165a1b0a0bf7303`](https://github.com/mvdnes/blip_buf-rs/tree/06c66ea66151a9d7a3d338342165a1b0a0bf7303).
It has no further dependencies and requires no C or C++ library.

The package declares MIT, copyright © 2015 Mathijs van de Nes. Its complete
[MIT license text](licenses/blip_buf-MIT.txt) is retained here. The dependency is
used through its ordinary Rust API without source modifications or vendoring.
The authored adapter is `src/render/blip.rs`; no upstream implementation has
been copied into it. Dependency notices do not license REforge's authored code,
whose repository-wide license remains undecided.
