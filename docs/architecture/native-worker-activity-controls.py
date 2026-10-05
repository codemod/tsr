#!/usr/bin/env python3
"""Fresh-process producer controls; independent benchmark qualification is separate."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import tempfile
import sys


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inspect_trace(path: Path, pid: int, status: int) -> dict:
    records = [json.loads(line) for line in path.read_text().splitlines()]
    assert records[0]["event"] == "invocation_start"
    assert records[0]["pid"] == pid
    assert records[-1]["event"] == "invocation_end"
    assert records[-1]["complete"] is True
    assert records[-1]["status"] == status
    assert records[0]["all_forcing_observed"] is False
    assert records[0]["memory_admission_budget"] is None
    assert all(a["recorded_at_ns"] <= b["recorded_at_ns"] for a, b in zip(records, records[1:]))
    active = {}
    peaks = {name: 0 for name in ("constructing", "semantic", "full", "leased", "observed")}
    pools, created, assignments, returned_constructors = {}, set(), set(), set()
    full_files = []
    for record in records:
        event = record["event"]
        if event == "pool_selected":
            pools[record["pool_id"]] = record["selected_count"]
        elif event == "checker_created":
            key = record["pool_id"], record["slot"]
            assert key not in created and key in returned_constructors
            created.add(key)
        elif event == "file_affinity":
            pool = record["pool_id"]
            assert record["slot"] == record["file_id"] % pools[pool]
            assignments.add((pool, record["file_id"]))
        elif event == "span_begin":
            token = record["token"]
            assert token not in active
            key = record["pool_id"], record["slot"]
            if record["operation"] == "lease":
                assert not any((r["pool_id"], r["slot"]) == key and r["operation"] == "lease" for r in active.values())
            elif record["operation"] != "constructor":
                assert key in created
            active[token] = record
            categories = {name: set() for name in peaks}
            for span in active.values():
                key = span["pool_id"], span["slot"]
                operation = span["operation"]
                if operation == "constructor":
                    categories["constructing"].add(key)
                    categories["observed"].add(key)
                elif operation == "lease":
                    categories["leased"].add(key)
                else:
                    categories["semantic"].add(key)
                    categories["observed"].add(key)
                    if operation == "source_file_check":
                        categories["full"].add(key)
            for name in peaks:
                peaks[name] = max(peaks[name], len(categories[name]))
        elif event == "span_end":
            begin = active.pop(record["token"])
            assert record["aborted"] is False
            if begin["operation"] == "constructor":
                returned_constructors.add((begin["pool_id"], begin["slot"]))
            if begin["operation"] == "source_file_check":
                full_files.append((begin["pool_id"], begin["file_id"], begin["slot"]))
    assert not active
    assert records[-1]["peaks"] == peaks
    assert created == returned_constructors
    assert all(sum(p == pool for p, _ in created) in (0, count) for pool, count in pools.items())
    assert all((pool, file) in assignments for pool, file, _ in full_files)
    return {"pools": pools, "instances": len(created), "peaks": peaks, "full_files": sorted(full_files),
            "events": len(records), "nonce": records[0]["nonce"]}


def wait_child(child: subprocess.Popen, timeout: float = 60) -> tuple[int, object, bool]:
    deadline = time.monotonic() + timeout
    while True:
        pid, status, usage = os.wait4(child.pid, os.WNOHANG)
        if pid:
            child.returncode = os.waitstatus_to_exitcode(status)
            return status, usage, False
        if time.monotonic() >= deadline:
            child.kill()
            _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
            return status, usage, True
        time.sleep(0.005)


def run_child(binary: Path, project: Path, args: list[str], trace: Path | None,
              qualify: bool = True) -> dict:
    env = dict(os.environ)
    env.pop("TSR_NATIVE_WORK_ACTIVITY", None)
    if trace is not None:
        env["TSR_NATIVE_WORK_ACTIVITY"] = str(trace)
    started = time.monotonic()
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        child = subprocess.Popen([str(binary), "--project", str(project / "tsconfig.json"),
                                  "--pretty", "false", "--noEmit", "--incremental", "false",
                                  "--composite", "false", "--listFiles", *args],
                                 cwd=project, env=env, stdout=out, stderr=err)
        _, usage, timed_out = wait_child(child)
        out.seek(0)
        err.seek(0)
        stdout, stderr = out.read().decode(), err.read().decode()
    result = {"pid": child.pid, "status": child.returncode, "stdout": stdout,
              "stderr": stderr, "wall_seconds": time.monotonic() - started,
              "cpu_seconds": usage.ru_utime + usage.ru_stime,
              "peak_rss_bytes": usage.ru_maxrss if sys.platform == "darwin" else usage.ru_maxrss * 1024,
              "timed_out": timed_out}
    if trace is not None and qualify:
        assert not timed_out and child.returncode >= 0
        assert "native worker activity warning:" not in stderr
        result["activity"] = inspect_trace(trace, child.pid, child.returncode)
        result["trace_sha256"] = digest(trace)
    return result


def failure_controls(binary: Path, project: Path, output: Path, baseline: dict) -> dict:
    stale = output / "stale.ndjson"
    stale.write_text("sentinel from a different invocation\n")
    unchanged = stale.read_bytes()
    stale_result = run_child(binary, project, [], stale, qualify=False)
    assert stale.read_bytes() == unchanged
    assert stale_result["status"] == baseline["status"]
    assert stale_result["stdout"] == baseline["stdout"]
    assert "native worker activity warning:" in stale_result["stderr"]
    missing = output / "absent-parent" / "trace.ndjson"
    failure = run_child(binary, project, [], missing, qualify=False)
    assert not missing.exists()
    assert failure["status"] == baseline["status"] and failure["stdout"] == baseline["stdout"]
    assert "native worker activity warning:" in failure["stderr"]
    # Kill a live fresh invocation after its own header exists. A large public
    # input prevents a completed tiny invocation from racing the test.
    kill_project = output / "kill-project"
    kill_project.mkdir()
    (kill_project / "main.ts").write_text("\n".join(f"export const v{i} = {i};" for i in range(50000)))
    (kill_project / "tsconfig.json").write_text('{"files":["main.ts"],"compilerOptions":{"noEmit":true}}')
    trace = output / "killed.ndjson"
    env = dict(os.environ, TSR_NATIVE_WORK_ACTIVITY=str(trace))
    started = time.monotonic()
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        child = subprocess.Popen([str(binary), "--project", str(kill_project / "tsconfig.json"),
                                  "--pretty", "false"], cwd=kill_project, env=env, stdout=out, stderr=err)
        while not (trace.exists() and trace.stat().st_size):
            if time.monotonic() - started > 10:
                child.kill()
                wait_child(child)
                raise AssertionError("live child produced no header")
            time.sleep(0.001)
        # wait4 with WNOHANG positively checks that this same handle is live.
        pid, _, _ = os.wait4(child.pid, os.WNOHANG)
        assert pid == 0, "kill control completed before observation"
        child.kill()
        _, usage, timed_out = wait_child(child)
        out.seek(0)
        err.seek(0)
        killed = {"pid": child.pid, "status": child.returncode, "stdout": out.read().decode(),
                  "stderr": err.read().decode(), "timed_out": timed_out,
                  "cpu_seconds": usage.ru_utime + usage.ru_stime,
                  "wall_seconds": time.monotonic() - started, "trace_sha256": digest(trace)}
    assert child.returncode == -9
    records = [json.loads(line) for line in trace.read_text().splitlines()]
    assert records[0]["pid"] == child.pid and records[0]["event"] == "invocation_start"
    assert not any(r["event"] == "invocation_end" for r in records)
    return {"stale": stale_result, "missing-parent": failure, "killed": killed}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--probe", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    project = args.output / "project"
    project.mkdir()
    files = {
        "tsconfig.json": json.dumps({"compilerOptions": {"strict": True, "skipLibCheck": True,
            "resolveJsonModule": True, "module": "NodeNext", "moduleResolution": "NodeNext"},
            "files": ["main.ts", "a.ts", "b.ts", "types.d.ts"]}),
        "main.ts": 'import {a} from "./a"; import data from "./data.json"; const wrong: number = "bad"; export const out = a + data.value;\n',
        "a.ts": 'import type {B} from "./b"; export type A = {b?: B}; export const a = 1;\n',
        "b.ts": 'import type {A} from "./a"; export type B = {a?: A};\n',
        "types.d.ts": 'interface Unused { x: string }\n',
        "data.json": '{"value":2}\n',
    }
    for name, content in files.items():
        (project / name).write_text(content)
    modes = {"default": [], "single": ["--singleThreaded"], "two": ["--checkers", "2"],
             "eight": ["--checkers", "8"], "no-check": ["--noCheck"],
             "libs": ["--skipLibCheck", "false"],
             "skip-default": ["--skipLibCheck", "false", "--skipDefaultLibCheck"],
             "list-only": ["--listFilesOnly"]}
    receipt = {"baseline_sha256": digest(args.baseline), "probe_sha256": digest(args.probe),
               "helper_sha256": digest(Path(__file__)), "files": {name: digest(project / name) for name in files},
               "modes": {}}
    nonces = set()
    for name, flags in modes.items():
        baseline = run_child(args.baseline.resolve(), project, flags, None)
        off = run_child(args.probe.resolve(), project, flags, None)
        on = run_child(args.probe.resolve(), project, flags, args.output / (name + ".ndjson"))
        repeat = run_child(args.probe.resolve(), project, flags, args.output / (name + "-repeat.ndjson"))
        for field in ("status", "stdout", "stderr"):
            assert baseline[field] == off[field] == on[field] == repeat[field], (name, field)
        assert on["activity"]["full_files"] == repeat["activity"]["full_files"]
        if name in ("no-check", "list-only"):
            assert not on["activity"]["full_files"]
        if name == "no-check":
            assert on["activity"]["instances"] == 4
        if name == "list-only":
            assert on["activity"]["instances"] == 0
        assert "error TS2322:" in baseline["stdout"] or name in ("no-check", "list-only")
        for run in (on, repeat):
            assert run["activity"]["nonce"] not in nonces
            nonces.add(run["activity"]["nonce"])
        receipt["modes"][name] = [baseline, off, on, repeat]
        print(name, "instances", on["activity"]["instances"], "full", len(on["activity"]["full_files"]), flush=True)
    receipt["failure_controls"] = failure_controls(args.probe.resolve(), project, args.output, receipt["modes"]["default"][0])
    (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print("receipt", args.output / "receipt.json")


if __name__ == "__main__":
    main()
