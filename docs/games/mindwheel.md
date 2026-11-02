# Mindwheel

Begin a journey through four minds in the DOS edition of Mindwheel, played
through REforge's BTZ interpreter.

> [!IMPORTANT]
> REforge does not include original game data. Keep **ADISK.OBJ**, **GO.EXE**
> and **SIDE.TXT** from a supported DOS release together in the game directory.
> Detection verifies all three files against their sizes and SHA-256 hashes.


## Start playing

Add the game directory to the launcher and select **Play**, or run:

```sh
reforge --original /path/to/mindwheel
```

> [!NOTE]
> Support is experimental. A complete winning playthrough has not been verified.
> See [BTZ compatibility notes](../../engines/btz/README.md#reference-and-compatibility).

The default is an SDL text window, with sharp pixels or Hyllian CRT presentation
and windowed/fullscreen options. Enter dismisses the title. For an attached
terminal, add `--renderer terminal`.

See the [BTZ player guide](btz.md) for commands, keyboard controls, launch options,
[bookmarks and transcripts](btz.md#bookmarks-and-transcripts), and timing behavior.
Bookmarks use the original DOS format, so saves can be exchanged with the
corresponding original game release.

