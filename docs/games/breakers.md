# Breakers

Explore an alien world in the DOS edition of Breakers, played through REforge's
BTZ interpreter with the original story data and background activity.

> [!IMPORTANT]
> REforge does not include original game data. Keep **ADISK.OBJ**, **RUN.EXE**,
> **SIDE.TXT** and **TITLE.EXE** from a supported DOS release together in the
> game directory. Detection verifies all four files by size and SHA-256 hash.

## Start playing

> [!NOTE]
> Support is experimental. A complete winning playthrough has not been verified.
> Opening artwork keeps the original timing but changes pages without the DOS
> wipe transitions.
> See [BTZ compatibility notes](../../engines/btz/README.md#reference-and-compatibility).

Add the game directory to the launcher and select **Play**, or run:

```sh
reforge --original /path/to/breakers
```

The opening artwork and credits advance automatically, followed by the framed
title screen. Press Enter to begin. The default is an SDL text window, with
sharp pixels or Hyllian CRT presentation and windowed/fullscreen options. For
an attached terminal, add `--renderer terminal`.

See the [BTZ player guide](btz.md) for commands, keyboard controls, launch options,
[bookmarks and transcripts](btz.md#bookmarks-and-transcripts), and timing behavior.
Bookmarks use the original DOS format; saves can be exchanged with the
corresponding original release. Replacing an existing Breakers bookmark requires
confirmation. No filename extension is added automatically.
