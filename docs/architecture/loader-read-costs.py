"""Measure a fixed physical read plan; no whole-project or native speed claim."""

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import statistics
import subprocess
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("perf", ROOT / "scripts/whole_project_perf.py")
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "manifest", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error("output must be a new file")
    binary = args.binary.resolve()
    manifest = args.manifest.resolve()
    paths = manifest.read_text().splitlines()
    if not paths or any(not Path(p).is_absolute() or "\t" in p for p in paths):
        parser.error("manifest must contain absolute physical paths without tabs")
    state = {
        "kind": "predetermined read-plan cost; not discovery, parsing or checking",
        "source": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "driver_sha256": hashlib.sha256((ROOT / "crates/tsr-compiler/examples/loader_reads.rs").read_bytes()).hexdigest(),
        "harness_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "manifest_sha256": hashlib.sha256(manifest.read_bytes()).hexdigest(),
        "plan_count": len(paths),
        "plan_fingerprint": perf.fingerprint(paths),
        "input_fingerprint_before": perf.input_fingerprint(paths),
        "protocol": "two rounds, one warmup per mode then five rotated samples per mode; serial processes",
        "runs": [],
        "native_target_verified": False,
    }

    def save():
        output.write_text(json.dumps(state, indent=2) + "\n")
        assert json.loads(output.read_text()) == state

    save()
    modes = ["direct", "1", "2", "4"]
    expected_output = None
    for round_index in range(2):
        for sample in range(-1, 5):
            shift = 0 if sample == -1 else (sample + round_index) % 4
            for mode in modes[shift:] + modes[:shift]:
                result = perf.process([str(binary), str(manifest), mode], ROOT, 60)
                row = {k: result[k] for k in ("wall_seconds", "user_seconds", "system_seconds",
                                             "peak_rss_bytes", "exit_code", "timed_out")}
                row.update(round=round_index + 1, mode=mode, warmup=sample == -1)
                state["runs"].append(row)
                save()
                assert not result["timed_out"] and result["exit_code"] == 0
                stdout = result["stdout"]
                if expected_output is None:
                    expected_output = stdout
                assert stdout == expected_output
                assert len(stdout.splitlines()) == len(paths)
                metrics = [line.split("\t")[1:] for line in result["stderr"].splitlines()
                           if line.startswith("preparation\t")]
                workers = [line.split("\t")[1:] for line in result["stderr"].splitlines()
                           if line.startswith("worker\t")]
                assert len(metrics) == 1 and len(metrics[0]) == 6
                m = metrics[0]
                row.update(output_fingerprint=hashlib.sha256(stdout.encode()).hexdigest(),
                           workers=int(m[0]), reads=int(m[1]), failures=int(m[2]),
                           decoded_bytes=int(m[3]), preparation_seconds=float(m[4]),
                           max_batch_decoded_bytes=int(m[5]), worker_counters=[{
                               "index": int(w[0]), "reads": int(w[1]), "failures": int(w[2]),
                               "decoded_bytes": int(w[3]), "read_seconds": float(w[4]),
                           } for w in workers])
                assert row["reads"] == len(paths) and row["failures"] == 0
                assert len(workers) == row["workers"]
                assert sum(w["reads"] for w in row["worker_counters"]) == len(paths)
                save()
        print(f"round {round_index + 1} completed", flush=True)
    state["input_fingerprint_after"] = perf.input_fingerprint(paths)
    assert state["input_fingerprint_after"] == state["input_fingerprint_before"]
    state["summary"] = {}
    for round_index in (1, 2):
        state["summary"][str(round_index)] = {}
        for mode in modes:
            rows = [r for r in state["runs"] if r["round"] == round_index
                    and r["mode"] == mode and not r["warmup"]]
            state["summary"][str(round_index)][mode] = {k: {
                "median": statistics.median(r[k] for r in rows),
                "min": min(r[k] for r in rows), "max": max(r[k] for r in rows),
            } for k in ("wall_seconds", "user_seconds", "system_seconds", "peak_rss_bytes",
                        "preparation_seconds", "max_batch_decoded_bytes")}
    state["complete"] = True
    save()
    print(json.dumps(state["summary"], indent=2))


if __name__ == "__main__":
    main()
