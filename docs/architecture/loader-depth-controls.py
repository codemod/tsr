"""Physical native loader depth/root controls. This measures fidelity, not speed."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("perf", ROOT / "scripts/whole_project_perf.py")
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)

FILES = {
    "globals.d.ts": "\n".join(
        "interface " + name + " {}"
        for name in ("Array<T>", "Boolean", "Function", "CallableFunction",
                     "NewableFunction", "IArguments", "Number", "Object", "RegExp", "String")
    ) + "\n",
    "main.ts": 'import "outer";\n',
    "node_modules/outer/package.json": json.dumps({
        "name": "outer", "version": "1.0.0", "main": "index.js"}),
    "node_modules/outer/index.js": 'import "inner"; export const outer = 1;\n',
    "node_modules/inner/package.json": json.dumps({
        "name": "inner", "version": "1.0.0", "main": "index.js"}),
    "node_modules/inner/index.js": 'import "./leaf.js"; export const inner = 1;\n',
    "node_modules/inner/leaf.js": 'import "./typed.js"; export const leaf = 1;\n',
    "node_modules/inner/typed.ts": 'export const wrong: number = "s";\n',
    "local-main.ts": 'import "./local.js";\n',
    "local.js": 'import "./local-typed.js"; export const local = 1;\n',
    "local-typed.ts": 'export const wrong: number = "s";\n',
}

INNER = "node_modules/inner/index.js"
OUTER = "node_modules/outer/index.js"
CASES = (
    ("elided-first-later-root", ["main.ts", INNER], 1, False),
    ("root-before-deep-import", [INNER, "main.ts"], 1, False),
    ("loaded-first-later-parent-root", ["main.ts", OUTER], 1, False),
    ("parent-root-before-import", [OUTER, "main.ts"], 1, False),
    ("elided-without-later-root", ["main.ts"], 1, False),
    ("outer-root-only", [OUTER], 1, False),
    ("raised-limit-two", ["main.ts"], 2, False),
    ("raised-limit-three", ["main.ts"], 3, False),
    ("zero-limit-later-root", ["main.ts", INNER], 0, False),
    ("duplicate-later-root", ["main.ts", INNER, INNER], 1, False),
    ("duplicate-main-root", ["main.ts", "main.ts"], 1, False),
    ("relative-js-outside-node-modules", ["local-main.ts"], 0, False),
    ("no-resolve-explicit-roots", ["main.ts", INNER], 1, True),
)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_info(path, relative_files):
    return {"head": perf.revision(path),
            "files_sha256": {name: sha(path / name) for name in relative_files}}


def output_image(stdout, cwd):
    """Keep every nonempty output line in diagnostics, loaded files or traces."""
    loaded, traces = [], []
    in_diagnostic = False
    for line in stdout.splitlines():
        if perf.DIAGNOSTIC_START.match(line):
            in_diagnostic = True
        elif in_diagnostic and line[:1].isspace():
            continue
        else:
            in_diagnostic = False
            if not line:
                continue
            if Path(line).is_absolute() and Path(line).is_file():
                # noLib/types=[] and only local fixture imports prohibit an
                # unrecorded library or unrelated file from masking work loss.
                assert line.startswith(str(cwd) + "/"), line
                loaded.append(line[len(str(cwd)) + 1:])
            else:
                traces.append(line.replace(str(cwd), "<project>"))
    assert loaded and len(loaded) == len(set(loaded)), loaded
    return {"diagnostics": perf.diagnostics(stdout, cwd),
            "loaded_order": loaded, "loaded_set": sorted(loaded),
            "ordered_trace": traces}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("tsr", "tsgo", "tsr-source", "native-source", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--native-repeats", type=int, default=3)
    args = parser.parse_args()
    if args.native_repeats < 1:
        parser.error("native repeats must be positive")
    out = args.output.resolve()  # /tmp -> /private/tmp on macOS, before roots exist.
    if out.exists():
        parser.error("output must be a new directory")
    out.mkdir(parents=True)
    tsr, tsgo = args.tsr.resolve(), args.tsgo.resolve()
    native_source = args.native_source.resolve()
    state = {
        "kind": "native depth/root fidelity controls; no concurrency or speed claim",
        "tsr_source": source_info(args.tsr_source.resolve(), [
            "crates/tsr-compiler/src/loader.rs", "crates/tsr-execute/src/compile.rs",
            "crates/tsr-execute/src/os_system.rs", "crates/tsr-module/src/resolver.rs"]),
        "native_source": source_info(native_source, [
            "internal/compiler/filesparser.go", "internal/compiler/fileloader.go",
            "internal/core/workgroup.go"]),
        "binary_sha256": {"tsr": sha(tsr), "tsgo": sha(tsgo)},
        "driver_sha256": sha(Path(__file__)), "physical_output": str(out),
        "fixtures": FILES, "cases": [],
    }
    env = {k: v for k, v in os.environ.items() if not k.startswith("TSR_")}
    env.update({"NO_COLOR": "1", "TSR_LIB_PATH": str(
        native_source / "internal/bundled/libs")})

    def save():
        p = out / "results.json"
        p.write_text(json.dumps(state, indent=2) + "\n")
        assert json.loads(p.read_text()) == state

    save()
    for name, roots, depth, no_resolve in CASES:
        cwd = out / name
        cwd.mkdir()
        for relative, content in FILES.items():
            p = cwd / relative
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content)
        config = cwd / "tsconfig.json"
        config.write_text(json.dumps({"compilerOptions": {
            "target": "es2020", "module": "esnext", "moduleResolution": "bundler",
            "allowJs": True, "checkJs": False, "maxNodeModuleJsDepth": depth,
            "noResolve": no_resolve, "noLib": True, "types": [],
            "noEmit": True, "incremental": False, "composite": False,
        }, "files": [*roots, "globals.d.ts"]}, indent=2) + "\n")
        inputs = {p: sha(cwd / p) for p in [*FILES, "tsconfig.json"]}
        case = {"name": name, "roots": roots, "depth": depth,
                "no_resolve": no_resolve, "input_sha256": inputs, "runs": []}
        roles = [("tsr", tsr, [])] + [
            (f"native-{mode}-{i}", tsgo, flags)
            for i in range(args.native_repeats)
            for mode, flags in (("one", ["--singleThreaded"]), ("default", []))]
        for role, binary, flags in roles:
            command = [str(binary), "--project", str(config), "--pretty", "false",
                       "--listFiles", "--traceResolution", *flags]
            result = subprocess.run(command, cwd=cwd, env=env, capture_output=True,
                                    text=True, timeout=60)
            assert result.returncode in (0, 1, 2), (role, result.returncode)
            assert not result.stderr, (role, result.stderr)
            for stream in ("stdout", "stderr"):
                (cwd / f"{role}.{stream}").write_text(getattr(result, stream))
            image = output_image(result.stdout, cwd)
            # Native still resolves imports under noResolve; it only suppresses
            # adding imported files. Preserve those traces as observable work.
            if role.startswith("native-"):
                assert any(line.startswith("======== Resolving module")
                           for line in image["ordered_trace"]), image["ordered_trace"]
            case["runs"].append({"role": role, "command": command,
                                 "exit_code": result.returncode, **image,
                                 "stdout_sha256": hashlib.sha256(
                                     result.stdout.encode()).hexdigest()})
        assert inputs == {p: sha(cwd / p) for p in inputs}, "inputs changed during controls"
        native = case["runs"][1:]
        actual = case["runs"][0]
        case["native_distinct_images"] = len({perf.fingerprint({
            k: run[k] for k in ("diagnostics", "loaded_order", "ordered_trace")})
            for run in native})
        case["tsr_matches_all_native"] = {
            key: all(actual[key] == run[key] for run in native)
            for key in ("diagnostics", "loaded_order", "loaded_set", "ordered_trace")}
        state["cases"].append(case)
        save()
        print(json.dumps({"case": name, "native_images": case["native_distinct_images"],
                          "matches": case["tsr_matches_all_native"]}), flush=True)
    state["complete"] = True
    state["all_match"] = all(all(c["tsr_matches_all_native"].values()) for c in state["cases"])
    save()
    return 0 if state["all_match"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
