#!/usr/bin/env python3
"""Validate archived allocation origins and actual synchronous pool activity."""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import statistics
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("jsdoc_cost", Path(__file__).with_name("jsdoc-cost-controls.py"))
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def numbers(fields):
    values = [int(field) for field in fields]
    require(all(0 <= value < 2**128 and str(value) == field
                for value, field in zip(values, fields)), "invalid unsigned record")
    return values


def peak(intervals):
    events = sorted((time, delta) for begin, end in intervals
                    for time, delta in ((begin, 1), (end, -1)))
    active = maximum = 0
    for _, delta in events:
        active += delta
        require(active >= 0, "invalid activity interval")
        maximum = max(maximum, active)
    require(active == 0, "unfinished activity")
    return maximum


def read_probe(path, child, loaded, expected_workers):
    require(not child["timed_out"] and child["exit_code"] in (0, 1), "incomplete child")
    with cost.regular_file(path) as stream:
        data = stream.read()
    require(data.endswith(b"\n"), "partial final record")
    lines = data.decode("utf-8").splitlines()
    header = lines[0].split("\t") if lines else []
    require(len(header) == 3 and header[:2] == ["tsr-reference-origins-v1", str(child["pid"])],
            "version/PID mismatch")
    epoch, = numbers(header[2:])
    wall_ns = int(child["wall_seconds"] * 1e9)
    require(child["started_at_unix_ns"] <= epoch <= child["started_at_unix_ns"] + wall_ns,
            "stale or future process epoch")
    rows = {name: [] for name in ("activity", "worker", "check", "site", "memory", "spelling")}
    widths = dict(activity=2, worker=3, check=4, site=4, memory=6, spelling=8)
    for line in lines[1:]:
        fields = line.split("\t")
        require(fields[0] in rows and len(fields) == widths[fields[0]] + 1, "unknown/invalid row")
        rows[fields[0]].append(numbers(fields[1:]))
    require(len(rows["activity"]) == len(rows["spelling"]) == 1, "missing/duplicate aggregate")
    workers = {owner: (begin, end) for owner, begin, end in rows["worker"]}
    require(len(workers) == len(rows["worker"]) == expected_workers and
            set(workers) == set(range(expected_workers)), "worker admission mismatch")
    require(all(0 <= begin < end <= wall_ns for begin, end in workers.values()), "worker interval outside process")
    checked = {}
    per_worker = {owner: [] for owner in workers}
    for owner, index, begin, end in rows["check"]:
        require(owner in workers and index < len(loaded) and index % expected_workers == owner,
                "unknown file or wrong affinity")
        require(index not in checked, "duplicate check")
        require(workers[owner][0] <= begin < end <= workers[owner][1], "check outside private lifetime")
        checked[index] = loaded[index]
        per_worker[owner].append((begin, end))
    require(len(checked) == cost.counts(child).get("Checked files"), "checked scope/count mismatch")
    require(all(peak(intervals) <= 1 for intervals in per_worker.values()), "overlapping private checks")
    actual = [peak(workers.values()), peak((begin, end) for _, _, begin, end in rows["check"])]
    require(actual == rows["activity"][0], "peak activity mismatch")
    sites = {kind: dict(calls=calls, outer=outer, elapsed_ns=ns) for kind, calls, outer, ns in rows["site"]}
    require(len(sites) == len(rows["site"]) == 3 and set(sites) == {1, 2, 3}, "missing/duplicate origin site")
    lifetime_ns = sum(end - begin for begin, end in workers.values())
    require(all(site["outer"] <= site["calls"] and
                (site["calls"] == 0 or site["outer"] > 0) and site["elapsed_ns"] <= lifetime_ns
                for site in sites.values()), "impossible site counters")
    names = ("allocation_requests", "cumulative_requested_bytes", "peak_requested_bytes",
             "live_requested_bytes", "live_padded_layout_bytes")
    memory = {owner: dict(zip(names, values)) for owner, *values in rows["memory"]}
    require(len(memory) == len(rows["memory"]) == 5 and set(memory) == set(range(5)), "missing/duplicate allocation origin")
    require(all(m["live_requested_bytes"] <= m["peak_requested_bytes"] <= m["cumulative_requested_bytes"] and
                m["live_padded_layout_bytes"] >= m["live_requested_bytes"] for m in memory.values()),
            "impossible memory counters")
    for field in ("allocation_requests", "cumulative_requested_bytes", "live_requested_bytes", "live_padded_layout_bytes"):
        require(memory[0][field] == sum(memory[owner][field] for owner in range(1, 5)), "origin aggregate mismatch")
    require(max(memory[owner]["peak_requested_bytes"] for owner in range(1, 5)) <= memory[0]["peak_requested_bytes"] <=
            sum(memory[owner]["peak_requested_bytes"] for owner in range(1, 5)), "impossible simultaneous peak")
    spelling_names = ("preparations", "rendered_strings", "written_strings", "returned_bytes",
                      "returned_capacity_bytes", "retained_preparations", "discarded_bytes", "discarded_capacity_bytes")
    spelling = dict(zip(spelling_names, rows["spelling"][0]))
    require(spelling["rendered_strings"] + spelling["written_strings"] >= spelling["preparations"] >=
            spelling["retained_preparations"] and
            spelling["returned_bytes"] <= spelling["returned_capacity_bytes"] and
            spelling["discarded_bytes"] <= spelling["returned_bytes"] and
            spelling["discarded_bytes"] <= spelling["discarded_capacity_bytes"] <= spelling["returned_capacity_bytes"],
            "impossible spelling counters")
    require(spelling["retained_preparations"] < spelling["preparations"] or spelling["discarded_capacity_bytes"] == 0,
            "all retained but discarded bytes")
    return dict(trace_sha256=cost.file_hash(path), admitted_workers=len(workers), peak_private=actual[0],
                peak_checking=actual[1], checked_files=[[index, checked[index]] for index in sorted(checked)],
                sites={str(key): value for key, value in sites.items()},
                memory={str(key): value for key, value in memory.items()}, spelling=spelling)


