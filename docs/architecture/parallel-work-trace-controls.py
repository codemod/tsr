#!/usr/bin/env python3
"""Physical CLI controls for tsr-1yb.1.2.3.2.4; no throughput acceptance."""

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))
from benchmark_inputs import snapshot
from checker_work_trace import validate_trace, validate_worker_activity
from whole_project_perf import process


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(binary, command, project, trace=None):
    previous = os.environ.pop("TSR_WORK_TRACE", None)
    try:
        if trace is not None:
            os.environ["TSR_WORK_TRACE"] = str(trace)
        return process([str(binary), *command], project, 300)
    finally:
        os.environ.pop("TSR_WORK_TRACE", None)
        if previous is not None:
            os.environ["TSR_WORK_TRACE"] = previous


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    binary, output = args.binary.resolve(), args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    project = output / "project"
    project.mkdir()
    fixture = {
        "tsconfig.json": json.dumps({"compilerOptions": {"target": "es2022", "module": "esnext",
            "moduleResolution": "bundler", "strict": True, "skipLibCheck": True, "noLib": True,
            "resolveJsonModule": True, "esModuleInterop": True, "noEmit": True}, "files": ["index.ts"]}),
        "index.ts": "import data from './data.json'; import type { A } from './a'; import type { Decl } from './types'; const wrong: string = data.value; export const use: A<Decl> | null = null;",
        "a.ts": "import type { B } from './b'; export interface A<T> { other?: B<T>; }",
        "b.ts": "import type { A } from './a'; export interface B<T> { other?: A<T>; }",
        "types.d.ts": "export interface Decl { value: string; }",
        "data.json": '{"value":17}',
    }
    for name, text in fixture.items():
        (project / name).write_text(text)
    sources = [REPO / name for name in ("crates/tsr-execute/src/work_trace.rs",
        "crates/tsr-execute/src/checker_pool.rs", "crates/tsr-execute/src/compile.rs",
        "scripts/checker_work_trace.py", "docs/architecture/parallel-work-trace-controls.py")]
    source_hashes = {str(path): digest(path) for path in sources}
    binary_hash = digest(binary)
    modes = [("default", []), ("single", ["--singleThreaded"]),
        ("two", ["--checkers", "2"]), ("four", ["--checkers", "4"]),
        ("zero-clamp", []), ("negative-clamp", []),
        ("file-clamp", ["--checkers", "999"]),
        ("single-precedence", ["--singleThreaded", "--checkers", "4"]),
        ("noCheck", ["--checkers", "2", "--noCheck"]),
        ("declarations", ["--skipLibCheck", "false"]), ("inventory", ["--listFilesOnly"])]
    results = []
    for name, flags in modes:
        # Worker options are intentionally absent from showConfig. Bind these
        # fixture-specific requests to the written config and explicit flags.
        workers = {}
        config_name = "tsconfig.json"
        if name in ("zero-clamp", "negative-clamp"):
            config_name = f"tsconfig-{name}.json"
            written = json.loads(fixture["tsconfig.json"])
            workers["checkers"] = 0 if name == "zero-clamp" else -1
            written["compilerOptions"].update(workers)
            (project / config_name).write_text(json.dumps(written))
        if "--checkers" in flags:
            workers["checkers"] = int(flags[flags.index("--checkers") + 1])
        if "--singleThreaded" in flags:
            workers["singleThreaded"] = True
        inputs = snapshot([str(project / item) for item in set(fixture) | {config_name}])
        base = ["--project", config_name, "--pretty", "false"]
        config_child = run(binary, [*base, *flags, "--showConfig"], project)
        assert config_child["exit_code"] == 0 and not config_child["timed_out"], config_child
        config = json.loads(config_child["stdout"])
        listing_flags = [flag for flag in flags if flag != "--listFilesOnly"]
        listing = run(binary, [*base, *listing_flags, "--listFilesOnly"], project)
        assert listing["exit_code"] == 0 and not listing["timed_out"], listing
        loaded = listing["stdout"].splitlines()
        children, streams, qualifications = [], [], []
        for role in ("off", "on", "repeat"):
            trace = None if role == "off" else output / f"{name}-{role}.ndjson"
            child = run(binary, [*base, *flags], project, trace)
            children.append(child)
            assert child["exit_code"] in (0, 1, 2) and not child["timed_out"], child
            if trace is None:
                continue
            rows = [json.loads(line) for line in trace.read_text().splitlines()]
            streams.append(rows)
            receipt = {"schema_version": 1, "current_directory": str(project), "child": child,
                "invocation_id": rows[0]["invocation_id"], "source_sha": None,
                "binary_sha256": binary_hash, "source_files_sha256": source_hashes,
                "inputs_before": inputs, "inputs_after": snapshot([row["path"] for row in inputs]),
                "show_config": config, "loaded_files": loaded,
                "requested_checkers": workers.get("checkers"),
                "requested_single_threaded": workers.get("singleThreaded"),
                "trace_sha256": digest(trace)}
            integrity = validate_trace(trace, receipt, inventory_only=name == "inventory")
            activity = validate_worker_activity(trace, receipt, "tsr")
            assert integrity["artifact_integrity_valid"], integrity
            assert activity["worker_activity_valid"], activity
            (output / f"{name}-{role}-receipt.json").write_text(json.dumps(receipt, indent=2))
            qualifications.append({"integrity": integrity, "activity": activity})
        assert len({(c["exit_code"], c["stdout"], c["stderr"]) for c in children}) == 1
        def work(rows):
            return Counter((r["checker_id"], r["operation"], tuple(r["file_ids"]),
                tuple(r["unmapped_source_node_ids"])) for r in rows if r["event"] == "work_begin")
        assert work(streams[0]) == work(streams[1])
        inventory = lambda rows: [{k: v for k, v in r.items() if k != "recorded_at_ns"}
            for r in rows if r["event"] == "program_file"]
        assert inventory(streams[0]) == inventory(streams[1])
        results.append({"mode": name, "flags": flags, "worker_requests": workers,
            "preflight_children": [config_child, listing],
            "children": children, "qualifications": qualifications})
        print(name, "passed", flush=True)
        (output / "controls.json").write_text(json.dumps({"source_head": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip(), "source_files_sha256": source_hashes,
            "binary_sha256": binary_hash, "fixture_sha256": {n: digest(project / n) for n in fixture},
            "results": results, "observer_cost": "Raw unpaired off/on wall/CPU/RSS only; no throughput claim.",
            "actual_checked_work_verified": False, "target_verified": False}, indent=2))
    assert digest(binary) == binary_hash
    assert source_hashes == {str(path): digest(path) for path in sources}


if __name__ == "__main__":
    main()
