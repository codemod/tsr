#!/usr/bin/env python3
"""Replay an archived, rejected experiment in an isolated frozen checkout."""
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
    parser.add_argument("--variant", choices=("candidate", "observer"), required=True)
    args = parser.parse_args()
    directory = Path(__file__).resolve().parent
    source = args.source.resolve()
    report = json.loads((directory / "checker-inline-walk.json").read_text())
    if source == directory.parents[1]:
        parser.error("use an isolated checkout, not the artifact repository")
    head = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    if head != report["source"]:
        parser.error("checkout does not match the frozen source revision")
    for path, expected in report["source_original_sha256"].items():
        if digest(source / path) != expected:
            parser.error(f"source has changed: {path}")
    patch = directory / f"checker-inline-walk-{args.variant}.patch"
    if digest(patch) != report["artifacts"][patch.name]:
        parser.error("archive patch digest changed")
    module_path = source / "crates/tsr-checker/src/graph_walk_probe.rs"
    if args.variant == "observer" and module_path.exists():
        parser.error("observer module already exists")
    subprocess.run(["git", "apply", "--unidiff-zero", "--check", str(patch)], cwd=source, check=True)
    subprocess.run(["git", "apply", "--unidiff-zero", str(patch)], cwd=source, check=True)
    expected_files = dict(report["source_original_sha256"])
    if args.variant == "candidate":
        expected_files.update(report["candidate_source_sha256"])
    else:
        # Reuse the qualified scalar/allocation observer. Only scratch backing
        # and its four-marker allocation control change in this archive.
        module = (directory / "checker-graph-walk-probe.rs").read_text()
        replacements = [
            ("pub fn push(&mut self, visited: &mut Vec<TypeId>, id: TypeId)",
             "pub(crate) fn push(&mut self, visited: &mut "
             "crate::inference::ParameterWalkVisited, id: TypeId)"),
            ("visited.len() as u64", "visited.as_slice().len() as u64"),
            ("visited.capacity() as u64", "visited.spill.capacity() as u64"),
            ('count(checker, "scratch_allocations"), 1',
             'count(checker, "scratch_allocations"), 0'),
        ]
        for before, after in replacements:
            if before not in module:
                raise RuntimeError(f"observer anchor missing: {before}")
            module = module.replace(before, after, 1)
        module_path.write_text(module)
        expected_files = report["observer_source_sha256"]
    observed = {path: digest(source / path) for path in expected_files}
    if observed != expected_files:
        raise RuntimeError("replay did not reconstruct the measured source")
    print(json.dumps({"variant": args.variant, "files": observed}, indent=2))


if __name__ == "__main__":
    main()
