# Alley Cat

Climb the alley's bins and clotheslines, enter windows to tackle the room
challenges, and reach the female cat in the Hearts stage. REforge includes all
five ordinary rooms, the underwater section and the Hearts bonus scene, with
PC Speaker music and effects.

> [!IMPORTANT]
> REforge does not include original Alley Cat assets or data. You must provide
> the original files from a supported release. See [What you need](#what-you-need)
> below for details.

## What you need

Supply the original DOS **CAT.EXE**. One verified DOS release is currently
supported; COM, booter and other platform versions are not interchangeable.
REforge will report an unsupported version if the file does not match.

## Start playing

After [building REforge](../../README.md#getting-started), run from the repository root:

```sh
./target/release/reforge --original /path/to/alley-cat
```

You can also point `--original` directly to `CAT.EXE`.
Press a key to leave the animated title, choose a skill, then press another key
at the instructions to start. K/H/T/A select Kitten, House Cat, Tomcat or Alley Cat.
You can also choose with the arrow keys and Enter, or a gamepad.

## Controls

| Action | Keyboard |
| --- | --- |
| Move, jump and swim | Arrow keys |
| Contextual action | Alt |
| Pause / resume | Esc, P or Pause/Break |
| Start a new run | Ctrl+R |
| Return to skill selection | Ctrl+M |
| Toggle the original game's sound switch | Ctrl+S |
| Settings | F10 |
| Quit | Ctrl+Q |

Alt lets you travel between paired cheese-room holes, drink from milk bowls,
and place carried gifts in Hearts. Gifts are picked up automatically.

By default, use the left stick or D-pad for movement and the bottom face
button for Alt's action. It also advances title/instruction screens and confirms
skill selection. Start/Menu pauses; Back/View opens Settings.

In **Settings → Alley Cat overrides → Controls**, enable **Override global
defaults** to choose a controller layout. **Directional jump** keeps Up to jump
and Down to drop, with a choice of Action button. **Button jump** uses left/right
movement and separate Jump/Drop and Action buttons. Hold Down before pressing
Jump to drop. Swimming always uses all four directions. Each movement mode
remembers its button layout; Apply saves the choices for that library entry.
Keyboard and menu controls keep their usual bindings.

## Game-specific options

**Responsive controls** are enabled by default. They reduce the wait before
direction and action requests are read, while preserving movement speed and
jump physics. To use the original polling behavior, open
**F10 → Alley Cat overrides → Controls → Input response**. Check
**Override global defaults** to edit, then Apply.

**CGA or PCjr graphics:** CGA is the default. Add `--machine pcjr` to select
PCjr graphics and the original mode-specific timing rules. In the fishbowl,
PCjr changes fish and eel cadence near the surface and adds a wait to the cat's
movement there, so the pace can vary as the cat swims up or down. Both modes
currently use PC Speaker sound.

**Starting skill:** use the menu, or add `--difficulty 0`, `1`, `2` or `3` for
Kitten through Alley Cat. The command line also accepts the later progression
levels `4` through `7`.

**Startup seed:** normal launches choose a timer-derived seed, printed in the
terminal. Add `--seed 0xfa59` (or another value from 2 through 65535) to repeat a
starting seed. The original random generator is retained; matching gameplay
also requires matching title duration and inputs.

For fullscreen, scaling, CRT presentation and volume, use the shared
[Settings menu](../configuration.md). Music and Effects have separate levels;
Speech is disabled because this game has no speech. The Ctrl+S game switch is
independent of the menu's host mute and volume controls.

## Current limits

Only the supported DOS executable is accepted. The original movement and landing
quirks are retained, so a difficult jump is not automatically an input fault.
Hardware timing is reconstructed and adjusted in selected places; exact behavior
on every historical machine is not guaranteed.

For implementation details and the intentional changes, see the
[engine documentation](../../engines/alleycat/README.md) and
[deviations reference](../../engines/alleycat/docs/deviations.md).
