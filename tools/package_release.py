#!/usr/bin/env python3
# Copyright (c) 2026 analogued.
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build REforge and package it with one complete third-party notices document."""

import argparse
import hashlib
import html
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile


ROOT = Path(__file__).resolve().parents[1]
ABOUT_VERSION = "0.9.2"


def run(command):
    return subprocess.run(command, cwd=ROOT, check=True, text=True, capture_output=True)


def application_version(args):
    command = [
        "cargo", "metadata", "--locked", "--no-deps", "--format-version", "1",
        "--manifest-path", "app/Cargo.toml",
    ]
    if args.offline:
        command.append("--offline")
    metadata = json.loads(run(command).stdout)
    return next(package["version"] for package in metadata["packages"]
                if package["name"] == "reforge")


def notices(args, target, project_version):
    version = run(["cargo", "about", "--version"]).stdout.strip()
    if version != f"cargo-about {ABOUT_VERSION}":
        raise ValueError(f"Expected cargo-about {ABOUT_VERSION}, got {version!r}")
    command = [
        "cargo", "about", "generate", "--manifest-path", "app/Cargo.toml",
        "--config", "about.toml", "--locked", "--fail", "--target", target,
        "--format", "json", *feature_flags(args),
    ]
    if args.offline:
        command.append("--offline")
    result = run(command)
    # Preserve upstream-fetch and scanning warnings even on a successful exit.
    if result.stderr:
        print(result.stderr, end="", file=sys.stderr)
    data = json.loads(result.stdout)
    if not data["licenses"]:
        raise ValueError("No dependency licenses were generated")
    for license in data["licenses"]:
        if license["id"] == "MIT" and "<copyright holders>" in license["text"]:
            names = ", ".join(item["crate"]["name"] for item in license["used_by"])
            raise ValueError(f"Missing upstream copyright notices: {names}; "
                             "retry online or update about.toml clarifications")

    escape = html.escape
    lock_hash = hashlib.sha256((ROOT / "Cargo.lock").read_bytes()).hexdigest()
    parts = [
        '<!doctype html><html lang="en"><meta charset="utf-8">',
        '<meta name="viewport" content="width=device-width, initial-scale=1">',
        '<title>REforge — Third-party notices</title>',
        '<style>body{font:16px system-ui;max-width:75em;margin:2em auto;padding:0 1em}'
        'pre{white-space:pre-wrap;overflow-wrap:anywhere}section{margin:2em 0}'
        'h2{border-bottom:1px solid #aaa;padding-bottom:.3em}</style>',
        '<h1>REforge — Third-party notices</h1>',
        f'<p>REforge version: {escape(project_version)}.</p>',
        f'<p>Target: {escape(target)}. Default features: '
        f'{"disabled" if args.no_default_features else "enabled"}. '
        f'Additional features: {escape(args.features or "none")}.</p>',
        f'<p>Cargo.lock SHA-256: <code>{lock_hash}</code>. '
        f'Generated with {escape(version)}.</p>',
        '<h2>Direct dependencies, imported code and assets</h2>',
        '<p>The source inventory below also covers assets omitted by headless builds. '
        'Their license texts follow it; no separate files or network access are '
        'needed to read these notices.</p>',
        f'<pre>{escape((ROOT / "THIRD_PARTY.md").read_text())}</pre>',
        '<h2>License texts for the source inventory</h2>',
    ]
    for path in sorted((ROOT / "LICENSES").glob("*.txt")):
        parts.append(f'<section><h3>{escape(path.stem)}</h3>'
                     f'<pre>{escape(path.read_text())}</pre></section>')
    parts.append('<h2>Cargo dependency notices</h2>')
    for license in data["licenses"]:
        components = ", ".join(sorted(
            f'{item["crate"]["name"]} {item["crate"]["version"]}'
            for item in license["used_by"]
        ))
        parts.append(f'<section><h3>{escape(license["name"])}</h3>'
                     f'<p>{escape(components)}</p>'
                     f'<pre>{escape(license["text"])}</pre></section>')
    parts.append('</html>\n')
    return "\n".join(parts)


def feature_flags(args):
    flags = ["--no-default-features"] if args.no_default_features else []
    if args.features:
        flags += ["--features", args.features]
    return flags


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", help="Rust target triple (default: rustc host)")
    parser.add_argument("--features", default="", help="Comma-separated Cargo features")
    parser.add_argument("--no-default-features", action="store_true")
    parser.add_argument("--offline", action="store_true", help="Require cached Cargo inputs")
    parser.add_argument("--notices-only", action="store_true", help="Generate HTML without building")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "out" / "releases")
    args = parser.parse_args()
    host = next(line.removeprefix("host: ") for line in run(["rustc", "-vV"]).stdout.splitlines()
                if line.startswith("host: "))
    target = args.target or host
    # Also protects the archive name from separators supplied in --target.
    if not target or any(c not in "abcdefghijklmnopqrstuvwxyz0123456789_-." for c in target):
        parser.error("Expected a Rust target triple")
    project_license = ROOT / "LICENSE"
    if not project_license.is_file():
        raise ValueError("Missing project LICENSE")
    version = application_version(args)
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    lock_before = (ROOT / "Cargo.lock").read_bytes()
    with tempfile.TemporaryDirectory(prefix="reforge-package-") as temporary:
        stage = Path(temporary) / f"reforge-{version}-{target}"
        stage.mkdir()
        notice = stage / "THIRD_PARTY.html"
        notice.write_text(notices(args, target, version), encoding="utf-8")
        if (ROOT / "Cargo.lock").read_bytes() != lock_before:
            raise ValueError("Cargo.lock changed during notice generation; retry")
        if args.notices_only:
            destination = output / f"THIRD_PARTY-{version}-{target}.html"
            shutil.copyfile(notice, destination)
            print(destination)
            return

        command = ["cargo", "build", "--locked", "--release", "-p", "reforge",
                   "--bin", "reforge", "--target", target,
                   "--message-format=json-render-diagnostics", *feature_flags(args)]
        if args.offline:
            command.append("--offline")
        built = run(command)
        if built.stderr:
            print(built.stderr, end="", file=sys.stderr)
        artifacts = [json.loads(line) for line in built.stdout.splitlines()]
        executable = next(Path(item["executable"]) for item in artifacts
                          if item.get("reason") == "compiler-artifact"
                          and item["target"]["name"] == "reforge" and item.get("executable"))
        shutil.copy2(executable, stage / executable.name)
        shutil.copy2(project_license, stage / "LICENSE")
        if (ROOT / "Cargo.lock").read_bytes() != lock_before:
            raise ValueError("Cargo.lock changed during the build; retry")
        if "windows" in target:
            archive = output / f"{stage.name}.zip"
            with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as package:
                for path in sorted(stage.iterdir()):
                    package.write(path, f"{stage.name}/{path.name}")
        else:
            archive = output / f"{stage.name}.tar.gz"
            with tarfile.open(archive, "w:gz") as package:
                package.add(stage, arcname=stage.name)
        print(archive)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.stderr or str(error)) from error
    except (OSError, ValueError) as error:
        raise SystemExit(str(error)) from error
