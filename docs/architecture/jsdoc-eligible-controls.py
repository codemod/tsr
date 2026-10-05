#!/usr/bin/env python3
"""Qualify conservative plain-comment cost; this reader never permits deferral."""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import sys

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("cost", Path(__file__).with_name("jsdoc-cost-controls.py"))
cost = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cost)

NAMES = ["parse_ns", "parse_arena_requested_bytes", "jsdoc_ns", "jsdoc_arena_requested_bytes",
         "eligible_leading_calls", "comments", "comment_range_bytes", "documented_nodes", "table_capacity_bytes",
         "plain_ns", "plain_arena_requested_bytes", "plain_comments", "plain_range_bytes"]


def read_probe(path, child):
    cost.require(not child["timed_out"] and child["exit_code"] in (0, 1), "incomplete eligible child")
    with cost.regular_file(path) as stream:
        data = stream.read()
    cost.require(data.endswith(b"\n"), "partial eligible trace")
    lines = data.decode().splitlines()
    header = lines[0].split("\t") if lines else []
    cost.require(len(header) == 3 and header[:2] == ["tsr-jsdoc-eligible-v1", str(child["pid"])],
                 "eligible trace PID/version mismatch")
    cost.require(int(header[2]) >= child["started_at_unix_ns"], "stale eligible trace")
    rows = []
    for line in lines[1:]:
        values = line.split("\t")
        cost.require(len(values) == 16, "eligible row width mismatch")
        name = bytes.fromhex(values[0]).decode()
        cost.require(name.encode().hex() == values[0], "noncanonical eligible filename")
        cost.require(values[2] in ("TypeScript", "Tsx", "Json"), "unknown eligible dialect")
        numbers = [int(value) for value in values[3:]]
        cost.require(all(value >= 0 for value in numbers) and int(values[1]) >= 0, "negative eligible counters")
        row = {"file": name, "source_bytes": int(values[1]), "parser_kind": values[2], **dict(zip(NAMES, numbers))}
        for small, large in (("plain_ns", "jsdoc_ns"), ("jsdoc_ns", "parse_ns"),
                             ("plain_arena_requested_bytes", "jsdoc_arena_requested_bytes"),
                             ("jsdoc_arena_requested_bytes", "parse_arena_requested_bytes"),
                             ("plain_comments", "comments"), ("plain_range_bytes", "comment_range_bytes")):
            cost.require(row[small] <= row[large], "eligible counters exceed enclosing work")
        cost.require(row["documented_nodes"] <= row["table_capacity_bytes"], "impossible eligible table")
        if cost.category_of(name) in ("js", "json"):
            cost.require(all(row[key] == 0 for key in NAMES[-4:]), "non-TS plain eligibility")
        rows.append(row)
    cost.require(len(rows) == cost.counts(child).get("Parsed files"), "missing eligible parse coverage")
    return {"trace_sha256": hashlib.sha256(data).hexdigest(), "rows": rows,
            "totals": {key: sum(row[key] for row in rows) for key in NAMES},
            "categories": {category: {key: sum(row[key] for row in rows if cost.category_of(row["file"]) == category)
                                      for key in NAMES} for category in ("declaration", "tsx", "js", "ts", "json")}}


def measure(binary, project, output, probe=False, repeats=3):
    original = cost.read_probe
    try:
        cost.read_probe = read_probe
        return cost.measure(binary, project, output, probe=probe, repeats=repeats)
    finally:
        cost.read_probe = original


def pairs(normal, probe, project, output, repeats=3, baseline=None):
    cost.require(repeats > 0, "positive pair count required")
    output.mkdir(parents=True, exist_ok=False)
    if baseline is None:
        baseline = measure(normal, project, output / "normal.json")
    else:
        cost.require(baseline["binary_sha256"] == cost.file_hash(normal), "reused baseline binary changes")
        cost.require(baseline["project"] == str(project.resolve()), "reused baseline project changes")
        cost.require(baseline["loaded_input_snapshot_after"] == cost.snapshot(
            [row["path"] for row in baseline["loaded_input_snapshot_after"]]), "reused baseline inputs change")
        cost.write(output / "normal.json", baseline)
    expected = baseline["children"][0]
    results = []
    for index in range(repeats):
        pair = {"index": index, "modes": {}}
        for mode in (("off", "on") if index % 2 == 0 else ("on", "off")):
            result = measure(probe, project, output / f"{index}-{mode}.json", probe=mode == "on", repeats=1)
            child = result["children"][0]
            cost.require(child["diagnostics"] == expected["diagnostics"], "eligible observer changes complete diagnostics")
            cost.require(child["counts"] == expected["counts"] and child["exit_code"] == expected["exit_code"],
                         "eligible observer changes reported checking/exit")
            cost.require(result["config_fingerprint"] == baseline["config_fingerprint"], "eligible config changes")
            cost.require(result["ordered_physical_source_identity"] == baseline["ordered_physical_source_identity"],
                         "eligible ordered physical payloads change")
            pair["modes"][mode] = result
        results.append(pair)
        cost.write(output / "pairs.json", results)
        print({"pair": index, "plain_ns": pair["modes"]["on"]["children"][0]["probe"]["totals"]["plain_ns"]}, flush=True)
    return results


def existing_control(name, args):
    spec = importlib.util.spec_from_file_location("eligible_control", Path(__file__).with_name(name))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.cost.read_probe = read_probe
    module.run(args)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="action", required=True)
    run = sub.add_parser("pairs")
    for name in ("normal", "probe", "project", "output"):
        run.add_argument("--" + name, type=Path, required=True)
    run.add_argument("--repeats", type=int, default=3)
    single = sub.add_parser("measure")
    for name in ("binary", "project", "output"):
        single.add_argument("--" + name, type=Path, required=True)
    single.add_argument("--probe", action="store_true")
    single.add_argument("--repeats", type=int, default=3)
    work = sub.add_parser("work")
    for name in ("binary", "source", "project", "output"):
        work.add_argument("--" + name, type=Path, required=True)
    work.add_argument("--source-sha", required=True)
    negatives = sub.add_parser("negatives")
    for name in ("normal", "probe", "native", "output"):
        negatives.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    if args.action == "pairs":
        print({"qualified_pairs": len(pairs(args.normal, args.probe, args.project, args.output, args.repeats))})
    elif args.action == "measure":
        print(measure(args.binary, args.project, args.output, args.probe, args.repeats)["summary"])
    elif args.action == "work":
        existing_control("jsdoc-cost-work-controls.py", args)
    else:
        existing_control("jsdoc-cost-negative-controls.py", args)
