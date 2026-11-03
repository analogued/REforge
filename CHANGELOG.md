# Changelog

Notable user-facing changes are grouped by area, with the newest releases first.
Unreleased contains changes intended for the next public release.

## Unreleased

### New games

- Added experimental support for Mindwheel (DOS).
- Added experimental support for Essex (DOS).

### General

- Restore the last selected library entry when reopening the launcher.
- Consistent top-left option badges for preview images and initials fallbacks.
- Fixed washed-out disabled launcher choices in Soft Tonal, Warm Amber and
  Crisp Light, and unreadable text on focused settings controls.

### Mach 3

- Interpolated scene movement is now the default, retaining the original
  mountains. Original scene movement remains selectable, with interpolated or
  original player craft movement. Gameplay timing and collisions are unchanged.

### Alley Cat

- Optional button-based jumping and dropping, with controller layout templates
  and diagrams in Settings. Swimming retains full directional control.
- Fixed twitching while approaching milk bowls with Responsive input.

### BTZ

- Optional terminal play through `--renderer terminal`.
- Read and write the original DOS bookmark format, allowing saves to be
  exchanged with the original games.

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
