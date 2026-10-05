#!/usr/bin/env python3
"""Supervise archived private-owner memory observations; never infer an RSS bound."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import statistics
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from benchmark_inputs import file_hash, snapshot, valid_snapshot
from whole_project_perf import diagnostics, fingerprint, process


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def write(path, result):
    path.write_text(json.dumps(result, indent=2) + "\n")
    require(json.loads(path.read_text()) == result, "persisted evidence differs")


def records(child, prefix):
    return [line.split("\t")[1:] for line in child["stderr"].splitlines() if line.startswith(prefix + "\t")]


def read_memory(child, workers):
    require(not child["timed_out"] and child["exit_code"] in (0, 1, 2), "incomplete memory child")
    headers = records(child, "MEMORY_HEADER")
    require(len(headers) == 1 and len(headers[0]) == 2 and int(headers[0][0]) == child["pid"],
            "memory child PID mismatch")
    require(int(headers[0][1]) >= child["started_at_unix_ns"], "stale memory child")
    names = ("live_requested_bytes", "peak_requested_bytes", "allocation_requests", "live_padded_layout_bytes")
    totals, owners = records(child, "MEMORY_TOTAL"), records(child, "MEMORY_OWNER")
    require(len(totals) == 3 and len(owners) == 3 * workers, "incomplete memory boundaries")
    result = {}
    for phase in ("initialized", "checked", "released"):
        rows = [row for row in totals if row[0] == phase]
        require(len(rows) == 1 and len(rows[0]) == 5, "invalid total boundary")
        total = dict(zip(names, map(int, rows[0][1:])))
        entries = {}
        for row in owners:
            require(len(row) == 6, "invalid owner boundary")
            if row[0] == phase:
                owner = int(row[1])
                require(owner not in entries and 1 <= owner <= workers, "duplicate or invalid private owner")
                entries[owner] = dict(zip(names, map(int, row[2:])))
        require(len(entries) == workers, "missing private owner")
        for item in [total, *entries.values()]:
            require(all(value >= 0 for value in item.values()), "negative allocation accounting")
            require(item["peak_requested_bytes"] >= item["live_requested_bytes"], "peak below live bytes")
            require(item["live_padded_layout_bytes"] >= item["live_requested_bytes"], "padding below payload")
        for key in ("live_requested_bytes", "allocation_requests", "live_padded_layout_bytes"):
            require(sum(item[key] for item in entries.values()) == total[key], "quiescent owner sum mismatch")
        result[phase] = {"total": total, "owners": {str(owner): value for owner, value in entries.items()}}
    return result


def measure(binary, project, output, workers=1, repeats=3, probe=False, budget=None):
    require(repeats > 0 and workers > 0, "positive repeats/workers required")
    require(budget is None or (probe and budget >= 0), "budget requires memory observer and nonnegative bytes")
    output.parent.mkdir(parents=True, exist_ok=True)
    for key in ("TSR_WORK_TRACE", "TSR_NATIVE_WORK_ACTIVITY", "TSR_JSDOC_COST_PROBE",
                "TSR_CHECKER_MEMORY_PROBE", "TSR_CHECKER_MEMORY_BUDGET"):
        os.environ.pop(key, None)
    command = [str(binary.resolve()), str(project.resolve() / "tsconfig.json"), str(workers)]
    preflight = process(command, project, 180)
    require(preflight["exit_code"] in (0, 1) and not preflight["timed_out"], "scope preflight failed")
    loaded = [row[0] for row in records(preflight, "LOADED")]
    require(loaded, "missing loaded identities")
    inputs = loaded + [str(project.resolve() / "tsconfig.json")]
    before = snapshot(inputs)
    require(valid_snapshot(before) and all(row["kind"] == "file" for row in before), "unreadable observed inputs")
    expected_diagnostics = diagnostics(preflight["stdout"], project)
    expected_checked = [row[0] for row in records(preflight, "CHECKED")]
    expected_policy = records(preflight, "POLICY")
    require(len(expected_policy) == 1 and len(expected_policy[0]) == 3, "invalid worker policy")
    selected = int(expected_policy[0][1])
    binary_hash = file_hash(binary)
    result = {"command": command, "binary_sha256": binary_hash, "inputs_before": before,
              "selected_workers": selected, "preflight": preflight, "children": []}
    for _ in range(repeats):
        if probe:
            os.environ["TSR_CHECKER_MEMORY_PROBE"] = "1"
        if budget is not None:
            os.environ["TSR_CHECKER_MEMORY_BUDGET"] = str(budget)
        child = process(command, project, 180)
        child["diagnostics"] = diagnostics(child["stdout"], project)
        child["checked"] = [row[0] for row in records(child, "CHECKED")]
        require(not child["timed_out"] and child["exit_code"] in (0, 1, 2), "memory process failed")
        require(records(child, "POLICY") == expected_policy, "worker selection changes")
        if budget is None:
            require(child["exit_code"] == preflight["exit_code"], "observer exit changes")
            require(child["diagnostics"] == expected_diagnostics, "complete diagnostics change")
            require(child["checked"] == expected_checked, "actual checking identities change")
            require([row[0] for row in records(child, "LOADED")] == loaded, "loaded order changes")
        else:
            require(child["exit_code"] == 2 and records(child, "MEMORY_BUDGET_EXCEEDED"),
                    "exceeded logical budget did not cause controlled failure")
        if probe:
            child["memory"] = read_memory(child, selected)
        result["children"].append(child)
        write(output, result)
    for key in ("TSR_CHECKER_MEMORY_PROBE", "TSR_CHECKER_MEMORY_BUDGET"):
        os.environ.pop(key, None)
    result["inputs_after"] = snapshot(inputs)
    require(result["inputs_after"] == before, "observed inputs change")
    require(file_hash(binary) == binary_hash, "binary changes")
    by_path = {row["path"]: row for row in before}
    result["ordered_physical_sources"] = [[by_path[name]["realpath"], by_path[name]["sha256"]] for name in loaded]
    result["summary"] = {key: statistics.median(child[key] for child in result["children"])
                         for key in ("wall_seconds", "user_seconds", "system_seconds", "peak_rss_bytes")}
    result["summary"].update(complete_process=budget is None, outputs_preserved=budget is None)
    result["diagnostics_fingerprint"] = expected_diagnostics["fingerprint"]
    result["checked_fingerprint"] = fingerprint(expected_checked)
    write(output, result)
    return result


def fixtures(directory, width=16, files=4):
    directory.mkdir(parents=True, exist_ok=False)
    sources = {
        "tsconfig.json": json.dumps({"compilerOptions": {"strict": True, "target": "es2022",
                                                         "skipLibCheck": True},
                                      "include": ["*.ts"]}, indent=2) + "\n",
        "types.ts": "export type Image<T> = {[K in keyof T]: T[K]};\n"
                    "export type Cycle<T> = { value: T; next?: Cycle<T> };\n"
                    "export type Unbox<T> = T extends {value: infer U} ? U : never;\n",
        "negative.ts": "import type {Cycle} from './types';\n"
                       "export const bad: Cycle<number> = {value: 'wrong'};\n",
    }
    for index in range(files):
        fields = "; ".join(f"p{column}: number" for column in range(width * (8 if index == 0 else 1)))
        sources[f"file{index}.ts"] = ("import type {Image, Cycle, Unbox} from './types';\n"
                                      f"export type Raw{index} = {{{fields}}};\n"
                                      f"export type Mapped{index} = Image<Raw{index}>;\n"
                                      f"export const value{index}: Unbox<Cycle<number>> = {index};\n")
    for name, text in sources.items():
        (directory / name).write_text(text)
    manifest = {name: file_hash(directory / name) for name in sources}
    write(directory / "fixture-manifest.json", manifest)
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="action", required=True)
    generate = sub.add_parser("fixtures")
    generate.add_argument("directory", type=Path)
    generate.add_argument("--width", type=int, default=16)
    generate.add_argument("--files", type=int, default=4)
    target = sub.add_parser("measure")
    for name in ("binary", "project", "output"):
        target.add_argument("--" + name, type=Path, required=True)
    target.add_argument("--workers", type=int, default=1)
    target.add_argument("--repeats", type=int, default=3)
    target.add_argument("--probe", action="store_true")
    target.add_argument("--budget", type=int)
    args = parser.parse_args()
    if args.action == "fixtures":
        require(args.width > 0 and args.files > 0, "positive fixture dimensions required")
        print(json.dumps(fixtures(args.directory, args.width, args.files)))
    else:
        require(args.binary and args.project and args.output, "binary/project/output required")
        print(json.dumps(measure(args.binary, args.project, args.output, args.workers,
                                 args.repeats, args.probe, args.budget)["summary"]))
