# `centurion` engine

The `centurion` crate reconstructs DOS 1.00A and 1.00B of Centurion: Defender
of Rome for [REforge](../../README.md). It implements province management,
diplomacy, land and naval battles, gladiator shows, chariot races, Cleopatra and
the closing presentations. It reads original resources without executing the
DOS program or sound driver. Audio currently implements AdLib output.

Both releases support VGA, with EGA available when all six additional EGA bank
pairs match the application catalog. The launcher Video choice and
`--video vga|ega` select the available original resource banks. Both modes use
the same 320×200 logical geometry and DOS save format. Application detection
selects the release layout and graphics capabilities independently and verifies
all required roles; engines read those exact resolved paths, including renamed
resources.

The loader adapts 1.00B's moved naval text and footer pointers to the native
runtime's shared layout without changing its version, credits, new-game data
or save blocks. EGA selects the E-prefixed graphics banks and menu sprites,
direct nibble colour maps and EGA drawing pens. Its packed full-screen images
replace the VGA raw backgrounds. VGA-only DAC cycling and cached expansion
aliasing are disabled for EGA; normal sprite animation and game timing remain.

For installation, controls, difficulty, saves and presentation options, see the
[player guide](../../docs/games/centurion.md). Original game assets and saved
games are external inputs and are not included or licensed by this repository.

## Technical references

- [Original DOS engine](docs/original-engine.md): partial save layout, difficulty,
  input presentation, enemy fleet records and movement/battle behavior.
- [Intentional deviations](docs/deviations.md): presentation options, input
  feedback, added inspection tables and bounded invalid states.
