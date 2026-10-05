#!/usr/bin/env python3
"""Run alternating observer pairs on one immutable archived helper build."""
import argparse
import importlib.util
from pathlib import Path
import sys

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("memory", Path(__file__).with_name("checker-memory-controls.py"))
memory = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(memory)


def run(normal, probe, project, output, repeats):
    memory.require(repeats > 0, "positive repeat count required")
    output.mkdir(parents=True, exist_ok=False)
    results = []
    expected = None
    for workers in (1, 2, 4):
        baseline_path = output / f"{workers}-normal.json"
        baseline = memory.measure(normal, project, baseline_path, workers, repeats=1)
        child = baseline["children"][0]
        signature = (child["diagnostics"], child["checked"], baseline["ordered_physical_sources"])
        if expected is None:
            expected = signature
        memory.require(signature == expected, "normal worker modes change complete scope/output")
        for index in range(repeats):
            pair = {"workers": workers, "index": index, "normal": str(baseline_path), "modes": {}}
            for mode in (("off", "on") if index % 2 == 0 else ("on", "off")):
                result = memory.measure(probe, project, output / f"{workers}-{index}-{mode}.json",
                                        workers, repeats=1, probe=mode == "on")
                child = result["children"][0]
                memory.require((child["diagnostics"], child["checked"], result["ordered_physical_sources"]) == expected,
                               "observer changes complete checking/physical/output scope")
                pair["modes"][mode] = result
            results.append(pair)
            memory.write(output / "pairs.json", results)
            print(f"qualified pair workers={workers} index={index}", flush=True)
    return results


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ("normal", "probe", "project", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=3)
    args = parser.parse_args()
    print({"qualified_pairs": len(run(args.normal, args.probe, args.project, args.output, args.repeats))})
