#!/usr/bin/env python3
# Copyright (c) 2026 analogued.
# Project license not yet selected; see the repository README.
"""Compare synchronized gameplay captures and write a first-divergence report.

Each capture contains frames.tsv (seq, game, body, pass, kind, palette, mode)
and frames.bin: one linear 16000-byte visible CGA frame per index row.
"""

import argparse
import csv
import html
import json
import struct
import zlib
from dataclasses import dataclass
from pathlib import Path

FRAME_BYTES = 16000
HEIGHT = 200
HEADER = ["seq", "game", "body", "pass", "kind", "palette", "mode"]
KINDS = {"initial", "gameplay", "transition", "ending"}
MODES = {"4": "color320", "color320": "color320", "6": "mono640", "mono640": "mono640"}
PALETTES = (
    ((0, 0, 0), (85, 255, 85), (255, 85, 85), (255, 255, 85)),
    ((0, 0, 0), (85, 255, 255), (255, 85, 255), (255, 255, 255)),
)


@dataclass(frozen=True)
class FrameInfo:
    seq: int
    game: int
    body: int
    pass_number: int
    kind: str
    palette: int
    mode: str

    def key(self):
        return (self.game, self.seq, self.body, self.pass_number, self.kind)

    def as_dict(self):
        return {
            "seq": self.seq,
            "game": self.game,
            "body": self.body,
            "pass": self.pass_number,
            "kind": self.kind,
            "palette": self.palette,
            "mode": self.mode,
        }


def index_rows(directory):
    """Validate metadata, including order, while streaming index rows."""
    with (directory / "frames.tsv").open(encoding="utf-8", newline="") as source:
        reader = csv.reader(source, delimiter="\t")
        if next(reader, None) != HEADER:
            raise ValueError("frames.tsv must have header: " + "\\t".join(HEADER))
        previous_seq = -1
        for row_number, row in enumerate(reader, 2):
            if len(row) != len(HEADER):
                raise ValueError(f"frames.tsv row {row_number}: expected seven fields")
            try:
                seq, game, body, pass_number = map(int, row[:4])
                palette = int(row[5])
                mode = MODES[row[6]]
            except (ValueError, KeyError) as error:
                raise ValueError(
                    f"frames.tsv row {row_number}: invalid integer/mode"
                ) from error
            if seq <= previous_seq:
                raise ValueError(
                    f"frames.tsv row {row_number}: sequence is out of order or duplicated"
                )
            if game < 0 or not 0 <= body <= 65535:
                raise ValueError(
                    f"frames.tsv row {row_number}: game/body/pass out of range"
                )
            if palette not in (0, 1) or row[4] not in KINDS:
                raise ValueError(f"frames.tsv row {row_number}: invalid palette/kind")
            limits = {
                "initial": (0, 0),
                "gameplay": (1, 3),
                "transition": (1, 7),
                "ending": (0, 65535),
            }
            low, high = limits[row[4]]
            if not low <= pass_number <= high:
                raise ValueError(
                    f"frames.tsv row {row_number}: pass out of range for kind"
                )
            previous_seq = seq
            yield FrameInfo(seq, game, body, pass_number, row[4], palette, mode)


def validate_capture(directory):
    count = sum(1 for _ in index_rows(directory))
    size = (directory / "frames.bin").stat().st_size
    if size != count * FRAME_BYTES:
        raise ValueError(
            f"frames.bin has {size} bytes; index requires {count * FRAME_BYTES}"
        )
    return count


def capture_frames(directory):
    with (directory / "frames.bin").open("rb") as source:
        for info in index_rows(directory):
            packed = source.read(FRAME_BYTES)
            if len(packed) != FRAME_BYTES:
                raise ValueError("frames.bin was truncated during comparison")
            yield info, packed
        if source.read(1):
            raise ValueError("frames.bin grew during comparison")


