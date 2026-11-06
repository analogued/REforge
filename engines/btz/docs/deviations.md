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
filename prompt. Both player frontends replace them with the same framed text
browser. Windowed play uses the game's bitmap font and selected display treatment;
terminal play uses ANSI attributes and the terminal's font. The browser adds folder
navigation, bookmark filtering, local modification dates, caret editing,
selection and clipboard shortcuts. Keyboard, mouse and controller navigation
operate the browser independently of the story's command editor.

The windowed browser adds monochrome hover and pressed-button feedback, an
underlined filename field and native arrow/I-beam pointers. Hover never changes
keyboard focus, the filename or the selected overwrite answer. Button activation
requires a press and release on the same control; leaving the window, opening
Settings or losing focus cancels a held press. Selected files remain marked
after focus moves to another control, until the filename is edited.

Originally, Mindwheel, Essex and Brimstone replace bookmarks without asking;
Breakers asks first. Original transcripts replace their destination without
confirmation. Both player frontends ask before replacing either kind of file in
every game, initially selecting No. Declining returns to filename selection, or cancels
the operation when a command-line option fixed its destination. Errors leave the
browser available for correction and retry.

Save and transcript selection start in an empty filename field; resume starts
in the file list when matching bookmarks are available.
The browser uses a temporary screen and stops the story clock while open.
Settings can open above it and return to the same selection or overwrite
question. Terminal keyboard navigation uses the same selection and completion
logic, with UTF-8 filename input and terminal paste. Clipboard copy/cut, mouse
and controller input belong to the windowed adapter. Diagnostic tools retain
typed filename prompts and the original game-specific overwrite policy.

Bookmark contents retain the original format, with no automatic extension.
Original-save compatibility is a preserved capability rather than a deviation.
See [bookmarks and transcripts](../../../docs/games/btz.md#bookmarks-and-transcripts)
for browser controls and file-selection options.

## Cursor presentation

Command input and file dialogs share a selectable cursor style in every
game. Original command input paints a solid block at the next insertion point;
the modern host replaces its appearance when drawing the window, preserving the
native screen bytes and input rules.

The default Pulsing underline takes its short/tall presentation from Breakers'
file interface. Blinking underline uses Mindwheel, Essex and Brimstone's
two-scanline file cursor without a permanently painted underscore. Steady block
keeps any underlying character readable by reversing its colours. The preference
applies to both frontends. Terminal play requests a native steady block or
blinking underline using [DECSCUSR cursor controls](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html).
Pulsing underline falls back to a native steady underline because these controls
cannot alternate underline thickness. Terminal font, preferences and support
determine exact shape and blink timing. The host removes the painted command
marker from the terminal presentation copy too, retaining the native screen.

The windowed animated styles use eight-frame phases at a fixed 60 Hz reference,
about 133 ms per phase. This follows the CGA DOSBox-X cursor cadence, independently of
host CPU speed and monitor refresh. The original selects scanlines 6–7 for CGA;
other DOS video adapters can use different shapes and blink rates. See
[cursor controls](../../../docs/games/btz.md#commands-and-controls) for selection.

## File locations and safety

DOS resolves filenames through its working directory and drive environment.
Both modern hosts instead start bookmark and transcript selection beside the
selected `ADISK.OBJ`, independently of the launch directory. `--save-dir` changes
that starting folder; explicit per-file options can select fixed destinations.
Paths use the host filesystem, including absolute paths. Relative browser paths
resolve from the displayed folder in either player frontend. Diagnostic typed
prompts use the starting folder.

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

Windowed play adds Esc/F10 Settings, Pause/Break host pause, and Ctrl+Q or window
close to exit. Settings, file dialogs and host pause suspend story execution and
exclude their elapsed time from the story clock, so resuming does not catch up
that interval. Esc is reserved for Settings during graphical gameplay and for
cancel/back inside dialogs. Ctrl-S retains the original keyboard pause; terminal
play also retains Escape for that pause. Host controls remain separate from
story commands such as `PAUSE NOVEL`.

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
