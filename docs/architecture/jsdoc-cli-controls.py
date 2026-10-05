#!/usr/bin/env python3
"""Physical JSDoc diagnostics/eligibility controls; not a speed benchmark."""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "jsdoc_cost", Path(__file__).with_name("jsdoc-cost-controls.py")
)
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)


def run(args):
    args.output.mkdir(parents=True, exist_ok=False)
    for key in list(os.environ):
        if key.startswith("TSR_"):
            os.environ.pop(key)
    source = """/** @type {number} */
export const badNumber = 'wrong';
/** @type {string} */
export const badString = 1;
/** @type {[number, string]} */
export const goodPair = [1, 'ok'];
"""
    base = {"allowJs": True, "checkJs": True, "strict": True,
            "skipLibCheck": True, "target": "es2022"}
    report = {"schema": "tsr-jsdoc-cli-controls-v1", "cases": {},
              "speed_qualified": False, "full_input_and_forcing_equivalence": False}
    if args.identity:
        report["source_identity"] = json.loads(args.identity.read_text())
    modes = [("baseline", args.baseline, []), ("candidate", args.candidate, []),
             ("candidateSingle", args.candidate, ["--singleThreaded"]),
             ("native", args.native, []),
             ("nativeSingle", args.native, ["--singleThreaded"]),
             ("native2", args.native, ["--checkers", "2"]),
             ("native4", args.native, ["--checkers", "4"])]
    modes.extend((f"worker{n}", args.helper, [str(n)]) for n in (1, 2, 4))
    for name, options, preamble in (
        ("checked", base, ""), ("unchecked", {**base, "checkJs": False}, ""),
        ("noCheck", {**base, "noCheck": True}, ""),
        ("fileNoCheck", base, "// @ts-nocheck\n"),
        ("libraries", {**base, "skipLibCheck": False}, ""),
    ):
        project = args.output / name
        project.mkdir()
        (project / "a.js").write_text(preamble + source)
        (project / "tsconfig.json").write_text(
            json.dumps({"compilerOptions": options, "files": ["a.js"]}, indent=2) + "\n"
        )
        inputs = cost.snapshot([str(project / "a.js"), str(project / "tsconfig.json")])
        rows = []
        report["cases"][name] = {"inputs": inputs, "results": rows}
        for mode, binary, flags in modes:
            command = ([str(binary), str(project / "tsconfig.json"), *flags]
                       if mode.startswith("worker") else
                       [str(binary), "--project", str(project / "tsconfig.json"),
                        "--noEmit", "--incremental", "false", "--composite", "false",
                        "--pretty", "false", "--listFiles", *flags])
            if mode.startswith("candidate") or mode == "baseline":
                command.append("--extendedDiagnostics")
            before = cost.file_hash(binary)
            child = cost.process(command, project, 180)
            rows.append({"mode": mode, "binary_sha256": before, "child": child,
                         "diagnostics": cost.diagnostics(child["stdout"] + child["stderr"], project)})
            cost.write(args.output / "results.json", report)
            cost.require(not child["timed_out"] and child["exit_code"] in (0, 1, 2),
                         "incomplete CLI control: " + mode)
            cost.require(cost.file_hash(binary) == before and
                         cost.snapshot([str(project / "a.js"), str(project / "tsconfig.json")]) == inputs,
                         "control binary/input drift")
        native = next(row["diagnostics"] for row in rows if row["mode"] == "native")
        cost.require(native["count"] == (2 if name in ("checked", "libraries") else 0),
                     "native control diagnostic count changed")
        cost.require(all(row["diagnostics"] == native for row in rows[1:]),
                     "complete native diagnostic mismatch: " + name)
        checked = cost.counts(rows[1]["child"])["Checked files"]
        cost.require(cost.counts(rows[2]["child"])["Checked files"] == checked,
                     "CLI mode checking count mismatch")
        for row in rows:
            if not row["mode"].startswith("worker"):
                continue
            lines = row["child"]["stderr"].splitlines()
            identities = [line.removeprefix("CHECKED\t") for line in lines if line.startswith("CHECKED\t")]
            workers = [line.split("\t") for line in lines if line.startswith("WORKER\t")]
            cost.require(len(identities) == checked and len(set(identities)) == checked,
                         "helper checked identities/count mismatch")
            cost.require(sum(int(fields[2]) for fields in workers) == checked,
                         "helper executed check count mismatch")
        report["cases"][name]["complete_diagnostics_match"] = True
        report["cases"][name]["cli_and_helper_checked_count"] = checked
        cost.write(args.output / "results.json", report)
        print(json.dumps({"case": name, "baseline": rows[0]["diagnostics"]["count"],
                          "native_candidate_helpers": native["count"], "checked": checked}), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("baseline", "candidate", "native", "helper", "output"):
        parser.add_argument("--" + name, type=lambda value: Path(value).resolve(), required=True)
    parser.add_argument("--identity", type=Path)
    run(parser.parse_args())


if __name__ == "__main__":
    main()
