# BTZ games

Play Mindwheel, Essex, Brimstone and Breakers for DOS through the BTZ interpreter,
with their original text layout, interactive commands and background story
activity.

> [!IMPORTANT]
> REforge does not include original BTZ assets or data. You must provide
> the original files from a supported release. See [What you need](#what-you-need).

Supported games: [Mindwheel](mindwheel.md), [Essex](essex.md),
[Brimstone: The Dream of Gawain](brimstone.md) and [Breakers](breakers.md).

## What you need

Keep the corresponding original files together in the game directory:

| Games | Required files |
| --- | --- |
| Mindwheel, Essex, Brimstone | **ADISK.OBJ**, **GO.EXE**, **SIDE.TXT** |
| Breakers | **ADISK.OBJ**, **RUN.EXE**, **SIDE.TXT**, **TITLE.EXE** |

ADISK.OBJ contains the story; the interpreter executable supplies tables and
native messages, and is read as data rather than executed. SIDE.TXT supplies
logical-disk mappings used by restart and bookmark restore. Breakers' TITLE.EXE
supplies its opening artwork and credits.

All required files must match the supported release's sizes and SHA-256 hashes.

## Start playing

> [!NOTE]
> A complete winning playthrough has not been verified. Random events can differ
> from a DOS run, and some pause and queued-input cases remain
> imperfect. See the [engine compatibility notes](../../engines/btz/README.md#reference-and-compatibility).

Add the game directory to the launcher and select **Play**, or run:

```sh
reforge --original /path/to/game
```

The default display is an SDL text window. Its Rendering and CRT & Colour
settings use the same controls as the other games. The launch options offer
**Display: Sharp pixels / Hyllian CRT** and **Window: Windowed / Fullscreen**.
Breakers first shows timed artwork and credits. Enter dismisses the framed title;
follow the game's prompts. **F10** opens Settings. **Ctrl+Q** or closing the
window exits; **Pause/Break** toggles the host pause.

To play directly in an existing terminal:

```sh
reforge --original /path/to/game --renderer terminal
```

Terminal play uses the terminal already attached to the process. It does not
open a terminal application. Choose **Renderer: Graphical / Terminal** in the
launch options, or **Terminal** in BTZ's per-game Rendering settings. Terminal
is available when a terminal is attached; the choice applies at game startup.
Display and Window launch options are disabled during terminal play. Graphical
preferences are retained and apply again when you select Graphical, including
the renderer configured in Settings.

## Commands and controls

Commands are interpreted by each game's original data. Availability and wording
vary by game; consult its manual. Common BTZ commands include:

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
remain; see the [runtime reference](../../engines/btz/docs/mindwheel-dos-runtime.md#compatibility-boundaries).

**Ctrl+G** rings the keyboard bell. Windowed play uses a PC-speaker tone and
respects the sound, master volume and Sound effects controls. Terminal play
sends BEL: your terminal's settings determine whether it sounds, flashes or
stays silent. REforge's mute controls can suppress it; terminal bell volume is
controlled by the terminal.

## Bookmarks and transcripts

By default, every `BOOKMARK`, `RESUME NOVEL` and `SCRIPT` request opens a
text file browser in windowed play, or prompts for a filename in the terminal.
`BOOKMARK` writes a bookmark; `RESUME NOVEL` reads it. No extension is appended.
Native bookmarks contain game state, not a complete recording of the screen,
editor, timing and random state.

`SCRIPT` prompts for a transcript filename. It records printed game text,
including submitted command echoes and background messages while active.
It is a text transcript, not a keystroke recording or command file to load
into the game. `UNSCRIPT` closes it.

The file browser starts beside the supplied `ADISK.OBJ`, independently of the
launch directory. `--save-dir /path/to/saves` selects another starting folder;
the directory must already exist. Filenames and relative paths resolve from the
folder shown in the browser, including `../` paths. Absolute paths work too.
Terminal prompts resolve relative names from the starting folder.

Use Up/Down to select a file or folder and Enter to open or choose it. Tab and
Shift+Tab move between the list, filename field, Save/Resume/Write and Cancel.
The filename field supports caret movement, selection and clipboard shortcuts.
Escape cancels; in an overwrite question it returns to filename selection.
Page Up/Down moves through longer lists; Backspace goes to the parent folder
when the list has focus. Click to select, double-click to open, or use a
controller's directions, confirm/cancel and shoulder buttons. Bookmarks are
recognized from their contents, including files without an extension.

The game pauses while the browser is open. Modification dates use your system's
local timezone. F10 opens Settings and returns to the same file dialog when closed.
Under the game's Display overrides, the final Text interface section selects
the file-dialog cursor: Pulsing underline (the default) stays visible while
alternating between two heights; Steady block reverses the colours of the
character underneath. Apply saves the choice for that game. Terminal play retains
this preference for the next windowed session.

The optional `--bookmark-path file` and `--transcript-path file` launch options
skip their respective filename prompts and use those fixed files for the whole
run. `--bookmark-path` supplies the destination for `BOOKMARK` and the source
for `RESUME NOVEL`; `--transcript-path` supplies the destination for each new
`SCRIPT` session. Without these options, the prompts remain available each
time, so you can choose different files.

Windowed play asks before replacing an existing bookmark or transcript in all
four games, including fixed filenames from the launch options. The overwrite
question initially selects No. In terminal play, Breakers asks before replacing
a bookmark; the other three games replace it directly. Terminal transcript
sessions replace their destination directly. A transcript session starts a new
file rather than appending.

Relative paths supplied by these launch options resolve from the launch
directory. Active game input files are protected from overwrite.

## Timing

Characters and background events may continue while you type or wait. The game
keeps its own pace, so a faster computer does not make time pass faster. When
its waiting time has expired, it handles that before accepting new keystrokes.
If you have already submitted a command when it checks, your command takes
priority.
