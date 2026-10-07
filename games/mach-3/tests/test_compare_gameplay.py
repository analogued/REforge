# Copyright (c) 2026 analogued.
# Project license not yet selected; see the repository README.
"""Authored synthetic capture comparison cases; no original game material."""

import importlib.util
import json
import struct
import subprocess
import sys
import tempfile
import unittest
import zlib
from pathlib import Path

TOOL = Path(__file__).resolve().parents[1] / "tools" / "compare_gameplay.py"
SPEC = importlib.util.spec_from_file_location("compare_gameplay", TOOL)
COMPARE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = COMPARE
SPEC.loader.exec_module(COMPARE)
BLACK = bytes(COMPARE.FRAME_BYTES)


def frame(seq=0, game=0, body=16, pass_number=1, kind="gameplay", palette=1, mode="4"):
    return [str(value) for value in (seq, game, body, pass_number, kind, palette, mode)]


def capture(directory, rows, packed=None):
    directory.mkdir()
    text = "\t".join(COMPARE.HEADER) + "\n"
    text += "".join("\t".join(row) + "\n" for row in rows)
    (directory / "frames.tsv").write_text(text, encoding="utf-8")
    (directory / "frames.bin").write_bytes(b"".join(packed or [BLACK] * len(rows)))
    return directory


def read_png(path):
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise AssertionError("not a PNG")
    cursor = 8
    compressed = bytearray()
    while cursor < len(data):
        size = struct.unpack_from(">I", data, cursor)[0]
        kind = data[cursor + 4 : cursor + 8]
        payload = data[cursor + 8 : cursor + 8 + size]
        crc = struct.unpack_from(">I", data, cursor + 8 + size)[0]
        if zlib.crc32(kind + payload) != crc:
            raise AssertionError("PNG CRC mismatch")
        if kind == b"IHDR":
            width, height = struct.unpack_from(">II", payload)
        if kind == b"IDAT":
            compressed.extend(payload)
        cursor += size + 12
    filtered = zlib.decompress(compressed)
    stride = width * 3
    pixels = b"".join(
        filtered[row * (stride + 1) + 1 : (row + 1) * (stride + 1)]
        for row in range(height)
    )
    return width, height, pixels


class CompareGameplayTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def compare(self, rows_a, rows_b=None, packed_a=None, packed_b=None, details=False):
        a = capture(self.root / "a", rows_a, packed_a)
        b = capture(self.root / "b", rows_b if rows_b is not None else rows_a, packed_b)
        return COMPARE.compare(a, b, self.root / "report", details)

    def test_exact_multiple_games_passes_and_mode_aliases(self):
        rows = [
            frame(0, kind="initial", pass_number=0),
            frame(1),
            frame(2, pass_number=2),
            frame(3, pass_number=3),
            frame(4, game=1),
        ]
        aliases = [row[:-1] + ["color320"] for row in rows]
        summary = self.compare(rows, aliases)
        self.assertEqual(summary["status"], "exact")
        self.assertEqual(summary["exact_frames"], 5)
        self.assertIsNone(summary["first_divergence"])
        self.assertFalse((self.root / "report" / "diff.png").exists())
        self.assertEqual(
            json.loads((self.root / "report" / "summary.json").read_text()), summary
        )

    def test_one_last_pixel_and_first_divergence_png(self):
        changed = bytearray(BLACK)
        changed[-1] = 1
        summary = self.compare(
            [frame(0), frame(1), frame(2)],
            packed_b=[BLACK, bytes(changed), BLACK],
            details=True,
        )
        self.assertEqual(summary["exact_frames"], 2)
        self.assertEqual(summary["different_rgb_pixels"], 1)
        first = summary["first_divergence"]
        self.assertEqual(first["native"]["seq"], 1)
        self.assertEqual(first["bounds"], [319, 199, 319, 199])
        self.assertIn("packed_pixels", first["reasons"])
        self.assertEqual(
            read_png(self.root / "report" / "native.png")[-1][-3:], b"\x55\xff\xff"
        )
        width, height, pixels = read_png(self.root / "report" / "diff.png")
        self.assertEqual((width, height), (320, 200))
        self.assertEqual(pixels[:-3], bytes(320 * 200 * 3 - 3))
        self.assertEqual(pixels[-3:], b"\xff\x00\xff")
        lines = (self.root / "report" / "mismatches.ndjson").read_text().splitlines()
        self.assertEqual([json.loads(line) for line in lines], [first])
        self.assertIn(
            'src="native.png"', (self.root / "report" / "index.html").read_text()
        )

    def test_all_seven_transition_frames_are_valid(self):
        rows = [frame(i - 1, pass_number=i, kind="transition") for i in range(1, 8)]
        summary = self.compare(rows)
        self.assertEqual(summary["status"], "exact")
        self.assertEqual(summary["exact_frames"], 7)

    def test_palette_only_changes_visible_colors(self):
        packed = bytes([0x55]) * COMPARE.FRAME_BYTES
        summary = self.compare(
            [frame(palette=0)], [frame(palette=1)], [packed], [packed]
        )
        self.assertEqual(summary["different_rgb_pixels"], 320 * 200)
        self.assertEqual(
            summary["first_divergence"]["reasons"], ["palette", "visible_rgb"]
        )

    def test_palette_interpretation_is_compared_even_if_frame_black(self):
        summary = self.compare([frame(palette=0)], [frame(palette=1)])
        self.assertEqual(summary["status"], "divergent")
        self.assertEqual(summary["different_rgb_pixels"], 0)
        self.assertEqual(summary["first_divergence"]["reasons"], ["palette"])

    def test_mode_six_checks_each_of_eight_bits(self):
        packed = bytearray(BLACK)
        packed[80] = 0x80
        summary = self.compare(
            [frame(mode="6")], [frame(mode="mono640")], packed_b=[bytes(packed)]
        )
        self.assertEqual(summary["different_rgb_pixels"], 1)
        self.assertEqual(summary["first_divergence"]["bounds"], [0, 1, 0, 1])
        width, _, pixels = read_png(self.root / "report" / "native.png")
        self.assertEqual(width, 640)
        self.assertEqual(pixels[640 * 3 : 640 * 3 + 3], b"\xff\xff\xff")

    def test_mode_change_normalizes_to_visible_640_grid(self):
        packed = bytes([0xFF]) * COMPARE.FRAME_BYTES
        summary = self.compare([frame(mode="4")], [frame(mode="6")], [packed], [packed])
        self.assertEqual(summary["first_divergence"]["reasons"], ["display_mode"])
        self.assertEqual(summary["different_rgb_pixels"], 0)
        self.assertEqual(
            read_png(self.root / "report" / "reference.png")[:2], (320, 200)
        )
        self.assertEqual(read_png(self.root / "report" / "diff.png")[:2], (640, 200))

    def test_missing_middle_and_extra_frames_do_not_shift_alignment(self):
        summary = self.compare(
            [frame(0), frame(1), frame(3)], [frame(0), frame(2), frame(3)], details=True
        )
        self.assertEqual(summary["missing_native_frames"], 1)
        self.assertEqual(summary["extra_native_frames"], 1)
        self.assertEqual(summary["exact_frames"], 2)
        self.assertEqual(summary["mismatched_frames"], 2)
        self.assertEqual(summary["first_divergence"]["reference"]["seq"], 1)
        self.assertIsNone(summary["first_divergence"]["native"])

    def test_pass_key_mismatch_is_reported_with_equal_pixels(self):
        summary = self.compare([frame(pass_number=1)], [frame(pass_number=2)])
        self.assertEqual(summary["first_divergence"]["reasons"], ["frame_key"])

    def test_order_duplicate_and_truncated_data_rejected(self):
        cases = [
            ([frame(1), frame(0)], None),
            ([frame(0), frame(0)], None),
            ([frame(0)], [BLACK[:-1]]),
            ([frame(0)], [BLACK + b"x"]),
        ]
        for number, (rows, packed) in enumerate(cases):
            with self.subTest(number=number):
                a = capture(self.root / f"a{number}", rows, packed)
                b = capture(self.root / f"b{number}", [frame(0)])
                report = self.root / f"report{number}"
                with self.assertRaises(ValueError):
                    COMPARE.compare(a, b, report)
                self.assertFalse(report.exists())

    def test_cli_exit_codes_and_existing_output_preservation(self):
        a = capture(self.root / "a", [frame(0)])
        b = capture(self.root / "b", [frame(0)])
        report = self.root / "report"
        command = [sys.executable, str(TOOL), str(a), str(b), str(report)]
        exact = subprocess.run(command, capture_output=True, check=False)
        self.assertEqual(exact.returncode, 0, exact.stderr)
        self.assertEqual(json.loads(exact.stdout)["status"], "exact")
        previous = (report / "summary.json").read_bytes()
        invalid = subprocess.run(command, capture_output=True, check=False)
        self.assertEqual(invalid.returncode, 2)
        self.assertEqual((report / "summary.json").read_bytes(), previous)
        (b / "frames.tsv").write_text(
            "\t".join(COMPARE.HEADER) + "\n" + "\t".join(frame(palette=0)) + "\n"
        )
        command[-1] = str(self.root / "different")
        divergent = subprocess.run(command, capture_output=True, check=False)
        self.assertEqual(divergent.returncode, 1, divergent.stderr)

    def test_report_cannot_modify_capture_directory(self):
        a = capture(self.root / "a", [frame(0)])
        b = capture(self.root / "b", [frame(0)])
        with self.assertRaisesRegex(ValueError, "outside capture"):
            COMPARE.compare(a, b, a / "report")
        self.assertFalse((a / "report").exists())


if __name__ == "__main__":
    unittest.main()
