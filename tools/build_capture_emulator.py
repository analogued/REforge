#!/usr/bin/env python3
# Copyright (c) 2026 analogued.
# Project license not yet selected; see the repository README.
"""Build an isolated, pinned DOSBox-X with the authored gameplay observer."""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
from pathlib import Path

UPSTREAM = "https://github.com/joncampbell123/dosbox-x.git"
REVISION = "478c860050b20afdb58bce28f1b43163952fc541"
OPTIONS = [
    "--enable-sdl2",
    "--enable-debug",
    "--disable-sdlnet",
    "--disable-libslirp",
    "--disable-libfluidsynth",
    "--disable-mt32",
    "--disable-printer",
    "--disable-gamelink",
    "--disable-dynamic-core",
    "--enable-opengl",
    "--disable-freetype",
]


def run(arguments, directory, log):
    with log.open("ab") as output:
        subprocess.run(
            arguments,
            cwd=directory,
            stdout=output,
            stderr=subprocess.STDOUT,
            check=True,
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--work-dir",
        type=Path,
        required=True,
        help="external build directory; never original game files",
    )
    parser.add_argument("--jobs", type=int, default=min(4, os.cpu_count() or 1))
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    source = work / "upstream"
    log = work / "build.log"
    if not source.exists():
        source.mkdir()
        run(["git", "init"], source, log)
        run(["git", "fetch", "--depth", "1", UPSTREAM, REVISION], source, log)
        run(["git", "checkout", "--detach", REVISION], source, log)
    revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=source, text=True
    ).strip()
    if revision != REVISION:
        parser.error("existing upstream source has a different revision")
    original = subprocess.check_output(
        ["git", "show", f"{REVISION}:src/cpu/core_normal.cpp"], cwd=source
    )
    included = original.replace(
        b"Bits CPU_Core_Normal_Run(void) {",
        b'#include "mach3_capture_hook.h"\n\nBits CPU_Core_Normal_Run(void) {',
        1,
    )
    included = included.replace(
        b"\t\tLOADIP;", b"\t\tLOADIP;\n\t\tmach3_capture::instruction();", 1
    )
    target = source / "src/cpu/core_normal.cpp"
    if target.read_bytes() not in (original, included):
        parser.error("CPU source has unrelated local changes")
    if target.read_bytes() != included:
        target.write_bytes(included)
    gui_relative = "src/gui/sdlmain.cpp"
    gui_original = subprocess.check_output(
        ["git", "show", f"{REVISION}:{gui_relative}"], cwd=source
    )
    quit_marker = b"case SDL_QUIT:"
    if gui_original.count(quit_marker) != 6:
        parser.error("unexpected upstream SDL quit handling")
    gui_included = gui_original.replace(
        quit_marker,
        quit_marker
        + b'\n            if (getenv("MACH3_CAPTURE_DIR") && *getenv("MACH3_CAPTURE_DIR")) exit(0);\n',
    )
    gui_target = source / gui_relative
    if gui_target.read_bytes() not in (gui_original, gui_included):
        parser.error("GUI source has unrelated local changes")
    if gui_target.read_bytes() != gui_included:
        gui_target.write_bytes(gui_included)
    hook = Path(__file__).with_name("dosbox_capture_hook.h")
    hook_target = source / "src/cpu/mach3_capture_hook.h"
    if not hook_target.exists() or hook_target.read_bytes() != hook.read_bytes():
        shutil.copyfile(hook, hook_target)
    if not (source / "configure").exists():
        run(["./autogen.sh"], source, log)
    config_options = work / "configure-options.json"
    configured = (
        json.loads(config_options.read_text()) if config_options.exists() else None
    )
    previous_options = [
        "--disable-opengl" if option == "--enable-opengl" else option
        for option in OPTIONS
    ]
    if (
        not (source / "Makefile").exists()
        or configured is None
        or configured == previous_options
    ):
        run(["./configure", *OPTIONS], source, log)
        config_options.write_text(json.dumps(OPTIONS, indent=2) + "\n")
    elif configured != OPTIONS:
        parser.error("existing configure options differ; use a fresh work directory")
    if "#define C_OPENGL 1" not in (source / "config.h").read_text():
        parser.error("OpenGL support requires the GL headers and development library")
    run(["make", f"-j{args.jobs}"], source, log)
    binary = source / "src/dosbox-x"
    metadata = {
        "upstream": UPSTREAM,
        "revision": REVISION,
        "upstream_license": "GPL-2.0-or-later; full COPYING retained in upstream",
        "integration": (
            "Authored observer include and call in the normal CPU core; "
            "capture-only clean exit on SDL quit before DOS cleanup"
        ),
        "hook_sha256": hashlib.sha256(hook.read_bytes()).hexdigest(),
        "core_sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
        "gui_sha256": hashlib.sha256(gui_target.read_bytes()).hexdigest(),
        "binary": str(binary),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "configure": OPTIONS,
    }
    (work / "build.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(binary)


if __name__ == "__main__":
    main()
