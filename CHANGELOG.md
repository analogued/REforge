# Changelog

Notable user-facing changes are grouped by area, with the newest releases first.
Unreleased contains changes intended for the next public release.

## Unreleased

### New games

- Added support for Centurion: Defender of Rome (DOS 1.00A and 1.00B, with
  VGA and optional EGA graphics).

### General

- Show concrete sound renderer choices in per-game Audio settings.
- Soften PC speaker output with shared small-speaker filtering across games,
  using DOSBox Staging's response parameters.
- Group Alley Cat and Mach 3 motion options and Alley Cat input response under
  Enhancements in Settings.
- Use equal-height previews in Split view, with Play aligned to the preview when
  text fits. Choose a height from 144 to 192 logical pixels (180 by default) on
  the separate Launcher page in Settings.
- Set the launcher minimum to 630 × 520 logical pixels so Gallery can fit six
  4:3 cards in two rows. Choose 4:3, Per row (default), or Whole gallery
  under Launcher settings to set how Gallery chooses its preview frame aspect.
- Give Gallery cards more room in scrolling libraries. Keep two rows in view
  where possible, and use extra rows in taller windows when the library fits.
- Keep launcher previews responsive during window resizing by scaling cached
  artwork and rebuilding the final-size previews in the background.
- Show larger game palettes as launcher mosaics, using taller shapes in Gallery
  to leave more room for game names while keeping card heights unchanged. Arrange
  16-colour swatches in four rows in Gallery and two rows in List and Split,
  preserving their original order, styling and existing line spacing.
- Restore pending display choices correctly when switching back before Apply,
  including renderer backends, manual scaling and explicit global defaults.

### Alley Cat

- Add Reforged scene movement for smoother actors and animations, keeping
  Original movement selectable independently of responsive controls.
- Stop dog-catch sounds promptly and restore ordinary room-completion sound
  cutoffs.
- Let the controller's Jump button jump from solid floors even while Down is
  held, keeping Drop available on higher surfaces.
- Add support for three more variants: NewRisingSun's Tandy enhancement, the
  DOS disk image and the PC Booter release.

### Centurion

- Choose AdLib or the original PC speaker sound arrangement independently of
  video mode, from launcher options or Audio overrides in Settings. The choice
  is saved for each library entry and applies on the next launch.
- Read and write DOS campaign saves, allowing progress to be shared with
  the original game.
- Reforged UI options include territory border highlighting and indicators
  for applied tribute and difficulty levels, with Original presentation
  available.
- Expanded enemy fleet information shows ship counts and embarked troops.
  Optional map hover information identifies places, legions and fleets.
- Mouse and keyboard menu feedback, with an optional Dynamic decoration
  style that follows the last input method.

## 2026.2.1 (2026-11-06)

### General

- Speed up original-file verification with BLAKE3 and reduce redundant checks
  during library updates, game launches and preview generation; load valid
  cached previews without reading originals.
- Recognize renamed original game files by their verified contents, regardless
  of filename or extension, within the game root and catalog-defined
  subdirectories.
- Fix launcher filtering, retain valid launch choices when another preference is
  invalid, keep Relocate available when game files are missing, and use the
  saved global controller dead zone for launcher navigation.
- Use Esc to open Settings during graphical play, keeping its cancel/back
  behavior inside dialogs and retaining Ctrl+Q to quit.
- Identify temporary game settings for unregistered launches and disable
  next-launch-only edits that cannot be saved.
- Keep dialogs, text and images visible after changing the graphical renderer,
  including when a failed change restores the previous renderer.

### Mach 3

- Added support for two additional DOS variants.

### BTZ

- Browse bookmarks and transcripts through a framed text interface, with
  keyboard, mouse and controller navigation, mouse feedback, overwrite
  confirmation and selectable cursor styles shared with command input. Terminal
  play shares the browser's keyboard navigation and uses native terminal cursor
  shapes.
- Fix Breakers' first input appearing one line below the visible cursor.

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