def render(info, packed, doubled=False):
    """Render the native display; double color pixels for cross-mode comparison."""
    if info.mode == "mono640":
        lookup = tuple(
            bytes(
                level
                for shift in range(7, -1, -1)
                for level in ((255 if value & (1 << shift) else 0),) * 3
            )
            for value in range(256)
        )
        width = 640
    else:
        colors = tuple(bytes(color) for color in PALETTES[info.palette])
        lookup = tuple(
            b"".join(
                colors[(value >> shift) & 3] * (2 if doubled else 1)
                for shift in (6, 4, 2, 0)
            )
            for value in range(256)
        )
        width = 640 if doubled else 320
    return width, b"".join(lookup[value] for value in packed)


def pixel_difference(left, right, width):
    """Return exact RGB pixel count/bounds and a high-contrast difference image."""
    count = 0
    minimum_x, minimum_y = width, HEIGHT
    maximum_x = maximum_y = -1
    diff = bytearray(len(left))
    for offset in range(0, len(left), 3):
        if left[offset : offset + 3] != right[offset : offset + 3]:
            count += 1
            x, y = (offset // 3) % width, (offset // 3) // width
            minimum_x, minimum_y = min(minimum_x, x), min(minimum_y, y)
            maximum_x, maximum_y = max(maximum_x, x), max(maximum_y, y)
            diff[offset : offset + 3] = b"\xff\x00\xff"
    bounds = [minimum_x, minimum_y, maximum_x, maximum_y] if count else None
    return count, bounds, bytes(diff)


def png(path, width, pixels):
    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data))
        )

    stride = width * 3
    filtered = b"".join(
        b"\0" + pixels[start : start + stride]
        for start in range(0, len(pixels), stride)
    )
    encoded = b"\x89PNG\r\n\x1a\n"
    encoded += chunk(b"IHDR", struct.pack(">IIBBBBB", width, HEIGHT, 8, 2, 0, 0, 0))
    encoded += chunk(b"IDAT", zlib.compress(filtered)) + chunk(b"IEND", b"")
    path.write_bytes(encoded)


def first_artifacts(report, record, left, right):
    available = left or right
    width, available_rgb = render(*available)
    pictures = {}
    for name, frame in (("reference", left), ("native", right)):
        pictures[name] = render(*frame) if frame else (width, bytes(len(available_rgb)))
        png(report / f"{name}.png", *pictures[name])
    doubled = left is not None and right is not None and left[0].mode != right[0].mode
    compare_width, left_rgb = (
        render(*left, doubled=doubled) if left else pictures["reference"]
    )
    _, right_rgb = render(*right, doubled=doubled) if right else pictures["native"]
    if left is None or right is None:
        difference = b"\xff\x00\xff" * (compare_width * HEIGHT)
    else:
        _, _, difference = pixel_difference(left_rgb, right_rgb, compare_width)
    png(report / "diff.png", compare_width, difference)
    metadata = html.escape(json.dumps(record, indent=2))
    document = """<!doctype html><meta charset="utf-8"><title>Gameplay comparison</title>
<style>body{font:16px system-ui;background:#181818;color:#eee;margin:24px}
section{display:flex;flex-wrap:wrap;gap:24px}figure{margin:0}img{image-rendering:pixelated;
width:640px;max-width:100%;height:auto;aspect-ratio:4/3;object-fit:fill;background:#000}pre{white-space:pre-wrap}
</style><h1>First gameplay divergence</h1><p>Magenta marks different visible pixels.
An absent frame is shown as black, with the entire difference image marked.</p><section>"""
    for name, label in (
        ("reference", "Reference"),
        ("native", "Native"),
        ("diff", "Difference"),
    ):
        document += f'<figure><figcaption>{label}</figcaption><img src="{name}.png" alt="{label}"></figure>'
    document += f"</section><pre>{metadata}</pre>\n"
    (report / "index.html").write_text(document, encoding="utf-8")


