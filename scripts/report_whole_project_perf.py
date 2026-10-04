#!/usr/bin/env python3
"""Publish observational CI evidence using the existing whole-project harness.

The public smoke workload tests report plumbing. Its timings cannot verify the
representative release target or activate the performance ratchet.
"""

from __future__ import annotations

import argparse
import html
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import sys

import whole_project_perf as perf


MODES = ("default", "single")
RESOURCES = ("wall_seconds", "user_seconds", "system_seconds", "peak_rss_bytes")


def completed_samples(tool: dict) -> list[dict]:
    return [sample for sample in tool.get("samples", [])
            if not sample.get("timed_out", True)
            and sample.get("exit_code") in (0, 1, 2)
            and sample.get("input_validation", {}).get("stable") is True
            and all(isinstance(sample.get(field), (int, float))
                    and math.isfinite(sample[field]) and sample[field] >= 0
                    for field in RESOURCES)
            and sample["wall_seconds"] > 0]


def paired_check_completed(report: dict) -> bool:
    counts = [len(completed_samples(report.get("tools", {}).get(name, {})))
              for name in ("tsr", "tsgo")]
    return (report.get("status") == "completed" and counts[0] > 0
            and counts[0] == counts[1] == len(report.get("pairs", [])))


def state(report: dict | None, exit_code: int) -> str:
    if report is None:
        return "harness_failed"
    samples = [sample for tool in report.get("tools", {}).values()
               for sample in tool.get("samples", [])]
    for field in ("rejected_measurement", "rejected_preflight"):
        if report.get(field):
            samples.append(report[field])
    if any(sample.get("timed_out") for sample in samples):
        return "timed_out"
    if any(sample.get("exit_code") not in (0, 1, 2) for sample in samples):
        return "tool_failed"
    if report.get("status") in ("inputs_changed", "invalid_inputs", "invalid_loaded_inputs",
                              "timed_out", "tool_failed"):
        return report["status"]
    if exit_code != 0 or not paired_check_completed(report):
        return "harness_failed"
    return "completed" if report.get("work_comparable") else "completed_incomparable"


def cell(value: object) -> str:
    return html.escape(str(value)).replace("|", "&#124;").replace("\n", " ")


def resource(values: list[float], scale: float = 1) -> str:
    if not values:
        return "unavailable"
    values = [value / scale for value in values]
    spread = f"{statistics.stdev(values):.4f}" if len(values) > 1 else "unavailable"
    return (f"{statistics.median(values):.4f} [{min(values):.4f}, {max(values):.4f}]"
            f"; stddev {spread}")


def render_summary(observations: dict) -> str:
    lines = ["## Whole-project observations", "",
             "Smoke workload: report plumbing; timings are not representative throughput.",
             "Release target TSR/tsgo median wall <= 0.50: **not evaluated** on this workload.",
             "Actual performed checker work, worker budgets and complete query inputs remain unverified.",
             "",
             f"Source checkout: `{cell(observations.get('source_sha'))}`",
             f"Oracle checkout: `{cell(observations.get('oracle_sha'))}`",
             f"Project config SHA256: `{cell(observations.get('project_config_sha256'))}`", ""]
    for mode, run in observations.get("runs", {}).items():
        report = run.get("report") or {}
        lines += [f"### {cell(mode)}: {cell(run['state'])}", "",
                  f"Harness exit: {cell(run.get('harness_exit_code'))}. "
                  f"Worker request: {'singleThreaded=true' if mode == 'single' else 'tool defaults'}; "
                  "actual worker count unavailable.", ""]
        if run.get("error"):
            lines += [cell(run["error"]), ""]
        if run.get("harness_exit_code") == 0 and paired_check_completed(report):
            ours = completed_samples(report["tools"]["tsr"])
            theirs = completed_samples(report["tools"]["tsgo"])
            ratio = (statistics.median(s["wall_seconds"] for s in ours)
                     / statistics.median(s["wall_seconds"] for s in theirs))
            lines += [f"Observed wall ratio: {ratio:.4f} (observation only).", ""]
        lines += ["| Tool | Complete samples | Wall seconds median [min, max] | CPU seconds median [min, max] | RSS MiB median [min, max] |",
                  "| --- | --- | --- | --- | --- |"]
        for name, tool in report.get("tools", {}).items():
            samples = completed_samples(tool)
            lines += [f"| {cell(name)} | {len(samples)}/{len(tool.get('samples', []))} | "
                      f"{resource([s['wall_seconds'] for s in samples])} | "
                      f"{resource([s['user_seconds'] + s['system_seconds'] for s in samples])} | "
                      f"{resource([s['peak_rss_bytes'] for s in samples], 1024 * 1024)} |"]
        lines.append("")
        for name, tool in report.get("tools", {}).items():
            samples = tool.get("samples", [])
            diag = samples[0].get("diagnostics", {}) if samples else {}
            lines += [f"- {cell(name)} binary SHA256: `{cell(tool.get('binary_sha256'))}`; "
                      f"loaded files: {cell(tool.get('loaded_file_count'))}; "
                      f"loaded-scope SHA256: `{cell(tool.get('loaded_files_fingerprint'))}`; "
                      f"loaded-input SHA256: `{cell(tool.get('input_fingerprint'))}`; "
                      f"diagnostics: {cell(diag.get('count'))}; "
                      f"complete-diagnostic SHA256: `{cell(diag.get('fingerprint'))}`."]
        lines += ["", f"Work comparable: {cell(report.get('work_comparable', False))}."]
        lines += [f"- {cell(reason)}" for reason in report.get("comparability_reasons", [])]
        lines += ["", "Full diagnostics, child identities, flags, input observations and available scope evidence are in the JSON artifacts.", ""]
    return "\n".join(lines)


