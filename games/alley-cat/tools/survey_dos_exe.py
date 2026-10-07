#!/usr/bin/env python3
"""Read a DOS MZ executable and write a survey as JSON to stdout.

Optional instruction windows use Rizin through rzpipe. Addresses in those
windows are file offsets, not relocated runtime addresses. The executable is
opened read-only; no game code is executed and no analysis database is created.
"""

import argparse
import hashlib
import json
import re
import struct
import subprocess
from pathlib import Path


HEADER_FIELDS = (
    "bytes_last_page", "pages", "relocations", "header_paragraphs",
    "min_extra_paragraphs", "max_extra_paragraphs", "initial_ss",
    "initial_sp", "checksum", "initial_ip", "initial_cs",
    "relocation_table_offset", "overlay_number",
)


def inspect_mz(data):
    if len(data) < 28 or data[:2] != b"MZ":
        raise ValueError("Expected a DOS MZ header of at least 28 bytes")
    header = dict(zip(HEADER_FIELDS, struct.unpack_from("<13H", data, 2)))
    if header["pages"] == 0 or header["bytes_last_page"] > 511:
        raise ValueError("Invalid MZ file-size fields")
    declared_size = ((header["pages"] - 1) * 512
                     + (header["bytes_last_page"] or 512))
    header_size = header["header_paragraphs"] * 16
    if not 28 <= header_size <= declared_size <= len(data):
        raise ValueError("MZ header or declared image extends beyond the file")
    table_start = header["relocation_table_offset"]
    if header["relocations"] and table_start < 28:
        raise ValueError("Relocation table overlaps the fixed MZ header")
    if table_start + header["relocations"] * 4 > header_size:
        raise ValueError("Relocation table extends beyond the MZ header")

    entry = header_size + header["initial_cs"] * 16 + header["initial_ip"]
    if not header_size <= entry < declared_size:
        raise ValueError("Initial CS:IP is outside the declared load image")
    relocations = []
    for index in range(header["relocations"]):
        offset, segment = struct.unpack_from("<HH", data, table_start + index * 4)
        target = header_size + segment * 16 + offset
        if not header_size <= target <= declared_size - 2:
            raise ValueError("Relocation target is outside the declared load image")
        relocations.append({
            "segment": segment, "offset": offset, "file_offset": target,
            "original_word": struct.unpack_from("<H", data, target)[0],
        })
    signatures = {}
    for signature in (b"PKLITE", b"LZEXE", b"UPX!"):
        signatures[signature.decode("ascii")] = [
            match.start() for match in re.finditer(re.escape(signature), data)
        ]
    return {
        "sha256": hashlib.sha256(data).hexdigest(),
        "file_size": len(data), "header": header,
        "header_size": header_size, "declared_size": declared_size,
        "load_image_size": declared_size - header_size,
        "trailing_size": len(data) - declared_size,
        "entry_file_offset": entry, "relocations": relocations,
        "packing_signature_hits": signatures,
        "ascii_runs": [
            {"file_offset": header_size + match.start(),
             "text": match.group().decode("ascii")}
            for match in re.finditer(rb"[\x20-\x7e]{8,}", data[header_size:])
        ],
    }


def instruction_window(value):
    try:
        address, count = (int(part, 0) for part in value.split(":"))
        if address < 0 or not 1 <= count <= 256:
            raise ValueError
        return address, count
    except ValueError as error:
        raise argparse.ArgumentTypeError(
            "Use FILE_OFFSET:COUNT (e.g. 0x7430:64), with 1-256 instructions"
        ) from error


def disassemble(path, windows, file_size):
    import rzpipe

    flags = ["-N", "-n", "-a", "x86", "-b", "16"]
    version = subprocess.run(
        ["rizin", "-v"], check=True, capture_output=True, text=True, timeout=10,
    ).stdout.strip()
    session = rzpipe.open(str(path.resolve()), flags=flags)
    try:
        session.cmd("b 8192")
        results = []
        for address, count in windows:
            if address >= file_size:
                raise ValueError("Instruction window starts beyond the input file")
            instructions = session.cmdj(f"pdj {count} @ {address}")
            if not isinstance(instructions, list):
                raise ValueError("Rizin did not return an instruction list")
            results.append({
                "file_offset": address, "requested_instructions": count,
                "instructions": [
                    ins for ins in instructions
                    if ins["offset"] + ins["size"] <= file_size
                ],
            })
        return {"version": version, "flags": flags, "block_size": 8192,
                "address_basis": "unrelocated file offset", "windows": results}
    finally:
        session.quit()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("--disassemble", action="store_true",
                        help="Include a 64-instruction entry window using rzpipe")
    parser.add_argument("--window", action="append", type=instruction_window,
                        default=[], help="Additional FILE_OFFSET:COUNT window")
    args = parser.parse_args()
    if args.window and not args.disassemble:
        parser.error("--window requires --disassemble")
    try:
        report = inspect_mz(args.executable.read_bytes())
        report["schema_version"] = 1
        report["input_name"] = args.executable.name
        report["tool_sha256"] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
        if args.disassemble:
            windows = [(report["entry_file_offset"], 64), *args.window]
            report["disassembly"] = disassemble(
                args.executable, windows, report["file_size"],
            )
    except (OSError, ValueError, ImportError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Survey failed: {error}\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
