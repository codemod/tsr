#!/usr/bin/env python3
"""Validate source-qualified graph observations; counts are not saved wall time."""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from benchmark_inputs import file_hash, snapshot, valid_snapshot
from whole_project_perf import file_identity, fingerprint, process

SPEC = importlib.util.spec_from_file_location(
    "graph_pool", Path(__file__).with_name("checker-mapper-pool-controls.py"))
pool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pool)
RUST_SOURCE = "1b8821a498114ba60bf841bd17875ac181f6f116"
NAMES = (
    "roots", "explicit_roots", "registry_roots", "true_roots", "false_roots",
    "visits", "edges", "unique_visits", "duplicate_visits", "explicit_parameter_tests",
    "registry_parameter_tests", "explicit_parameter_comparisons", "parameter_hits",
    "visited_tests", "visited_comparisons", "visited_hits", "markers", "terminal_leaves",
    "fallbacks", "fallback_payload_bytes", "empty_names_fallbacks", "scratch_allocations",
    "scratch_requested_bytes", "scratch_usable_bytes", "printing_allocations",
    "printing_requested_bytes", "printing_usable_bytes", "root_capacity_sum", "max_visited",
    "max_capacity", "roots_visited_0", "roots_visited_1_4", "roots_visited_5_16",
    "roots_visited_17_64", "roots_visited_65_256", "roots_visited_over_256",
)


def validate_counts(counts):
    pool.require(set(counts) == set(NAMES), "incomplete/unknown graph counters")
    pool.require(all(type(value) is int and value >= 0 for value in counts.values()), "bad graph count")
    c = counts
    pool.require(c["roots"] == c["explicit_roots"] + c["registry_roots"], "root mode partition")
    pool.require(c["roots"] == c["true_roots"] + c["false_roots"], "root answer partition")
    pool.require(c["visits"] == c["roots"] + c["edges"], "recursive edge reconciliation")
    pool.require(c["visits"] == c["unique_visits"] + c["duplicate_visits"], "visited identity partition")
    pool.require(c["visits"] == c["explicit_parameter_tests"] + c["registry_parameter_tests"],
                 "parameter mode partition")
    pool.require(c["visited_tests"] + c["parameter_hits"] == c["visits"], "parameter precedence")
    pool.require(c["roots"] == sum(c[name] for name in NAMES if name.startswith("roots_visited_")),
                 "root size histogram")
    pool.require(c["max_visited"] <= c["max_capacity"] <= c["root_capacity_sum"], "scratch capacity")
    pool.require(c["max_visited"] <= c["markers"] <= c["visited_tests"], "cycle markers")
    pool.require(c["visited_hits"] <= c["duplicate_visits"], "duplicate cycle membership")
    pool.require(c["visited_hits"] <= c["visited_comparisons"] <=
                 c["visited_tests"] * c["max_visited"], "visited comparison bounds")
    pool.require(c["empty_names_fallbacks"] <= c["fallbacks"], "empty fallback partition")
    pool.require(c["parameter_hits"] + c["visited_hits"] + c["terminal_leaves"] +
                 c["fallbacks"] <= c["visits"], "exclusive terminal branches")
    for site in ("scratch", "printing"):
        pool.require(c[site + "_allocations"] <= c[site + "_requested_bytes"] <=
                     c[site + "_usable_bytes"], "allocation bounds")


def count_rows(rows):
    result = {}
    for row in rows:
        pool.require(len(row) == 3 and row[0] == "count" and row[1] not in result,
                     "duplicate/malformed counter")
        pool.require(re.fullmatch(r"[0-9]+", row[2]) is not None, "noninteger counter")
        result[row[1]] = int(row[2])
    validate_counts(result)
    return result


