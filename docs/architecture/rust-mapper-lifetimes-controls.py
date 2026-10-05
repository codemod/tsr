#!/usr/bin/env python3
"""Compare supported mapper observations; keep private lifetime proof separate."""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import sys

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from benchmark_inputs import file_hash, snapshot
from whole_project_perf import process

spec = importlib.util.spec_from_file_location("cost", HERE / "jsdoc-cost-controls.py")
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)
NATIVE_SHA = "b3cd1909b5dbc6582681e0a9d2641c7eefd9b7b6c3401cfad82b5e8b935b8033"
FAMILIES = ["ordered", "composition", "nominal", "shadow", "inference", "receiver", "mapped", "conditional"]
NEGATIVES = [
    ["const badOrdered: number = ordered.first;", "const badReordered: string = reordered.first;"],
    ["const badComposition: number = nested.box.value;"],
    ["const badNominal: Left.Token = right;"],
    ["const badShadow: string = shadow.inner(1);"],
    ["const badInference: number = inferredString;"],
    ["const badReceiver: string = new Derived().next().own;"],
    ["const badMapped: number = deepString.next.next.value;"],
    ["const badConditional: number = flattened;"],
]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust-binary", type=Path, required=True)
    parser.add_argument("--native-binary", type=Path, required=True)
    parser.add_argument("--rust-source", required=True)
    parser.add_argument("--rust-build-identity", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binaries = {"rust": args.rust_binary.resolve(), "native": args.native_binary.resolve()}
    cost.require(file_hash(binaries["native"]) == NATIVE_SHA, "unqualified native CLI")
    binding = json.loads(args.rust_build_identity.read_text())
    cost.require(binding["source"] == args.rust_source, "Rust source binding mismatch")
    cost.require(binding["sha256"] == file_hash(binaries["rust"]), "Rust binary binding mismatch")
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    for key in list(os.environ):
        if key.startswith("TSR_") or key in ("RAYON_NUM_THREADS", "GOMAXPROCS"):
            os.environ.pop(key)
    fixture = HERE / "native-mapper-identity.ts"
    groups = fixture.read_text().strip().split("\n\n")
    cost.require(len(groups) == 9 and "NEGATIVE CONTROLS" in groups[-1], "fixture family layout changed")
    report = {
        "rust_source": args.rust_source,
        "native_source": "5b1047d10d32e7d5b446be4de56b126ff42f82bb",
        "binary_sha256": {key: file_hash(path) for key, path in binaries.items()},
        "driver_sha256": file_hash(Path(__file__)), "fixture_sha256": file_hash(fixture),
        "rust_build_identity_sha256": file_hash(args.rust_build_identity),
        "cases": [], "complete": False, "speed_claim": False,
        "actual_checked_work_verified": False, "private_mapper_executions_measured": False,
        "complete_input_equivalence_verified": False,
        "input_coverage": "fixture/config/compiler bytes; physical standard-library bytes are not snapshotted",
    }

    def save() -> None:
        cost.write(args.output / "results.json", report)

    save()
    for family, source, negatives in zip(FAMILIES, groups, NEGATIVES):
        for variant in ("positive", "negative"):
            project = args.output / (family + "-" + variant)
            project.mkdir()
            text = source + "\n" + ("\n".join(negatives) + "\n" if variant == "negative" else "")
            (project / "main.ts").write_text(text)
            cost.write(project / "tsconfig.json", {"compilerOptions": {
                "strict": True, "noEmit": True, "skipLibCheck": True, "target": "ES2022",
            }, "files": ["main.ts"]})
            inputs = [str(project / name) for name in ("main.ts", "tsconfig.json")]
            inputs += [str(path) for path in binaries.values()] + [str(fixture), str(Path(__file__)),
                      str(args.rust_build_identity.resolve())]
            observed = snapshot(inputs)
            case = {"family": family, "variant": variant, "input_snapshot": observed, "children": []}
            report["cases"].append(case)
            save()
            for mode, extra in (("default", []), ("single", ["--singleThreaded"])):
                for tool, binary in binaries.items():
                    command = [str(binary), "--project", str(project / "tsconfig.json"),
                               "--pretty", "false", "--noEmit", "--incremental", "false",
                               "--composite", "false", "--listFiles", "--extendedDiagnostics", *extra]
                    child = process(command, project, 60)
                    raw = {}
                    for stream in ("stdout", "stderr"):
                        path = project / (tool + "-" + mode + "." + stream)
                        path.write_text(child[stream])
                        raw[stream] = {"path": str(path), "sha256": file_hash(path)}
                    row = {key: value for key, value in child.items() if key not in ("stdout", "stderr")}
                    row.update(tool=tool, mode=mode, raw=raw,
                               diagnostics=cost.diagnostics(child["stdout"] + child["stderr"], project),
                               reported_counts=cost.counts(child), loaded=cost.files(child))
                    case["children"].append(row)
                    save()
                    cost.require(not child["timed_out"] and child["exit_code"] in (0, 1, 2), "incomplete compiler")
                    cost.require(not child["stderr"], "unexpected compiler stderr")
                    cost.require(snapshot(inputs) == observed, "observed input changed")
                    if tool == "native":
                        import re
                        codes = re.findall(r"error TS(\d+):", child["stdout"])
                        cost.require(codes == (["2322"] * len(negatives) if variant == "negative" else []),
                                     "unexpected native fixture diagnostics")
            for tool in binaries:
                rows = [row for row in case["children"] if row["tool"] == tool]
                cost.require(rows[0]["diagnostics"] == rows[1]["diagnostics"]
                             and rows[0]["loaded"] == rows[1]["loaded"]
                             and rows[0]["exit_code"] == rows[1]["exit_code"], "mode changes observation")
            rust = next(row for row in case["children"] if row["tool"] == "rust")
            native = next(row for row in case["children"] if row["tool"] == "native")
            case["diagnostics_match"] = rust["diagnostics"] == native["diagnostics"] and rust["exit_code"] == native["exit_code"]
            save()
            print(family, variant, "native diagnostic parity", case["diagnostics_match"], flush=True)
    report["complete"] = True
    save()


if __name__ == "__main__":
    main()
