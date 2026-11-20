<!-- Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later -->

# Cryo DOS archive directory

`reforge_cryo_archive` reads the ordered archive directory observed in
Dune's DOS CD `DUNE.DAT`, Dragon Lore Gold's `DRAGON1` and `DRAGON2`, and
Lost Eden's DOS CD `EDEN.DAT`. Callers supply a `reforge_disc::Region`
spanning the archive. This works with a standalone archive file or a member
of a disc image.

The 64 KiB directory begins with a little-endian 16-bit slot count. Each
25-byte record contains a 16-byte name, a little-endian 32-bit length and
absolute archive offset, and one flag byte. An all-zero record ends the
directory early. The observed flag bytes are zero. Names are read as UTF-8
up to the first NUL, preserving the original case, path separators and
duplicate records. Member extents are checked against the directory and
archive bounds. Payloads stay in bounded regions for callers to read or
stream as needed.

The caller owns name matching, active archive choice and resource fallback.
The parser is licensed under GPL-3.0-or-later. See the [workspace license](../../LICENSE).

Validate with `cargo test --locked -p reforge_cryo_archive` and
`cargo clippy --locked -p reforge_cryo_archive --all-targets -- -D warnings`.
