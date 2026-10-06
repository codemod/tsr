#!/usr/bin/env python3
"""Apply or restore archived hooks in a clean, isolated, exact-pinned checkout."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
ARCHIVE = Path(__file__).resolve().parent
SOURCE = "3e2073fc90c1646b9c4284e9ab1c5739c3bfb99e"
FILES = (
    "crates/tsr-checker/src/declared.rs", "crates/tsr-checker/src/index_signatures.rs",
    "crates/tsr-checker/src/members.rs", "crates/tsr-checker/src/inference.rs",
    "crates/tsr-checker/src/mapped.rs", "crates/tsr-checker/src/lib.rs",
    "crates/tsr-execute/src/checker_pool.rs",
)
MODULES = {
    "crates/tsr-checker/src/mapper_key_probe.rs": "checker-mapper-pool-probe.rs",
    "crates/tsr-checker/src/reference_state_probe.rs": "checker-reference-state-probe.rs",
    "crates/tsr-checker/src/alias_body_state_probe.rs": "checker-alias-body-state-probe.rs",
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def git(source, *args):
    return subprocess.check_output(["git", *args], cwd=source)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")
    require(json.loads(path.read_text()) == value, "receipt write differs")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--restore", action="store_true")
    args = parser.parse_args()
    source, output = args.source.resolve(), args.output.resolve()
    require(source != ROOT and ROOT not in source.parents and source not in ROOT.parents,
            "use a separate isolated checkout")
    require(output != source and source not in output.parents, "receipt must be outside source")
    require(Path(git(source, "rev-parse", "--show-toplevel").decode().strip()).resolve() == source,
            "source must be checkout root")
    require(git(source, "rev-parse", "HEAD").decode().strip() == SOURCE, "source pin differs")
    receipt = output / "replay.json"
    if args.restore:
        row = json.loads(receipt.read_text())
        require(row.get("source_root") == str(source) and row.get("source") == SOURCE
                and row.get("applied") is True and not row.get("restored"), "unqualified restore receipt")
        require(set(row["files"]) == set(FILES) | set(MODULES), "restore file scope differs")
        # Check every owned byte before restoring any; external edits are refused.
        for relative, expected in row["files"].items():
            require(digest((source / relative).read_bytes()) == expected["hooked_sha256"],
                    "hook bytes changed: " + relative)
            if relative in FILES:
                original = git(source, "show", SOURCE + ":" + relative)
                require(digest(original) == expected["original_sha256"], "original bytes differ")
        for relative in FILES:
            (source / relative).write_bytes(git(source, "show", SOURCE + ":" + relative))
        for relative in MODULES:
            (source / relative).unlink()
        for relative in FILES:
            require(digest((source / relative).read_bytes()) == row["files"][relative]["original_sha256"],
                    "restored bytes differ")
        row["restored"] = True
        write(receipt, row)
        return
    require(not output.exists(), "output already exists")
    require(not git(source, "status", "--porcelain"), "source is dirty")
    require(all(not (source / relative).exists() for relative in MODULES), "probe module already exists")
    originals = {relative: (source / relative).read_bytes() for relative in FILES}
    patch = ARCHIVE / "checker-alias-body-state-probe.patch"
    try:
        for relative, archive in MODULES.items():
            shutil.copyfile(ARCHIVE / archive, source / relative)
        subprocess.run(["git", "apply", "--unidiff-zero", "--check", str(patch)], cwd=source, check=True)
        subprocess.run(["git", "apply", "--unidiff-zero", str(patch)], cwd=source, check=True)
        row = {"source": SOURCE, "source_root": str(source), "applied": True, "restored": False,
               "patch_sha256": digest(patch.read_bytes()), "files": {}}
        for relative in (*FILES, *MODULES):
            row["files"][relative] = {
                "original_sha256": digest(originals[relative]) if relative in originals else None,
                "hooked_sha256": digest((source / relative).read_bytes()),
            }
        output.mkdir(parents=True)
        write(receipt, row)
    except BaseException:
        for relative, original in originals.items():
            (source / relative).write_bytes(original)
        for relative in MODULES:
            (source / relative).unlink(missing_ok=True)
        raise


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as exc:
        raise SystemExit(str(exc))
