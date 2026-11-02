# Changelog

Notable user-facing changes are grouped by area, with the newest releases first.
Unreleased contains changes intended for the next public release.

## Unreleased

### New games

- Added experimental support for Mindwheel (DOS).

### General

- Consistent top-left option badges for preview images and initials fallbacks.
- Fixed washed-out disabled launcher choices in Soft Tonal, Warm Amber and
  Crisp Light by using the same fading as other disabled controls.

### Mindwheel

- Require the verified `SIDE.TXT` disk map alongside `ADISK.OBJ` and `GO.EXE`
  during detection and launcher verification.

- SDL text presentation with the shared display settings and CRT filter.
- Optional terminal play through `--renderer terminal` using an attached terminal.
- Launcher options for graphical/terminal play, sharp pixels/CRT and
  windowed/fullscreen, retaining graphical preferences during terminal play.
  Click a saved choice again to follow defaults, as with other games.
- Initials fallback artwork with a monochrome text palette strip.
- Ctrl+G keyboard bell: PC-speaker audio in windowed play, respecting audio
  settings, and immediate BEL output in terminal play.
- Native bookmarks and transcripts, with filename prompts in either frontend.

## 2026.1.0 (2026-11-01)

### New games

- Added support for Alley Cat (DOS).
- Added support for Mach 3 (DOS).

### General

- A persistent launcher library with game-file verification, per-entry launch
  options and relocation of game files.
- Split, Gallery and Compact launcher layouts, with Classic Dark, Soft Tonal,
  Warm Amber and Crisp Light themes.
- Cached, engine-generated game previews with symbolic placeholders as a fallback.
- Shared settings with global defaults and per-entry overrides, including display
  scaling, windowed/fullscreen modes, Hyllian CRT presentation and audio levels.
- Keyboard and gamepad controls, including navigation of the settings menu.

### Alley Cat

- CGA and PCjr graphics paths.
- Responsive input, with original input polling available in Settings.

### Mach 3

- Colour and monochrome graphics modes.
- PC speaker output and optional Enhanced PCM audio.

### Linux

- Initial desktop platform support.
