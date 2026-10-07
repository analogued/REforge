#!/usr/bin/env python3
# Copyright (c) 2026 analogued.
# Project license not yet selected; see the repository README.
"""Compare captured gameplay state independently of graphical comparisons.

Rows align by frame sequence and its game/body/pass/kind metadata. Object slots
and draw positions supply the additional per-frame keys. Every compared row is
retained in comparisons.ndjson; summary.json identifies the first difference.
Empty or absent native fields are unmodeled, and unresolved original sprite
pointers exclude only the corresponding normalized sprite comparison.
"""

import argparse
import csv
import json
from pathlib import Path

FRAME_KEY = ("seq", "game", "body", "pass", "kind")
TABLES = (
    ("state.tsv", ()),
    ("objects-normalized.tsv", ("slot",)),
    ("draw-order.tsv", ("position",)),
)
TEXT_FIELDS = {"kind", "resolved_status"}
RESOLVED_STATUSES = {"resolved", "null", "unknown", "unavailable"}


def rows(path, extra_key):
    """Validate and stream rows in frame order, sorting each frame's small group."""
    with path.open(encoding="utf-8", newline="") as source:
        reader = csv.DictReader(source, delimiter="\t")
        headers = reader.fieldnames
        keys = FRAME_KEY + extra_key
        if not headers or not set(keys).issubset(headers):
            raise ValueError(f"{path.name}: missing required key columns")
        if len(set(headers)) != len(headers):
            raise ValueError(f"{path.name}: duplicate header columns")
        group = {}
        frame = None
        for number, raw in enumerate(reader, 2):
            if None in raw or None in raw.values():
                raise ValueError(f"{path.name} row {number}: incorrect field count")
            row = {}
            for name, value in raw.items():
                if value == "":
                    row[name] = None
                elif name in TEXT_FIELDS:
                    row[name] = value
                else:
                    try:
                        row[name] = int(value)
                    except ValueError as error:
                        raise ValueError(
                            f"{path.name} row {number}: invalid integer {name}"
                        ) from error
            if any(row[name] is None for name in keys):
                raise ValueError(f"{path.name} row {number}: empty key field")
            if (
                row["seq"] < 0
                or row["game"] < 0
                or not 0 <= row["body"] <= 65535
                or not 0 <= row["pass"] <= 65535
                or row["kind"] not in {"initial", "gameplay", "transition", "ending"}
                or any(row[name] < 0 for name in extra_key)
            ):
                raise ValueError(f"{path.name} row {number}: invalid frame key")
            if (
                row.get("resolved_status") is not None
                and row["resolved_status"] not in RESOLVED_STATUSES
            ):
                raise ValueError(f"{path.name} row {number}: invalid resolved status")
            prefix = tuple(row[name] for name in FRAME_KEY)
            if frame is not None and prefix != frame:
                if prefix[0] <= frame[0]:
                    raise ValueError(
                        f"{path.name} row {number}: inconsistent frame order"
                    )
                yield from sorted(group.items())
                group.clear()
            frame = prefix
            key = tuple(row[name] for name in keys)
            if key in group:
                raise ValueError(f"{path.name} row {number}: duplicate row key")
            group[key] = row
        yield from sorted(group.items())


def compare_row(original, native, keys):
    """Keep comparable values and account for every excluded field explicitly."""
    compared, skipped = {}, {}
    for field in sorted(set(original) | set(native)):
        if field in keys or field == "resolved_status":
            continue
        if field not in original or original[field] is None:
            skipped[field] = "original_unmodeled"
        elif field not in native or native[field] is None:
            skipped[field] = "native_unmodeled"
        elif (
            field == "resolved_sprite" and original.get("resolved_status") != "resolved"
        ):
            skipped[field] = "original_sprite_" + str(
                original.get("resolved_status", "unavailable")
            )
        else:
            compared[field] = {
                "original": original[field],
                "native": native[field],
                "equal": original[field] == native[field],
            }
    return compared, skipped


