#!/usr/bin/env python3
"""Measure fresh CLI processes; keep mismatched work visible (bd tsr-1yb.1).

Build both compilers first. This POSIX harness measures child CPU/RSS with wait4,
not cumulative RUSAGE_CHILDREN. Compiler invocations disable incremental/composite reuse.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import re
import signal
import statistics
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
INPUT_SPEC = importlib.util.spec_from_file_location(
    "benchmark_inputs", Path(__file__).with_name("benchmark_inputs.py"))
inputs = importlib.util.module_from_spec(INPUT_SPEC)
INPUT_SPEC.loader.exec_module(inputs)
DIAGNOSTIC = re.compile(r"(?:error|warning) TS\d+:")
DIAGNOSTIC_START = re.compile(r"^(?:.+\(\d+,\d+\): )?(?:error|warning) TS\d+:")
ANSI_CSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
ERROR_SUMMARY = re.compile(r"^Found \d+ errors?\b|^\s*Errors\s+Files\s*$")


def fingerprint(value: object) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def revision(path: Path) -> str | None:
    result = subprocess.run(
        ["git", "-C", str(path), "rev-parse", "HEAD"], capture_output=True, text=True
    )
    return result.stdout.strip() if result.returncode == 0 else None


def process(command: list[str], cwd: Path, timeout: float, *, environment: dict | None = None) -> dict:
    """Temporary files avoid pipe deadlocks; wait4 owns reaping this child."""
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        started_at_unix_ns = time.time_ns()
        start = time.perf_counter()
        child = subprocess.Popen(
            command, cwd=cwd, stdout=stdout, stderr=stderr, start_new_session=True,
            env={**{key: value for key, value in os.environ.items()
                    if key not in ("TSR_WORK_TRACE", "TSR_WORK_TRACE_BINARY_SHA256")},
                 "NO_COLOR": "1", **(environment or {})},
        )
        timed_out = threading.Event()

        def kill() -> None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
                timed_out.set()
            except ProcessLookupError:
                pass

        timer = threading.Timer(timeout, kill)
        timer.start()
        try:
            _, status, usage = os.wait4(child.pid, 0)
            # Python 3.8 on the reporting runner has wait4 but not the 3.9
            # waitstatus_to_exitcode helper. Preserve subprocess exit semantics.
            if os.WIFSIGNALED(status):
                child.returncode = -os.WTERMSIG(status)
            elif os.WIFEXITED(status):
                child.returncode = os.WEXITSTATUS(status)
            else:
                raise ValueError(f"Unexpected child wait status: {status}")
        finally:
            timer.cancel()
            timer.join()
        seconds = time.perf_counter() - start
        stdout.seek(0)
        stderr.seek(0)
        return {
            "pid": child.pid,
            "started_at_unix_ns": started_at_unix_ns,
            "command": list(command),
            "wall_seconds": seconds,
            "user_seconds": usage.ru_utime,
            "system_seconds": usage.ru_stime,
            "peak_rss_bytes": usage.ru_maxrss * (1 if sys.platform == "darwin" else 1024),
            "exit_code": child.returncode,
            "timed_out": timed_out.is_set(),
            "stdout": stdout.read().decode(errors="replace"),
            "stderr": stderr.read().decode(errors="replace"),
        }


def diagnostics(output: str, cwd: Path) -> dict:
    # Preserve multiline messages; sort complete diagnostics, never their lines.
    entries: list[str] = []
    in_diagnostic = False
    for line in ANSI_CSI.sub("", output).splitlines():
        line = line.replace(str(cwd) + "/", "<project>/").rstrip()
        if DIAGNOSTIC_START.match(line):
            entries.append(line)
            in_diagnostic = True
        elif ERROR_SUMMARY.match(line):
            in_diagnostic = False
        elif in_diagnostic and line[:1].isspace():
            entries[-1] += "\n" + line.rstrip()
        elif line:
            # Plain diagnostics have indented continuations. An unindented
            # summary/phase heading ends that block; its subsequent indented
            # rows must not become part of the preceding diagnostic.
            in_diagnostic = False
    return {"count": len(entries), "fingerprint": fingerprint(sorted(entries)), "entries": entries}


def file_identity(name: str, cwd: Path) -> str:
    name = name.replace("\\", "/")
    # Both compilers bundle the pinned standard libs, under different prefixes.
    base = name.rsplit("/", 1)[-1]
    if (name.startswith("bundled:///libs/") or "/typescript-go/internal/bundled/libs/" in name) \
            and re.fullmatch(r"lib\.[\w.]+\.d\.ts", base):
        return "<typescript-lib>/" + base
    # Keep logical paths: distinct symlink identities can affect module semantics.
    return inputs.path_identity(name, cwd)


def input_fingerprint(names: list[str]) -> str:
    digest = hashlib.sha256()
    for name in sorted(set(names)):
        digest.update(name.encode())
        if not name.startswith("<typescript-lib>/"):
            digest.update(bytes.fromhex(inputs.file_hash(name)))
    return digest.hexdigest()


def summary(samples: list[dict]) -> dict:
    result = {}
    for field in ("wall_seconds", "user_seconds", "system_seconds", "peak_rss_bytes"):
        values = sorted(sample[field] for sample in samples)
        result[field] = {
            "median": statistics.median(values),
            "p95": values[math.ceil(0.95 * len(values)) - 1],
            "min": values[0], "max": values[-1],
        }
    return result


def option_differences(left: dict, right: dict) -> dict:
    a, b = dict(left.get("compilerOptions", {})), dict(right.get("compilerOptions", {}))
    # lib spelling is case-insensitive. Retain all other differences, even when
    # showConfig might omit an option that the actual loader does honor.
    for options in (a, b):
        if "lib" in options:
            options["lib"] = [value.lower() for value in options["lib"]]
    return {key: {"tsr": a.get(key), "tsgo": b.get(key)}
            for key in sorted(a.keys() | b.keys()) if a.get(key) != b.get(key)}


def equivalence_certificate(report: dict) -> dict:
    """Inventory independent obligations; partial traces never discharge them."""
    constraints = {
        "five_fresh_pairs_and_warmups": report.get("sampling_protocol_verified") is True,
        "matching_full_loaded_scope": report.get("scope_match") is True,
        "matching_effective_options": report.get("options_match") is True,
        "stable_complete_cli_diagnostic_text": (report.get("diagnostics_stable") is True
                                                 and report.get("diagnostics_match") is True),
        "observed_inputs_unchanged": report.get("inputs_unchanged") is True,
        "pinned_native_revision": report.get("oracle_sha") == "5b1047d10d32e7d5b446be4de56b126ff42f82bb",
        # Neither current producer observes these obligations. Do not accept
        # externally supplied booleans as complete-work evidence.
        "source_to_binary_build_provenance": False,
        "complete_cross_tool_query_and_bundled_library_bytes": False,
        "actual_timed_program_inventory_and_eligibility": False,
        "actual_timed_full_worker_completion_and_cancellation": False,
        "initialization_and_all_metadata_forcing": False,
        "thread_preserving_private_checker_ownership": False,
        "diagnostic_spans_chains_related_information_and_filtering": False,
        "full_corpus_exact_parity_ge_99_9_and_no_prior_RIGHT_loss": False,
    }
    missing = [name for name, satisfied in constraints.items() if not satisfied]
    return {"schema_version": 1, "verified": not missing, "constraints": constraints,
            "proof_gap_count": len(missing), "unmet_constraints": missing,
            "source_qualification": {"source_sha": report.get("source_sha"),
                                     "oracle_sha": report.get("oracle_sha"),
                                     "harness_sha256": report.get("harness_sha256")}}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", type=Path, required=True, help="tsconfig.json path")
    parser.add_argument("--tsr", type=Path, default=ROOT / "target/release/tsr")
    parser.add_argument("--tsgo", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=5)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--mode", choices=("default", "single"), default="default")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--input-manifest", type=Path,
                        help="source-qualified JSON paths observed by resolver/config/host queries")
    parser.add_argument("--require-comparable", action="store_true")
    parser.add_argument("--capture-work", action="store_true",
                        help="capture separate untimed current TSR/native traces; never certify timed work from them")
    args = parser.parse_args()
    if args.samples < 1 or args.warmups < 0 or args.timeout <= 0:
        parser.error("samples and timeout must be positive; warmups must be nonnegative")
    project = args.project.resolve(strict=True)
    cwd = project.parent
    binaries = {name: path.resolve(strict=True) for name, path in (("tsr", args.tsr), ("tsgo", args.tsgo))}
    flags = ["--project", str(project), "--noEmit", "--incremental", "false",
             "--composite", "false", "--pretty", "false"]
    if args.mode == "single":
        flags += ["--singleThreaded", "true"]
    query_paths, manifest = inputs.load_manifest(args.input_manifest, cwd)
    input_paths = sorted({str(project), *query_paths, *(str(p) for p in binaries.values())})
    if args.input_manifest is not None:
        input_paths.append(str(args.input_manifest.resolve(strict=True)))
    capture_started = time.perf_counter()
    reference_inputs = inputs.snapshot(input_paths)
    setup_capture_seconds = time.perf_counter() - capture_started
    initial_by_path = {row["path"]: row for row in reference_inputs}
    required_files_valid = all(initial_by_path[str(path)].get("kind") == "file"
                               for path in (project, *binaries.values()))
    report = {
        "schema_version": 2, "source_sha": revision(ROOT),
        "oracle_sha": revision(ROOT / "vendor/typescript-go"),
        "project_sha": revision(cwd), "project": str(project),
        "project_config_sha256": initial_by_path[str(project)].get("sha256"),
        "machine": {"platform": platform.platform(), "cpu_count": os.cpu_count(),
                    "load_average": list(os.getloadavg())},
        "mode": args.mode, "flags": flags, "target_wall_ratio": 0.5,
        "cache_state": "fresh compiler processes, warmed filesystem; incremental/composite disabled",
        "tools": {}, "pairs": [],
        "status": "in_progress", "work_comparable": False,
        "verified_wall_ratio": None, "target_verified": False,
        "input_manifest": manifest, "input_observations": [],
        "input_setup_capture_seconds": setup_capture_seconds,
        "input_discovery_observations": [],
        "harness_sha256": {name: inputs.file_hash(ROOT / "scripts" / name)
                           for name in ("whole_project_perf.py", "benchmark_inputs.py", "checker_work_trace.py")},
        "warmups": [], "work_captures": {},
        "fresh_launch_delay": {"platform": sys.platform, "measured_separately": False,
                               "seconds": None, "security_settings_changed": False},
        "build_provenance_verified": False,
        "complete_input_equivalence_verified": False,
        "actual_checked_work_verified": False,
        "input_limits": [
            "Query paths are caller-supplied; absent or partial capture cannot prove complete inputs.",
            "Bundled library bytes, environment and unobserved queries are not covered.",
            "Before/after snapshots cannot detect all transient changes between observations.",
            "Fingerprinting runs outside child timing and warms OS caches.",
        ],
    }
    if manifest["provided"]:
        manifest["source_matches_harness_checkout"] = (
            manifest["provenance"]["source_sha"] == report["source_sha"])

    def save() -> None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        temporary = args.output.with_suffix(args.output.suffix + ".tmp")
        temporary.write_text(json.dumps(report, indent=2) + "\n")
        temporary.replace(args.output)
        assert json.loads(args.output.read_text()) == report

    def input_check(rows: list[dict]) -> dict:
        return {"fingerprint": fingerprint(rows), "path_count": len(rows),
                "valid": inputs.valid_snapshot(rows),
                "matches_reference": rows == reference_inputs,
                "kind_counts": {kind: sum(row.get("kind") == kind for row in rows)
                                for kind in ("file", "directory", "missing", "other")}}

    def controlled_process(command: list[str], *, environment: dict | None = None) -> dict | None:
        started = time.perf_counter()
        before = inputs.snapshot(input_paths)
        before_seconds = time.perf_counter() - started
        event = {"command": command, "before": input_check(before),
                 "before_capture_seconds": before_seconds}
        if not inputs.valid_snapshot(before) or before != reference_inputs:
            event["stable"] = False
            report["input_observations"].append(event)
            report.update(status="inputs_changed", inputs_unchanged=False)
            save()
            return None
        measurement = process(command, cwd, args.timeout, environment=environment)
        started = time.perf_counter()
        after = inputs.snapshot(input_paths)
        event.update(after=input_check(after), after_capture_seconds=time.perf_counter() - started,
                     pid=measurement["pid"],
                     stable=inputs.valid_snapshot(after) and after == reference_inputs)
        report["input_observations"].append(event)
        measurement["input_validation"] = event
        if not event["stable"]:
            report.update(status="inputs_changed", inputs_unchanged=False)
        save()
        return measurement

    def extend_inputs(additional: list[str]) -> bool:
        nonlocal input_paths, reference_inputs
        previous = {row["path"]: row for row in reference_inputs}
        input_paths = sorted({*input_paths, *additional})
        started = time.perf_counter()
        extended = inputs.snapshot(input_paths)
        report["input_discovery_observations"].append({
            "capture_seconds": time.perf_counter() - started,
            "path_count": len(extended), "valid": inputs.valid_snapshot(extended),
        })
        if not inputs.valid_snapshot(extended) or any(row != previous[row["path"]]
                                              for row in extended if row["path"] in previous):
            report.update(status="inputs_changed", inputs_unchanged=False)
            save()
            return False
        kinds = {row["path"]: row.get("kind") for row in extended}
        if any(kinds[path] != "file" for path in additional):
            report.update(status="invalid_loaded_inputs", inputs_unchanged=False)
            save()
            return False
        reference_inputs = extended
        report["input_reference_rows"] = reference_inputs
        report["input_reference"] = input_check(reference_inputs)
        save()
        return True

    if not inputs.valid_snapshot(reference_inputs) or not required_files_valid:
        report.update(status="invalid_inputs", inputs_unchanged=False)
        save()
        return 1
    report["input_reference_rows"] = reference_inputs
    report["input_reference"] = input_check(reference_inputs)
    save()
    for name, binary in binaries.items():
        config = controlled_process([str(binary), *flags, "--showConfig"])
        if config is None or not config["input_validation"]["stable"]:
            return 1
        listing = controlled_process([str(binary), *flags, "--listFilesOnly"])
        if listing is None or not listing["input_validation"]["stable"]:
            return 1
        if config["exit_code"] != 0 or listing["exit_code"] != 0:
            rejected = config if config["exit_code"] != 0 else listing
            report["rejected_preflight"] = rejected
            report["status"] = "timed_out" if rejected["timed_out"] else "tool_failed"
            save()
            raise RuntimeError(f"{name} preflight failed: {config['stdout']} {listing['stdout']} {listing['stderr']}")
        if DIAGNOSTIC.search(listing["stdout"]):
            raise RuntimeError(f"{name} file listing contains diagnostics")
        names = [file_identity(line, cwd) for line in listing["stdout"].splitlines() if line.strip()]
        if not names:
            raise RuntimeError(f"{name} did not load any files")
        if not extend_inputs([path for path in names if not path.startswith("<typescript-lib>/")]):
            return 1
        report["tools"][name] = {
            "binary": str(binary), "binary_sha256": inputs.file_hash(binary),
            "effective_config": json.loads(config["stdout"]), "loaded_files": sorted(names),
            "loaded_file_count": len(names), "loaded_files_fingerprint": fingerprint(sorted(names)),
            "input_fingerprint": input_fingerprint(names),
            "samples": [],
        }
        save()
    # After discovery, every full check uses the same union of both tools' paths.
    ours, theirs = report["tools"]["tsr"], report["tools"]["tsgo"]
    report["scope_difference"] = {
        "tsr_only": sorted(set(ours["loaded_files"]) - set(theirs["loaded_files"])),
        "tsgo_only": sorted(set(theirs["loaded_files"]) - set(ours["loaded_files"])),
    }
    report["option_differences"] = option_differences(ours["effective_config"], theirs["effective_config"])
    if args.capture_work:
        spec = importlib.util.spec_from_file_location("checker_work_trace", ROOT / "scripts/checker_work_trace.py")
        work = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(work)
        artifacts = args.output.resolve().with_suffix(args.output.suffix + ".work")
        # Native truncates existing trace files. Never reuse a capture directory.
        artifacts.mkdir(parents=True, exist_ok=False)
        for name in ("tsr", "tsgo"):
            directory = artifacts / name
            directory.mkdir()
            trace = directory / ("work.ndjson" if name == "tsr" else "trace.json")
            command = [str(binaries[name]), *flags]
            environment = {}
            if name == "tsr":
                environment = {"TSR_WORK_TRACE": str(trace),
                               "TSR_WORK_TRACE_BINARY_SHA256": report["tools"][name]["binary_sha256"]}
            else:
                command += ["--generateTrace", str(directory)]
            before = inputs.snapshot(input_paths)
            child = controlled_process(command, environment=environment)
            if child is None:
                return 1
            after = inputs.snapshot(input_paths)
            source_paths = work.qualified_source_paths(ROOT, name)
            receipt = {
                "schema_version": 1, "child": child, "current_directory": str(cwd),
                "source_sha": report["source_sha"], "oracle_sha": report["oracle_sha"],
                "binary_sha256": report["tools"][name]["binary_sha256"],
                "source_files_sha256": {str(path): inputs.file_hash(path) for path in source_paths},
                "inputs_before": before, "inputs_after": after,
                "loaded_files": [line for line in report["tools"][name]["loaded_files"]],
                "show_config": report["tools"][name]["effective_config"],
                "requested_checkers": report["tools"][name]["effective_config"].get("compilerOptions", {}).get("checkers"),
                "requested_single_threaded": True if args.mode == "single" else None,
                "trace_sha256": inputs.file_hash(trace) if trace.is_file() else None,
            }
            if name == "tsr" and trace.is_file():
                with work.regular_file(trace) as stream:
                    receipt["invocation_id"] = work.decode(stream.readline())["invocation_id"]
                # Program paths, not listFilesOnly's normalized cross-tool names.
                with work.regular_file(trace) as stream:
                    receipt["loaded_files"] = [row["path"] for row in map(work.decode, stream)
                                               if row.get("event") == "program_file"]
                result = work.validate_worker_activity(trace, receipt, "tsr")
            elif name == "tsgo" and trace.is_file():
                result = work.validate_native_trace(trace, receipt)
            else:
                result = work.empty_result()
                result["reasons"].append("Compiler did not produce a current work trace")
            receipt_path = directory / "receipt.json"
            receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
            report["work_captures"][name] = {
                "trace": str(trace), "receipt": str(receipt_path), "validation": result,
                "diagnostics": diagnostics(child["stdout"], cwd),
                "outside_timing": True, "same_binary": True,
                "timed_threading_equivalence_verified": False,
            }
            save()
            if not child["input_validation"]["stable"]:
                return 1
    for index in range(args.warmups + args.samples):
        order = ("tsr", "tsgo") if index % 2 == 0 else ("tsgo", "tsr")
        for name in order:
            measurement = controlled_process([str(binaries[name]), *flags])
            if measurement is None:
                return 1
            measurement["diagnostics"] = diagnostics(measurement["stdout"], cwd)
            if index >= args.warmups:
                report["tools"][name]["samples"].append(measurement)
            else:
                report["warmups"].append({"tool": name, "measurement": measurement})
            save()  # Preserve every measurement before running another process.
            if not measurement["input_validation"]["stable"]:
                report["rejected_measurement"] = measurement
                save()
                return 1
            print(f"{name}: {measurement['wall_seconds']:.3f}s, exit {measurement['exit_code']}, "
                  f"{measurement['diagnostics']['count']} diagnostics", file=sys.stderr, flush=True)
            if measurement["timed_out"] or measurement["exit_code"] not in (0, 1, 2):
                # Warmups do not enter samples, but their failures are still
                # evidence. Preserve resources and failure kind before aborting.
                report["rejected_measurement"] = measurement
                report["status"] = "timed_out" if measurement["timed_out"] else "tool_failed"
                save()
                raise RuntimeError(f"{name} did not complete a compiler check")
        if index >= args.warmups:
            report["pairs"].append({"order": order, "wall_ratio":
                ours["samples"][-1]["wall_seconds"] / theirs["samples"][-1]["wall_seconds"]})
            # Tuples round-trip through JSON as lists.
            report["pairs"][-1]["order"] = list(order)
            save()
    for tool in report["tools"].values():
        tool["summary"] = summary(tool["samples"])
    report["diagnostics_stable"] = all(
        len({(sample["diagnostics"]["fingerprint"], sample["exit_code"]) for sample in tool["samples"]}) == 1
        for tool in report["tools"].values()
    )
    report["diagnostics_match"] = (
        ours["samples"][0]["diagnostics"]["fingerprint"] == theirs["samples"][0]["diagnostics"]["fingerprint"]
        and ours["samples"][0]["exit_code"] == theirs["samples"][0]["exit_code"]
    )
    report["scope_match"] = ours["loaded_files_fingerprint"] == theirs["loaded_files_fingerprint"]
    report["options_match"] = not report["option_differences"]
    report["inputs_unchanged"] = all(event["stable"] for event in report["input_observations"])
    report["sampling_protocol_verified"] = args.samples >= 5 and args.warmups >= 1
    report["equivalent_work_certificate"] = equivalence_certificate(report)
    report["work_comparable"] = (report["sampling_protocol_verified"]
                                 and report["build_provenance_verified"]
                                 and report["scope_match"] and report["options_match"]
                                 and report["diagnostics_stable"] and report["diagnostics_match"]
                                 and report["inputs_unchanged"]
                                 and report["complete_input_equivalence_verified"]
                                 and report["actual_checked_work_verified"])
    report["comparability_reasons"] = [
        "Complete cross-tool query-input coverage is unverified.",
        "Actual performed checker work and worker budgets are unverified.",
    ]
    report["comparability_reasons"].extend(report["equivalent_work_certificate"]["unmet_constraints"])
    for field in ("scope_match", "options_match", "diagnostics_stable", "diagnostics_match"):
        if not report[field]:
            report["comparability_reasons"].append(f"{field} is false.")
    report["observed_wall_ratio"] = ours["summary"]["wall_seconds"]["median"] / theirs["summary"]["wall_seconds"]["median"]
    report["verified_wall_ratio"] = report["observed_wall_ratio"] if report["work_comparable"] else None
    report["target_verified"] = report["work_comparable"] and report["diagnostics_match"] and report["observed_wall_ratio"] <= 0.5
    report["status"] = "completed"
    save()
    print(json.dumps({key: report[key] for key in (
        "observed_wall_ratio", "verified_wall_ratio", "scope_match", "options_match",
        "diagnostics_stable", "diagnostics_match", "target_verified",
    )}))
    return 1 if args.require_comparable and not report["work_comparable"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
