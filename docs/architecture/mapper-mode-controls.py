#!/usr/bin/env python3
"""Qualify supported captured/shadowed generic diagnostics for a cache-mode repair."""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("cost", HERE / "jsdoc-cost-controls.py")
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)
NATIVE_SHA = "b3cd1909b5dbc6582681e0a9d2641c7eefd9b7b6c3401cfad82b5e8b935b8033"
NEGATIVES = [
    "const badOuter: number = result.outer;",
    "const badOwn: string = result.own;",
    "const badOther: string = result.other;",
    "const badCapture: boolean = captured.value;",
]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bindings", type=Path, required=True)
    parser.add_argument("--native-binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    bindings = json.loads(args.bindings.read_text())
    binaries = {role: Path(bindings[role]["path"]).resolve() for role in ("baseline", "candidate")}
    binaries["native"] = args.native_binary.resolve()
    for role in ("baseline", "candidate"):
        cost.require(bindings[role]["source"] and bindings[role]["sha256"] == cost.file_hash(binaries[role]),
                     "Rust build binding mismatch")
    cost.require(cost.file_hash(binaries["native"]) == NATIVE_SHA, "unqualified native CLI")
    for key in list(os.environ):
        if key.startswith("TSR_") or key in ("RAYON_NUM_THREADS", "GOMAXPROCS"):
            os.environ.pop(key)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    fixture = HERE / "mapper-mode.ts"
    report = {
        "bindings": bindings,
        "native_source": "5b1047d10d32e7d5b446be4de56b126ff42f82bb",
        "binary_sha256": {role: cost.file_hash(path) for role, path in binaries.items()},
        "fixture_sha256": cost.file_hash(fixture), "driver_sha256": cost.file_hash(Path(__file__)),
        "cases": [], "complete": False, "speed_claim": False,
        "private_cache_mode_execution_verified": False,
        "actual_checked_work_verified": False,
        "input_coverage": "fixture/config/compiler bytes; physical standard libraries and unobserved queries excluded",
    }

    def save() -> None:
        cost.write(output / "results.json", report)

    save()
    for variant in ("positive", "negative"):
        project = output / variant
        project.mkdir()
        text = fixture.read_text() + ("\n" + "\n".join(NEGATIVES) + "\n" if variant == "negative" else "")
        (project / "main.ts").write_text(text)
        cost.write(project / "tsconfig.json", {"compilerOptions": {
            "strict": True, "skipLibCheck": True, "target": "ES2022", "noEmit": True,
        }, "files": ["main.ts"]})
        paths = [str(project / name) for name in ("main.ts", "tsconfig.json")]
        paths += [str(path) for path in binaries.values()]
        paths += [str(fixture), str(Path(__file__)), str(args.bindings.resolve())]
        observed = cost.snapshot(paths)
        case = {"variant": variant, "inputs": observed, "children": []}
        report["cases"].append(case)
        save()
        for mode, extra in (("default", []), ("single", ["--singleThreaded"])):
            for role, binary in binaries.items():
                command = [str(binary), "--project", str(project / "tsconfig.json"),
                           "--pretty", "false", "--noEmit", "--incremental", "false",
                           "--composite", "false", "--listFiles", *extra]
                child = cost.process(command, project, 60)
                raw = {}
                for stream in ("stdout", "stderr"):
                    path = project / (role + "-" + mode + "." + stream)
                    path.write_text(child[stream])
                    raw[stream] = {"path": str(path), "sha256": cost.file_hash(path)}
                row = {key: value for key, value in child.items() if key not in ("stdout", "stderr")}
                row.update(role=role, mode=mode, raw=raw, loaded=cost.files(child),
                           diagnostics=cost.diagnostics(child["stdout"] + child["stderr"], project))
                case["children"].append(row)
                save()
                cost.require(not child["timed_out"] and not child["stderr"], "incomplete compiler output")
                cost.require(child["exit_code"] in (0, 1, 2), "compiler failed")
                cost.require(cost.snapshot(paths) == observed, "observed input changed")
                codes = re.findall(r"error TS(\d+):", child["stdout"])
                cost.require(codes == (["2322"] * 4 if variant == "negative" else []),
                             "unexpected public diagnostics")
        first = case["children"][0]
        for row in case["children"]:
            cost.require(row["diagnostics"] == first["diagnostics"]
                         and row["exit_code"] == first["exit_code"], "full diagnostic observation differs")
        for role in binaries:
            rows = [row for row in case["children"] if row["role"] == role]
            cost.require(rows[0]["loaded"] == rows[1]["loaded"], "worker mode changes loaded order")
        for mode in ("default", "single"):
            rows = [row for row in case["children"] if row["mode"] == mode and row["role"] != "native"]
            cost.require(rows[0]["loaded"] == rows[1]["loaded"], "candidate changes loaded order")
        case["complete_diagnostics_match"] = True
        save()
        print(variant, "complete diagnostic parity", flush=True)
    report["complete"] = True
    save()


if __name__ == "__main__":
    main()
