# Mach 3

Steer and shoot through the DOS version of Mach 3, with its animated opening,
recorded speech and music, and procedural sound effects. REforge offers the
original PC Speaker presentation and an alternative Enhanced PCM audio renderer.

> [!IMPORTANT]
> REforge does not include original Mach 3 assets or data. You must provide
> the original files from a supported release. See [What you need](#what-you-need)
> below for details.

## What you need

Supply the original DOS game directory, keeping **MACH3.EXE** and **DATA1**
through **DATAH** together. Use the complete resource set for normal play,
including the sound files. Two verified DOS distributions are supported: one
with a 43,008-byte executable and an equivalent PKLITE-compressed version with
a 23,929-byte executable. Both provide the same game and share settings
compatibility. Other releases and platform versions are not automatically compatible.

## Start playing

After [building REforge](../../README.md#getting-started), run from the repository root:

```sh
./target/release/reforge --original /path/to/mach3
```

Press Space on the intro screen to start.
During intro audio, the start request waits for the current playback sequence
to reach its next input check, so it may not take effect immediately.

## Controls

| Action | Keyboard |
| --- | --- |
| Steer | Arrow keys, or S / X / B / N for up / down / left / right |
| Fire / advance title screens | Space; keypad 5 also fires |
| Pause / resume | P or Pause/Break |
| Toggle the original game's sound switch | F2 |
| Switch colour / monochrome display | F3 |
| Settings | Esc or F10 |
| Quit | Ctrl+Q |

On a gamepad, use the left stick or D-pad to steer and the bottom face button
to fire or advance title screens. Start/Menu pauses; Back/View opens Settings.

F2 toggles the original game's sound switch. Host mute and volume controls in
Settings operate independently of that switch.

F1 also retains the original pause/end-game control: release it, then press and
release another gameplay key to resume. Holding F1 can end the current game;
P or Pause/Break provides a simpler pause toggle.

## Game-specific options

**Sound renderer:** under **F10 → Mach 3 overrides → Audio**, choose PC Speaker
for the original speaker treatment, or **Enhanced PCM** for full sample values
while keeping the synthesized procedural effects. Check **Override global
defaults** to edit, then Apply. The default is PC Speaker.

**Digitized samples:** setting this to Off on the same page skips recorded
speech/music in the intro, pre-game, completion and Game Over/High Score sequences
while retaining procedural effects. Skipping these sequences also removes their
playback time. Add the game to the launcher library to save this choice for the
next launch. To silence a category without skipping its sequence, use the Music
or Speech volume instead.
As an original-game easter egg, bare `S` aliases `--digitized-samples off` for
that run.

**Colour / monochrome:** choose the initial display in the launcher's Video
options, or use `--video cga|mono`. Both modes work with optional CRT presentation.

For fullscreen, scaling, CRT presentation, audio levels and controller settings,
see the shared [Settings menu](../configuration.md).

## Motion

Reforged scene movement is the default. It smooths the moving scene while
retaining native gameplay, input polling, collisions and sound timing. Mountains
keep their original artwork and updates.

Use **F10 → Mach 3 overrides → Display → Scene movement** to choose
**Reforged** or **Original**. With Original scene movement, **Player craft
movement** offers **Enhanced** (craft and shadow only) or **Original**
(native craft placement and publication rhythm). Enable **Override global
defaults** to edit the game section. Apply changes presentation immediately and
saves the preference.

`--motion reforged|enhanced|original` temporarily selects a mode and locks its menu
controls for that run. `reforged` selects Reforged scene movement, `enhanced`
retains the earlier craft/shadow-only interpolation, and `original` retains the
native presentation. Existing saved `enhanced` and `original` choices keep their
previous meanings.

The display frame limit and VSync control presentation; they do not change game
speed. Long host stalls can cause a visible position correction when playback
catches up.

## Current limits

Support currently covers the verified DOS release. Comparisons include controlled
later encounters and difficulty cases, but do not exhaust every gameplay path.
Other platform versions and additional hardware modes are outside the current
supported set.

For implementation details, diagnostic tools and sound rendering, see the
[engine documentation](../../engines/mach3/README.md).
Intentional changes are catalogued in the
[deviations reference](../../engines/mach3/docs/deviations.md).
