#!/usr/bin/env python3
"""Prove disabled incremental reuse in an owned copy of generic-imports.

Build the binaries first. This does not alter an application or evict OS caches.
The small fixture diagnoses cache isolation, not the representative 2x target.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys

from whole_project_perf import (
    ROOT, diagnostics, file_identity, fingerprint, input_fingerprint,
    option_differences, process, revision, summary,
)


POISON = "CACHE_ISOLATION_POISON"
SOURCE_NAMES = ("index.ts", "other.ts", "schema.ts")
STATES = ("absent", "valid_incremental", "stale_incremental", "poisoned_incremental", "malformed")
LOCATION = re.compile(r"^(.+)(\(\d+,\d+\): (?:error|warning) TS\d+:)")
STATISTIC = re.compile(r"^([A-Za-z ]+):\s+(\d+)\s*$", re.MULTILINE)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build_state(workspace: Path) -> dict:
    """Observe all build info in the owned fixture, including the default path."""
    result = {}
    for path in sorted(workspace.rglob("*.tsbuildinfo")):
        stat = path.stat()
        result[path.relative_to(workspace).as_posix()] = {
            "sha256": sha256(path), "size_bytes": stat.st_size,
            "mtime_ns": stat.st_mtime_ns,
        }
    return result


def normalize_diagnostics(output: str, workspace: Path) -> dict:
    result = diagnostics(output, workspace)
    entries = []
    sources = {os.path.normpath(str(workspace / name)): name for name in SOURCE_NAMES}
    for entry in result["entries"]:
        match = LOCATION.match(entry)
        if match:
            # TSR and native can render relative to the launching shell or the
            # subprocess cwd. Only canonicalize a path naming this owned fixture;
            # an unrelated file with the same basename must stay distinguishable.
            name = match[1].replace("<project>/", str(workspace) + "/")
            for cwd in (workspace, Path.cwd()):
                absolute = os.path.normpath(str(cwd / name))
                if absolute in sources:
                    entry = "<fixture>/" + sources[absolute] + entry[len(match[1]):]
                    break
        entries.append(entry)
    entries.sort()
    return {"count": len(entries), "entries": entries,
            "fingerprint": fingerprint(entries)}


def poison_build_info(data: bytes) -> bytes:
    """Change a cached diagnostic, not a file version or compiler option."""
    info = json.loads(data)
    changed = 0
    for row in info.get("semanticDiagnosticsPerFile", []):
        if not isinstance(row, list) or len(row) != 2:
            continue
        for diagnostic in row[1]:
            if diagnostic.get("code") == 2322 and diagnostic.get("messageArgs"):
                diagnostic["messageArgs"][0] = POISON
                changed += 1
    if not changed:
        raise ValueError("native did not persist a TS2322 diagnostic for the positive control")
    return (json.dumps(info) + "\n").encode()


def full_check_gates(sample: dict, reference: dict, tool: str) -> dict:
    entries = sample["diagnostics"]["entries"]
    sentinel_files = {match[1] for entry in entries
                      if (match := re.match(r"^<fixture>/([^/]+)\(\d+,\d+\): error TS2322:", entry))}
    gates = {
        "completed": not sample["timed_out"] and sample["exit_code"] in (1, 2),
        "complete_diagnostics_unchanged": sample["diagnostics"] == reference["diagnostics"],
        "every_source_diagnosed": sentinel_files == set(SOURCE_NAMES),
        "exactly_three_sentinels": len(entries) == len(SOURCE_NAMES),
        "cached_poison_absent": all(POISON not in entry for entry in entries),
        "build_state_unchanged": sample["build_state_before"] == sample["build_state_after"],
        "fresh_process_recorded": sample["pid"] > 0 and sample["started_at_unix_ns"] > 0,
        "stderr_empty": not sample.get("stderr"),
    }
    if tool == "tsr":
        gates["checked_source_count"] = sample["statistics"].get("Checked files") == len(SOURCE_NAMES)
    else:
        # An incremental replay of the complete cached messages could pass every
        # diagnostic gate. Require the native generic workload to run as well.
        expected = reference["statistics"].get("Instantiations", 0)
        gates["native_instantiations_repeated"] = (
            expected > 0 and sample["statistics"].get("Instantiations") == expected
        )
    return gates


def save_report(path: Path, report: dict) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(report, indent=2) + "\n")
    temporary.replace(path)
    assert json.loads(path.read_text()) == report


def run(args: argparse.Namespace) -> dict:
    binaries = {name: binary.resolve(strict=True)
                for name, binary in (("tsr", args.tsr), ("tsgo", args.tsgo))}
    # Exclusive creation prevents this tool from overwriting a user's project,
    # a previous run's build state, or another agent's fixture.
    workspace = args.work_dir.absolute()
    workspace.mkdir(parents=True, exist_ok=False)
    project = workspace / "tsconfig.json"
    fixture = ROOT / "benches/projects/generic-imports"
    for name in SOURCE_NAMES:
        source = (fixture / name).read_text()
        if name != "index.ts":
            source += '\nexport const cacheSentinel: number = "cache isolation";\n'
        (workspace / name).write_text(source)
    config = json.loads((fixture / "tsconfig.json").read_text())
    config["compilerOptions"].update(
        incremental=True, composite=True, tsBuildInfoFile="cache.tsbuildinfo",
    )
    project.write_text(json.dumps(config, indent=2) + "\n")
    input_hashes = {name: sha256(workspace / name) for name in (*SOURCE_NAMES, "tsconfig.json")}
    common = ["--project", str(project), "--noEmit", "--pretty", "false",
              "--composite", "false"]
    if args.mode == "single":
        common += ["--singleThreaded", "true"]
    full = [*common, "--incremental", "false"]
    incremental = [*common, "--incremental", "true"]
    report = {
        "schema_version": 1, "source_sha": revision(ROOT),
        "oracle_sha": revision(ROOT / "vendor/typescript-go"),
        "mode": args.mode, "workspace": str(workspace), "flags": full,
        "fixture_origin": "benches/projects/generic-imports",
        "fixture_input_hashes": input_hashes,
        "cache_state": "fresh compiler processes; filesystem warmed by setup and preflight",
        "first_observed_disk_state": "uncontrolled; never claimed disk-cold",
        "watch_or_server_reuse": False,
        "release_target_wall_ratio": 0.5, "verified_release_wall_ratio": None,
        "target_verified": False, "tools": {}, "states": {},
        "positive_control": {}, "cache_isolation_verified": False,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)

    def save() -> None:
        save_report(args.output, report)

    def invoke(tool: str, flags: list[str]) -> dict:
        before = build_state(workspace)
        sample = process([str(binaries[tool]), *flags], workspace, args.timeout)
        output = sample.pop("stdout")
        sample["diagnostics"] = normalize_diagnostics(output, workspace)
        sample["statistics"] = {name: int(value) for name, value in STATISTIC.findall(output)}
        sample["build_state_before"] = before
        sample["build_state_after"] = build_state(workspace)
        return sample

    # Preflight observes loaded identities separately from actual source visits.
    # Each TS source has one deliberate error. Library files are skipLibCheck'd.
    for tool, binary in binaries.items():
        effective = process([str(binary), *full, "--showConfig"], workspace, args.timeout)
        listing = process([str(binary), *full, "--listFilesOnly"], workspace, args.timeout)
        if any(result["timed_out"] or result["exit_code"] != 0 for result in (effective, listing)):
            raise RuntimeError(f"{tool} cache-proof preflight failed")
        options = json.loads(effective["stdout"])
        if any(options["compilerOptions"].get(key) is not False for key in ("incremental", "composite")):
            raise RuntimeError(f"{tool} did not report disabled incremental/composite")
        if diagnostics(listing["stdout"], workspace)["count"]:
            raise RuntimeError(f"{tool} file listing contains diagnostics")
        names = sorted(file_identity(line, workspace) for line in listing["stdout"].splitlines() if line)
        if not names or {str(workspace / name) for name in SOURCE_NAMES} - set(names):
            raise RuntimeError(f"{tool} did not load the complete fixture")
        report["tools"][tool] = {
            "binary": str(binary), "binary_sha256": sha256(binary),
            "effective_config": options, "loaded_files": names,
            "loaded_files_fingerprint": fingerprint(names), "input_fingerprint": input_fingerprint(names),
            "preflight_processes": [{key: result[key] for key in ("pid", "started_at_unix_ns", "command")}
                                    for result in (effective, listing)],
            "reference": invoke(tool, [*full, "--extendedDiagnostics"]),
        }
        save()
    ours, native = report["tools"]["tsr"], report["tools"]["tsgo"]
    report["option_differences"] = option_differences(ours["effective_config"], native["effective_config"])
    report["loaded_scope_match"] = ours["loaded_files_fingerprint"] == native["loaded_files_fingerprint"]
    reference = native["reference"]
    report["reference_gates"] = {tool: full_check_gates(data["reference"], reference, tool)
                                 for tool, data in report["tools"].items()}
    save()
    if not report["loaded_scope_match"] or not all(
        all(gates.values()) for gates in report["reference_gates"].values()
    ):
        raise RuntimeError("fixture baseline does not prove the intended loaded/checking work")

    seeded = invoke("tsgo", [*incremental, "--extendedDiagnostics"])
    report["positive_control"]["seed"] = seeded
    save()
    build_info = workspace / "cache.tsbuildinfo"
    if not build_info.exists():
        raise RuntimeError("native incremental control did not produce configured build info")
    valid = build_info.read_bytes()
    reused = invoke("tsgo", [*incremental, "--extendedDiagnostics"])
    report["positive_control"]["reuse"] = reused
    save()
    poisoned = poison_build_info(valid)
    build_info.write_bytes(poisoned)
    replayed = invoke("tsgo", [*incremental, "--extendedDiagnostics"])
    report["positive_control"]["poison_replay"] = replayed
    report["positive_control"]["verified"] = all((
        seeded["exit_code"] in (1, 2), not seeded["timed_out"],
        not seeded["stderr"],
        seeded["statistics"].get("Instantiations", 0) > 0,
        reused["exit_code"] in (1, 2), not reused["timed_out"],
        not reused["stderr"],
        reused["diagnostics"] == reference["diagnostics"],
        reused["statistics"].get("Instantiations") == 0,
        replayed["exit_code"] in (1, 2), not replayed["timed_out"],
        not replayed["stderr"],
        replayed["statistics"].get("Instantiations") == 0,
        any(POISON in entry for entry in replayed["diagnostics"]["entries"]),
    ))
    save()
    if not report["positive_control"]["verified"]:
        raise RuntimeError("positive control failed to demonstrate native cache replay")

    # Produce genuinely stale native build info from a different source, then
    # restore the current fixture. This setup is outside every timed full check.
    source_path = workspace / "schema.ts"
    original = source_path.read_bytes()
    previous = original.replace(b'cacheSentinel: number = "cache isolation"',
                                b'cacheSentinel: string = 1')
    if previous == original:
        raise RuntimeError("stale-state setup did not change the sentinel")
    try:
        source_path.write_bytes(previous)
        build_info.unlink()
        stale_seed = invoke("tsgo", [*incremental, "--extendedDiagnostics"])
        report["stale_seed"] = stale_seed
        save()
        if stale_seed["timed_out"] or stale_seed["exit_code"] not in (1, 2):
            raise RuntimeError("native did not complete stale-state setup")
        stale = build_info.read_bytes()
        if stale_seed["diagnostics"] == reference["diagnostics"]:
            raise RuntimeError("stale build info did not store different source diagnostics")
    finally:
        source_path.write_bytes(original)
    report["stale_source_restored"] = sha256(source_path) == input_hashes["schema.ts"]
    save()

    for state in STATES:
        for name in ("cache.tsbuildinfo", "tsconfig.tsbuildinfo"):
            path = workspace / name
            if state == "absent":
                path.unlink(missing_ok=True)
            else:
                path.write_bytes({"valid_incremental": valid, "stale_incremental": stale,
                                  "poisoned_incremental": poisoned,
                                  "malformed": b"not valid build info\n"}[state])
        record = {"initial_build_state": build_state(workspace), "pairs": [],
                  "samples": {tool: [] for tool in binaries}}
        report["states"][state] = record
        save()
        for index in range(args.samples):
            order = ["tsr", "tsgo"] if index % 2 == 0 else ["tsgo", "tsr"]
            for tool in order:
                sample = invoke(tool, [*full, "--extendedDiagnostics"])
                sample["gates"] = full_check_gates(sample, report["tools"][tool]["reference"], tool)
                record["samples"][tool].append(sample)
                save()  # Preserve a failed check before reporting or aborting.
                if not all(sample["gates"].values()):
                    raise RuntimeError(f"{tool} failed cache-isolation gates in {state}: {sample['gates']}")
            record["pairs"].append({"order": order})
            save()
        record["summary"] = {tool: summary(samples) for tool, samples in record["samples"].items()}
        save()
        print(f"{state}: {args.samples} fresh-process pairs passed", file=sys.stderr, flush=True)
    report["inputs_unchanged"] = all(sha256(workspace / name) == value for name, value in input_hashes.items())
    report["loaded_inputs_unchanged"] = all(
        data["input_fingerprint"] == input_fingerprint(data["loaded_files"])
        for data in report["tools"].values()
    )
    report["binaries_unchanged"] = all(sha256(binaries[tool]) == data["binary_sha256"]
                                       for tool, data in report["tools"].items())
    report["cache_isolation_verified"] = all(report[key] for key in (
        "inputs_unchanged", "loaded_inputs_unchanged", "binaries_unchanged",
        "stale_source_restored",
    ))
    save()
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tsr", type=Path, default=ROOT / "target/release/tsr")
    parser.add_argument("--tsgo", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path, required=True, help="new directory exclusively owned by this run")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--mode", choices=("default", "single"), default="default")
    args = parser.parse_args()
    if args.samples < 5 or args.timeout <= 0:
        parser.error("cache proof needs at least five pairs and a positive timeout")
    try:
        report = run(args)
    except (OSError, RuntimeError, ValueError) as error:
        print(f"cache-isolation proof failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps({"cache_isolation_verified": report["cache_isolation_verified"],
                      "target_verified": report["target_verified"]}))
    return 0 if report["cache_isolation_verified"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
