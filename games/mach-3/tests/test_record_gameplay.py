# Copyright (c) 2026 analogued. Project license has not been selected.
"""Recorder setup checks using authored files and mocked external programs."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

TOOL = Path(__file__).resolve().parents[1] / "tools" / "record_gameplay.py"
SPEC = importlib.util.spec_from_file_location("record_gameplay", TOOL)
RECORD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RECORD)


class RecordGameplayTests(unittest.TestCase):
    def test_configuration_mounts_only_working_copy_with_fixed_normal_core(self):
        config = RECORD.configuration(
            Path("/example/working copy"), Path("/capture"), 8, 3000
        )
        self.assertIn('mount c "/example/working copy"', config)
        self.assertIn("windowresolution=2560x1920", config)
        self.assertIn("output=openglnb", config)
        self.assertIn("core=normal", config)
        self.assertIn("aspect=true", config)
        with self.assertRaises(ValueError):
            RECORD.configuration(Path('/bad"path'), Path("/capture"), 3, 3000)

    def test_rejects_capture_inside_supplied_game_before_starting_any_program(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "game").mkdir()
            with patch.object(RECORD.subprocess, "check_output") as run:
                with self.assertRaises(ValueError):
                    RECORD.record(
                        root / "game",
                        root / "game" / "capture",
                        root / "emulator",
                        root / "replay",
                    )
                run.assert_not_called()

    def test_partial_replay_does_not_report_overall_success_when_pixels_match(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = root / "game"
            assets.mkdir()
            (assets / "authored.dat").write_bytes(b"synthetic input")
            binary = root / "fake-program"
            binary.write_text("#!/bin/sh\nexit 0\n")
            binary.chmod(0o755)
            destination = root / "recording"
            calls = 0

            def program_run(*_args, **_kwargs):
                nonlocal calls
                calls += 1
                if calls == 1:
                    reference = destination / "reference"
                    (reference / "frames.tsv").write_text("authored frame index\n")
                    (reference / "games.tsv").write_text("authored header\n0\n")
                return Mock(returncode=1 if calls == 3 else 0)

            with (
                patch.object(RECORD.subprocess, "check_output", return_value="00" * 32),
                patch.object(RECORD.subprocess, "run", side_effect=program_run),
            ):
                result = RECORD.record(
                    assets, destination, binary, binary, observer_tool=binary
                )
            self.assertEqual(result, 1)
            receipt = json.loads((destination / "recording.json").read_text())
            self.assertEqual(receipt["replay_exit_code"], 1)
            self.assertEqual(receipt["compare_exit_code"], 0)
            self.assertTrue(receipt["supplied_inputs_unchanged"])

    def test_file_hashes_ignore_subdirectories_and_detect_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "state").mkdir()
            resource = root / "authored.dat"
            resource.write_bytes(b"one")
            first = RECORD.file_hashes(root)
            self.assertEqual(set(first), {"authored.dat"})
            resource.write_bytes(b"two")
            self.assertNotEqual(RECORD.file_hashes(root), first)


if __name__ == "__main__":
    unittest.main()
