#!/usr/bin/env python3
"""Validate archive-only mapper pool observations; no speed acceptance."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import statistics
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from whole_project_perf import diagnostics, fingerprint, process, file_identity, input_fingerprint
from benchmark_inputs import file_hash, snapshot, valid_snapshot

COUNTER_NAMES = {
    site + suffix for site in ("anonymous", "signature", "mapped", "literal")
    for suffix in ("_requests", "_key_items", "_max_key_items", "_key_ns", "_lookup_ns",
                   "_active_hits", "_nonactive_hits", "_nonactive_error_hits", "_worker_starts",
                   "_worker_inclusive_ns", "_worker_successes", "_worker_failures")
} | {
    "predicate_top_requests", "predicate_unused_nested_keys", "predicate_top_key_items",
    "predicate_unused_nested_key_items", "predicate_key_ns", "predicate_top_hits",
    "predicate_top_workers", "predicate_nested_workers", "reference_lookup_requests",
    "reference_key_items", "reference_key_ns", "reference_lookup_ns", "reference_active_hits",
    "reference_nonactive_hits", "reference_miss_blocks", "reference_miss_inclusive_ns",
} | {
    site + suffix for site in ("anonymous_key", "signature_key", "mapped_key", "literal_key",
        "predicate_top", "predicate_unused_nested", "publication_clone", "literal_prelookup_property",
        "literal_prelookup_signature", "reference_key", "reference_publication", "reference_target")
    for suffix in ("_allocations", "_requested_bytes", "_usable_bytes")
}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")
    require(json.loads(path.read_text()) == value, "result write differs")


def lines(path):
    data = path.read_bytes()
    require(data.endswith(b"\n"), "partial trace")
    return [line.split("\t") for line in data.decode().splitlines()]


def peak(intervals):
    events = [(start, 1) for start, end in intervals if end > start]
    events += [(end, -1) for start, end in intervals if end > start]
    activity = maximum = 0
    for _, change in sorted(events):
        activity += change
        require(activity >= 0, "unbalanced activity")
        maximum = max(maximum, activity)
    require(activity == 0, "unfinished activity")
    return maximum


def inspect_trace(path, child, counters, cwd):
    require(not child["timed_out"] and child["exit_code"] in (0, 1), "incomplete child")
    rows = lines(path)
    require(rows[0] == ["schema", "mapper-pool-v1"], "trace version")
    header = rows[1]
    require(len(header) == 5 and header[0] == "process", "trace header")
    pid, count, enabled, epoch = map(int, header[1:])
    require(pid == child["pid"] and enabled == int(counters) and 1 <= count <= 256, "process binding")
    limit = int(child["wall_seconds"] * 1e9)
    require(child["started_at_unix_ns"] <= epoch <= child["started_at_unix_ns"] + limit, "stale epoch")
    require(rows[-1][0] == "pool_joined" and len(rows[-1]) == 2, "unfinished pool")
    joined = int(rows[-1][1])
    require(0 <= joined <= limit, "pool interval outside child")
    observed = {}
    for row in rows[2:-1]:
        require(len(row) == 3 and row[0] == "count" and row[1] not in observed, "duplicate/unknown count")
        value = int(row[2])
        require(value >= 0, "negative count")
        observed[row[1]] = value
    require(bool(observed) == counters, "counter mode mismatch")
    if counters:
        require(set(observed) == COUNTER_NAMES, "counter schema incomplete/unknown")
        for site in ("anonymous", "signature", "mapped", "literal"):
            require(observed[site + "_requests"] == sum(observed[site + suffix] for suffix in
                ("_active_hits", "_nonactive_hits", "_nonactive_error_hits", "_worker_starts")), "lookup classification")
            require(observed[site + "_worker_starts"] == observed[site + "_worker_successes"] +
                    observed[site + "_worker_failures"], "unfinished mapper frames")
        require(observed["reference_lookup_requests"] == sum(observed[key] for key in
            ("reference_active_hits", "reference_nonactive_hits", "reference_miss_blocks")), "reference classification")
    owners, checks, capacities = [], [], {}
    for owner in range(count):
        owner_path = path.with_suffix(f".owner-{owner}.tsv")
        records = lines(owner_path)
        record = records[0]
        require(len(record) == 8 and record[0] == "owner", "owner header")
        opid, oid, admission, start, setup, end, completed = map(int, record[1:])
        require((opid, oid, admission, completed) == (pid, owner, count, 1), "owner binding/incomplete")
        require(0 <= start <= setup <= end <= joined, "owner interval")
        local_checks, local_capacity, local_counts = [], {}, {}
        for record in records[1:]:
            if record[0] == "check":
                require(len(record) == 5, "check row")
                index, begin, finish = map(int, record[1:4])
                require(index >= 0 and index % count == owner and setup <= begin <= finish <= end, "affinity/check interval")
                local_checks.append({"index": index, "file": file_identity(record[4], cwd),
                                     "start_ns": begin, "end_ns": finish, "owner": owner})
            elif record[0] == "owner_count":
                require(len(record) == 3 and counters and record[1] in observed, "owner count name/mode")
                require(record[1] not in local_counts and int(record[2]) >= 0, "owner count value")
                local_counts[record[1]] = int(record[2])
            else:
                require(len(record) == 3 and record[0] == "capacity" and counters, "capacity row")
                require(record[1] not in local_capacity and int(record[2]) >= 0, "capacity value")
                local_capacity[record[1]] = int(record[2])
        require([x["index"] for x in local_checks] == sorted({x["index"] for x in local_checks}), "owner order/duplicate")
        for name, value in local_capacity.items():
            capacities[name] = capacities.get(name, 0) + value
        checks.extend(local_checks)
        owners.append({"owner": owner, "start_ns": start, "setup_end_ns": setup, "end_ns": end,
                       "checks": len(local_checks), "capacity": local_capacity, "counters": local_counts})
    if counters:
        require(all(set(x["counters"]) == set(observed) for x in owners), "incomplete owner counters")
        for name, value in observed.items():
            values = [x["counters"][name] for x in owners]
            expected = max(values) if name.endswith("_max_key_items") else sum(values)
            require(value == expected, "owner/global reconciliation differs")
    require(len({x["index"] for x in checks}) == len(checks), "cross-owner duplicate check")
    checks.sort(key=lambda x: x["index"])
    return {"counters": observed, "owners": owners, "checks": checks, "capacity_sum": capacities,
            "worker_count": count, "peak_worker_lifetimes": peak([(x["start_ns"], x["end_ns"]) for x in owners]),
            "peak_source_file_checks": peak([(x["start_ns"], x["end_ns"]) for x in checks]),
            "trace_sha256": file_hash(path)}


def output_scope(child, cwd):
    loaded = [file_identity(line, cwd) for line in child["stdout"].splitlines()
              if line.startswith(("/", "bundled:///")) and "error TS" not in line]
    reported = {}
    for line in child["stdout"].splitlines():
        for label in ("Files", "Checked files", "Parsed files"):
            if line.startswith(label + ":"):
                reported[label] = int(line.split(":", 1)[1].strip())
    return {"diagnostics": diagnostics(child["stdout"] + child["stderr"], cwd), "loaded": loaded,
            "reported": reported, "exit_code": child["exit_code"]}


def stable_counts(trace):
    return {k: v for k, v in trace["counters"].items() if "_ns" not in k}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bindings", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--project", type=Path)
    parser.add_argument("--pool-controls", action="store_true")
    parser.add_argument("--repeats", type=int, default=5)
    args = parser.parse_args()
    require(not args.output.exists() and args.repeats >= 2, "fresh output / at least two repeats required")
    bindings = json.loads(args.bindings.read_text())
    for role in ("normal", "probe", "native"):
        require(len(bindings[role]["source"]) == 40, "missing source qualification")
        require(file_hash(Path(bindings[role]["path"])) == bindings[role]["sha256"], "binary binding differs")
    require(bindings["normal"]["source"] == bindings["probe"]["source"], "different Rust sources")
    require(bindings["native"]["source"] == "5b1047d10d32e7d5b446be4de56b126ff42f82bb", "native pin")
    args.output.mkdir(parents=True)
    original_env = dict(os.environ)
    env = {k: v for k, v in os.environ.items() if not k.startswith(("TSR_", "CARGO_PROFILE_"))
           and k not in ("RUSTFLAGS", "GOMAXPROCS", "RAYON_NUM_THREADS")}
    os.environ.clear()
    os.environ.update(env)
    state = {"bindings": bindings, "kind": "mapper locating and observer overhead; no saved-wall claim", "cases": []}

    def persist():
        write(args.output / "results.json", state)

    def invoke(role, config, mode, destination, counters=False, trace=False, extra=()):
        destination.mkdir()
        command = [bindings[role]["path"], "--project", str(config.resolve()), "--noEmit",
                   "--incremental", "false", "--composite", "false", "--pretty", "false",
                   "--listFiles", "--extendedDiagnostics", *extra]
        if mode == "single":
            command += ["--singleThreaded"]
        path = destination / "trace.tsv"
        if trace:
            os.environ["TSR_MAPPER_TRACE"] = str(path.resolve())
        if counters:
            os.environ["TSR_MAPPER_PROFILE"] = "1"
        try:
            child = process(command, config.parent.resolve(), 180)
        finally:
            os.environ.pop("TSR_MAPPER_TRACE", None)
            os.environ.pop("TSR_MAPPER_PROFILE", None)
        for stream in ("stdout", "stderr"):
            (destination / stream).write_text(child[stream])
        result = {k: v for k, v in child.items() if k not in ("stdout", "stderr")}
        result["raw_sha256"] = {s: file_hash(destination / s) for s in ("stdout", "stderr")}
        result["role"] = role
        result["scope"] = output_scope(child, config.parent.resolve())
        require(not child["timed_out"] and child["exit_code"] in (0, 1, 2), "compiler did not complete")
        if trace:
            result["trace"] = inspect_trace(path, child, counters, config.parent.resolve())
            require(len(result["trace"]["checks"]) == result["scope"]["reported"].get("Checked files"), "reported/direct check mismatch")
        write(destination / "receipt.json", result)
        return result

    try:
        if args.project:
            require(not args.pool_controls, "project and pool controls are separate")
            cases = [("whole-project", args.project.resolve(), ())]
        elif args.pool_controls:
            globals_text = "\n".join([
                "interface Array<T> { length: number; [n: number]: T }",
                "interface IArguments { length: number; [n: number]: any }",
                *[f"interface {name} {{}}" for name in
                  ("Boolean", "Function", "CallableFunction", "NewableFunction", "Number", "Object", "RegExp", "String")],
            ]) + "\n"
            cases = []
            for name, extra in (("small", ()), ("skewed-two", ("--checkers", "2")), ("no-check", ("--noCheck",))):
                cwd = args.output / name
                cwd.mkdir()
                text = globals_text + 'declare function box<T>(x: T): { nested: { value: T } };\n'
                text += "\n".join(f"const value{i}: number = box<number>(1).nested.value;" for i in range(1 if name == "small" else 128))
                (cwd / "main.ts").write_text(text)
                files = ["main.ts"]
                if name != "small":
                    for index in range(20):
                        file = f"tiny{index}.ts"
                        (cwd / file).write_text(f"export const tiny{index} = {index};\n")
                        files.append(file)
                config = cwd / "tsconfig.json"
                write(config, {"compilerOptions": {"noLib": True, "strict": True, "noEmit": True}, "files": files})
                cases.append((name, config, extra))
        else:
            spec = importlib.util.spec_from_file_location("mapper_fixtures", ROOT / "docs/architecture/checker-mapper-key-controls.py")
            fixtures = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(fixtures)
            cases = []
            for name, text in fixtures.fixtures().items():
                cwd = args.output / name
                cwd.mkdir()
                (cwd / "input.ts").write_text(text)
                config = cwd / "tsconfig.json"
                write(config, {"compilerOptions": {"strict": True, "target": "es2020", "types": [],
                    "skipLibCheck": True, "noEmit": True, "incremental": False, "composite": False}, "files": ["input.ts"]})
                cases.append((name, config, ()))
        persist()
        for name, config, extra in cases:
            parent = args.output / name if args.project else config.parent
            parent.mkdir(exist_ok=True)
            for mode in ("default", "single"):
                directory = parent / mode
                directory.mkdir()
                case = {"name": name, "mode": mode, "extra_flags": list(extra), "config_sha256": file_hash(config), "runs": []}
                state["cases"].append(case)
                show_command = [bindings["normal"]["path"], "--project", str(config), "--showConfig",
                                "--noEmit", "--incremental", "false", "--composite", "false", *extra]
                if mode == "single":
                    show_command += ["--singleThreaded"]
                show = process(show_command, config.parent, 180)
                require(show["exit_code"] == 0 and not show["timed_out"], "effective config failed")
                case["effective_config"] = json.loads(show["stdout"])
                case["effective_config_fingerprint"] = fingerprint(case["effective_config"])
                expected = trace_identity = expected_counts = None
                repeats = args.repeats if args.project else 1
                for index in range(repeats):
                    roles = [("normal", "normal", False, False), ("disabled", "probe", False, True),
                             ("enabled", "probe", True, True)]
                    if not args.project:
                        roles += [("repeat", "probe", True, True), ("native", "native", False, False)]
                    else:
                        roles = roles[index % 3:] + roles[:index % 3]
                    for label, role, counters, traced in roles:
                        result = invoke(role, config, mode, directory / f"{index}-{label}", counters, traced, extra)
                        result["label"] = label
                        case["runs"].append(result)
                        persist()
                        if role == "native":
                            case["native_diagnostics_match"] = result["scope"]["diagnostics"] == expected["diagnostics"]
                        else:
                            if expected is None:
                                expected = result["scope"]
                                physical = [f for f in expected["loaded"] if not f.startswith("<typescript-lib>/")]
                                case["observed_inputs_before"] = snapshot(physical + [str(config)])
                                require(valid_snapshot(case["observed_inputs_before"]), "unreadable observed inputs")
                            require(result["scope"] == expected, "probe or repeat changes complete output/scope")
                            if traced:
                                trace = result["trace"]
                                current = [(x["index"], x["file"], x["owner"]) for x in trace["checks"]]
                                if trace_identity is None:
                                    trace_identity = current
                                require(current == trace_identity, "off/on actual checked ownership/order differs")
                                if counters:
                                    if expected_counts is None:
                                        expected_counts = stable_counts(trace)
                                    require(stable_counts(trace) == expected_counts, "nontiming counters differ")
                case["observed_inputs_after"] = snapshot(physical + [str(config)])
                require(case["observed_inputs_before"] == case["observed_inputs_after"], "observed inputs changed")
                case["actual_checked_fingerprint"] = fingerprint(trace_identity)
                if name == "small":
                    require(all(r["trace"]["worker_count"] == 1 for r in case["runs"] if "trace" in r), "small admission")
                if name == "skewed-two":
                    require(all(r["trace"]["worker_count"] == (1 if mode == "single" else 2)
                                for r in case["runs"] if "trace" in r), "override/single admission")
                if name == "no-check":
                    require(not trace_identity and all(not r["trace"]["checks"] for r in case["runs"] if "trace" in r), "noCheck executes a file")
                case["checked_input_fingerprint"] = input_fingerprint([x[1] for x in trace_identity])
                case["summary"] = {label: {field: statistics.median([r[field] for r in case["runs"] if r["label"] == label])
                    for field in ("wall_seconds", "user_seconds", "system_seconds", "peak_rss_bytes")}
                    for label in {r["label"] for r in case["runs"]}}
                case["complete"] = True
                persist()
                print(name, mode, "completed", len(case["runs"]), flush=True)
        if args.pool_controls:
            state["failure_controls"] = []
            config = cases[-1][1]
            for mode in ("default", "single"):
                directory = args.output / ("failure-" + mode)
                directory.mkdir()
                path = directory / "trace.tsv"
                os.environ.update(TSR_MAPPER_TRACE=str(path.resolve()), TSR_MAPPER_PROFILE="1", TSR_MAPPER_FAIL_OWNER="0")
                command = [bindings["probe"]["path"], "--project", str(config.resolve()), "--pretty", "false"]
                if mode == "single":
                    command += ["--singleThreaded"]
                try:
                    child = process(command, config.parent.resolve(), 180)
                finally:
                    for key in ("TSR_MAPPER_TRACE", "TSR_MAPPER_PROFILE", "TSR_MAPPER_FAIL_OWNER"):
                        os.environ.pop(key, None)
                for stream in ("stdout", "stderr"):
                    (directory / stream).write_text(child[stream])
                write(directory / "child.json", child)
                require(child["exit_code"] in (-6, 101) and not child["timed_out"], "failure injection did not abort/unwind")
                require(not path.exists(), "failed pool emitted a completed aggregate")
                owner_path = path.with_suffix(".owner-0.tsv")
                failed_owner_complete = None
                if owner_path.exists():
                    failed = lines(owner_path)[0]
                    require(failed[0] == "owner" and int(failed[1]) == child["pid"] and failed[-1] == "0", "failed owner marked complete")
                    failed_owner_complete = False
                # Release uses panic=abort: a missing Drop receipt is not proof
                # of successful cleanup, and cannot be promoted to completion.
                try:
                    inspect_trace(path, child, True, config.parent.resolve())
                except ValueError:
                    rejected = True
                else:
                    rejected = False
                require(rejected, "failed compiler accepted as measurement")
                state["failure_controls"].append({k: v for k, v in child.items() if k not in ("stdout", "stderr")})
                state["failure_controls"][-1].update(mode=mode, aggregate_absent=True, failed_owner_complete=failed_owner_complete,
                    reader_rejected=True, raw_sha256={s: file_hash(directory / s) for s in ("stdout", "stderr")})
                persist()
        state["complete"] = True
        persist()
    finally:
        os.environ.clear()
        os.environ.update(original_env)


if __name__ == "__main__":
    main()
