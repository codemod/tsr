#!/usr/bin/env python3
"""Independently supervise both lifecycle producers; never infer speed acceptance."""
from __future__ import annotations

import argparse
import copy
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from benchmark_inputs import file_hash, snapshot
from checker_work_trace import validate_worker_activity
from whole_project_perf import process


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def environment(producer, trace):
    os.environ.pop("TSR_WORK_TRACE", None)
    os.environ.pop("TSR_NATIVE_WORK_ACTIVITY", None)
    if trace is not None:
        os.environ["TSR_WORK_TRACE" if producer == "tsr" else "TSR_NATIVE_WORK_ACTIVITY"] = str(trace)


def loaded(child):
    return [line for line in child["stdout"].splitlines()
            if line.startswith(("/", "bundled:///")) and "error TS" not in line]


def binding(child, trace, producer, source_sha, sources, before, after, config, project, names):
    # Read only the initial header; the independent reader owns the rest.
    with trace.open() as stream:
        header = json.loads(stream.readline())
    command = child["command"]
    # These native internal driver switches are omitted by showConfig. Bind the
    # exact supervised command, without treating the trace's request as evidence.
    requested = (int(command[command.index("--checkers") + 1]) if "--checkers" in command else
                 config.get("compilerOptions", {}).get("checkers"))
    single = True if "--singleThreaded" in command else config.get("compilerOptions", {}).get("singleThreaded")
    return {"schema_version": 1, "child": child, "current_directory": str(project.resolve()),
            "invocation_id": header["invocation_id" if producer == "tsr" else "nonce"],
            "source_sha": source_sha, "source_files_sha256": sources,
            "binary_sha256": file_hash(child["command"][0]), "trace_sha256": file_hash(trace),
            "inputs_before": before, "inputs_after": after, "loaded_files": names,
            "show_config": config, "requested_checkers": requested, "requested_single_threaded": single}


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def corruption_controls(trace, receipt, producer, output):
    rows = [json.loads(line) for line in trace.read_text().splitlines()]
    for name, mutate in [
        ("pid", lambda r: r[0].update(pid=r[0]["pid"] + 1)),
        ("clock", lambda r: r[-1].update(recorded_at_ns=-1)),
        ("peak", lambda r: r[-1]["peaks"].update(observed=999) if producer == "native" else
         r[-1].update(peak_observed_checkers=999)),
        ("completion", lambda r: r.append(copy.deepcopy(r[-1]))),
    ]:
        changed = copy.deepcopy(rows)
        mutate(changed)
        path = output / (producer + "-corrupt-" + name + ".ndjson")
        path.write_text("".join(json.dumps(row) + "\n" for row in changed))
        context = {**receipt, "trace_sha256": file_hash(path)}
        verdict = validate_worker_activity(path, context, producer)
        require(not verdict["worker_activity_valid"] and verdict["reasons"], "corrupt trace accepted: " + name)
        write(path.with_suffix(".result.json"), verdict)


