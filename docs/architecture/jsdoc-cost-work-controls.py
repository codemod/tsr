#!/usr/bin/env python3
"""Qualify observed full-file workers separately from intrusive cost timings."""
import argparse
import importlib.util
import os
from pathlib import Path
import sys

sys.dont_write_bytecode = True
HERE = Path(__file__).parent


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


cost = load("cost", HERE / "jsdoc-cost-controls.py")
lifecycle = load("lifecycle", HERE / "worker-lifecycle-controls.py")
from checker_work_trace import validate_trace


def run(args):
    args.output.mkdir(parents=True, exist_ok=False)
    paths = ["crates/tsr-checker/src/work_trace.rs", "crates/tsr-execute/src/work_trace.rs",
             "crates/tsr-execute/src/os_system.rs", "crates/tsr-execute/src/compile.rs",
             "crates/tsr-parser/src/jsdoc_cost.rs", "crates/tsr-parser/src/jsdoc.rs",
             "crates/tsr-compiler/src/lib.rs", "crates/tsr-compiler/src/loader.rs"]
    source_hashes = {str(args.source.resolve() / name): cost.file_hash(args.source / name) for name in paths}
    for key in ("TSR_WORK_TRACE", "TSR_NATIVE_WORK_ACTIVITY", "TSR_JSDOC_COST_PROBE"):
        os.environ.pop(key, None)
    baseline = cost.measure(args.binary, args.project, args.output / "baseline.json", repeats=1)
    inputs = [str(Path(name).resolve()) for name in baseline["children"][0]["loaded"] if name.startswith("/")]
    inputs.append(str(args.project.resolve() / "tsconfig.json"))
    answers = []
    for mode in ("off", "on"):
        work = args.output.resolve() / f"work-{mode}.ndjson"
        doc = args.output.resolve() / "doc.tsv"
        os.environ["TSR_WORK_TRACE"] = str(work)
        if mode == "on":
            os.environ["TSR_JSDOC_COST_PROBE"] = str(doc)
        before = cost.snapshot(inputs)
        child = cost.process(baseline["command"], args.project, 180)
        after = cost.snapshot(inputs)
        os.environ.pop("TSR_WORK_TRACE", None)
        os.environ.pop("TSR_JSDOC_COST_PROBE", None)
        receipt = lifecycle.binding(child, work, "tsr", args.source_sha, source_hashes, before, after,
                                    baseline["config"], args.project, lifecycle.loaded(child))
        verdict = validate_trace(work, receipt)
        cost.require(verdict["artifact_integrity_valid"], verdict["reasons"])
        cost.require(cost.diagnostics(child["stdout"] + child["stderr"], args.project) ==
                     baseline["children"][0]["diagnostics"], "trace changes complete diagnostics")
        cost.require(cost.counts(child) == baseline["children"][0]["counts"], "trace changes reported scope")
        answer = {"mode": mode, "child": child, "receipt": receipt, "verdict": verdict}
        if mode == "on":
            answer["probe"] = cost.read_probe(doc, child)
        answers.append(answer)
        cost.write(args.output / "results.json", answers)
    cost.require(answers[0]["verdict"]["checked_file_ids"] == answers[1]["verdict"]["checked_file_ids"],
                 "observed checking order changes")
    cost.require(all(cost.file_hash(Path(path)) == digest for path, digest in source_hashes.items()),
                 "observer source changes")
    print({"observed_full_file_workers": len(answers[0]["verdict"]["checked_file_ids"]),
           "all_forcing_verified": False})


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ("binary", "source", "project", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--source-sha", required=True)
    run(parser.parse_args())