def save(output: Path, observations: dict) -> None:
    temporary = output / "observations.json.tmp"
    temporary.write_text(json.dumps(observations, indent=2) + "\n")
    temporary.replace(output / "observations.json")
    (output / "summary.md").write_text(render_summary(observations))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", type=Path, required=True)
    parser.add_argument("--tsr", type=Path, required=True)
    parser.add_argument("--tsgo", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--builds-ready", choices=("true", "false"), default="true")
    args = parser.parse_args()
    if args.samples < 1 or not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("samples and timeout must be positive")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    project, tsr, tsgo = (path.resolve() for path in (args.project, args.tsr, args.tsgo))
    paths = perf.inputs.snapshot([str(project), str(tsr), str(tsgo)])
    by_path = {row["path"]: row for row in paths}
    observations = {
        "schema_version": 1, "workload_kind": "smoke-plumbing",
        "source_sha": perf.revision(perf.ROOT),
        "oracle_sha": perf.revision(perf.ROOT / "vendor/typescript-go"),
        "project": str(project), "project_config_sha256": by_path[str(project)].get("sha256"),
        "setup_inputs": paths, "samples_requested": args.samples,
        "builds_ready": args.builds_ready == "true",
        "reporter_sha256": perf.inputs.file_hash(Path(__file__)),
        "cache_state": "fresh compiler processes, warmed filesystem; incremental/composite disabled",
        "worker_environment": {name: os.environ.get(name) for name in
                               ("GOMAXPROCS", "GOMEMLIMIT", "GOGC", "RAYON_NUM_THREADS")},
        "release_target_wall_ratio": 0.5, "release_target_verified": False, "runs": {},
    }
    # Outputs belong to this invocation. A failed preflight must not reuse a
    # previous run's completed JSON, including when the build step was skipped.
    for mode in MODES:
        (output / f"{mode}.json").unlink(missing_ok=True)
    save(output, observations)
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("TSR_", "TSGO_"))}
    environment.update(TSR_LIB_PATH=str(perf.ROOT / "vendor/typescript-go/internal/bundled/libs"),
                       PYTHONDONTWRITEBYTECODE="1")
    for mode in MODES:
        report_path = output / f"{mode}.json"
        run = {"state": "setup_failed", "harness_exit_code": None, "report": None,
               "error": "Compiler build/setup did not succeed.", "report_path": str(report_path)}
        stdout = stderr = ""
        if args.builds_ready == "true":
            command = [sys.executable, str(perf.ROOT / "scripts/whole_project_perf.py"),
                       "--project", str(project), "--tsr", str(tsr), "--tsgo", str(tsgo),
                       "--samples", str(args.samples), "--warmups", "1", "--timeout", str(args.timeout),
                       "--mode", mode, "--output", str(report_path)]
            child = subprocess.run(command, capture_output=True, text=True, env=environment)
            stdout, stderr = child.stdout, child.stderr
            error = None
            try:
                report = json.loads(report_path.read_text())
                if not isinstance(report, dict):
                    raise ValueError("Harness JSON is not an object")
            except (OSError, ValueError) as exc:
                report, error = None, str(exc)
            run.update(state=state(report, child.returncode), harness_exit_code=child.returncode,
                       report=report, error=error)
        (output / f"{mode}.stdout.log").write_text(stdout)
        (output / f"{mode}.stderr.log").write_text(stderr)
        observations["runs"][mode] = run
        save(output, observations)
        print(f"{mode}: {run['state']}", flush=True)
    return int(any(run["state"] not in ("completed", "completed_incomparable")
                   for run in observations["runs"].values()))


if __name__ == "__main__":
    raise SystemExit(main())
