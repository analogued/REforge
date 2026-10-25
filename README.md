# REforge

Play classic DOS games on a modern desktop, using your original game files.
REforge recreates their behavior natively, with modern display, audio and
controller options. Linux is the current development platform.

## Supported engines

| Engine | Games | Player guide |
| --- | --- | --- |
| Alley Cat | Alley Cat | [Playing Alley Cat](docs/games/alleycat.md) |
| Mach 3 | Mach 3 | [Playing Mach 3](docs/games/mach3.md) |

Each engine currently supports one verified DOS release. See its guide for the
required files and current limitations. Original game data is not included.

## Features

- Sharp pixels or optional Hyllian CRT scanlines.
- Automatic window sizing, manual scaling and borderless fullscreen.
- Keyboard and gamepad controls.
- Separate master, music, effects and speech levels, where the game supports them.
- Shared preferences with per-game overrides, changed through an in-game menu.

Game-specific features, including Alley Cat's Responsive controls and Mach 3's
Enhanced PCM audio, are described in the player guides.

## Getting started

Build from the repository root with Rust 1.85 or newer, Cargo and SDL3 3.4 or
newer development/runtime libraries installed:

```sh
cargo build --locked --release -p reforge
```

The application is written to `target/release/reforge` unless you select another
Cargo output directory. Launch a game using its original files:

```sh
./target/release/reforge --original /path/to/alley-cat
./target/release/reforge --original /path/to/mach3
```

Use the directories containing the required files, as described in each engine's
player guide.
REforge detects the game from its files; there is no separate game selector.

## Settings and shared controls

Press **F10** to open Settings. Choose windowed or fullscreen play, adjust scaling,
try CRT presentation, or change audio levels and your controller's stick dead zone.
**Apply** activates and saves edits; **Cancel** discards them. Preferences can be
shared globally or overridden for an individual game.

| Action | Keyboard | Gamepad |
| --- | --- | --- |
| Pause / resume | P or Pause/Break | Start/Menu |
| Open / close Settings | F10 | Back/View |
| Quit | Ctrl+Q | — |

Game-specific controls are in the player guides. The left stick or D-pad supplies
movement, and the bottom face button performs the game's main action.

See the [settings reference](docs/configuration.md) for all options, saved-file
locations and command-line overrides. For available launch options, run:

```sh
./target/release/reforge --original /path/to/game --help
```

## Development

Build with `--features dev-tools` to enable diagnostics. During play,
**F9** toggles a small FPS indicator in either game, including in Settings.
It starts hidden and reports completed frame presentations averaged over
half-second intervals, rather than simulation updates or monitor refresh rate.

REforge is under active development. Please include the game, your system,
REforge revision, relevant settings and steps to reproduce when reporting a bug.

For source layout, validation and contribution guidance, see
[CONTRIBUTING.md](CONTRIBUTING.md) and the technical engine documentation for
[Alley Cat](engines/alleycat/README.md) and [Mach 3](engines/mach3/README.md).

## License

The project license has not yet been selected. Third-party terms and attribution
are in [THIRD_PARTY.md](THIRD_PARTY.md), with license texts in
[LICENSES/](LICENSES/). Release packages include a combined third-party notices
document; see [release packaging](docs/releases.md).
Original game data is not included or licensed by this repository.
