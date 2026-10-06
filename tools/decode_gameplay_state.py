#!/usr/bin/env python3
# Copyright (c) 2026 analogued.
# Project license not yet selected; see the repository README.
"""Normalize captured sprite pointers while retaining the original object fields.

The observer profile supplies all game-specific memory interpretations. This
decoder only reads its named TSV fields and an external table of four-byte
sprite pointer entries. The first word of each entry is a source offset relative
to the captured color resource. Unknown and null pointers stay explicitly marked
instead of being inferred from the requested sprite.
"""

import argparse
import csv
import struct
from pathlib import Path

REQUIRED_FIELDS = {"source_pointer", "source_relative"}
NORMALIZED_FIELDS = ["resolved_sprite", "resolved_status"]


def sprite_offsets(data):
    """Read unique source offsets from an external, packed pointer table."""
    if not data or len(data) % 4:
        raise ValueError("sprite pointer table must contain four-byte entries")
    offsets = {}
    for index, (offset, _segment) in enumerate(struct.iter_unpack("<HH", data)):
        if offset in offsets:
            raise ValueError("sprite pointer table contains ambiguous source offsets")
        offsets[offset] = index
    return offsets


def normalize_object(row, offsets):
    """Append a known sprite index or an explicit unresolved status to one row."""
    try:
        pointer = int(row["source_pointer"])
        relative = int(row["source_relative"])
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError("object row has invalid source pointer fields") from error
    if not 0 <= pointer < 1 << 32:
        raise ValueError("object source pointer must be an unsigned 32-bit value")
    if pointer == 0:
        index, status = -1, "null"
    elif offsets is None:
        index, status = -1, "unavailable"
    elif relative not in offsets:
        index, status = -1, "unknown"
    else:
        index, status = offsets[relative], "resolved"
    return {**row, "resolved_sprite": str(index), "resolved_status": status}


def decode_objects(source, destination, table=None):
    """Stream captured objects to an exclusively created normalized TSV."""
    source = Path(source)
    destination = Path(destination)
    if source.resolve() == destination.resolve():
        raise ValueError("normalized output must differ from its source")
    if table is not None and Path(table).resolve() == destination.resolve():
        raise ValueError("normalized output must differ from its sprite table")
    offsets = None if table is None else sprite_offsets(Path(table).read_bytes())
    counts = dict.fromkeys(("resolved", "null", "unknown", "unavailable"), 0)
    with source.open(encoding="utf-8", newline="") as original:
        reader = csv.DictReader(original, delimiter="\t")
        fields = reader.fieldnames
        if fields is None or not REQUIRED_FIELDS.issubset(fields):
            raise ValueError(
                "objects.tsv must include source_pointer and source_relative"
            )
        if len(set(fields)) != len(fields):
            raise ValueError("objects.tsv has duplicate header fields")
        if set(NORMALIZED_FIELDS).intersection(fields):
            raise ValueError("objects.tsv is already normalized")
        with destination.open("x", encoding="utf-8", newline="") as output:
            writer = csv.DictWriter(
                output,
                fieldnames=fields + NORMALIZED_FIELDS,
                delimiter="\t",
                lineterminator="\n",
            )
            writer.writeheader()
            for number, row in enumerate(reader, 2):
                if None in row or None in row.values():
                    raise ValueError(f"objects.tsv row {number}: incorrect field count")
                try:
                    decoded = normalize_object(row, offsets)
                except ValueError as error:
                    raise ValueError(f"objects.tsv row {number}: {error}") from error
                counts[decoded["resolved_status"]] += 1
                writer.writerow(decoded)
    return counts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "capture", type=Path, help="external observer capture directory"
    )
    parser.add_argument("output", type=Path, help="new normalized object TSV")
    parser.add_argument(
        "--sprite-table",
        type=Path,
        help="override the captured sprite-pointers.bin table",
    )
    args = parser.parse_args()
    table = args.sprite_table
    if table is None:
        captured_table = args.capture / "sprite-pointers.bin"
        if captured_table.is_file():
            table = captured_table
    try:
        counts = decode_objects(args.capture / "objects.tsv", args.output, table)
    except (OSError, ValueError) as error:
        parser.exit(2, f"decode_gameplay_state: {error}\n")
    print(" ".join(f"{name}={count}" for name, count in counts.items()))


if __name__ == "__main__":
    main()
