#!/usr/bin/env python3
"""Archive-only failures proving protection of required reference spelling."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("spelling", Path(__file__).with_name("reference-spelling-controls.py"))
spelling = importlib.util.module_from_spec(spec)
spec.loader.exec_module(spelling)
cost = spelling.cost
require = spelling.require

BARE = "            self.qualified_written_text.entry(id).or_insert(text);"
COMPOSED = "                self.qualified_written_text.insert(id, composed);"
MUTATIONS = {
    "drop-reference-writes": [(BARE, "            let _ = (id, text);"),
                              (COMPOSED, "                let _ = (id, composed);")],
    "expand-written-bare": [(BARE, "            self.qualified_written_text.entry(id).or_insert(format!(\"{text}<number>\"));")],
}


def apply_mutation(text, replacements):
    for before, after in replacements:
        require(text.count(before) == 1, "mutation source anchor is not unique")
        text = text.replace(before, after)
    return text


def run(source, target, oracle_path, directory):
    for key in list(os.environ):
        if key.startswith("TSR_"):
            os.environ.pop(key)
    oracle = json.loads(oracle_path.read_text())
    require(oracle["complete"] and all(case["complete"] for case in oracle["cases"]), "incomplete baseline oracle")
    build = oracle["builds"]["rust"]
    require(source != Path(__file__).resolve().parents[2], "canonical source mutation prohibited")
    require(source == Path(build["source_root"]).resolve() and target == Path(build["target"]).resolve(), "wrong archive source/target")
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    require(head == build["source"], "source SHA differs from baseline")
    require(not subprocess.check_output(["git", "diff", "--name-only", "HEAD", "--", ":!vendor/typescript-go"],
                                       cwd=source, text=True), "archive source has tracked edits")
    native = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source / "vendor/typescript-go", text=True).strip()
    require(native == oracle["builds"]["native"]["native_sha"], "archive native substrate differs")
    helper = source / "crates/tsr-conformance/examples/reference_spelling_contract.rs"
    require(cost.file_hash(helper) == build["helper_sha256"], "archive helper differs")
    binary = target / "release/examples/reference_spelling_contract"
    require(cost.file_hash(binary) == build["binary_sha256"], "baseline helper binary differs")
    directory.mkdir(parents=True, exist_ok=False)
    original = source / "crates/tsr-checker/src/declared.rs"
    contents = original.read_bytes()
    (directory / "declared.rs.original").write_bytes(contents)
    result = {"schema": 1, "source": head, "source_root": str(source), "target": str(target),
              "oracle": {"path": str(oracle_path), "sha256": cost.file_hash(oracle_path)},
              "driver_sha256": cost.file_hash(Path(__file__)), "original_sha256": cost.file_hash(original),
              "mutations": [], "complete": False, "source_restored": False, "speed_claim": False}
    output = directory / "results.json"
    cost.write(output, result)
    try:
        for label, replacements in MUTATIONS.items():
            mutated = apply_mutation(contents.decode(), replacements)
            original.write_text(mutated)
            record = {"label": label, "source_sha256": cost.file_hash(original), "replacements": [list(pair) for pair in replacements],
                      "build": None, "cases": [], "detected_new_native_mismatches": [], "complete": False}
            result["mutations"].append(record)
            cost.write(output, result)
            env = {key: value for key, value in os.environ.items()
                   if not key.startswith(("TSR_", "CARGO_PROFILE_")) and key != "RUSTFLAGS"}
            env["CARGO_TARGET_DIR"] = str(target)
            old_env = dict(os.environ)
            try:
                os.environ.clear()
                os.environ.update(env)
                child = cost.process(["cargo", "build", "--release", "--offline", "-p", "tsr-conformance",
                                      "--example", "reference_spelling_contract", "-j", "2"], source, 300)
            finally:
                os.environ.clear()
                os.environ.update(old_env)
            record["build"] = child
            cost.write(output, result)
            require(child["exit_code"] == 0 and not child["timed_out"], "mutation build incomplete")
            record["binary_sha256"] = cost.file_hash(binary)
            for baseline in oracle["cases"]:
                child = cost.process([str(binary), baseline["fixture"]], source, 90)
                case = {"family": baseline["family"], "child": child}
                record["cases"].append(case)
                cost.write(output, result)
                require(child["exit_code"] == 0 and not child["timed_out"], "mutation helper incomplete")
                units, rows = spelling.rust_maps(child["stdout"], baseline["names"])
                require(units == baseline["rust"]["units"], "mutation input bytes differ")
                prior = {(row["file"], row["name"]): row["type"] for row in baseline["rust"]["types"]}
                native = spelling.native_maps(baseline["native"]["baseline"], "baseline", baseline["names"])
                case["changed_displays"] = [{"file": key[0], "name": key[1], "before": prior[key], "after": value,
                                             "native": native[key]} for key, value in rows.items() if prior[key] != value]
                for delta in case["changed_displays"]:
                    if delta["before"] == delta["native"] and delta["after"] != delta["native"]:
                        record["detected_new_native_mismatches"].append(dict(family=case["family"], **delta))
                cost.write(output, result)
            names = {row["name"] for row in record["detected_new_native_mismatches"]}
            require("probe_fnBare" in names and "probe_fnNested" in names, "written bare/nested failure control ineffective")
            if label == "drop-reference-writes":
                require("probe_fnPartial" in names and "probe_fnChain" in names, "partial/dependent failure control ineffective")
            record["complete"] = True
            original.write_bytes(contents)
            require(cost.file_hash(original) == result["original_sha256"], "archive restoration differs")
            cost.write(output, result)
            print(json.dumps({"mutation": label, "new_native_mismatches": len(record["detected_new_native_mismatches"])}), flush=True)
    finally:
        original.write_bytes(contents)
        result["source_restored"] = cost.file_hash(original) == result["original_sha256"]
        cost.write(output, result)
        # Rebuild the restored source instead of copying an executable over
        # Cargo's hardlinked artifacts and leaving its cache inconsistent.
        prior_env = dict(os.environ)
        try:
            for key in list(os.environ):
                if key.startswith(("TSR_", "CARGO_PROFILE_")) or key == "RUSTFLAGS":
                    os.environ.pop(key)
            os.environ["CARGO_TARGET_DIR"] = str(target)
            result["restoration_build"] = cost.process(["cargo", "build", "--release", "--offline", "-p",
                "tsr-conformance", "--example", "reference_spelling_contract", "-j", "2"], source, 300)
        finally:
            os.environ.clear()
            os.environ.update(prior_env)
        cost.write(output, result)
        restored = result["restoration_build"]
        require(restored["exit_code"] == 0 and not restored["timed_out"], "restoration build incomplete")
        result["restored_binary_sha256"] = cost.file_hash(binary)
        require(result["restored_binary_sha256"] == build["binary_sha256"], "restored binary differs from baseline")
        cost.write(output, result)
    result["complete"] = True
    cost.write(output, result)


def main():
    parser = argparse.ArgumentParser()
    for name in ("source", "target", "oracle", "directory"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    run(*(getattr(args, name).resolve() for name in ("source", "target", "oracle", "directory")))


if __name__ == "__main__":
    main()
