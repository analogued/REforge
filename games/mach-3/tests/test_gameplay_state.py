# Copyright (c) 2026 analogued.
# Project license not yet selected; see the repository README.
"""Synthetic checks for captured-state normalization and profile boundaries."""

import csv
import importlib.util
import json
import struct
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "decode_gameplay_state", REPOSITORY / "tools" / "decode_gameplay_state.py"
)
decoder = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(decoder)
COMPARE_SPEC = importlib.util.spec_from_file_location(
    "compare_gameplay_state", REPOSITORY / "tools" / "compare_gameplay_state.py"
)
comparator = importlib.util.module_from_spec(COMPARE_SPEC)
COMPARE_SPEC.loader.exec_module(comparator)


def write_table(directory, filename, fields, data):
    with (directory / filename).open("w", newline="") as output:
        writer = csv.writer(output, delimiter="\t", lineterminator="\n")
        writer.writerow(fields)
        writer.writerows(data)


def state_fixture(directory, values=(10, 20, 30), extra_native=False):
    """Authored rows model no original instructions, memory, or resources."""
    directory.mkdir()
    meta = ["seq", "game", "body", "pass", "kind"]
    rows = [[i, 0, i, 1, "gameplay", value] for i, value in enumerate(values)]
    fields = meta + ["fuel"]
    if extra_native:
        fields += ["unmodeled"]
        rows = [row + [""] for row in rows]
    write_table(directory, "state.tsv", fields, rows)
    write_table(
        directory,
        "objects-normalized.tsv",
        meta + ["slot", "requested_sprite", "resolved_sprite", "resolved_status"],
        [[i, 0, i, 1, "gameplay", 5, 10, 9, "resolved"] for i in range(len(values))],
    )
    write_table(
        directory,
        "draw-order.tsv",
        meta + ["position", "slot"],
        [[i, 0, i, 1, "gameplay", 0, 5] for i in range(len(values))],
    )


class StateComparisonTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.original = self.directory / "original"
        self.native = self.directory / "native"
        state_fixture(self.original)
        state_fixture(self.native, extra_native=True)

    def test_equal_fields_and_every_comparison_are_retained(self):
        output = self.directory / "report"
        summary = comparator.compare_capture(self.original, self.native, output)
        self.assertEqual(summary["status"], "equal")
        self.assertEqual(summary["compared_fields"], 12)
        self.assertEqual(summary["uncompared_fields"], 3)
        trace = [
            json.loads(line)
            for line in (output / "comparisons.ndjson").read_text().splitlines()
        ]
        self.assertEqual(len(trace), 9)
        self.assertEqual(
            trace[0]["compared"]["fuel"], {"original": 10, "native": 10, "equal": True}
        )
        self.assertEqual(trace[0]["uncompared"], {"unmodeled": "original_unmodeled"})
        with self.assertRaises(FileExistsError):
            comparator.compare_capture(self.original, self.native, output)

    def test_missing_middle_row_keeps_later_frame_alignment(self):
        fields = list(comparator.FRAME_KEY) + ["fuel"]
        write_table(
            self.native,
            "state.tsv",
            fields,
            [[0, 0, 0, 1, "gameplay", 10], [2, 0, 2, 1, "gameplay", 30]],
        )
        output = self.directory / "report"
        summary = comparator.compare_capture(self.original, self.native, output)
        self.assertEqual(summary["status"], "different")
        self.assertEqual(summary["missing_rows"], 1)
        self.assertEqual(summary["differing_fields"], 0)
        self.assertEqual(summary["first_difference"]["key"]["seq"], 1)
        trace = [
            json.loads(line)
            for line in (output / "comparisons.ndjson").read_text().splitlines()
        ]
        self.assertEqual(
            [row["status"] for row in trace[:3]], ["equal", "missing_native", "equal"]
        )

    def test_unresolved_source_skips_only_sprite_and_counts_unmodeled_fields(self):
        original = {
            "fuel": 10,
            "requested_sprite": 10,
            "resolved_sprite": -1,
            "resolved_status": "unknown",
            "raw_pointer": 123,
        }
        native = {
            "fuel": "",
            "requested_sprite": 11,
            "resolved_sprite": 0,
            "resolved_status": "resolved",
        }
        # Parsed empty native fields are None; preserve raw original pointer coverage.
        native["fuel"] = None
        compared, skipped = comparator.compare_row(original, native, ())
        self.assertEqual(set(compared), {"requested_sprite"})
        self.assertFalse(compared["requested_sprite"]["equal"])
        self.assertEqual(
            skipped,
            {
                "fuel": "native_unmodeled",
                "raw_pointer": "native_unmodeled",
                "resolved_sprite": "original_sprite_unknown",
            },
        )
        for status in ("null", "unavailable"):
            original["resolved_status"] = status
            _, skipped = comparator.compare_row(original, native, ())
            self.assertEqual(skipped["resolved_sprite"], f"original_sprite_{status}")

    def test_first_difference_is_earliest_frame_across_tables(self):
        write_table(
            self.native,
            "state.tsv",
            list(comparator.FRAME_KEY) + ["fuel"],
            [[i, 0, i, 1, "gameplay", value] for i, value in enumerate((10, 20, 31))],
        )
        fields = list(comparator.FRAME_KEY) + [
            "slot",
            "requested_sprite",
            "resolved_sprite",
            "resolved_status",
        ]
        write_table(
            self.native,
            "objects-normalized.tsv",
            fields,
            [
                [i, 0, i, 1, "gameplay", 5, 11 if i == 1 else 10, 9, "resolved"]
                for i in range(3)
            ],
        )
        summary = comparator.compare_capture(
            self.original, self.native, self.directory / "report"
        )
        self.assertEqual(summary["differing_fields"], 2)
        self.assertEqual(summary["first_difference"]["table"], "objects-normalized.tsv")
        self.assertEqual(summary["first_difference"]["key"]["seq"], 1)

    def test_empty_native_capture_reports_all_missing_rows(self):
        for filename, _extra_key in comparator.TABLES:
            path = self.native / filename
            header = path.read_text().splitlines()[0]
            path.write_text(header + "\n")
        summary = comparator.compare_capture(
            self.original, self.native, self.directory / "report"
        )
        self.assertEqual(summary["status"], "different")
        self.assertEqual(summary["missing_rows"], 9)
        self.assertEqual(summary["compared_fields"], 0)

    def test_duplicate_header_duplicate_key_and_inconsistent_metadata_are_invalid(self):
        path = self.native / "state.tsv"
        invalid = (
            "seq\tgame\tbody\tpass\tkind\tfuel\tfuel\n0\t0\t0\t1\tgameplay\t10\t10\n",
            "seq\tgame\tbody\tpass\tkind\tfuel\n0\t0\t0\t1\tgameplay\t10\n0\t0\t0\t1\tgameplay\t10\n",
            "seq\tgame\tbody\tpass\tkind\tfuel\n0\t0\t0\t1\tgameplay\t10\n0\t0\t1\t1\tgameplay\t10\n",
        )
        for index, text in enumerate(invalid):
            path.write_text(text)
            with self.subTest(index=index), self.assertRaises(ValueError):
                comparator.compare_capture(
                    self.original, self.native, self.directory / f"report{index}"
                )

    def test_cli_equal_difference_and_invalid_exit_codes(self):
        script = REPOSITORY / "tools" / "compare_gameplay_state.py"

        def run(name):
            return subprocess.run(
                [
                    sys.executable,
                    script,
                    self.original,
                    self.native,
                    self.directory / name,
                ],
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertEqual(run("equal").returncode, 0)
        write_table(
            self.native,
            "state.tsv",
            list(comparator.FRAME_KEY) + ["fuel"],
            [[i, 0, i, 1, "gameplay", value] for i, value in enumerate((10, 21, 30))],
        )
        self.assertEqual(run("different").returncode, 1)
        (self.native / "state.tsv").write_text("duplicate\tduplicate\n1\t1\n")
        self.assertEqual(run("invalid").returncode, 2)


class SpriteNormalizationTests(unittest.TestCase):
    def test_resolved_source_takes_precedence_over_requested_sprite(self):
        offsets = decoder.sprite_offsets(struct.pack("<HHHH", 100, 0, 200, 0))
        row = {
            "source_pointer": "12345",
            "source_relative": "200",
            "requested_sprite": "0",
        }
        normalized = decoder.normalize_object(row, offsets)
        self.assertEqual(normalized["resolved_sprite"], "1")
        self.assertEqual(normalized["resolved_status"], "resolved")
        self.assertEqual(normalized["requested_sprite"], "0")
        self.assertEqual(
            row,
            {
                "source_pointer": "12345",
                "source_relative": "200",
                "requested_sprite": "0",
            },
        )

    def test_unknown_missing_and_null_sources_are_distinct(self):
        row = {"source_pointer": "12345", "source_relative": "-200"}
        self.assertEqual(
            decoder.normalize_object(row, {100: 0})["resolved_status"], "unknown"
        )
        self.assertEqual(
            decoder.normalize_object(row, None)["resolved_status"], "unavailable"
        )
        row["source_pointer"] = "0"
        normalized = decoder.normalize_object(row, {100: 0})
        self.assertEqual(normalized["resolved_status"], "null")
        self.assertEqual(normalized["resolved_sprite"], "-1")

    def test_invalid_or_ambiguous_tables_are_rejected(self):
        for data in (b"", b"\0\0\0", struct.pack("<HHHH", 100, 0, 100, 7)):
            with self.subTest(length=len(data)), self.assertRaises(ValueError):
                decoder.sprite_offsets(data)

    def test_stream_preserves_metadata_and_refuses_replacement(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            source = directory / "objects.tsv"
            source.write_text(
                "seq\tgame\tbody\tpass\tkind\tsource_pointer\tsource_relative\n"
                "9\t1\t65535\t3\tgameplay\t12345\t200\n"
                "10\t1\t0\t1\ttransition\t0\t-10000\n"
            )
            table = directory / "sprites.bin"
            table.write_bytes(struct.pack("<HHHH", 100, 0, 200, 0))
            output = directory / "normalized.tsv"
            counts = decoder.decode_objects(source, output, table)
            self.assertEqual(
                counts, {"resolved": 1, "null": 1, "unknown": 0, "unavailable": 0}
            )
            with output.open(newline="") as opened:
                rows = list(csv.DictReader(opened, delimiter="\t"))
            self.assertEqual(rows[0]["seq"], "9")
            self.assertEqual(rows[0]["body"], "65535")
            self.assertEqual(rows[1]["kind"], "transition")
            with self.assertRaises(FileExistsError):
                decoder.decode_objects(source, output, table)
            with self.assertRaises(ValueError):
                decoder.decode_objects(source, source, table)

    def test_bad_rows_and_double_normalization_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for index, text in enumerate(
                (
                    "source_pointer\tsource_relative\n123\n",
                    "source_pointer\tsource_relative\nword\t10\n",
                    "source_pointer\tsource_relative\tresolved_sprite\n123\t10\t0\n",
                )
            ):
                source = directory / f"source{index}.tsv"
                source.write_text(text)
                with self.assertRaises(ValueError):
                    decoder.decode_objects(source, directory / f"output{index}.tsv")


class CaptureProfileTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.profile = tomllib.loads((REPOSITORY / "capture" / "mach3.toml").read_text())

    def test_external_signature_and_instruction_boundaries(self):
        signature = self.profile["signatures"][0]
        self.assertEqual(signature["bytes_binding"], "program_signature")
        self.assertNotIn("bytes", signature)
        transitions = [
            event
            for event in self.profile["events"]
            if event["name"].startswith("transition_frame_")
        ]
        self.assertEqual(len(transitions), 7)
        for event in transitions:
            self.assertIn("mem16(base.ss + reg.bp + 2) == 0", event["guard"])
            self.assertIn("reg.cs == game_cs", event["guard"])
        frame_group = self.profile["action_groups"][0]
        dumps = [
            action
            for action in frame_group["actions"]
            if action.get("stream") == "data_segment"
        ]
        self.assertEqual(len(dumps), 1)
        self.assertEqual(dumps[0]["bytes_per_row"], "65536")
        self.assertEqual(
            dumps[0]["guard"],
            'kind == "initial" or (kind == "gameplay" and frame_pass == 3)',
        )


if __name__ == "__main__":
    unittest.main()
