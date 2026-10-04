"""Controls for the temporary member-cost probe; no production speed claim."""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("perf", ROOT / "scripts/whole_project_perf.py")
perf = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(perf)


def fixtures():
    cases = {}
    for size in (1, 32):
        cases[f"repeated-members-{size}"] = (
            "interface Shape { alpha: number; beta: string }\n"
            "declare const source: { alpha: number; beta: string; extra: boolean };\n"
            + "".join(f"const x{i}: Shape = source;\n" for i in range(size))
        )
    cases["distinct-arguments-negative"] = """
interface Box<T> { value: T }
declare const n: Box<number>;
declare const s: Box<string>;
const nok: { value: number } = n;
const sok: { value: string } = s;
const wrong: { value: number } = s;
"""
    cases["diamond"] = """
interface Root { root: number }
interface Left extends Root { left: string }
interface Right extends Root { right: boolean }
interface Diamond extends Left, Right { own: number }
declare const source: { root: number; left: string; right: boolean; own: number };
const a: Diamond = source;
const b: Diamond = source;
"""
    cases["cycle"] = """
interface Left extends Right { left: number }
interface Right extends Left { right: string }
declare const source: { left: number; right: string };
const a: Left = source;
const b: Right = source;
"""
    cases["failed-base"] = """
interface Shape extends MissingBase { value: number }
declare const source: { value: number };
const a: Shape = source;
const b: Shape = source;
"""
    cases["same-spelled-distinct-negative"] = """
namespace Left { export interface Shape { left: number } }
namespace Right { export interface Shape { right: string } }
declare const left: { left: number };
declare const right: { right: string };
const a: Left.Shape = left;
const b: Right.Shape = right;
const wrong: Left.Shape = right;
"""
    cases["receiver-this"] = """
class Base { value = 1; self(): this { return this; } }
class Derived extends Base { extra = "s"; }
declare const derived: Derived;
const a: Derived = derived.self();
const b: { value: number; extra: string } = derived.self();
"""
    cases["mapped"] = """
type Fields<T> = { [K in keyof T]: T[K] };
declare const source: { n: number; s: string };
const a: Fields<{ n: number; s: string }> = source;
const b: Fields<{ n: number; s: string }> = source;
"""
    cases["computed-overlay"] = """
const key = "value";
class Shape { [key] = 1; }
declare const value: Shape;
const a: { value: number } = value;
const b: { value: number } = value;
"""
    cases["intersection-missing-negative"] = """
interface Need { a: number; b: string; missing: boolean }
declare const source: { a: number } & { b: string };
const bad: Need = source;
const badAgain: Need = source;
"""
    return {name: source.lstrip() for name, source in cases.items()}


def counters(stderr):
    result = {}
    for line in stderr.splitlines():
        match = re.fullmatch(r"(MEMBER_QUERY|MEMBER_PATH|MEMBER_TYPE|MEMBER_SITE|MEMBER_OBSERVER) (.+)", line)
        if match:
            result.setdefault(match[1], []).append(json.loads(match[2]))
    return result


