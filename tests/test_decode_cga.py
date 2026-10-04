# Copyright (c) 2026 analogued.
# Project license not yet selected; see the repository README.
"""Synthetic framebuffer cases; no original game bytes are included."""

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOL = Path(__file__).resolve().parents[1] / "tools" / "decode_cga.py"
SPEC = importlib.util.spec_from_file_location("decode_cga", TOOL)
DECODER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DECODER)
COLORS = (
    bytes([0, 0, 0]),
    bytes([85, 255, 255]),
    bytes([255, 85, 255]),
    bytes([255, 255, 255]),
)


class DecodeCgaTests(unittest.TestCase):
    def test_scanline_banks_bit_order_and_last_pixel(self):
        data = bytearray(16384)
        data[0] = 0x1B  # First even row: indices 0, 1, 2, 3.
        data[8192] = 0xB1  # First odd row: indices 2, 3, 0, 1.
        data[80] = 0xFF  # Next even row begins with four white pixels.
        data[16191] = 0x01  # Final visible pixel is index 1.
        data[8000:8192] = bytes([0xFF]) * 192  # Non-visible bank padding.
        pixels = DECODER.decode_mode4(data, COLORS)
        self.assertEqual(len(pixels), 320 * 200 * 3)
        self.assertEqual(pixels[:12], b"".join(COLORS))
        self.assertEqual(pixels[960:972], b"".join(COLORS[i] for i in [2, 3, 0, 1]))
        self.assertEqual(pixels[1920:1932], COLORS[3] * 4)
        self.assertEqual(pixels[-3:], COLORS[1])
        self.assertEqual(pixels[320 * 198 * 3 : 320 * 199 * 3], COLORS[0] * 320)

    def test_truncated_second_bank_is_rejected(self):
        with self.assertRaises(ValueError):
            DECODER.decode_mode4(bytes(16191), COLORS)

    def test_cli_writes_ppm_and_preserves_existing_output(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "synthetic.bin"
            target = Path(directory) / "synthetic.ppm"
            source.write_bytes(bytes(16192))
            result = subprocess.run(
                [sys.executable, str(TOOL), str(source), str(target)],
                capture_output=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            expected = b"P6\n320 200\n255\n" + bytes(320 * 200 * 3)
            self.assertEqual(target.read_bytes(), expected)
            result = subprocess.run(
                [sys.executable, str(TOOL), str(source), str(target)],
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(target.read_bytes(), expected)
            self.assertEqual(source.read_bytes(), bytes(16192))


if __name__ == "__main__":
    unittest.main()
