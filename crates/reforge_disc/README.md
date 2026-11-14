<!-- Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later -->

# REforge disc reader

`reforge_disc` indexes files in a caller-selected cooked ISO-9660 image or
extracted disc directory, and exposes bounded read-only streams. It has no
runtime dependencies and contains no original game data. Authored code and
this documentation use GPL-3.0-or-later; see the workspace [license](../../LICENSE).

Each engine can layer its own archive formats and resource selection above this
reader. This crate preserves directory hierarchy and never runs an installer,
mounts an image, writes originals, selects a game release, or expands archives.
The application's detector remains responsible for choosing and verifying the
physical source; normal engines should obtain that path from its resolved file
roles. Parser checks remain necessary for synthetic and research inputs too.

```rust,no_run
use std::io::{Read, Seek, SeekFrom};
use reforge_disc::Disc;

# fn inspect(selected_image: &std::path::Path) -> std::io::Result<()> {
let disc = Disc::open(selected_image)?;
let data = disc.get("DATA/GAME.DAT")?;
let mut stream = data.open()?;
stream.seek(SeekFrom::Start(128))?;
let mut header = [0; 16];
stream.read_exact(&mut header)?;
let section = data.slice(4096, 1024)?.open()?;
# let _ = section;
# Ok(())
# }
```

`Disc::files()` exposes an ordered `BTreeMap` of ASCII uppercase, `/`-separated
root-relative paths. `get()` performs ASCII case folding on a complete logical
path; it does not flatten basenames or search alternate directories. Ambiguous
case-folded paths are rejected. Empty, dot and parent components, non-printable
or non-ASCII names, and DOS-incompatible reserved separators/punctuation are
rejected. ISO file-version suffix `;1` and a trailing file-name dot are removed
while indexing; queries use the resulting logical name. Directory-tree symlinks
and other non-regular entries are unsupported. A symlink selecting the source
itself is resolved once when opening the caller's selected source.

`Region` is a cheap cloneable view of a physical byte range. `slice()` validates
that a subrange stays inside its enclosing region. `open()` creates a reader
with an independent position; readers can be interleaved safely even when their
regions share an image file. Reads stop at the region boundary, so a decoder
cannot spill into a neighboring disc file. Seeking beyond the logical end is
allowed, like `std::fs::File`; reads there return EOF. Negative or overflowing
seeks fail without moving the cursor. Shortened sources produce errors rather
than exposing bytes outside a region.

Indexing reads metadata only. Large files have no payload-size cap and remain
streamed; an engine can open a small subrange of a file hundreds of megabytes
long. `read_at()` is an explicit allocation for callers that want a selected
range copied into memory, and checks the range before allocating.

`Limits` separately bounds entry count, nesting depth and indexing bytes.
Defaults are 100,000 files/directories, depth 16 below the root, and 16 MiB of
ISO directory-record bytes or normalized full-path bytes for directory sources.
Directory metadata budgets do not claim to measure allocator overhead. Engines
may choose tighter limits without imposing a file payload limit.

The initial image profile supports the primary ISO-9660 name tree with 2048-byte
logical sectors, one volume and ordinary contiguous extents. It validates
little/big-endian copies, extent containment, record boundaries, descriptor
termination, duplicate names and directory cycles. Unsupported layouts include
raw BIN/CUE sector streams, multi-volume or multi-extent files, interleaving,
extended attributes and file versions other than 1. Joliet and Rock Ridge names
are not interpreted; a compatible primary tree can still be read. No broader
optical-media or filesystem support is implied.

Public tests use authored directory/ISO fixtures and sparse files. From the
workspace, run `cargo test --locked -p reforge_disc` and
`cargo clippy --locked -p reforge_disc --all-targets -- -D warnings`.