def non_timing(value):
    if isinstance(value, dict):
        return {k: non_timing(v) for k, v in value.items() if k != "observed_ns"}
    if isinstance(value, list):
        return [non_timing(v) for v in value]
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("normal", "scope", "probe", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--project", type=Path)
    parser.add_argument("--input-manifest", type=Path)
    parser.add_argument("--tsgo", type=Path)
    parser.add_argument("--ledger", type=Path, help="local experiment ledger updated after every child")
    parser.add_argument("--ledger-field", default="controls", help="separate field for each distinct probe snapshot")
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True)
    binaries = {name: getattr(args, name).resolve(strict=True) for name in ("normal", "scope", "probe")}
    native = args.tsgo.resolve(strict=True) if args.tsgo else None
    state = {"kind": __doc__, "source": perf.revision(ROOT),
             "binaries": {name: perf.inputs.file_hash(path) for name, path in binaries.items()},
             "native_binary_sha256": perf.inputs.file_hash(native) if native else None,
             "driver_sha256": perf.inputs.file_hash(Path(__file__)), "cases": [], "complete": False,
             "target_verified": False, "complete_cross_tool_inputs_verified": False}
    env = {k: v for k, v in os.environ.items() if not k.startswith("TSR_")}
    env["TSR_LIB_PATH"] = str(ROOT / "vendor/typescript-go/internal/bundled/libs")

    def save():
        path = args.output / "results.json"
        path.write_text(json.dumps(state, indent=2) + "\n")
        assert json.loads(path.read_text()) == state
        if args.ledger:
            ledger = json.loads(args.ledger.read_text())
            entry = ledger["cost_observations"]["member-projection-359a"]
            entry[args.ledger_field] = state
            args.ledger.write_text(json.dumps(ledger, indent=2) + "\n")
            assert json.loads(args.ledger.read_text())["cost_observations"]["member-projection-359a"][args.ledger_field] == state

    save()
    projects = []
    if args.project:
        projects.append(("private-app", args.project.resolve(strict=True)))
    else:
        for name, source in fixtures().items():
            cwd = args.output / name
            cwd.mkdir()
            (cwd / "input.ts").write_text(source)
            config = cwd / "tsconfig.json"
            config.write_text(json.dumps({"compilerOptions": {
                "strict": True, "target": "es2020", "types": [], "skipLibCheck": True,
                "noEmit": True, "incremental": False, "composite": False,
            }, "files": ["input.ts"]}, indent=2) + "\n")
            projects.append((name, config))

    for name, config in projects:
        cwd = config.parent
        flags = ["--project", str(config), "--noEmit", "--incremental", "false", "--composite", "false", "--pretty", "false"]
        query_paths, provenance = perf.inputs.load_manifest(args.input_manifest, cwd)
        paths = sorted(set(query_paths + [str(config), *map(str, binaries.values())]))
        if args.input_manifest:
            paths.append(str(args.input_manifest.resolve()))
        if native:
            paths.append(str(native))
        reference = perf.inputs.snapshot(paths)
        case = {"name": name, "input_manifest": provenance, "runs": [], "complete": False}
        state["cases"].append(case)

        def run(role, binary, extra, settings):
            before = perf.inputs.snapshot(paths)
            assert before == reference and perf.inputs.valid_snapshot(before)
            saved_env = dict(os.environ)
            try:
                os.environ.clear()
                os.environ.update(env | settings)
                result = perf.process([str(binary), *flags, *extra], cwd, 90)
            finally:
                os.environ.clear()
                os.environ.update(saved_env)
            after = perf.inputs.snapshot(paths)
            raw = args.output / f"{name}-{role}"
            raw.with_suffix(".stdout").write_text(result["stdout"])
            raw.with_suffix(".stderr").write_text(result["stderr"])
            row = {k: v for k, v in result.items() if k not in ("stdout", "stderr", "command")}
            checked = [line.split("\t", 1)[1] for line in result["stderr"].splitlines() if line.startswith("TSR_META_CHECKED\t")]
            loaded = [line for line in result["stdout"].splitlines() if line.startswith("/")]
            row.update(role=role, checked=checked, loaded=loaded,
                       diagnostics=perf.diagnostics(result["stdout"], cwd),
                       counters=counters(result["stderr"]), input_before=perf.fingerprint(before),
                       input_after=perf.fingerprint(after))
            case["runs"].append(row)
            save()  # Preserve measurements before semantic/scope checks.
            assert before == after and perf.inputs.valid_snapshot(after)
            assert not result["timed_out"] and result["exit_code"] in (0, 1, 2)
            print(json.dumps({"case": name, "role": role, "wall": row["wall_seconds"], "diagnostics": row["diagnostics"]["count"]}), flush=True)
            return result, row

        listing, _ = run("listing", binaries["normal"], ["--listFilesOnly"], {})
        loaded = listing["stdout"].splitlines()
        assert listing["exit_code"] == 0 and loaded and all(Path(p).is_file() for p in loaded)
        previous = {row["path"]: row for row in reference}
        paths = sorted(set(paths + loaded))
        reference = perf.inputs.snapshot(paths)
        assert perf.inputs.valid_snapshot(reference)
        assert all(row == previous[row["path"]] for row in reference if row["path"] in previous)
        case["input_fingerprint"] = perf.fingerprint(reference)
        for role, binary in binaries.items():
            result, _ = run("config-" + role, binary, ["--showConfig"], {})
            fingerprint = perf.fingerprint(json.loads(result["stdout"]))
            case.setdefault("effective_config_fingerprint", fingerprint)
            assert case["effective_config_fingerprint"] == fingerprint
        expected = None
        checked_reference = None
        count_reference = None
        for index in range(2):
            modes = [("normal", binaries["normal"], {}),
                     ("scope", binaries["scope"], {"TSR_META_SCOPE": "1"}),
                     ("disabled", binaries["probe"], {"TSR_META_SCOPE": "1"}),
                     ("enabled", binaries["probe"], {"TSR_META_SCOPE": "1", "TSR_PROFILE_MEMBER_QUERIES": "1"}),
                     ("timing", binaries["probe"], {"TSR_META_SCOPE": "1", "TSR_PROFILE_MEMBER_QUERIES": "1", "TSR_PROFILE_MEMBER_TIMING": "1"})]
            if index:
                modes.reverse()
            for role, binary, settings in modes:
                _, row = run(f"{role}{index}", binary, ["--listFiles", "--extendedDiagnostics"], settings)
                expected = row["diagnostics"] if expected is None else expected
                assert row["diagnostics"] == expected
                assert row["loaded"] == loaded
                if role != "normal":
                    checked_reference = row["checked"] if checked_reference is None else checked_reference
                    assert row["checked"] == checked_reference and checked_reference
                if role in ("enabled", "timing"):
                    count_reference = non_timing(row["counters"]) if count_reference is None else count_reference
                    assert non_timing(row["counters"]) == count_reference
                else:
                    assert not row["counters"]
        if native and not args.project:
            _, row = run("native", native, ["--singleThreaded", "--extendedDiagnostics"], {})
            case["native_diagnostics_match"] = row["diagnostics"] == expected
        case["complete"] = True
        save()
    state["complete"] = True
    save()


if __name__ == "__main__":
    main()