- [Validation and limits](#validation-and-limits): feature checks, supported
  scope and unresolved timing/undefined-state behavior.
- [Development recording](#development-recording): research frontend and retained
  recording contents.

These references distinguish original behavior from replacement behavior.
Verified original knowledge remains useful independently of replacement support;
raw captures, dumps and investigation scripts are not repository dependencies.

## Implementation and loading

| Module | Responsibility |
| --- | --- |
| [`assets`](src/assets.rs), [`graphics`](src/graphics.rs) | Checked resource decoding, original fonts/bitmaps and indexed frame drawing |
| [`state`](src/state.rs), [`rng`](src/rng.rs) | Campaign data, original save blocks and random-number rules |
| [`session`](src/session.rs) | Input, scene transitions, menus and composition of gameplay systems |
| [`campaign`](src/campaign.rs), [`map_hover`](src/map_hover.rs) | Map symbols, ownership borders and optional hover names |
| [`battle`](src/battle.rs), [`naval`](src/naval.rs), [`games`](src/games.rs) | Land/naval combat and provincial games, with supporting modules for their simulation |
| [`music`](src/music.rs), [`sound`](src/sound.rs), [`audio`](src/audio.rs) | Sequencing, sound requests and OPL synthesis |
| [`player`](src/player.rs), [`audio_output`](src/audio_output.rs) | Shared SDL host, settings, input adaptation and callback playback |
| [`launcher`](src/launcher.rs), [`launcher_palette`](src/launcher_palette.rs) | Preview rendering and original-resource color extraction |

`centurion::cli::run(args, files, profile)` takes `OsString` arguments excluding the
executable name, a `reforge_inputs::ResolvedFiles` map and a typed `LoadingProfile`.
The profile holds the `DosRelease` layout and verified EGA capability.
Application detection owns release identification and full-file verification.
Playback and launcher
previews read exactly those selected paths, including renamed resources, without
searching for substitutes. Structural parsing and bounds checks remain in the
engine.

Normal play needs `OVL0.OVL` and the `DAT`/`DIR` pairs for `CENT`, `MINIT`, `MMAP`,
`BATL`, `MGLAD`, `MRACE`, `MNAVAL`, `NAVL` and `MNEGO`. The nineteen resource
roles are selected by the application catalog for either VGA-only profile. EGA
profiles additionally require the six E-prefixed bank pairs listed in the player
guide, for 31 roles in total. The 1.00A EGA profile uses the verified 1.00B bank
hashes, based on matching shared artwork and compatible resource names/layouts;
it does not identify a separately examined historical 1.00A EGA package.
`CENTURIO.EXE` is not a normal player dependency. Launch through REforge as
described in the player guide.

The default `sdl` feature enables interactive playback and launcher previews.
`--no-default-features` excludes SDL while retaining headless simulation and raw
research tools. The standalone `centurion-prototype` uses explicit paths and
original filenames rather than application release detection. Its
`run_prototype()` entry point and separate window host serve research, not normal
player integration.

### Launcher palette

The launcher derives its color mosaic while preparing a preview from the selected
resources. EGA exposes its sixteen active colours. VGA counts each stored visible
artwork pixel once, including both raw
backgrounds, then includes cached bitmap expansion, province uniform remaps and
procedural drawing pens. Colors found only through those runtime rules receive
weight one. The extracted palette preserves CMAP order and RGB values; the shared
launcher applies its display arrangement. These weights describe stored artwork,
not time on screen, reachability or unobscured coverage during play.

## Development recording

Run commands from the combined repository root, using the dependencies described
in [Getting started](../../README.md#getting-started). Recording requires the
standalone research frontend's opt-in `dev-tools` feature and installed `b3sum`.
Debug/release optimization is independent of this feature. The normal REforge
player does not expose recording.

```sh
cargo run --locked -p centurion --bin centurion-prototype --features dev-tools -- \
  --original /path/to/DOS --state-dir /path/to/saves \
  --record /path/to/new-recording
```

Supply the original-named resource set above; `CENTURIO.EXE`, when present, also
has its identity recorded. The selected video mode is retained for replay. The
output directory must be new; an existing recording is refused.
F12 adds a marker without a game action. Use `centurion-prototype --help` for the
frontend's other diagnostic arguments.

Recordings retain logical inputs, timer advances, RNG and sound requests,
compressed frame/palette/state checkpoints, initial save slots and a copy of
the running native executable, with BLAKE3 identities. Retaining that executable
supports replay against the recorded build. Output contains original-derived
data and belongs outside source control. These traces support aligned comparisons;
they do not automatically translate a whole playthrough into DOS controls or
establish full equivalence.

## Validation and limits

Synthetic tests require no original data or desktop. From the repository root:

```sh
cargo fmt --all -- --check
cargo test --locked -p centurion
cargo test --locked -p centurion --no-default-features
cargo test --locked -p centurion --features dev-tools
cargo clippy --locked -p centurion --all-targets -- -D warnings
cargo clippy --locked -p centurion --all-targets --features dev-tools -- -D warnings
cargo clippy --locked -p centurion --all-targets --no-default-features -- -D warnings
```

Shared changes also follow the workspace checks in
[CONTRIBUTING.md](../../CONTRIBUTING.md#validation). Original-data comparisons
remain external. They cover selected frames, routines and complete play sequences,
not every possible campaign. Playable scope does not establish exhaustive DOS
parity. Other releases/platforms and non-AdLib sound modes remain unsupported.

Enemy inspection has a known co-location difference: the native selection path
prioritizes local Roman fleets. Controlled DOS captures instead follow the
original roaming-army/permanent-fleet lookup described in the
[DOS reference](docs/original-engine.md#enemy-fleets-and-transported-armies). The
added information panels do not resolve this selection difference.

Logical ticks preserve recovered durations, but CPU drawing cost, VGA retrace,
interrupt timing and cursor refresh are not cycle exact. Naval busy-loop AI uses
a calibrated execution budget; its RNG cadence can differ with DOS emulated CPU
speed. Original uninitialized stack values in some racing/enemy paths are not
generally reproducible. The deliberate exhausted-arena safeguard is documented
in [deviations](docs/deviations.md#invalid-arena-offers).

Interactive audio runs independently of rendering. Pacing uses estimated consumed
source samples, falling back to wall time when audio is unavailable. Muting the
normal player retains synthesis and its clock. Late requests apply at the next
unrendered sample; this does not measure DAC latency or make every scene
transition sample exact. Headless research stepping instead uses explicit logical
inputs. See [host presentation and sound](docs/deviations.md#host-presentation-and-sound)
for host controls and [the DOS reference](docs/original-engine.md#movement-and-battle-behavior)
for the original movement/music boundary.

## License

Authored source and documentation use [GPL-3.0-or-later](../../LICENSE).
Dependency notices and license texts are maintained in the repository's
[central inventory](../../THIRD_PARTY.md).
