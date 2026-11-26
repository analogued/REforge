# NihAV

The five crates here come from [NihAV's official source bundle](https://nihav.org/nihav.gitbundle)
at revision `13f31633cf325b0036d697af7bf05c611b4b8967`. REforge uses
them as Cargo path dependencies. The source here already includes the local
patches, so a normal build needs only the REforge checkout and its ordinary
Cargo dependencies. See the [third-party inventory](../../THIRD_PARTY.md#nihav-quicktime-support)
and [AGPLv3 license](../../LICENSES/AGPL-3.0.txt).

## Rebuild or update the import

From the REforge repository root, set `upstream` to an absolute path to a
local NihAV clone made from the source bundle. To test an update, set
`revision` to the candidate commit. The commands stage the result in a
temporary directory.

```sh
set -eu
upstream=/absolute/path/to/nihav
revision=13f31633cf325b0036d697af7bf05c611b4b8967
vendor="$(pwd)/third_party/nihav"
snapshot=$(mktemp -d)

git -C "$upstream" archive -o "$snapshot/upstream.tar" "$revision" -- \
    nihav-core nihav-codec-support nihav-registry nihav-commonfmt nihav-qt
tar -xf "$snapshot/upstream.tar" -C "$snapshot"

for patch in "$vendor"/patches/*.patch; do
    (cd "$snapshot" && git apply --check "$patch" && git apply "$patch")
done
```

For the pinned revision, compare the result with every checked-in crate.

```sh
for crate in "$snapshot"/nihav-*; do
    diff -r -- "$crate" "$vendor/${crate##*/}"
done
```

`rustfmt.toml` is local to REforge and sits beside the imported crates.

For a new revision, reconcile patches that no longer apply against upstream's
changes. Remove fixes that upstream now provides and regenerate the remaining
patches against the new revision. Replace the five crate directories with the
verified result, update the pin here and in `THIRD_PARTY.md`, and compare
upstream's license and notices. Run the workspace checks in
[CONTRIBUTING.md](../../CONTRIBUTING.md#checks), including the Rust 1.88
check, and rerun the affected engine's original-media checks.
