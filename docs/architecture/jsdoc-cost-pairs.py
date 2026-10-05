#!/usr/bin/env python3
"""Reproduce serial fresh-process normal/off/on JSDoc cost observations."""
import argparse
import importlib.util
from pathlib import Path
import sys

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("cost", Path(__file__).with_name("jsdoc-cost-controls.py"))
cost = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cost)


def run(args):
    args.output.mkdir(parents=True, exist_ok=False)
    normal = cost.measure(args.normal, args.project, args.output / "normal.json", repeats=3)
    expected = normal["children"][0]
    pairs = []
    for index in range(args.repeats):
        pair = {"index": index, "modes": {}}
        for mode in (("off", "on") if index % 2 == 0 else ("on", "off")):
            answer = cost.measure(args.probe, args.project, args.output / f"{index}-{mode}.json",
                                  probe=mode == "on", repeats=1)
            child = answer["children"][0]
            cost.require(child["diagnostics"] == expected["diagnostics"], "observer changes diagnostics")
            cost.require(child["counts"] == expected["counts"] and
                         child["exit_code"] == expected["exit_code"], "observer changes reported scope/exit")
            cost.require(answer["config_fingerprint"] == normal["config_fingerprint"], "config changes")
            # Archive paths differ; compare every ordered physical payload, including libraries.
            cost.require(answer["ordered_physical_source_identity"] == normal["ordered_physical_source_identity"],
                         "loaded physical source/order changes")
            pair["modes"][mode] = answer
        pairs.append(pair)
        cost.write(args.output / "pairs.json", pairs)
    print({"pairs": len(pairs), "normal_wall_seconds": normal["summary"]["wall_seconds"]})


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ("normal", "probe", "project", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=3)
    args = parser.parse_args()
    cost.require(args.repeats > 0, "repeat count must be positive")
    run(args)
