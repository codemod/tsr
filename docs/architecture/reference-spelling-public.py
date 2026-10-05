#!/usr/bin/env python3
"""Complete ordinary CLI diagnostics for the reference contract's fixture units."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("spelling", Path(__file__).with_name("reference-spelling-controls.py"))
spelling = importlib.util.module_from_spec(spec)
spec.loader.exec_module(spelling)
cost, require = spelling.cost, spelling.require


def run(normal, native, oracle_path, directory):
    oracle = json.loads(oracle_path.read_text())
    require(oracle["complete"], "incomplete display oracle")
    # This is the original unmodified binary from the lifecycle-producer task,
    # built before its observation patch. The probe is a different executable.
    require(cost.file_hash(native) == "b3cd1909b5dbc6582681e0a9d2641c7eefd9b7b6c3401cfad82b5e8b935b8033", "unverified native CLI artifact")
    directory.mkdir(parents=True, exist_ok=False)
    result = {"schema": 1, "kind": "reference-public-diagnostics", "source": oracle["builds"]["rust"]["source"],
              "native_sha": oracle["builds"]["native"]["native_sha"], "driver_sha256": cost.file_hash(Path(__file__)),
              "native_cli_kind": "original unmodified pinned-native CLI, archived before native-worker-activity.patch",
              "binary_sha256": {"normal": cost.file_hash(normal), "native": cost.file_hash(native)},
              "cases": [], "complete": False, "speed_claim": False}
    output = directory / "results.json"
    cost.write(output, result)
    for case in oracle["cases"]:
        root = directory / case["family"]
        root.mkdir()
        units = {unit["Name"]: unit["Text"] for unit in case["native"]["baseline"]["units"]}
        for name, text in units.items():
            path = root / name
            require(path.resolve().is_relative_to(root.resolve()), "fixture escapes root")
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        cost.write(root / "tsconfig.json", dict(compilerOptions=dict(strict=True, skipLibCheck=True, target="esnext"), files=list(units)))
        record = {"family": case["family"], "units_sha256": {name: cost.file_hash(root / name) for name in units}, "children": []}
        result["cases"].append(record)
        cost.write(output, result)
        for mode, extra in (("default", []), ("single", ["--singleThreaded"])):
            for compiler, binary in (("normal", normal), ("native", native)):
                for key in list(os.environ):
                    if key.startswith("TSR_"):
                        os.environ.pop(key)
                child = cost.process([str(binary), "--project", "tsconfig.json", "--noEmit", "--incremental", "false",
                                      "--composite", "false", "--pretty", "false", "--listFiles", "--extendedDiagnostics", *extra], root, 90)
                child.update(mode=mode, compiler=compiler, diagnostics=cost.diagnostics(child["stdout"] + child["stderr"], root),
                             counts=cost.counts(child), loaded_files=cost.files(child))
                record["children"].append(child)
                cost.write(output, result)
                require(child["exit_code"] in (0, 1) and not child["timed_out"], "incomplete public CLI child")
                require(child["loaded_files"], "missing public input listing")
        for compiler in ("normal", "native"):
            children = [child for child in record["children"] if child["compiler"] == compiler]
            require(children[0]["diagnostics"] == children[1]["diagnostics"] and children[0]["loaded_files"] == children[1]["loaded_files"],
                    "worker mode changes public output/input scope")
        normal_diag, native_diag = record["children"][0]["diagnostics"], record["children"][1]["diagnostics"]
        require(native_diag["count"] == (1 if case["family"] == "negative" else 0), "native positive/error CLI control ineffective")
        record["complete_native_diagnostics_match"] = normal_diag == native_diag
        cost.write(output, result)
        print(json.dumps(dict(family=case["family"], normal_diagnostics=normal_diag["count"], native_diagnostics=native_diag["count"],
                              match=record["complete_native_diagnostics_match"])), flush=True)
    result["complete"] = True
    result["all_complete_native_diagnostics_match"] = all(case["complete_native_diagnostics_match"] for case in result["cases"])
    cost.write(output, result)


def main():
    parser = argparse.ArgumentParser()
    for name in ("normal", "native", "oracle", "directory"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    run(*(getattr(args, name).resolve() for name in ("normal", "native", "oracle", "directory")))


if __name__ == "__main__":
    main()
