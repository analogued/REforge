# Mindwheel

Play the DOS edition of Mindwheel through the BTZ interpreter, with its
original text layout, interactive commands and background story activity.

> [!IMPORTANT]
> REforge does not include original Mindwheel assets or data. You must provide
> the original files from a supported release. See [What you need](#what-you-need).

[Back to REforge](../../README.md)

## What you need

Keep the corresponding **ADISK.OBJ**, **GO.EXE** and **SIDE.TXT** together in
the game directory. ADISK.OBJ contains the story; GO.EXE supplies tables and
native messages, and is read as data rather than executed. SIDE.TXT supplies
logical-disk mappings used by restart and bookmark restore.

> [!NOTE]
> A complete winning playthrough has not been verified. Random events can differ
> from a DOS run, and some pause, queued-input and bell feedback cases remain
> imperfect. See the [runtime compatibility notes](../../engines/btz/docs/mindwheel-dos-runtime.md#compatibility-boundaries).

## Start playing

Add the game directory to the launcher and select **Play**, or run:

```sh
reforge --original /path/to/mindwheel
```

The default display is an SDL text window. Its Rendering and CRT & Colour
settings use the same controls as the other games. The launch options offer
**Display: Sharp pixels / Hyllian CRT** and **Window: Windowed / Fullscreen**.
Enter dismisses the title;
follow the game's prompts. **F10** opens Settings. **Ctrl+Q** or closing the
window exits; **Pause/Break** toggles the host pause.

To play directly in an existing terminal:

```sh
reforge --original /path/to/mindwheel --renderer terminal
```

Terminal play uses the terminal already attached to the process. It does not
open a terminal application. Choose **Renderer: Graphical / Terminal** in the
launch options, or **Terminal** in BTZ's per-game Rendering settings. Terminal
is available when a terminal is attached; the choice applies at game startup.
Display and Window launch options are disabled during terminal play. Graphical
preferences are retained and apply again when you select Graphical, including
the renderer configured in Settings.

## Commands and controls

These commands are interpreted by the original game data:

| Command | Function |
| --- | --- |
| `LOOK` / `L` | Describe the surroundings |
| `INVENTORY` | List what you carry |
| `AGAIN` / `G` | Repeat the previous command |
| `HEALTH` / `DIAGNOSE` | Describe your condition |
| `PROGRESS` / `PR` / `SCORE` | Report progress |
| `BOOKMARK` | Save a bookmark |
| `RESUME NOVEL` / `RESUME` / `RESTORE` | Restore a bookmark |
| `SCRIPT` / `PRINTER ON` | Start a transcript |
| `UNSCRIPT` / `PRINTER OFF` | Stop the transcript |
| `PAUSE NOVEL` | Suspend activity until a key resumes it |
| `TIME FASTER` / `TIME SLOWER` | Adjust background activity intervals |
| `TIME` | Report the time within the story |
| `BRIEF` / `SUPERBRIEF` | Select minimum verbosity |
| `VERBOSE` | Select maximum verbosity |
| `WAIT` / `Z` | Let time pass |
| `HELP` | Request the game's contextual advice |
| `RESTART NOVEL` / `RESTART` | Report progress, wait for a key and restart |
| `QUIT NOVEL` | Report progress, wait for a key and exit |

Use `BOOKMARK` for saving. `SAVE` has an adventure meaning: rescuing/freeing
someone. Bare `QUIT` asks you to enter `QUIT NOVEL`.

Enter submits a command. Backspace or Left deletes the last character from the
command you are typing; Left does not just move the cursor. Tab inserts ten
spaces.

Home/F5 erases the current line. End/F3 recalls the previous line, and Right/F1
recalls its next character.

Ctrl-S or Escape pauses the game. Enter dismisses an ordinary display pause
without submitting an empty command. Some pause and queued-input edge cases
remain; see the runtime reference.

## Bookmarks and transcripts

By default, every `BOOKMARK`, `RESUME NOVEL` and `SCRIPT` request prompts for a
filename or path, in a windowed dialog or in the terminal. `BOOKMARK` writes a bookmark; `RESUME NOVEL` reads it. No
extension is appended. Native bookmarks contain game state, not a complete
recording of the screen, editor, timing and random state.

`SCRIPT` prompts for a transcript filename. It records printed game text,
including submitted command echoes and background messages while active.
It is a text transcript, not a keystroke recording or command file to load
into the game. `UNSCRIPT` closes it.

Relative prompted names resolve beside the supplied `ADISK.OBJ`, independently
of the launch directory. `--save-dir /path/to/saves` selects another base; it
does not skip filename prompts. The directory must already exist. Absolute
prompted filenames choose their own location. Existing destination files are
replaced.

The optional `--bookmark-path file` and `--transcript-path file` launch options
skip their respective filename prompts and use those fixed files for the whole
run. With `--bookmark-path`, each `BOOKMARK` replaces the same save and
`RESUME NOVEL` reads it. With `--transcript-path`, each new `SCRIPT` session
starts that file afresh, replacing its previous contents rather than appending.
There is no overwrite confirmation. Without these options, the prompts remain
available each time, so you can choose different files.

Relative paths supplied by these launch options resolve from the launch
directory. Active game input files are protected from overwrite.

## Timing

Characters and background events may continue while you type or wait. The game
keeps its own pace, so a faster computer does not make time pass faster. When
its waiting time has expired, it handles that before accepting new keystrokes.
If you have already submitted a command when it checks, your command takes
priority.