def kill_observed(command, project, trace, producer):
    """Stop a live child, prove an open covered span, then kill and reap it."""
    environment(producer, trace)
    started_at = time.time_ns()
    started = time.monotonic()
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        child = subprocess.Popen(command, cwd=project, stdout=out, stderr=err, start_new_session=True)
        observed = []
        status = usage = None
        try:
            while time.monotonic() - started < 30:
                if trace.exists():
                    data = trace.read_bytes()
                    # A concurrent final record may still be being written.
                    rows = [json.loads(line) for line in data[:data.rfind(b"\n") + 1].splitlines()]
                    active = {}
                    for row in rows:
                        if row["event"] == ("span_begin" if producer == "native" else "work_begin"):
                            active[row["token" if producer == "native" else "span_id"]] = row
                        elif row["event"] == ("span_end" if producer == "native" else "work_end"):
                            active.pop(row["token" if producer == "native" else "span_id"], None)
                    if any(row["operation"] == "source_file_check" for row in active.values()):
                        os.kill(child.pid, signal.SIGSTOP)
                        _, stopped, stopped_usage = os.wait4(child.pid, os.WUNTRACED)
                        if not os.WIFSTOPPED(stopped):
                            status, usage = stopped, stopped_usage
                            child.returncode = os.waitstatus_to_exitcode(status)
                        require(os.WIFSTOPPED(stopped), "compiler completed before stop")
                        frozen = trace.read_bytes()
                        rows = [json.loads(line) for line in frozen[:frozen.rfind(b"\n") + 1].splitlines()]
                        active = {}
                        for row in rows:
                            if row["event"] in ("span_begin", "work_begin"):
                                active[row.get("token", row.get("span_id"))] = row
                            elif row["event"] in ("span_end", "work_end"):
                                active.pop(row.get("token", row.get("span_id")), None)
                        observed = [row for row in active.values() if row["operation"] == "source_file_check"]
                        require(observed, "no covered work active at actual stop")
                        break
                time.sleep(0.001)
        finally:
            if status is None:
                # Popen.send_signal/kill polls and may reap; wait4 owns this PID.
                try:
                    os.kill(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                _, status, usage = os.wait4(child.pid, 0)
                child.returncode = os.waitstatus_to_exitcode(status)
        require(observed and child.returncode == -9, "live observed kill control failed")
        out.seek(0)
        err.seek(0)
        return {"pid": child.pid, "command": command, "started_at_unix_ns": started_at,
                "exit_code": child.returncode, "timed_out": False,
                "stdout": out.read().decode(), "stderr": err.read().decode(),
                "wall_seconds": time.monotonic() - started,
                "user_seconds": usage.ru_utime, "system_seconds": usage.ru_stime,
                "peak_rss_bytes": usage.ru_maxrss * (1 if sys.platform == "darwin" else 1024)}, observed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("tsr-probe", "native-probe", "native-baseline", "tsr-source", "native-source", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--tsr-sha", required=True)
    parser.add_argument("--native-sha", required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    projects = {}
    for name in ("cycle", "small", "skewed", "cap", "kill"):
        project = args.output / name
        project.mkdir()
        projects[name] = project
        files = {"main.ts": 'export const wrong: number = "bad";\n'}
        options = {"noEmit": True, "strict": True, "skipLibCheck": True,
                   "module": "NodeNext", "moduleResolution": "NodeNext", "resolveJsonModule": True}
        if name == "cycle":
            files.update({"main.ts": 'import {a} from "./a"; import data from "./data.json"; export const wrong: number = "bad"; export const out = a + data.value;\n',
                          "a.ts": 'import type {B} from "./b"; export type A = {b?: B}; export const a = 1;\n',
                          "b.ts": 'import type {A} from "./a"; export type B = {a?: A};\n',
                          "types.d.ts": 'interface Unused { x: string }\n', "data.json": '{"value":2}\n'})
        else:
            options["noLib"] = True
            files["globals.d.ts"] = "\n".join([
                "interface Array<T> { length: number; [n: number]: T }",
                "interface IArguments { length: number; [n: number]: any }",
                *[f"interface {name} {{}}" for name in
                  ("Boolean", "Function", "CallableFunction", "NewableFunction", "Number", "Object", "RegExp", "String")],
            ])
            if name in ("skewed", "cap"):
                files.update({f"tiny{i}.ts": f"export const tiny{i} = {i};\n" for i in range(260 if name == "cap" else 20)})
                files["main.ts"] += "\n".join(f"export const value{i} = {{nested: {{x: {i}}}}};" for i in range(1000))
            elif name == "kill":
                # Named symbol queries fill TSR's 64 KiB trace buffer during
                # the long worker; a short trace can remain invisible to the
                # supervisor until normal completion flushes it.
                files["main.ts"] += "\n".join(f"export const value{i}: typeof wrong = wrong;" for i in range(50000))
        for file, content in files.items():
            (project / file).write_text(content)
        write(project / "tsconfig.json", {"compilerOptions": options, "files": [f for f in files if f.endswith((".ts", ".tsx"))]})
    source_paths = {
        "tsr": [args.tsr_source / "crates" / crate / "src" / file for crate, file in
                [("tsr-execute", "work_trace.rs"), ("tsr-execute", "compile.rs"),
                 ("tsr-execute", "os_system.rs"), ("tsr-checker", "work_trace.rs")]],
        "native": [args.native_source / file for file in
                   ["cmd/tsgo/main.go", "internal/compiler/checkerpool.go", "internal/checker/checker.go",
                    "internal/workactivity/activity.go", "internal/workactivity/activity_test.go", "internal/compiler/worker_activity_test.go"]],
    }
    harness_paths = [Path(__file__), ROOT / "scripts/checker_work_trace.py", ROOT / "scripts/benchmark_inputs.py",
                     ROOT / "scripts/whole_project_perf.py", ROOT / "docs/architecture/native-worker-activity.patch"]
    binaries = {"tsr": args.tsr_probe.resolve(), "native": args.native_probe.resolve()}
    sources = {producer: {str(path.resolve()): file_hash(path) for path in paths + harness_paths}
               for producer, paths in source_paths.items()}
    modes = [("cycle", "default", []), ("cycle", "single", ["--singleThreaded"]),
             ("cycle", "two", ["--checkers", "2"]), ("cycle", "one", ["--checkers", "1"]),
             ("cycle", "file-clamp", ["--checkers", "257"]),
             ("cycle", "single-precedence", ["--singleThreaded", "--checkers", "257"]),
             ("cycle", "no-check", ["--noCheck"]), ("cycle", "libs", ["--skipLibCheck", "false"]),
             ("cycle", "skip-default", ["--skipLibCheck", "false", "--skipDefaultLibCheck"]),
             ("cycle", "list-only", ["--listFilesOnly"]),
             ("small", "default", []), ("small", "clamp", ["--checkers", "257"]),
             ("skewed", "default", []), ("skewed", "two", ["--checkers", "2"]),
             ("cap", "upper-clamp", ["--checkers", "257"])]
    report = {"tsr_source_sha": args.tsr_sha, "native_source_sha": args.native_sha,
              "binary_sha256": {p: file_hash(b) for p, b in binaries.items()},
              "native_baseline_sha256": file_hash(args.native_baseline), "source_files_sha256": sources,
              "cases": [], "failures": [], "target_verified": False,
              "actual_checked_work_verified": False, "safe_memory_admission_verified": False}
    contexts = {}
    for project_name, mode, flags in modes:
        project = projects[project_name]
        for producer, binary in binaries.items():
            name = project_name + "-" + mode + "-" + producer
            command = [str(binary), "--project", str(project / "tsconfig.json"), "--pretty", "false",
                       "--noEmit", "--incremental", "false", "--composite", "false", "--listFiles", *flags]
            environment(producer, None)
            show = process([*command, "--showConfig"], project, 60)
            require(show["exit_code"] == 0 and not show["stderr"], "showConfig failed: " + name)
            config = json.loads(show["stdout"])
            off = process(command, project, 60)
            require(not off["timed_out"] and not off["stderr"], "off child failed: " + name)
            require("error TS2322:" in off["stdout"] or mode in ("no-check", "list-only"),
                    "deliberate semantic error not reached: " + name)
            original = None
            if producer == "native":
                original = process([str(args.native_baseline.resolve()), *command[1:]], project, 60)
                require(all(original[key] == off[key] for key in ("stdout", "stderr", "exit_code")), "native archive changed output")
            paths = [str(p.resolve()) for p in project.iterdir() if p.is_file()]
            paths += [str(p.resolve()) for p in (args.tsr_source / "vendor/typescript-go/internal/bundled/libs").glob("*.d.ts")]
            paths += [str(binary), *sources[producer]]
            samples = []
            for variant in ("on", "repeat"):
                trace = args.output / (name + "-" + variant + ".ndjson")
                require(not trace.exists(), "stale trace path")
                before = snapshot(paths)
                environment(producer, trace)
                child = process(command, project, 60)
                after = snapshot(paths)
                require(all(child[key] == off[key] for key in ("stdout", "stderr", "exit_code")), "observer changed output: " + name)
                receipt = binding(child, trace, producer, args.tsr_sha if producer == "tsr" else args.native_sha,
                                  sources[producer], before, after, config, project, loaded(off))
                verdict = validate_worker_activity(trace, receipt, producer)
                write(trace.with_suffix(".receipt.json"), receipt)
                write(trace.with_suffix(".result.json"), verdict)
                require(verdict["worker_activity_valid"], name + ": " + str(verdict["reasons"]))
                samples.append({"child": child, "verdict": verdict, "trace_sha256": file_hash(trace)})
                if project_name == "small" and mode == "default" and variant == "on":
                    corruption_controls(trace, receipt, producer, args.output)
                    contexts[producer] = receipt
            require(samples[0]["verdict"]["full_file_affinity"] == samples[1]["verdict"]["full_file_affinity"], "unstable full-file ownership")
            report["cases"].append({"name": name, "config": config, "preflight": show,
                                    "original": original, "off": off, "samples": samples})
            print(name, "instances", samples[0]["verdict"]["checker_instances_created"], "full", len(samples[0]["verdict"]["full_file_affinity"]), flush=True)
    for producer, binary in binaries.items():
        context = contexts[producer]
        command = context["child"]["command"]
        for name in ("stale", "missing-parent"):
            trace = args.output / (producer + "-" + name + ".ndjson")
            if name == "stale":
                trace.write_text("sentinel from a previous invocation\n")
            else:
                trace = args.output / (producer + "-absent-parent") / "trace.ndjson"
            before = snapshot([row["path"] for row in context["inputs_before"]])
            environment(producer, trace)
            child = process(command, projects["small"], 60)
            after = snapshot([row["path"] for row in context["inputs_before"]])
            require(child["stdout"] == context["child"]["stdout"]
                    and child["exit_code"] == context["child"]["exit_code"], "trace failure changed compiler result")
            require((trace.read_text() == "sentinel from a previous invocation\n") if name == "stale" else not trace.exists(),
                    "trace failure overwrote/created an artifact")
            receipt = {**context, "child": child, "inputs_before": before, "inputs_after": after,
                       "trace_sha256": file_hash(trace) if trace.exists() else None}
            verdict = validate_worker_activity(trace, receipt, producer)
            require(not verdict["worker_activity_valid"] and "trace failure" in str(verdict["reasons"]), "warning-only artifact accepted")
            report["failures"].append({"producer": producer, "case": name, "child": child, "verdict": verdict})
        project = projects["kill"]
        trace = args.output / (producer + "-killed.ndjson")
        command = [str(binary), "--project", str(project / "tsconfig.json"), "--pretty", "false"]
        paths = [str(p.resolve()) for p in project.iterdir() if p.is_file()] + [str(binary), *sources[producer]]
        before = snapshot(paths)
        child, observed = kill_observed(command, project, trace, producer)
        receipt = binding(child, trace, producer, args.tsr_sha if producer == "tsr" else args.native_sha,
                          sources[producer], before, snapshot(paths), {}, project, [])
        verdict = validate_worker_activity(trace, receipt, producer)
        require(not verdict["worker_activity_valid"] and "signaled" in str(verdict["reasons"]), "killed compiler accepted")
        write(trace.with_suffix(".receipt.json"), receipt)
        write(trace.with_suffix(".result.json"), verdict)
        report["failures"].append({"producer": producer, "child": child, "active_at_stop": observed, "verdict": verdict})
    environment("tsr", None)
    write(args.output / "receipt.json", report)
    print("qualified worker mechanics; forcing/input/memory/speed gates remain false", flush=True)


if __name__ == "__main__":
    main()
