#!/usr/bin/env python3
# Copyright (c) 2026 analogued. Project license has not been selected.
"""Record one original game and compare every displayed pass with Rust replay."""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


def file_hashes(directory):
    return {
        path.name: hashlib.sha256(path.read_bytes()).hexdigest()
        for path in sorted(directory.iterdir())
        if path.is_file()
    }


def configuration(drive, captures, scale, cycles):
    for path in (drive, captures):
        if any(char in str(path) for char in ('"', "\n", "\r")):
            raise ValueError("DOSBox paths cannot contain quotes or line breaks")
    return f"""[sdl]
output=openglnb
fullscreen=false
windowresolution={320 * scale}x{240 * scale}
showmenu=false
[render]
aspect=true
aspect_ratio=4:3
scaler=none
doublescan=false
[dosbox]
machine=cga
memsize=16
quit warning=false
captures={captures}
[cpu]
core=normal
cputype=386
cycles=fixed {cycles}
[mixer]
nosound=false
[midi]
mididevice=none
[speaker]
pcspeaker=true
[sblaster]
sbtype=none
[autoexec]
mount c "{drive}"
c:
loadfix -64 MACH3.EXE
exit
"""


def record(assets, destination, emulator, replay_binary, scale=3, cycles=3000):
    assets = assets.resolve()
    destination = destination.resolve()
    emulator = emulator.resolve()
    replay_binary = replay_binary.resolve()
    if not assets.is_dir():
        raise ValueError("the game resource directory does not exist")
    if not 1 <= scale <= 16 or cycles < 1:
        raise ValueError("scale must be 1..16 and cycles must be positive")
    if destination == assets or assets in destination.parents:
        raise ValueError("the recording must be outside the supplied game directory")
    if destination.exists():
        raise ValueError("the recording directory must be new")
    for binary in (emulator, replay_binary):
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise ValueError(f"executable not found: {binary}")
    signature = subprocess.check_output(
        [str(replay_binary), "--hook-signature", str(assets)], text=True
    ).strip()
    if len(signature) != 64 or any(
        char not in "0123456789abcdef" for char in signature
    ):
        raise ValueError("replay tool returned an invalid program signature")
    before = file_hashes(assets)
    destination.mkdir(parents=True, exist_ok=False)
    reference = destination / "reference"
    reference.mkdir()
    drive = destination / "drive"
    drive.mkdir()
    for name in before:
        shutil.copyfile(assets / name, drive / name)
    config = destination / "dosbox-x.conf"
    config.write_text(
        configuration(drive, destination / "emulator-captures", scale, cycles),
        encoding="utf-8",
    )
    env = os.environ.copy()
    # An inherited investigation replay or frame limit must not alter manual play.
    for name in tuple(env):
        if name.startswith("MACH3_CAPTURE_") or name == "MACH3_REPLAY_INPUTS":
            del env[name]
    env.update(
        MACH3_CAPTURE_DIR=str(reference),
        MACH3_CAPTURE_SIGNATURE=signature,
        MACH3_CAPTURE_FIRST_GAME="1",
    )
    if env.get("XDG_SESSION_TYPE") == "wayland" and env.get("DISPLAY"):
        env.setdefault("SDL_VIDEODRIVER", "x11")
    command = [
        str(emulator),
        "-defaultconf",
        "-conf",
        str(config),
        "-fastlaunch",
        "-nogui",
        "-nomenu",
    ]
    receipt = {
        "format_version": 1,
        "command": command,
        "emulator_sha256": hashlib.sha256(emulator.read_bytes()).hexdigest(),
        "replay_sha256": hashlib.sha256(replay_binary.read_bytes()).hexdigest(),
        "input_hashes_before": before,
        "scope": "first game; initial, gameplay, and scene-transition frames",
    }
    try:
        print(
            "Play the original normally. Game over or closing the window ends recording.",
            flush=True,
        )
        with (destination / "emulator.log").open("wb") as log:
            receipt["emulator_exit_code"] = subprocess.run(
                command,
                cwd=destination,
                env=env,
                stdout=log,
                stderr=subprocess.STDOUT,
                check=False,
            ).returncode
        if receipt["emulator_exit_code"] != 0:
            raise ValueError("recording failed; see emulator.log")
        if not (reference / "frames.tsv").is_file():
            raise ValueError(
                "no recording was produced; use the capture-enabled emulator"
            )
        if not (reference / "games.tsv").read_text(encoding="utf-8").splitlines()[1:]:
            raise ValueError("gameplay did not start; the recording contains no game")
        print("Recording saved. Replaying sampled inputs...", flush=True)
        recreation = destination / "recreation"
        replay_command = [
            str(replay_binary),
            str(drive),
            str(reference),
            str(recreation),
        ]
        receipt["replay_command"] = replay_command
        with (destination / "replay.log").open("wb") as log:
            receipt["replay_exit_code"] = subprocess.run(
                replay_command, stdout=log, stderr=subprocess.STDOUT, check=False
            ).returncode
        if receipt["replay_exit_code"] not in (0, 1):
            raise ValueError("replay failed; see replay.log")
        if receipt["replay_exit_code"] == 1:
            print(
                "Replay reported a difference or stopped early; see replay.log",
                flush=True,
            )
        print("Comparing captured graphics...", flush=True)
        compare = Path(__file__).with_name("compare_gameplay.py")
        compare_command = [
            sys.executable,
            str(compare),
            str(reference),
            str(recreation),
            str(destination / "report"),
            "--details",
        ]
        receipt["compare_command"] = compare_command
        result = subprocess.run(compare_command, check=False)
        receipt["compare_exit_code"] = result.returncode
        if result.returncode not in (0, 1):
            raise ValueError("invalid capture; see the comparison diagnostic")
        print(
            f"Graphical comparison: {destination / 'report' / 'index.html'}", flush=True
        )
        return max(result.returncode, receipt["replay_exit_code"])
    finally:
        after = file_hashes(assets)
        receipt["input_hashes_after"] = after
        receipt["supplied_inputs_unchanged"] = before == after
        (destination / "recording.json").write_text(
            json.dumps(receipt, indent=2) + "\n", encoding="utf-8"
        )
        if before != after:
            raise ValueError("the supplied game directory changed during recording")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("assets", type=Path)
    parser.add_argument(
        "destination", type=Path, help="new external recording directory"
    )
    parser.add_argument("--emulator", type=Path, required=True)
    parser.add_argument("--replay-binary", type=Path, required=True)
    parser.add_argument("--scale", type=int, choices=range(1, 17), default=3)
    parser.add_argument("--cycles", type=int, default=3000)
    args = parser.parse_args()
    try:
        return record(
            args.assets,
            args.destination,
            args.emulator,
            args.replay_binary,
            args.scale,
            args.cycles,
        )
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"record_gameplay: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
