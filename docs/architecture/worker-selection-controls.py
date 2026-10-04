"""Validate opt-in checker selection and measure fresh 1/2/4 worker processes."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import statistics
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("perf", ROOT / "scripts/whole_project_perf.py")
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, cwd):
    original = dict(os.environ)
    environment = {k: v for k, v in original.items() if not k.startswith("TSR_")}
    environment["TSR_LIB_PATH"] = str(ROOT / "vendor/typescript-go/internal/bundled/libs")
    try:
        os.environ.clear()
        os.environ.update(environment)
        result = perf.process(command, cwd, 90)
    finally:
        os.environ.clear()
        os.environ.update(original)
    assert not result["timed_out"] and result["exit_code"] in (0, 1, 2)
    return result


def probe_records(result):
    loaded, checked, workers = [], [], []
    phase, policy = None, None
    for line in result["stderr"].splitlines():
        fields = line.split("\t")
        if fields[0] == "LOADED":
            loaded.append(fields[1])
        elif fields[0] == "CHECKED":
            checked.append(fields[1])
        elif fields[0] == "POLICY":
            policy = {"requested": fields[1], "effective": int(fields[2]), "single": fields[3] == "true"}
        elif fields[0] == "PHASE":
            phase = {"effective_workers": int(fields[1]), "available_cpus": int(fields[2]),
                     "program_seconds": float(fields[3]), "check_join_seconds": float(fields[4]),
                     "before_scope_log_seconds": float(fields[5])}
        elif fields[0] == "WORKER":
            workers.append(dict(zip(("worker", "checked", "initial_types", "final_types", "computations", "initialization_seconds", "checking_seconds"),
                                    [int(x) for x in fields[1:6]] + [float(x) for x in fields[6:8]])))
        else:
            raise AssertionError(f"unexpected probe output: {line}")
    assert phase is not None and policy is not None
    assert policy["effective"] == phase["effective_workers"] == len(workers)
    assert sum(w["checked"] for w in workers) == len(checked)
    return loaded, checked, workers, phase, policy


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")
    assert json.loads(path.read_text()) == value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("normal", "probe", "tsgo", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--project", type=Path)
    parser.add_argument("--public-only", action="store_true")
    parser.add_argument("--samples", type=int, default=5)
    args = parser.parse_args()
    assert args.samples > 0 and (args.public_only or args.project is not None)
    output = args.output.resolve()
    assert not output.exists()
    output.mkdir(parents=True)
    state = {"source": "064d40db2be28a63e3fb85ae9ebbba2aba5b4b42", "production_cli_parallel": False,
             "native_target_verified": False, "binary_sha256": {k: sha(getattr(args, k)) for k in ("normal", "probe", "tsgo")},
             "public_controls": [], "runs": [], "complete": False}
    result_path = output / "results.json"
    flags = ["--noEmit", "--incremental", "false", "--composite", "false", "--pretty", "false", "--extendedDiagnostics", "false"]

    def capture(label, command, cwd):
        result = run(command, cwd)
        for stream in ("stdout", "stderr"):
            (output / (label + "." + stream)).write_text(result[stream])
        return result

    files = {"main.ts": 'import {value} from "./dep";\nconst bad: number = value;\n',
             "dep.ts": "export declare const value: string;\n"}
    files.update({f"skip{i}.d.ts": f"interface Skipped{i} {{ value: string }}\n" for i in range(6)})
    cases = [("default", None, None, False), ("parsed-eight", 8, None, False),
             ("positional-eight", 2, 8, False), ("single-wins", 8, 8, True),
             ("parsed-one", 1, None, False), ("all-skipped", 8, None, False)]
    for name, configured, override, single in cases:
        cwd = output / name
        cwd.mkdir()
        for filename, text in files.items():
            (cwd / filename).write_text(text)
        options = {"strict": True, "skipLibCheck": True, "types": [], "target": "es2020", "singleThreaded": single}
        if configured is not None:
            options["checkers"] = configured
        roots = [f for f in files if f.endswith(".d.ts")] if name == "all-skipped" else list(files)
        project = cwd / "tsconfig.json"
        save(project, {"compilerOptions": options, "files": roots})
        probe = capture(name + "-probe", [str(args.probe.resolve()), str(project)] + ([str(override)] if override is not None else []), cwd)
        loaded, checked, workers, phase, policy = probe_records(probe)
        requested = override if override is not None else configured
        expected_workers = min(max(1, len(loaded)), 256, 1 if single else (requested or 4))
        assert policy["effective"] == expected_workers
        native_flags = ["--checkers", str(override)] if override is not None else []
        diagnostics = perf.diagnostics(probe["stdout"], cwd)
        for role, binary in (("normal", args.normal), ("native", args.tsgo)):
            observed = capture(name + "-" + role, [str(binary.resolve()), "--project", str(project), *flags, *native_flags, "--listFiles"], cwd)
            assert not observed["stderr"]
            assert perf.diagnostics(observed["stdout"], cwd) == diagnostics
            listed = [line for line in observed["stdout"].splitlines()
                      if line.endswith(".ts") and not perf.DIAGNOSTIC_START.match(line)]
            if role == "normal":
                assert listed == loaded
            else:
                assert [Path(line).name for line in listed] == [Path(line).name for line in loaded]
        expected_checked = [] if name == "all-skipped" else [str(cwd / "dep.ts"), str(cwd / "main.ts")]
        assert checked == expected_checked
        row = {"case": name, "configured": configured, "override": override, "single": single,
               "loaded_count": len(loaded), "checked_count": len(checked), "policy": policy,
               "diagnostics": diagnostics, "workers": workers, "phase": phase, "config_sha256": sha(project),
               "input_fingerprint": perf.input_fingerprint(loaded), "matched_normal_and_native": True}
        state["public_controls"].append(row)
        save(result_path, state)
    for invalid in ("0", "-1", "1.5", "many", "2147483648"):
        result = capture("invalid-" + invalid, [str(args.probe.resolve()), "/nonexistent/tsconfig.json", invalid], output)
        assert result["exit_code"] == 2 and result["stderr"].strip() == "worker count must be a positive 32-bit integer"
    state["invalid_override_controls"] = 5
    save(result_path, state)

    if not args.public_only:
        project = args.project.resolve()
        cwd = project.parent
        def cli_control(position):
            result = capture("cli-" + position, [str(args.normal.resolve()), "--project", str(project), *flags, "--listFiles"], cwd)
            assert not result["stderr"]
            loaded = [line for line in result["stdout"].splitlines() if Path(line).is_absolute() and Path(line).is_file()]
            return {"diagnostics": perf.diagnostics(result["stdout"], cwd), "loaded_paths": loaded,
                    "input_fingerprint": perf.input_fingerprint(loaded), "root_config_sha256": sha(project)}
        before = cli_control("before")
        state["cli_control"] = before
        expected_checked = None
        options_fingerprints = {}
        for mode in (1, 2, 4):
            result = capture(f"options-{mode}", [str(args.normal.resolve()), "--project", str(project), *flags, "--checkers", str(mode), "--showConfig"], cwd)
            assert result["exit_code"] == 0 and not result["stderr"]
            effective = json.loads(result["stdout"])
            assert effective["compilerOptions"].pop("checkers", mode) == mode
            options_fingerprints[str(mode)] = perf.fingerprint(effective)
        assert len(set(options_fingerprints.values())) == 1
        state["effective_options_without_worker_count"] = options_fingerprints
        for iteration in range(args.samples + 1):
            modes = (1, 2, 4)
            rotated = modes[iteration % 3:] + modes[:iteration % 3]
            for mode in rotated:
                result = capture(f"sample-{iteration}-{mode}", [str(args.probe.resolve()), str(project), str(mode)], cwd)
                loaded, checked, workers, phase, policy = probe_records(result)
                assert policy["effective"] == mode
                assert loaded == before["loaded_paths"]
                assert perf.input_fingerprint(loaded) == before["input_fingerprint"]
                assert sha(project) == before["root_config_sha256"]
                diagnostics = perf.diagnostics(result["stdout"], cwd)
                assert diagnostics == before["diagnostics"]
                if expected_checked is None:
                    expected_checked = checked
                assert checked == expected_checked
                row = {k: result[k] for k in ("wall_seconds", "user_seconds", "system_seconds", "peak_rss_bytes", "exit_code", "timed_out")}
                row.update(iteration=iteration, warmup=iteration == 0, requested=mode, workers=workers, phase=phase,
                           loaded_count=len(loaded), checked_count=len(checked), loaded_fingerprint=perf.fingerprint(loaded),
                           checked_fingerprint=perf.fingerprint(checked), checked_input_fingerprint=perf.input_fingerprint(checked),
                           input_fingerprint=before["input_fingerprint"], diagnostics=diagnostics)
                state["runs"].append(row)
                save(result_path, state)
                print(json.dumps({"iteration": iteration, "workers": mode, "wall": result["wall_seconds"]}), flush=True)
        assert cli_control("after") == before
        for mode in (1, 2, 4):
            result = capture(f"options-after-{mode}", [str(args.normal.resolve()), "--project", str(project), *flags, "--checkers", str(mode), "--showConfig"], cwd)
            assert result["exit_code"] == 0 and not result["stderr"]
            effective = json.loads(result["stdout"])
            assert effective["compilerOptions"].pop("checkers", mode) == mode
            assert perf.fingerprint(effective) == options_fingerprints[str(mode)]
        state["summary"] = {}
        for mode in (1, 2, 4):
            rows = [r for r in state["runs"] if r["requested"] == mode and not r["warmup"]]
            state["summary"][str(mode)] = {k: {"median": statistics.median(r[k] for r in rows), "min": min(r[k] for r in rows), "max": max(r[k] for r in rows)}
                                         for k in ("wall_seconds", "user_seconds", "system_seconds", "peak_rss_bytes")}
        state["scope_and_complete_diagnostics_stable"] = True
    assert all(sha(getattr(args, k)) == h for k, h in state["binary_sha256"].items())
    state["complete"] = True
    save(result_path, state)


if __name__ == "__main__":
    main()
