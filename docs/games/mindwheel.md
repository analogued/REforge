# Mindwheel — experimental BTZ frontend

Mindwheel DOS currently runs through the experimental BTZ terminal frontend.
It is not yet selectable through the normal REforge application. A winning
playthrough has not been verified, and other BTZ releases are unvalidated.

## Start the frontend

Supply the corresponding files from your Mindwheel DOS copy:

| File | Used for |
| --- | --- |
| `ADISK.OBJ` | The story program, world data, text and vocabulary |
| `GO.EXE` | Tables needed to decode text and interpret input, some prompt strings, and a random-generator setting. The recreation reads it as data; it does not run the DOS executable. |
| `SIDE.TXT` | The original data-disk map, used for disk handling, including restart and bookmark restore |

From the repository root:

```sh
cargo run --locked -p btz --features dev-tools --bin btz-core -- \
  --original /path/to/ADISK.OBJ --interpreter /path/to/GO.EXE \
  --side-map /path/to/SIDE.TXT
```

Interactive play requires Unix, a terminal and `stty`. Enter dismisses the
title; follow the game's prompts. Ctrl-C/Ctrl-D exits and restores terminal
settings. See the [engine overview](../../engines/btz/README.md) for support
and the [development guide](../../engines/btz/CONTRIBUTING.md) for diagnostic
options.

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
filename or path. `BOOKMARK` writes a bookmark; `RESUME NOVEL` reads it. No
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

## Timing and current limits

Characters and background events may continue while you type or wait. The game
keeps its own pace, so a faster computer does not make time pass faster. When
its waiting time has expired, it handles that before accepting new keystrokes.
If you have already submitted a command when it checks, your command takes
priority.

Random events can differ from a DOS run. Some pause, queued-input and bell
feedback cases also remain imperfect. Technical details are in the
[runtime compatibility notes](../../engines/btz/docs/mindwheel-dos-runtime.md#compatibility-boundaries).
