# Changelog

Notable user-facing changes are grouped by area, with the newest releases first.
Unreleased contains changes intended for the next public release.

## Unreleased

### General

- Speed up original-file verification with BLAKE3 and reduce redundant checks during library updates, game launches and preview generation; load valid cached previews without reading originals.
- Recognize renamed original game files by their verified contents, regardless of filename or extension, within the game root and catalog-defined subdirectories.
- Fix launcher filtering, retain valid launch choices when another preference is invalid, keep Relocate available when game files are missing, and use the saved global controller dead zone for launcher navigation.
- Identify temporary game settings for unregistered launches and disable next-launch-only edits that cannot be saved.
- Keep dialogs, text and images visible after changing the graphical renderer,
  including when a failed change restores the previous renderer.

### BTZ

- Browse bookmarks and transcripts through a framed text interface, with
  keyboard, mouse and controller navigation, overwrite confirmation and
  selectable cursor styles shared with command input. Terminal play shares the
  browser's keyboard navigation and uses native terminal cursor shapes.
- Fix Breakers' first input appearing one line below the visible cursor.
- Temporarily enlarge small game windows to fit Settings and restore their size
  when it closes.

## 2026.2.0 (2026-11-04)

### New games

- Added experimental support for Mindwheel (DOS).
- Added experimental support for Essex (DOS).
- Added experimental support for Brimstone: The Dream of Gawain (DOS).
- Added experimental support for Breakers (DOS).

### General

- Truncate long library titles with an ellipsis so they stay inside their rows
  and cards, preserving the full title in Split details.
- Hide the idle mouse cursor during graphical play, including fullscreen.
- Restore the last selected library entry when reopening the launcher.
- Consistent top-left option badges for preview images and initials fallbacks.
- Fixed washed-out disabled launcher choices in Soft Tonal, Warm Amber and
  Crisp Light, and unreadable text on focused settings controls.

### Mach 3

- Reforged scene movement is now the default, retaining the original
  mountains. Original scene movement remains selectable, with Enhanced or
  Original player craft movement. Gameplay timing and collisions are unchanged.

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