def inspect_trace(path, child, counters, cwd):
    pool.require(not child["timed_out"] and child["exit_code"] in (0, 1, 2), "incomplete child")
    rows = pool.lines(path)
    pool.require(rows[0] == ["schema", "graph-walk-v1"] and rows[-1] == ["joined"], "unfinished pool")
    header = rows[1]
    pool.require(len(header) == 4 and header[0] == "process", "process header")
    pid, count, enabled = map(int, header[1:])
    pool.require(pid == child["pid"] and enabled == int(counters) and 1 <= count <= 256,
                 "process/mode binding")
    observed = count_rows(rows[2:-1]) if counters else {}
    pool.require(counters or not rows[2:-1], "disabled counters")
    owners, checked = [], []
    for owner in range(count):
        records = pool.lines(path.with_suffix(f".owner-{owner}.tsv"))
        pool.require(records[0] == ["owner", str(pid), str(owner), str(count), str(enabled)] and
                     records[-1] == ["complete"], "owner binding/completion")
        local_checks, local_counts, query = [], [], None
        for row in records[1:-1]:
            if row[0] == "check":
                pool.require(len(row) == 3 and int(row[1]) >= 0 and int(row[1]) % count == owner,
                             "file affinity")
                local_checks.append({"index": int(row[1]), "file": file_identity(row[2], cwd)})
            elif row[0] == "count":
                local_counts.append(row)
            else:
                pool.require(counters and row[0] == "query_shapes" and len(row) == 4 and query is None,
                             "unknown/duplicate owner row")
                query = dict(zip(("distinct_request_shapes", "observer_query_capacity", "distinct_root_ids"),
                                 map(int, row[1:])))
        pool.require([x["index"] for x in local_checks] == sorted({x["index"] for x in local_checks}),
                     "duplicate/unordered checks")
        local = count_rows(local_counts) if counters else {}
        pool.require(counters or (not local_counts and query is None), "disabled owner counters")
        if counters:
            pool.require(query is not None and 0 <= query["distinct_root_ids"] <=
                         query["distinct_request_shapes"] <= local["roots"] and
                         query["distinct_request_shapes"] <= query["observer_query_capacity"], "query shape bounds")
        owners.append({"owner": owner, "counts": local, "query_shapes": query, "checks": local_checks})
        checked.extend(local_checks)
    pool.require(len({x["index"] for x in checked}) == len(checked), "cross-owner duplicate check")
    if counters:
        for name, value in observed.items():
            local = [x["counts"][name] for x in owners]
            pool.require(value == (max(local) if name.startswith("max_") else sum(local)),
                         "owner/global counter reconciliation")
    return {"counts": observed, "owners": owners, "checks": sorted(checked, key=lambda x: x["index"])}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bindings", type=Path, required=True)
    parser.add_argument("--project", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=2)
    args = parser.parse_args()
    pool.require(args.repeats >= 2, "repeat counter qualification required")
    bindings = json.loads(args.bindings.read_text())
    for role in ("normal", "probe"):
        pool.require(bindings[role]["source"] == RUST_SOURCE and
                     file_hash(Path(bindings[role]["path"])) == bindings[role]["sha256"], "compiler binding")
    args.output.mkdir()
    config = args.project.resolve()
    env = {key: value for key, value in os.environ.items() if not key.startswith(("TSR_", "CARGO_PROFILE_"))
           and key not in ("RUSTFLAGS", "GOMAXPROCS", "RAYON_NUM_THREADS")}
    os.environ.clear()
    os.environ.update(env)
    state = {"source": RUST_SOURCE, "bindings": bindings, "driver_sha256": file_hash(Path(__file__)),
             "kind": "archive-only graph locating; not saved wall or semantic-equivalent cache keys", "cases": []}
    def save():
        pool.write(args.output / "results.json", state)
    save()
    for mode in ("default", "single"):
        directory = args.output / mode
        directory.mkdir()
        flags = ["--project", str(config), "--noEmit", "--incremental", "false", "--composite", "false",
                 "--pretty", "false"] + (["--singleThreaded"] if mode == "single" else [])
        show = process([bindings["normal"]["path"], *flags, "--showConfig"], config.parent, 180)
        pool.require(not show["timed_out"] and show["exit_code"] == 0, "effective configuration failed")
        case = {"mode": mode, "flags": flags, "effective_config": json.loads(show["stdout"]), "runs": []}
        case["config_fingerprint"] = fingerprint(case["effective_config"])
        case["config_sha256"] = file_hash(config)
        state["cases"].append(case)
        expected = checked = previous = None
        for index in range(args.repeats):
            roles = [("normal", "normal", False), ("disabled", "probe", False), ("enabled", "probe", True)]
            roles = roles[index % 3:] + roles[:index % 3]
            for label, role, counters in roles:
                destination = directory / f"{index}-{label}"
                destination.mkdir()
                trace = destination / "trace.tsv"
                if role == "probe": os.environ["TSR_GRAPH_TRACE"] = str(trace)
                if counters: os.environ["TSR_GRAPH_PROFILE"] = "1"
                try:
                    child = process([bindings[role]["path"], *flags, "--listFiles", "--extendedDiagnostics"], config.parent, 180)
                finally:
                    os.environ.pop("TSR_GRAPH_TRACE", None)
                    os.environ.pop("TSR_GRAPH_PROFILE", None)
                for stream in ("stdout", "stderr"): (destination / stream).write_text(child[stream])
                result = {k: v for k, v in child.items() if k not in ("stdout", "stderr")}
                result.update(label=label, role=role, scope=pool.output_scope(child, config.parent),
                              raw_sha256={s: file_hash(destination / s) for s in ("stdout", "stderr")})
                case["runs"].append(result)
                save()
                pool.require(not child["timed_out"] and child["exit_code"] in (0, 1, 2), "incomplete compiler")
                if expected is None:
                    expected = result["scope"]
                    physical = [x for x in expected["loaded"] if not x.startswith("<typescript-lib>/")]
                    case["inputs_before"] = snapshot(physical + [str(config)])
                    pool.require(valid_snapshot(case["inputs_before"]), "unreadable observed inputs")
                pool.require(result["scope"] == expected, "complete output/performed summary changed")
                if role == "probe":
                    result["trace"] = inspect_trace(trace, child, counters, config.parent)
                    current = result["trace"]["checks"]
                    pool.require(len(current) == expected["reported"].get("Checked files"), "reported/direct scope")
                    pool.require(all(x["file"] in expected["loaded"] for x in current), "checked file not loaded")
                    if checked is None: checked = current
                    pool.require(current == checked, "off/on checked identity changed")
                    if counters:
                        payload = {"counts": result["trace"]["counts"], "owners": result["trace"]["owners"]}
                        if previous is None: previous = payload
                        pool.require(payload == previous, "repeated owner counts changed")
                save()
        case["inputs_after"] = snapshot(physical + [str(config)])
        pool.require(case["inputs_after"] == case["inputs_before"], "observed inputs changed")
        case["checked_fingerprint"] = fingerprint(checked)
        case["complete"] = True
        save()
    state["complete"] = True
    save()


if __name__ == "__main__":
    main()
