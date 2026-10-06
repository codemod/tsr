#!/usr/bin/env python3
"""Check archived app counts against their original receipts; prove no speed claim."""

import argparse
import hashlib
import json
from pathlib import Path


def diagnostic_count(summary):
    entries = summary["entries"]
    count = summary["count"]
    if not isinstance(entries, list) or not all(isinstance(x, str) for x in entries):
        raise ValueError("diagnostic entries must be complete strings")
    if type(count) is not int or count != len(entries):
        raise ValueError("diagnostic count disagrees with entries")
    fingerprint = hashlib.sha256(json.dumps(sorted(entries), sort_keys=True).encode()).hexdigest()
    if summary["fingerprint"] != fingerprint:
        raise ValueError("diagnostic fingerprint disagrees with entries")
    return count


def validate_archive(report, raw_bytes):
    raw_hash = hashlib.sha256(raw_bytes).hexdigest()
    raw = json.loads(raw_bytes)
    if raw["complete"] is not True:
        raise ValueError("incomplete raw application receipt")
    for role in ("normal", "probe"):
        expected = report["bindings"][role]
        actual = raw["bindings"][role]
        if actual["source"] != report["source"] or any(
            actual[k] != expected[k] for k in ("source", "sha256")
        ):
            raise ValueError("raw compiler binding disagrees with archive")
    rows = report["application"]
    cases = raw["cases"]
    row_modes = [row["mode"] for row in rows]
    case_modes = [case["mode"] for case in cases]
    if not rows or len(set(row_modes)) != len(rows) or sorted(row_modes) != sorted(case_modes):
        raise ValueError("missing or duplicate application mode")
    summaries = []
    for row in rows:
        hash_keys = [key for key in ("report_sha256", "raw_report_sha256") if key in row]
        if not hash_keys or any(row[key] != raw_hash for key in hash_keys):
            raise ValueError("raw application hash disagrees with archive")
        case = next(case for case in cases if case["mode"] == row["mode"])
        runs = case["runs"]
        if case["complete"] is not True or not runs or len(runs) != row["compiler_children"]:
            raise ValueError("incomplete application children")
        diagnostics = runs[0]["scope"]["diagnostics"]
        for run in runs:
            if run["timed_out"] or run["exit_code"] < 0:
                raise ValueError("failed application child")
            diagnostic_count(run["scope"]["diagnostics"])
            if run["scope"]["diagnostics"] != diagnostics:
                raise ValueError("application diagnostic output changed")
        count = diagnostic_count(diagnostics)
        if type(row["diagnostic_count"]) is not int or row["diagnostic_count"] != count:
            raise ValueError("archive diagnostic count disagrees with raw entries")
        summaries.append({"mode": row["mode"], "diagnostic_count": count,
                          "diagnostic_fingerprint": diagnostics["fingerprint"]})
    return {"raw_report_sha256": raw_hash, "application": summaries,
            "qualification": "diagnostic counts only; no performed-work or speed qualification"}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--raw", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(validate_archive(json.loads(args.report.read_text()), args.raw.read_bytes()), indent=2))
