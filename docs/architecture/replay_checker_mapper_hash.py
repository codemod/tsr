#!/usr/bin/env python3
"""Reconstruct the qualified mapper observer in an isolated frozen checkout."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    args = parser.parse_args()
    directory = Path(__file__).resolve().parent
    repository = directory.parents[1]
    source = args.source.resolve()
    report = json.loads((directory / "checker-mapper-hash.json").read_text())
    if source == repository or source.is_relative_to(repository):
        parser.error("use an isolated checkout outside the artifact repository")
    head = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    if head != report["source"]:
        parser.error("checkout does not match the frozen source revision")
    for relative, expected in report["source_original_sha256"].items():
        path = source / relative
        if expected is None:
            if path.exists():
                parser.error(f"observer module already exists: {relative}")
        elif not path.is_file() or digest(path) != expected:
            parser.error(f"source has changed: {relative}")
    module = directory / "checker-mapper-hash-probe.rs"
    module_path = source / "crates/tsr-checker/src/mapper_hash_probe.rs"
    if module_path.exists():
        parser.error("hash observer module already exists")
    patch = directory / "checker-mapper-hash-probe.patch"
    for artifact in (patch, module):
        if digest(artifact) != report["artifacts"][artifact.name]:
            parser.error(f"archive digest changed: {artifact.name}")
    subprocess.run(["git", "apply", "--unidiff-zero", "--check", str(patch)], cwd=source, check=True)
    subprocess.run(["git", "apply", "--unidiff-zero", str(patch)], cwd=source, check=True)
    module_path.write_bytes(module.read_bytes())
    observed = {path: digest(source / path)
                for path in report["observer_source_sha256"]}
    if observed != report["observer_source_sha256"]:
        raise RuntimeError("replay did not reconstruct the measured observer source")
    print(json.dumps({"source": head, "files": observed}, indent=2))


if __name__ == "__main__":
    main()