def compare(reference, native, report, details=False):
    """Compare every frame with bounded frame storage, streaming optional details."""
    reference, native, report = map(Path, (reference, native, report))
    for source in (reference, native):
        if report.resolve().is_relative_to(source.resolve()):
            raise ValueError("report directory must be outside capture directories")
    counts = [validate_capture(source) for source in (reference, native)]
    report.mkdir(parents=True, exist_ok=False)
    summary = {
        "status": "exact",
        "reference_frames": counts[0],
        "native_frames": counts[1],
        "paired_frames": 0,
        "exact_frames": 0,
        "mismatched_frames": 0,
        "missing_native_frames": 0,
        "extra_native_frames": 0,
        "different_rgb_pixels": 0,
        "first_divergence": None,
    }
    detail_file = (
        (report / "mismatches.ndjson").open("w", encoding="utf-8") if details else None
    )
    streams = [iter(capture_frames(source)) for source in (reference, native)]
    left, right = [next(stream, None) for stream in streams]
    try:
        while left is not None or right is not None:
            pair_left, pair_right = left, right
            if right is None or (left is not None and left[0].seq < right[0].seq):
                pair_right = None
                left = next(streams[0], None)
                summary["missing_native_frames"] += 1
                reasons = ["missing_native_frame"]
            elif left is None or right[0].seq < left[0].seq:
                pair_left = None
                right = next(streams[1], None)
                summary["extra_native_frames"] += 1
                reasons = ["extra_native_frame"]
            else:
                left, right = [next(stream, None) for stream in streams]
                summary["paired_frames"] += 1
                reasons = []
            record = {
                "reference": pair_left[0].as_dict() if pair_left else None,
                "native": pair_right[0].as_dict() if pair_right else None,
                "reasons": reasons,
                "different_rgb_pixels": None,
                "bounds": None,
            }
            if pair_left is not None and pair_right is not None:
                a, a_bytes = pair_left
                b, b_bytes = pair_right
                if a.key() != b.key():
                    reasons.append("frame_key")
                if a.mode != b.mode:
                    reasons.append("display_mode")
                if a.palette != b.palette:
                    reasons.append("palette")
                if a_bytes != b_bytes:
                    reasons.append("packed_pixels")
                if reasons:
                    doubled = a.mode != b.mode
                    width, a_rgb = render(a, a_bytes, doubled=doubled)
                    _, b_rgb = render(b, b_bytes, doubled=doubled)
                    changed, bounds, _ = pixel_difference(a_rgb, b_rgb, width)
                    record.update(
                        different_rgb_pixels=changed,
                        bounds=bounds,
                        comparison_width=width,
                    )
                    summary["different_rgb_pixels"] += changed
                    if changed:
                        reasons.append("visible_rgb")
            if reasons:
                summary["mismatched_frames"] += 1
                summary["status"] = "divergent"
                if summary["first_divergence"] is None:
                    summary["first_divergence"] = record
                    first_artifacts(report, record, pair_left, pair_right)
                if detail_file:
                    detail_file.write(json.dumps(record) + "\n")
            else:
                summary["exact_frames"] += 1
    finally:
        if detail_file:
            detail_file.close()
        for stream in streams:
            stream.close()
    (report / "summary.json").write_text(
        json.dumps(summary, indent=2) + "\n", encoding="utf-8"
    )
    if summary["first_divergence"] is None:
        (report / "index.html").write_text(
            '<!doctype html><meta charset="utf-8"><title>Gameplay comparison</title>'
            "<h1>Gameplay captures match exactly</h1><p>All indexed frames, packed pixels, "
            "palettes and display modes agree.</p>\n",
            encoding="utf-8",
        )
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("native", type=Path)
    parser.add_argument(
        "report", type=Path, help="new report directory outside captures"
    )
    parser.add_argument(
        "--details",
        action="store_true",
        help="stream every mismatch to mismatches.ndjson",
    )
    args = parser.parse_args()
    try:
        summary = compare(args.reference, args.native, args.report, args.details)
    except (OSError, ValueError, csv.Error) as error:
        parser.exit(2, f"{parser.prog}: {error}\n")
    print(json.dumps(summary))
    return 0 if summary["status"] == "exact" else 1


if __name__ == "__main__":
    raise SystemExit(main())