def measure(normal_identity, probe_identity, baseline_path, output, rounds, ledger=None):
    require(rounds >= 2, "at least two observer rounds required")
    normal = json.loads(normal_identity.read_text())
    probe = json.loads(probe_identity.read_text())
    baseline = json.loads(baseline_path.read_text())
    require(normal["source"] == probe["source"] == baseline["source"], "source mismatch")
    require(normal["native_sha"] == probe["native_sha"] == baseline["native_pin"], "native pin mismatch")
    binaries = dict(normal=Path(normal["binary"]), off=Path(probe["binary"]), on=Path(probe["binary"]))
    for label, identity in (("normal", normal), ("off", probe)):
        require(cost.file_hash(binaries[label]) == identity["binary_sha256"], "binary identity mismatch")
    for item in probe["changed_sources"]:
        require(cost.file_hash(Path(probe["source_root"]) / item["path"]) == item["sha256"], "probe source changed")
    project = Path(normal["project"])
    flags = ["--project", str(project), "--noEmit", "--incremental", "false", "--composite", "false", "--pretty", "false"]
    baseline_paths = [row["path"] for row in baseline["physical_input_snapshot"]]
    require(cost.snapshot(baseline_paths) == baseline["physical_input_snapshot"], "baseline observed inputs changed")
    paths = baseline_paths + [str(binaries["off"])]
    reference = cost.snapshot(sorted(set(paths)))
    require(cost.valid_snapshot(reference), "invalid observed inputs")
    result = dict(source=normal["source"], native_pin=normal["native_sha"], normal_identity=normal,
                  probe_identity=probe, harness_sha256=cost.file_hash(Path(__file__)),
                  children=[], complete=False, input_fingerprint=cost.fingerprint(reference),
                  complete_input_equivalence_verified=False, all_semantic_work_equivalence_verified=False)
    def save():
        cost.write(output, result)
        cost.write(output.with_suffix(".result.yaml"), dict(receipt=str(output), complete=result["complete"],
                    children=len(result["children"]), kind="archive-only locating observer"))
        if ledger and result["children"]:
            log = json.loads(ledger.read_text())
            entry = next((e for e in log["experiments"] if e.get("hypothesis_id") == "reference-preparation-origins"), None)
            if entry is None:
                entry = dict(iteration=1, hypothesis_id="reference-preparation-origins", kind="locating observer",
                             opportunity=log["hypothesis_backlog"][0])
                log["experiments"].append(entry)
            entry.update(receipt=str(output), complete=result["complete"], outcome="attribution only; no runtime candidate",
                         samples=[{k: c.get(k) for k in ("mode", "variant", "round", "wall_seconds", "user_seconds", "system_seconds", "peak_rss_bytes")}
                                  for c in result["children"]])
            cost.write(ledger, log)
    save()
    for round_number in range(rounds):
        modes = ("default", "single") if round_number % 2 == 0 else ("single", "default")
        for mode in modes:
            for label in (("normal", "off", "on") if round_number % 2 == 0 else ("on", "off", "normal")):
                require(cost.snapshot(sorted(set(paths))) == reference, "observed inputs changed")
                for key in list(os.environ):
                    if key.startswith("TSR_"):
                        os.environ.pop(key)
                trace = output.with_name(f"{output.stem}-{round_number}-{mode}-{label}.tsv")
                if label == "on":
                    require(not trace.exists(), "receipt destination already exists")
                    os.environ["TSR_REFERENCE_ORIGINS"] = str(trace)
                extra = [] if mode == "default" else ["--singleThreaded"]
                child = cost.process([str(binaries[label]), *flags, *extra, "--listFiles", "--extendedDiagnostics"], project.parent, 180)
                child.update(mode=mode, variant=label, round=round_number)
                result["children"].append(child)
                save()
                require(not child["timed_out"] and child["exit_code"] in (0, 1), "incomplete observer child")
                child.update(diagnostics=cost.diagnostics(child["stdout"] + child["stderr"], project.parent), counts=cost.counts(child))
                require(cost.files(child) == baseline["ordered_loaded"], "loaded order changed")
                expected = next(c for c in baseline["children"] if c["mode"] == mode)
                require(child["diagnostics"] == expected["diagnostics"] and child["counts"] == expected["counts"] and
                        child["exit_code"] == expected["exit_code"], "complete output or scope changed")
                if label == "on":
                    child["probe"] = read_probe(trace, child, cost.files(child), 4 if mode == "default" else 1)
                    child["trace_path"] = str(trace)
                require(cost.snapshot(sorted(set(paths))) == reference, "observed inputs changed after child")
                save()
                print(json.dumps(dict(mode=mode, variant=label, round=round_number, wall=child["wall_seconds"],
                                      spelling=child.get("probe", {}).get("spelling"))), flush=True)
    result["summary"] = {mode: {label: {field: statistics.median(c[field] for c in result["children"]
        if c["mode"] == mode and c["variant"] == label) for field in ("wall_seconds", "user_seconds", "system_seconds", "peak_rss_bytes")}
        for label in binaries} for mode in ("default", "single")}
    probes = [c["probe"] for c in result["children"] if c["variant"] == "on"]
    require(all(p["checked_files"] == probes[0]["checked_files"] for p in probes), "actual checked identities changed across modes")
    result["complete"] = True
    result["actual_pool_activity_verified"] = True
    save()
    return result


def main():
    parser = argparse.ArgumentParser()
    for name in ("normal-identity", "probe-identity", "baseline", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=2)
    parser.add_argument("--ledger", type=Path)
    args = parser.parse_args()
    result = measure(args.normal_identity, args.probe_identity, args.baseline, args.output, args.rounds, args.ledger)
    print(json.dumps(result["summary"]))


if __name__ == "__main__":
    main()