def compare_capture(original, native, destination):
    """Write a new complete row trace and independent state-comparison summary."""
    original, native, destination = map(Path, (original, native, destination))
    for capture in (original, native):
        if destination.resolve().is_relative_to(capture.resolve()):
            raise ValueError("state report must be outside both capture directories")
    # Validate all headers before creating persistent report output.
    for filename, extra_key in TABLES:
        for directory in (original, native):
            next(rows(directory / filename, extra_key), None)
    destination.mkdir()
    summary = {
        "original": str(original.resolve()),
        "native": str(native.resolve()),
        "status": "equal",
        "compared_fields": 0,
        "differing_fields": 0,
        "uncompared_fields": 0,
        "missing_rows": 0,
        "first_difference": None,
        "tables": {},
    }
    summary_path = destination / "summary.json"
    try:
        with (destination / "comparisons.ndjson").open("x", encoding="utf-8") as trace:
            for filename, extra_key in TABLES:
                counts = {
                    "rows": 0,
                    "compared_fields": 0,
                    "differing_fields": 0,
                    "uncompared_fields": 0,
                    "missing_rows": 0,
                }
                summary["tables"][filename] = counts
                left, right = (
                    rows(original / filename, extra_key),
                    rows(native / filename, extra_key),
                )
                a, b = next(left, None), next(right, None)
                keys = FRAME_KEY + extra_key
                while a is not None or b is not None:
                    key = min(item[0] for item in (a, b) if item is not None)
                    key_fields = dict(zip(keys, key, strict=True))
                    record = {"table": filename, "key": key_fields}
                    advance_left = advance_right = False
                    if a is None or a[0] != key:
                        record.update(status="missing_original", native=b[1])
                        advance_right = True
                    elif b is None or b[0] != key:
                        record.update(status="missing_native", original=a[1])
                        advance_left = True
                    else:
                        compared, skipped = compare_row(a[1], b[1], keys)
                        differences = [
                            name
                            for name, values in compared.items()
                            if not values["equal"]
                        ]
                        record.update(
                            status="different" if differences else "equal",
                            compared=compared,
                            uncompared=skipped,
                        )
                        counts["compared_fields"] += len(compared)
                        counts["differing_fields"] += len(differences)
                        counts["uncompared_fields"] += len(skipped)
                        if differences and (
                            summary["first_difference"] is None
                            or key_fields["seq"]
                            < summary["first_difference"]["key"]["seq"]
                        ):
                            field = differences[0]
                            summary["first_difference"] = {
                                "table": filename,
                                "key": key_fields,
                                "field": field,
                                **compared[field],
                            }
                        advance_left = advance_right = True
                    if record["status"].startswith("missing_"):
                        counts["missing_rows"] += 1
                        if (
                            summary["first_difference"] is None
                            or key_fields["seq"]
                            < summary["first_difference"]["key"]["seq"]
                        ):
                            summary["first_difference"] = {
                                "table": filename,
                                "key": key_fields,
                                "reason": record["status"],
                            }
                    counts["rows"] += 1
                    trace.write(json.dumps(record, separators=(",", ":")) + "\n")
                    if advance_left:
                        a = next(left, None)
                    if advance_right:
                        b = next(right, None)
                for field in (
                    "compared_fields",
                    "differing_fields",
                    "uncompared_fields",
                    "missing_rows",
                ):
                    summary[field] += counts[field]
        if summary["compared_fields"] == 0 and summary["missing_rows"] == 0:
            raise ValueError("no modeled state fields were available for comparison")
        if summary["first_difference"] is not None:
            summary["status"] = "different"
    except (OSError, ValueError) as error:
        summary["status"] = "invalid"
        summary["error"] = str(error)
        summary_path.write_text(json.dumps(summary, indent=2) + "\n")
        raise
    summary_path.write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    parser.add_argument("native", type=Path)
    parser.add_argument("output", type=Path, help="new directory for the state report")
    args = parser.parse_args()
    try:
        summary = compare_capture(args.original, args.native, args.output)
    except (OSError, ValueError) as error:
        parser.exit(2, f"compare_gameplay_state: {error}\n")
    print(
        f"state={summary['status']} compared={summary['compared_fields']} differing={summary['differing_fields']} uncompared={summary['uncompared_fields']} missing_rows={summary['missing_rows']}"
    )
    raise SystemExit(int(summary["status"] == "different"))


if __name__ == "__main__":
    main()
