"""Reproduce config worker-count transport and native CLI numeric diagnostics."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "selection", ROOT / "docs/architecture/worker-selection-controls.py")
selection = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selection)
perf = selection.perf


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("normal", "probe", "tsgo", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    assert not output.exists()
    output.mkdir(parents=True)
    binaries = {name: getattr(args, name).resolve() for name in ("normal", "probe", "tsgo")}
    state = {"binary_sha256": {k: selection.sha(v) for k, v in binaries.items()},
             "production_cli_parallel": False, "native_target_verified": False,
             "config_controls": [], "cli_controls": [], "complete": False}

    def save():
        selection.save(output / "results.json", state)

    def capture(label, command, cwd):
        result = selection.run(command, cwd)
        for stream in ("stdout", "stderr"):
            (output / (label + "." + stream)).write_text(result[stream])
        assert not result["timed_out"]
        return result

    common = {"strict": True, "skipLibCheck": True, "types": [], "target": "es2020"}
    base = {"compilerOptions": {**common, "checkers": 8, "singleThreaded": True},
            "files": ["main.ts", "dep.ts"] + [f"skip{i}.d.ts" for i in range(6)]}
    cases = [
        ("absent", {**base, "compilerOptions": common}, "default", False),
        ("root", base, "8", True),
        ("inherited", {"extends": "./base.json"}, "8", True),
        ("own", {"extends": "./base.json", "compilerOptions": {"checkers": 2, "singleThreaded": False}}, "2", False),
        ("own-null", {"extends": "./base.json", "compilerOptions": {"checkers": None, "singleThreaded": None}}, "default", False),
        ("tiny", {"compilerOptions": {"noLib": True, "types": [], "checkers": 8}, "files": ["globals.d.ts"]}, "8", False),
        ("array-last", {"extends": ["./base.json", "./second.json"]}, "4", False),
        ("array-own", {"extends": ["./base.json", "./second.json"], "compilerOptions": {"checkers": 2, "singleThreaded": True}}, "2", True),
        ("array-null", {"extends": ["./base.json", "./clear.json"]}, "default", False),
        ("array-derived-clear", {"extends": ["./base.json", "./derived-clear.json"]}, "8", True),
    ]
    for name, config, requested, single in cases:
        cwd = output / name
        cwd.mkdir()
        (cwd / "main.ts").write_text('import {value} from "./dep";\nconst bad: number = value;\n')
        (cwd / "dep.ts").write_text('export declare const value: string;\n')
        (cwd / "globals.d.ts").write_text(
            "interface Array<T> { length: number; [n: number]: T }\n" +
            "\n".join("interface " + name + " {}" for name in
                      ("Boolean", "Function", "IArguments", "Number", "Object", "RegExp", "String", "CallableFunction", "NewableFunction")))
        for i in range(6):
            (cwd / f"skip{i}.d.ts").write_text(f"interface Skipped{i} {{ value: string }}\n")
        selection.save(cwd / "base.json", base)
        selection.save(cwd / "second.json", {"compilerOptions": {"checkers": 4, "singleThreaded": False}})
        selection.save(cwd / "clear.json", {"compilerOptions": {"checkers": None, "singleThreaded": None}})
        selection.save(cwd / "derived-clear.json", {"extends": "./clear.json"})
        project = cwd / "tsconfig.json"
        selection.save(project, config)
        result = capture(name + "-probe", [str(binaries["probe"]), str(project)], cwd)
        loaded, checked, workers, phase, policy = selection.probe_records(result)
        expected = min(len(loaded), 256, 1 if single else (4 if requested == "default" else int(requested)))
        assert policy == {"requested": requested, "effective": expected, "single": single}
        assert checked == ([str(cwd / "globals.d.ts")] if name == "tiny" else
                           [str(cwd / "dep.ts"), str(cwd / "main.ts")])
        if name == "tiny":
            assert loaded == [str(cwd / "globals.d.ts")] and expected == 1
        diagnostics = perf.diagnostics(result["stdout"], cwd)
        for role in ("normal", "tsgo"):
            control = capture(name + "-" + role, [str(binaries[role]), "--project", str(project),
                              "--noEmit", "--pretty", "false", "--listFiles"], cwd)
            assert not control["stderr"]
            assert perf.diagnostics(control["stdout"], cwd) == diagnostics
            files = [line for line in control["stdout"].splitlines()
                     if line.endswith(".ts") and not perf.DIAGNOSTIC_START.match(line)]
            assert (files == loaded if role == "normal" else
                    [Path(f).name for f in files] == [Path(f).name for f in loaded])
        state["config_controls"].append({"case": name, "policy": policy,
            "loaded_count": len(loaded), "checked_count": len(checked), "diagnostics": diagnostics,
            "input_fingerprint": perf.input_fingerprint(loaded), "config_sha256": selection.sha(project)})
        save()
    project = output / "own" / "tsconfig.json"
    for value in ("0", "-1", "1.5", "many", "2", "2147483648", "9007199254740993", "null"):
        results = [capture("cli-" + value + "-" + role,
                          [str(binaries[role]), "--project", str(project), "--noEmit", "--pretty", "false",
                           "--checkers", value, "--singleThreaded", "false"], project.parent)
                   for role in ("normal", "tsgo")]
        assert not any(r["stderr"] for r in results)
        assert results[0]["exit_code"] == results[1]["exit_code"]
        diagnostic_rows = [perf.diagnostics(r["stdout"], project.parent) for r in results]
        assert diagnostic_rows[0] == diagnostic_rows[1]
        raw_match = results[0]["stdout"] == results[1]["stdout"]
        if value in ("0", "-1", "1.5", "many"):
            assert raw_match
        elif not raw_match:
            # Pin the exact pre-existing footer defect; do not accept arbitrary
            # unclassified output changes or hide diagnostics with --quiet.
            footer = "\nFound 1 error in main.ts\x1b[90m:2\x1b[0m\n\n"
            assert results[0]["stdout"] == results[1]["stdout"] + footer
        state["cli_controls"].append({"value": value, "exit_code": results[0]["exit_code"],
                                     "diagnostics": diagnostic_rows[0],
                                     "complete_stdout_match": raw_match,
                                     "known_footer_issue": None if raw_match else "tsr-6.60",
                                     "stdout_sha256": [hashlib.sha256(r["stdout"].encode()).hexdigest() for r in results]})
        save()
    assert all(selection.sha(binaries[k]) == v for k, v in state["binary_sha256"].items())
    state["complete"] = True
    save()
    print(json.dumps({"config_controls": len(state["config_controls"]),
                      "cli_controls": len(state["cli_controls"]), "complete": True}))


if __name__ == "__main__":
    main()
