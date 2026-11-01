# Mach 3

Steer and shoot through the DOS version of Mach 3, with its animated opening,
recorded speech and music, and procedural sound effects. REforge offers the
original PC Speaker presentation and an alternative Enhanced PCM audio renderer.

[Back to REforge](../../README.md)

## What you need

Supply the original DOS game directory, keeping **MACH3.EXE** and **DATA1**
through **DATAH** together. Use the complete resource set for normal play,
including the sound files. One verified DOS release is currently supported;
other releases and platform versions are not automatically compatible.

## Start playing

After [building REforge](../../README.md#getting-started), run from the repository root:

```sh
./target/release/reforge --original /path/to/mach3
```

Space skips the splash and starts from the intro. Release it between presses.
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
| Settings | F10 |
| Quit | Ctrl+Q or Esc |

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

**Digitized samples:** setting this to Off on the same page reproduces the
original DOS game's `S`/`s` command-line behavior. It skips recorded speech/music
in the intro, pre-game, completion and Game Over/High Score sequences while
retaining procedural effects. The native playback calls and their timing/input
effects are skipped, and the preference applies on the next application launch.
REforge also accepts bare `S`/`s` as aliases for `--digitized-samples off`, a
temporary override for that run. To silence a category without skipping its
sequence, use the Music or Speech volume instead.

**Colour / monochrome:** choose the initial display in the launcher's Video
options, or use `--video cga|mono`. F3 switches once per press during play;
release it before switching again. Both modes work with optional CRT presentation.

For fullscreen, scaling, CRT presentation, audio levels and controller settings,
see the shared [Settings menu](../configuration.md).

## Current limits

Support currently covers the verified DOS release. Later gameplay paths still
need broader comparisons with the original, and other platform versions and
additional hardware modes are outside the current supported set.

For implementation details, diagnostic tools and sound rendering, see the
[engine documentation](../../engines/mach3/README.md).
