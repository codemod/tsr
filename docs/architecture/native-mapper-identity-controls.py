#!/usr/bin/env python3
"""Pin public mapper semantics separately from private Go identity/work counts."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from benchmark_inputs import file_hash, snapshot
from checker_work_trace import validate_worker_activity
from whole_project_perf import process

# Reuse the already qualified process/trace binding, not the producer's claims.
spec = importlib.util.spec_from_file_location("lifecycle_controls", HERE / "worker-lifecycle-controls.py")
lifecycle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lifecycle)
require = lifecycle.require
write = lifecycle.write

PIN = "5b1047d10d32e7d5b446be4de56b126ff42f82bb"
NEGATIVES = [
    'const badOrdered: number = ordered.first;',
    'const badReordered: string = reordered.first;',
    'const badComposition: number = nested.box.value;',
    'const badNominal: Left.Token = right;',
    'const badShadow: string = shadow.inner(1);',
    'const badInference: number = inferredString;',
    'const badReceiver: string = new Derived().next().own;',
    'const badMapped: number = deepString.next.next.value;',
    'const badConditional: number = flattened;',
]
NATIVE_PATHS = ["internal/checker/" + name + ".go" for name in
                ("checker", "mapper", "inference", "types", "utilities")] + ["internal/ast/symbol.go"]
PROBE_PATHS = ["cmd/tsgo/main.go", "internal/compiler/checkerpool.go", "internal/checker/checker.go",
               "internal/workactivity/activity.go", "internal/workactivity/activity_test.go",
               "internal/compiler/worker_activity_test.go"]


def result(child):
    require(not child["timed_out"], "compiler timed out")
    return tuple(child[key] for key in ("stdout", "stderr", "exit_code"))


def qualify_sources(args):
    qualification = json.loads((HERE / "worker-lifecycle-qualification.json").read_text())
    patch = HERE / "native-worker-activity.patch"
    require(qualification["native_source_sha"] == PIN, "qualification pin changed")
    require(file_hash(patch) == qualification["native_patch_sha256"], "qualification patch changed")
    require(file_hash(args.native_baseline) == qualification["native_baseline_sha256"], "unqualified baseline binary")
    require(file_hash(args.native_probe) == qualification["binary_sha256"]["native"], "unqualified probe binary")
    originals = list(dict.fromkeys(NATIVE_PATHS + PROBE_PATHS[:3]))
    with tempfile.TemporaryDirectory(prefix="mapper-source-control-") as directory:
        expected = Path(directory)
        for path in originals:
            pinned = subprocess.check_output(["git", "-C", str(ROOT / "vendor/typescript-go"), "show", PIN + ":" + path])
            require(file_hash(args.native_source / path) == hashlib.sha256(pinned).hexdigest(), "native anchor source changed: " + path)
            destination = expected / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(pinned)
        subprocess.run(["git", "apply", str(patch)], cwd=expected, check=True, capture_output=True)
        for path in PROBE_PATHS:
            require(file_hash(args.probe_source / path) == file_hash(expected / path), "probe source changed: " + path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("native-baseline", "native-probe", "native-source", "probe-source", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    qualify_sources(args)
    args.output.mkdir(parents=True, exist_ok=False)
    source_paths = [args.native_source / path for path in dict.fromkeys(NATIVE_PATHS + PROBE_PATHS[:3])]
    source_paths += [args.probe_source / path for path in PROBE_PATHS]
    source_paths += [Path(__file__), HERE / "native-mapper-identity.ts", HERE / "native-mapper-identity-test.go",
                     HERE / "worker-lifecycle-controls.py", HERE / "native-worker-activity.patch",
                     HERE / "worker-lifecycle-qualification.json",
                     *[ROOT / "scripts" / (name + ".py") for name in
                       ("checker_work_trace", "benchmark_inputs", "whole_project_perf")]]
    sources = {str(path.resolve()): file_hash(path) for path in source_paths}
    baseline, probe = args.native_baseline.resolve(), args.native_probe.resolve()
    report = {"native_sha": PIN, "source_files_sha256": sources,
              "binary_sha256": {"baseline": file_hash(baseline), "probe": file_hash(probe)},
              "cases": [], "private_mapper_counts_measured_by_cli": False,
              "actual_checked_work_verified": False, "target_verified": False}
    fixture = (HERE / "native-mapper-identity.ts").read_text()
    default_output = {}
    for variant in ("positive", "negative"):
        project = args.output / variant
        project.mkdir()
        (project / "main.ts").write_text(fixture + ("\n".join(NEGATIVES) + "\n" if variant == "negative" else ""))
        write(project / "tsconfig.json", {"compilerOptions": {"strict": True, "noEmit": True,
              "skipLibCheck": True, "target": "ES2022"}, "files": ["main.ts"]})
        for mode, flags in (("default", []), ("single", ["--singleThreaded"])):
            name = variant + "-" + mode
            command = [str(probe), "--project", str(project / "tsconfig.json"), "--pretty", "false",
                       "--noEmit", "--incremental", "false", "--composite", "false", "--listFiles", *flags]
            lifecycle.environment("native", None)
            show = process([*command, "--showConfig"], project, 60)
            require(show["exit_code"] == 0 and not show["stderr"] and not show["timed_out"], "showConfig failed")
            config = json.loads(show["stdout"])
            original = process([str(baseline), *command[1:]], project, 60)
            off = process(command, project, 60)
            require(result(original) == result(off), "probe-off changed complete output")
            diagnostics = re.findall(r"main\.ts\((\d+),(\d+)\): error TS(\d+):", original["stdout"])
            expected = [(str(fixture.count("\n") + i + 1), "7", "2322") for i in range(len(NEGATIVES))]
            require(not original["stderr"], "unexpected compiler stderr")
            require(diagnostics == (expected if variant == "negative" else []), "unexpected diagnostics: " + str(diagnostics))
            require(original["exit_code"] == (1 if variant == "negative" else 0), "unexpected compiler status")
            if mode == "default":
                default_output[variant] = result(original)
            else:
                require(result(original) == default_output[variant], "worker mode changed complete output")
            paths = [str(path.resolve()) for path in project.iterdir() if path.is_file()]
            paths += lifecycle.loaded(original) + [str(baseline), str(probe), *sources]
            samples = []
            for observation in ("on", "repeat"):
                trace = args.output / (name + "-" + observation + ".ndjson")
                before = snapshot(paths)
                lifecycle.environment("native", trace)
                child = process(command, project, 60)
                after = snapshot(paths)
                require(result(child) == result(off), "observer changed complete output")
                receipt = lifecycle.binding(child, trace, "native", PIN, sources, before, after,
                                            config, project, lifecycle.loaded(original))
                verdict = validate_worker_activity(trace, receipt, "native")
                require(verdict["worker_activity_valid"], str(verdict["reasons"]))
                write(trace.with_suffix(".receipt.json"), receipt)
                write(trace.with_suffix(".result.json"), verdict)
                samples.append({"child": child, "trace_sha256": file_hash(trace), "verdict": verdict})
            report["cases"].append({"name": name, "config": config, "preflight": show,
                                    "original": original, "off": off, "samples": samples})
            print(name, "diagnostics", len(diagnostics), "checkers", samples[0]["verdict"]["checker_instances_created"], flush=True)
    lifecycle.environment("native", None)
    write(args.output / "receipt.json", report)


if __name__ == "__main__":
    main()
