<!-- Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later -->
# Deliberate deviations from the original

This records intentional presentation and usability changes to the four
supported DOS games. The [BTZ player guide](../../../docs/games/btz.md) explains
how to use these features. Missing features, reconstruction bugs and unresolved
timing/input fidelity questions remain in the
[compatibility notes](../README.md#reference-and-compatibility) and
[runtime boundaries](mindwheel-dos-runtime.md#compatibility-boundaries).

## File dialogs

The original file pages show a passive directory listing followed by a typed
filename prompt. Windowed play replaces them with a framed text browser using
the game's bitmap font and the selected display treatment. It adds folder
navigation, bookmark filtering, local modification dates, caret editing,
selection and clipboard shortcuts. Keyboard, mouse and controller navigation
operate the browser independently of the story's command editor.

The file-dialog cursor is selectable for each game. The default Pulsing
underline takes its short/tall presentation from Breakers; Steady block keeps
the underlying character readable by reversing its colours. Both choices are
available in all four games and affect only these dialogs.

Originally, Mindwheel, Essex and Brimstone replace bookmarks without asking;
Breakers asks first. Original transcripts replace their destination without
confirmation. Windowed play asks before replacing either kind of file in every
game, initially selecting No. Declining returns to filename selection, or cancels
the operation when a command-line option fixed its destination. Errors leave the
browser available for correction and retry.

The browser uses a temporary screen and stops the story clock while open.
Settings can open above it and return to the same selection or overwrite
question. Terminal play retains typed filename prompts and the original
game-specific overwrite policy rather than using this browser.

Bookmark contents retain the original format, with no automatic extension.
Original-save compatibility is a preserved capability rather than a deviation.
See [bookmarks and transcripts](../../../docs/games/btz.md#bookmarks-and-transcripts)
for controls, cursor settings and file-selection options.

## File locations and safety

DOS resolves filenames through its working directory and drive environment.
Both modern hosts instead start bookmark and transcript selection beside the
selected `ADISK.OBJ`, independently of the launch directory. `--save-dir` changes
that starting folder; explicit per-file options can select fixed destinations.
Paths use the host filesystem, including absolute paths. Relative browser paths
resolve from the displayed folder; terminal prompts use the starting folder.

Bookmark writes use a temporary file and atomic replacement instead of directly
truncating the destination. Output selection protects active original input
paths from overwrite. These safeguards change host file handling without
changing the bookmark codec or story rules.

Requests for another physical disk become host folder prompts. The host reads
the selected folder's data and side map; DOS drive characters remain mapping
labels rather than host drive paths. This is the intended modern media workflow;
compatibility with original secondary media remains unverified. See
[persistence and media](mindwheel-dos-runtime.md#host-requests-bookmarks-and-media)
for the service and path-resolution rules.

## Host controls and presentation

Windowed play adds F10 Settings, Pause/Break host pause, and Ctrl+Q or window
close to exit. Settings, file dialogs and host pause suspend story execution and
exclude their elapsed time from the story clock, so resuming does not catch up
that interval. These controls are separate from the original Ctrl-S/Escape
keyboard pause and story commands such as `PAUSE NOVEL`.

SDL presentation adds window sizing, fullscreen and optional CRT treatment.
The alternative terminal frontend uses the attached terminal's glyphs and
colours instead of the bitmap rasterizer. Controllers navigate host interfaces;
story commands still use the keyboard. See
[launch and presentation options](../../../docs/games/btz.md#start-playing) and
the [shared configuration guide](../../../docs/configuration.md).

## Keyboard bell and terminal input

The original BIOS bell blocks execution while sounding. Windowed playback uses
an asynchronous speaker tone, allowing execution to continue, with shared volume,
mute and output-device controls. Its tone follows a DOSBox-X BIOS reference
rather than claiming one sound for every original PC BIOS. Terminal play sends
BEL and lets terminal preferences determine audible or visual feedback.

Unix terminals encode arrows and function keys as sequences beginning with
Escape. The adapter allows up to 30 ms to distinguish a bare Escape from those
sequences; the DOS BIOS supplies distinct codes without this delay. These are
host adaptations, separate from the unresolved native polling differences. The
[keyboard reference](mindwheel-dos-runtime.md#keyboard-editor-and-read-service)
gives the bell parameters and key-decoding rules.
