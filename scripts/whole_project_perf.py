#!/usr/bin/env python3
"""Measure fresh CLI processes; keep mismatched work visible (bd tsr-1yb.1).

Build both compilers first. This POSIX harness measures child CPU/RSS with wait4,
not cumulative RUSAGE_CHILDREN, and never reads or writes incremental build info.
"""

from __future__ import annotations

import argparse
import hashlib
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


def process(command: list[str], cwd: Path, timeout: float) -> dict:
    """Temporary files avoid pipe deadlocks; wait4 owns reaping this child."""
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        start = time.perf_counter()
        child = subprocess.Popen(
            command, cwd=cwd, stdout=stdout, stderr=stderr, start_new_session=True,
            env={**os.environ, "NO_COLOR": "1"},
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
            child.returncode = os.waitstatus_to_exitcode(status)
        finally:
            timer.cancel()
            timer.join()
        seconds = time.perf_counter() - start
        stdout.seek(0)
        stderr.seek(0)
        return {
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
    path = Path(name)
    absolute = path if path.is_absolute() else cwd / path
    # Keep logical paths: distinct symlink identities can affect module semantics.
    return os.path.normpath(str(absolute))


def input_fingerprint(names: list[str]) -> str:
    digest = hashlib.sha256()
    for name in sorted(set(names)):
        digest.update(name.encode())
        if not name.startswith("<typescript-lib>/"):
            digest.update(Path(name).read_bytes())
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
    parser.add_argument("--require-comparable", action="store_true")
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
    report = {
        "schema_version": 1, "source_sha": revision(ROOT),
        "oracle_sha": revision(ROOT / "vendor/typescript-go"),
        "project_sha": revision(cwd), "project": str(project),
        "project_config_sha256": hashlib.sha256(project.read_bytes()).hexdigest(),
        "machine": {"platform": platform.platform(), "cpu_count": os.cpu_count(),
                    "load_average": list(os.getloadavg())},
        "mode": args.mode, "flags": flags, "target_wall_ratio": 0.5,
        "cache_state": "fresh compiler processes, warmed filesystem; incremental/composite disabled",
        "tools": {}, "pairs": [],
    }

    def save() -> None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        temporary = args.output.with_suffix(args.output.suffix + ".tmp")
        temporary.write_text(json.dumps(report, indent=2) + "\n")
        temporary.replace(args.output)
        assert json.loads(args.output.read_text()) == report

    for name, binary in binaries.items():
        config = process([str(binary), *flags, "--showConfig"], cwd, args.timeout)
        listing = process([str(binary), *flags, "--listFilesOnly"], cwd, args.timeout)
        if config["exit_code"] != 0 or listing["exit_code"] != 0:
            raise RuntimeError(f"{name} preflight failed: {config['stdout']} {listing['stdout']} {listing['stderr']}")
        if DIAGNOSTIC.search(listing["stdout"]):
            raise RuntimeError(f"{name} file listing contains diagnostics")
        names = [file_identity(line, cwd) for line in listing["stdout"].splitlines() if line.strip()]
        if not names:
            raise RuntimeError(f"{name} did not load any files")
        report["tools"][name] = {
            "binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "effective_config": json.loads(config["stdout"]), "loaded_files": sorted(names),
            "loaded_file_count": len(names), "loaded_files_fingerprint": fingerprint(sorted(names)),
            "input_fingerprint": input_fingerprint(names),
            "samples": [],
        }
        save()
    ours, theirs = report["tools"]["tsr"], report["tools"]["tsgo"]
    report["scope_difference"] = {
        "tsr_only": sorted(set(ours["loaded_files"]) - set(theirs["loaded_files"])),
        "tsgo_only": sorted(set(theirs["loaded_files"]) - set(ours["loaded_files"])),
    }
    report["option_differences"] = option_differences(ours["effective_config"], theirs["effective_config"])
    for index in range(args.warmups + args.samples):
        order = ("tsr", "tsgo") if index % 2 == 0 else ("tsgo", "tsr")
        for name in order:
            measurement = process([str(binaries[name]), *flags], cwd, args.timeout)
            measurement["diagnostics"] = diagnostics(measurement.pop("stdout"), cwd)
            if index >= args.warmups:
                report["tools"][name]["samples"].append(measurement)
                save()  # Preserve every measurement before running another process.
            print(f"{name}: {measurement['wall_seconds']:.3f}s, exit {measurement['exit_code']}, "
                  f"{measurement['diagnostics']['count']} diagnostics", file=sys.stderr, flush=True)
            if measurement["timed_out"] or measurement["exit_code"] not in (0, 1, 2):
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
    report["inputs_unchanged"] = all(tool["input_fingerprint"] == input_fingerprint(tool["loaded_files"])
                                      for tool in report["tools"].values())
    report["work_comparable"] = (report["scope_match"] and report["options_match"]
                                 and report["diagnostics_stable"] and report["inputs_unchanged"])
    report["observed_wall_ratio"] = ours["summary"]["wall_seconds"]["median"] / theirs["summary"]["wall_seconds"]["median"]
    report["verified_wall_ratio"] = report["observed_wall_ratio"] if report["work_comparable"] else None
    report["target_verified"] = report["work_comparable"] and report["diagnostics_match"] and report["observed_wall_ratio"] <= 0.5
    save()
    print(json.dumps({key: report[key] for key in (
        "observed_wall_ratio", "verified_wall_ratio", "scope_match", "options_match",
        "diagnostics_stable", "diagnostics_match", "target_verified",
    )}))
    return 1 if args.require_comparable and not report["work_comparable"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
