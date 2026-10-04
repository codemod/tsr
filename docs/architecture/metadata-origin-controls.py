"""Check scope and metadata origin counts; this is attribution, not a speed gate."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("perf", ROOT / "scripts/whole_project_perf.py")
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("normal", "scope", "probe", "project", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    if out.exists():
        parser.error("output must be a new directory")
    out.mkdir(parents=True)
    config = args.project.resolve()
    cwd = config.parent
    binaries = {"normal": args.normal.resolve(), "scope": args.scope.resolve(),
                "disabled": args.probe.resolve(), "enabled": args.probe.resolve(),
                "repeat": args.probe.resolve()}
    flags = ["--project", str(config), "--noEmit", "--incremental", "false",
             "--composite", "false", "--pretty", "false"]
    state = {"kind": "metadata origin attribution; no speed claim",
             "binary_sha256": {role: hashlib.sha256(p.read_bytes()).hexdigest()
                               for role, p in binaries.items()},
             "driver_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
             "config_sha256_before": hashlib.sha256(config.read_bytes()).hexdigest(),
             "flags": flags, "runs": [], "config_controls": []}

    def save():
        p = out / "results.json"
        p.write_text(json.dumps(state, indent=2) + "\n")
        assert json.loads(p.read_text()) == state

    def run(binary, command, profile=False):
        saved = dict(os.environ)
        env = {k: v for k, v in saved.items() if not k.startswith("TSR_")}
        env["TSR_LIB_PATH"] = str(ROOT / "vendor/typescript-go/internal/bundled/libs")
        env["TSR_META_SCOPE"] = "1"
        if profile:
            env["TSR_META_PROFILE"] = "1"
        try:
            os.environ.clear()
            os.environ.update(env)
            result = perf.process([str(binary)] + command, cwd, 90)
        finally:
            os.environ.clear()
            os.environ.update(saved)
        assert not result["timed_out"] and result["exit_code"] in (0, 1, 2)
        return result

    save()
    listing = run(binaries["normal"], flags + ["--listFilesOnly"])
    paths = listing["stdout"].splitlines()
    assert paths and all(Path(p).is_absolute() and Path(p).is_file() for p in paths)
    state.update(loaded_paths=paths, loaded_order_fingerprint=perf.fingerprint(paths),
                 input_fingerprint_before=perf.input_fingerprint(paths))
    save()

    expected_diagnostics = None
    expected_checked = None
    for role, binary in binaries.items():
        configuration = run(binary, flags + ["--showConfig"])
        effective = json.loads(configuration["stdout"])
        state["config_controls"].append({"role": role, "fingerprint": perf.fingerprint(effective)})
        if role == "normal":
            state["effective_config"] = effective
        else:
            assert effective == state["effective_config"]
        assert perf.input_fingerprint(paths) == state["input_fingerprint_before"]
        result = run(binary, flags + ["--listFiles"], profile=role in ("enabled", "repeat"))
        for stream in ("stdout", "stderr"):
            (out / f"{role}.{stream}").write_text(result[stream])
        observed_paths = []
        for line in result["stdout"].splitlines():
            if not Path(line).is_absolute() or not Path(line).is_file():
                break
            observed_paths.append(line)
        assert observed_paths == paths
        checked = [line.split("\t", 1)[1] for line in result["stderr"].splitlines()
                   if line.startswith("TSR_META_CHECKED\t")]
        counters = [json.loads(line.split("\t", 1)[1]) for line in result["stderr"].splitlines()
                    if line.startswith("TSR_META_COUNTS\t")]
        assert len(counters) == int(role in ("enabled", "repeat"))
        diagnostics = perf.diagnostics(result["stdout"], cwd)
        row = {k: result[k] for k in ("wall_seconds", "user_seconds", "system_seconds",
                                    "peak_rss_bytes", "exit_code", "timed_out")}
        row.update(role=role, diagnostics=diagnostics, checked_paths=checked,
                   loaded_order_fingerprint=perf.fingerprint(observed_paths),
                   checked_order_fingerprint=perf.fingerprint(checked) if checked else None,
                   checked_input_fingerprint=perf.input_fingerprint(checked) if checked else None,
                   counts=counters[0] if counters else None,
                   input_fingerprint_after=perf.input_fingerprint(paths))
        state["runs"].append(row)
        save()
        if expected_diagnostics is None:
            expected_diagnostics = diagnostics
        assert diagnostics == expected_diagnostics
        if role != "normal":
            assert checked
            if expected_checked is None:
                expected_checked = checked
            assert checked == expected_checked
        assert row["input_fingerprint_after"] == state["input_fingerprint_before"]
        print(json.dumps({"role": role, "loaded": len(paths), "checked": len(checked),
                          "diagnostics": diagnostics["count"]}), flush=True)

    def without_intervals(counts):
        return {"rows": [{k: v for k, v in r.items() if k != "stat_ns"}
                         for r in counts["rows"]],
                "global_unique_paths": counts["global_unique_paths"]}

    assert without_intervals(state["runs"][-2]["counts"]) == without_intervals(
        state["runs"][-1]["counts"])
    state["config_sha256_after"] = hashlib.sha256(config.read_bytes()).hexdigest()
    assert state["config_sha256_after"] == state["config_sha256_before"]
    state["input_fingerprint_after"] = perf.input_fingerprint(paths)
    assert state["input_fingerprint_after"] == state["input_fingerprint_before"]
    state["complete"] = True
    save()


if __name__ == "__main__":
    main()
