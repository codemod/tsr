"""Fixed public controls for the temporary mapper-key probe.

This is a diagnostic measurement, not a timing benchmark or a general growth
generator. Supply separately frozen normal/probe TSR binaries and pinned tsgo.
The companion report describes the source identities and remaining gaps.
"""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("whole_project_perf", ROOT / "scripts/whole_project_perf.py")
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)


def fixtures():
    signature = "declare function make<T>(v: T): { run: (x: T) => T };\n"
    literal = "declare function box<T>(v: T): { value: T; nested: { value: T } };\n"
    cases = {}
    for size in (1, 32):
        cases[f"signature-repeat-{size}"] = signature + "".join(
            f"const x{i}: number = make<number>(1).run(2);\n" for i in range(size)
        )
        cases[f"literal-repeat-{size}"] = literal + "".join(
            f"const x{i}: number = box<number>(1).nested.value;\n" for i in range(size)
        )
    cases["signature-distinct-negative"] = signature + """
const numberOk: number = make<number>(1).run(2);
const stringOk: string = make<string>("s").run("t");
const wrong: number = make<string>("s").run("t");
"""
    cases["ordered-arguments-negative"] = """
declare function pair<T, U>(a: T, b: U): { first: T; second: U };
const ns = pair<number, string>(1, "s");
const sn = pair<string, number>("s", 1);
const numberOk: number = ns.first;
const stringOk: string = sn.first;
const wrong: number = sn.first;
"""
    cases["composed-mapper-negative"] = """
interface Box<T> { value: T }
interface List<T> { head: T }
declare function listBox<T>(v: T): List<Box<T>>;
const numberOk: number = listBox<number>(1).head.value;
const stringOk: string = listBox<string>("s").head.value;
const wrong: number = listBox<string>("s").head.value;
"""
    cases["fresh-shadow-negative"] = """
declare function outer<T>(v: T): { own: <T>(x: T) => T; captured: () => T };
const a = outer<number>(1);
const stringOk: string = a.own<string>("s");
const numberOk: number = a.captured();
const wrong: number = a.own<string>("s");
"""
    cases["mapped-negative"] = """
declare function mapped<T>(v: T): { readonly [K in keyof T]: { value: T[K] } };
const result = mapped({ n: 1, s: "s" });
const numberOk: number = result.n.value;
const stringOk: string = result.s.value;
const wrong: number = result.s.value;
"""
    cases["recursive-receiver-negative"] = """
interface Link<T> { value: T; next: Link<T> }
declare const numeric: Link<number>;
declare const textual: Link<string>;
const numberOk: number = numeric.next.next.value;
const stringOk: string = textual.next.next.value;
const wrong: number = textual.next.next.value;
"""
    cases["ordered-arguments-reversed-negative"] = cases["ordered-arguments-negative"].replace(
        'const ns = pair<number, string>(1, "s");\nconst sn = pair<string, number>("s", 1);',
        'const sn = pair<string, number>("s", 1);\nconst ns = pair<number, string>(1, "s");',
    )
    cases["same-spelled-identity-negative"] = """
namespace Left { export class Token { private brand = 1; } }
namespace Right { export class Token { private brand = 1; } }
declare function box<T>(v: T): { value: T };
declare const left: Left.Token;
declare const right: Right.Token;
const leftOk: Left.Token = box<Left.Token>(left).value;
const rightOk: Right.Token = box<Right.Token>(right).value;
const wrong: Left.Token = box<Right.Token>(right).value;
"""
    cases["contextual-inference-negative"] = """
declare function adapt<T>(v: T, f: (x: T) => T): T;
const numberOk: number = adapt(1, x => x);
const stringOk: string = adapt("s", x => x);
const wrong: number = adapt("s", x => x);
"""
    cases["flatten-negative"] = """
type Flatten<T> = T extends readonly (infer U)[] ? Flatten<U> : T;
declare function flatten<T>(v: T): Flatten<T>;
const value = flatten([["s"]]);
const stringOk: string = value;
const wrong: number = value;
"""
    return {name: source.lstrip() for name, source in cases.items()}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--normal", type=Path, required=True)
    parser.add_argument("--probe", type=Path, required=True)
    parser.add_argument("--tsgo", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("output must be a new directory; preserve previous measurements")
    args.output.mkdir(parents=True)
    state = {"kind": "mapper locating controls; no speed claim", "binaries": {
        name: {"path": str(path.resolve()), "sha256": sha(path)}
        for name, path in [("normal", args.normal), ("probe", args.probe), ("native", args.tsgo)]
    }, "cases": []}
    env = {k: v for k, v in os.environ.items() if not k.startswith("TSR_")}
    env["TSR_LIB_PATH"] = str(ROOT / "vendor/typescript-go/internal/bundled/libs")

    def persist():
        path = args.output / "results.json"
        path.write_text(json.dumps(state, indent=2) + "\n")
        assert json.loads(path.read_text()) == state

    persist()
    for name, source in fixtures().items():
        cwd = args.output / name
        cwd.mkdir()
        input_file = cwd / "input.ts"
        input_file.write_text(source)
        config = cwd / "tsconfig.json"
        config.write_text(json.dumps({"compilerOptions": {
            "strict": True, "target": "es2020", "types": [], "skipLibCheck": True,
            "noEmit": True, "incremental": False, "composite": False,
        }, "files": ["input.ts"]}, indent=2) + "\n")
        case = {"name": name, "source": source, "input_sha256": sha(input_file),
                "config_sha256": sha(config), "runs": []}
        state["cases"].append(case)
        for role, binary, enabled in [
            ("normal", args.normal, False), ("disabled", args.probe, False),
            ("enabled", args.probe, True), ("repeat", args.probe, True),
            ("native", args.tsgo, False),
        ]:
            runenv = dict(env)
            if enabled:
                runenv["TSR_MAPPER_PROFILE"] = "1"
            command = [str(binary.resolve()), "--project", str(config.resolve()), "--pretty", "false"]
            if role == "native":
                command += ["--singleThreaded", "--extendedDiagnostics"]
            saved_env = dict(os.environ)
            try:
                os.environ.clear()
                os.environ.update(runenv)
                run = perf.process(command, cwd.resolve(), timeout=60)
            finally:
                os.environ.clear()
                os.environ.update(saved_env)
            for stream in ("stdout", "stderr"):
                (cwd / f"{role}.{stream}").write_text(run[stream])
            counts = [json.loads(line.split("\t", 1)[1]) for line in run["stderr"].splitlines()
                      if line.startswith("TSR_MAPPER_COUNTS\t")]
            checked = [perf.file_identity(line.split("\t", 1)[1], cwd.resolve())
                       for line in run["stderr"].splitlines() if line.startswith("TSR_MAPPER_CHECKED\t")]
            native_counts = dict(re.findall(r"^(Types|Instantiations):\s*(\d+)\s*$", run["stdout"], re.M))
            result = {k: run[k] for k in ("command", "exit_code", "timed_out", "wall_seconds",
                                         "user_seconds", "system_seconds", "peak_rss_bytes")}
            result.update({"role": role, "diagnostics": perf.diagnostics(run["stdout"], cwd.resolve()),
                           "checked": checked, "checked_input_fingerprint": perf.input_fingerprint(checked) if checked else None,
                           "counts": counts[0] if counts else None, "native_counts": native_counts})
            case["runs"].append(result)
            persist()
            assert not run["timed_out"] and run["exit_code"] in (0, 1, 2)
            assert len(counts) == int(enabled)
            if enabled:
                assert checked == [str(input_file.resolve())]
            else:
                assert not checked
        runs = case["runs"]
        assert all(r["diagnostics"] == runs[0]["diagnostics"] for r in runs[:4])
        assert all(r["exit_code"] == runs[0]["exit_code"] for r in runs[:4])
        assert runs[2]["checked"] == runs[3]["checked"]
        assert runs[2]["checked_input_fingerprint"] == runs[3]["checked_input_fingerprint"]
        assert {k: v for k, v in runs[2]["counts"].items() if not k.endswith("_ns")} == {
            k: v for k, v in runs[3]["counts"].items() if not k.endswith("_ns")}
        case["native_diagnostics_match"] = runs[0]["diagnostics"] == runs[-1]["diagnostics"]
        case["expected_native_errors"] = int(name.endswith("negative"))
        assert runs[-1]["diagnostics"]["count"] == case["expected_native_errors"]
        persist()
        print(json.dumps({"case": name, "tsr_errors": runs[0]["diagnostics"]["count"],
                          "native_errors": runs[-1]["diagnostics"]["count"],
                          "native_match": case["native_diagnostics_match"]}), flush=True)
    state["complete"] = True
    state["equivalent_diagnostics_gate"] = all(case["native_diagnostics_match"] for case in state["cases"])
    persist()


if __name__ == "__main__":
    main()
