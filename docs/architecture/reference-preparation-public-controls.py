#!/usr/bin/env python3
"""Public positive/error and worker-mode controls for the archived observer."""
import argparse
import importlib.util
import json
import os
from pathlib import Path

spec = importlib.util.spec_from_file_location("origins", Path(__file__).with_name("reference-preparation-controls.py"))
origins = importlib.util.module_from_spec(spec)
spec.loader.exec_module(origins)
cost = origins.cost


def run(normal, probe, native, directory):
    directory.mkdir(parents=True, exist_ok=False)
    contents = {
        "pair.ts": "export interface Pair<T, U = T> { left: T; right: U }\nexport function read<T>(pair: Pair<T>) { return pair.right; }\n",
        "main.ts": "import { Pair, read } from './pair';\nconst pair: Pair<number> = {left: 1, right: 2};\nexport const value: number = read(pair);\n",
        "second.ts": "export namespace Left { export interface Box<T = string> { value: T } }\nexport namespace Right { export interface Box<T = number> { value: T } }\nexport const left: Left.Box = {value: 'ok'};\nexport const right: Right.Box = {value: 1};\n",
        "third.ts": "export interface Wrapper<T = number> { value: T }\nexport interface Outer<T> { child: T }\nexport const nested: Outer<Wrapper> = {child: {value: 1}};\n",
        "negative.ts": "export const wrong: number = 'wrong';\n",
    }
    for name, text in contents.items():
        (directory / name).write_text(text)
    result = dict(fixture_sources=contents, fixture_hashes={name: cost.file_hash(directory / name) for name in contents},
                  binary_hashes={label: cost.file_hash(binary) for label, binary in (("normal", normal), ("probe", probe), ("native", native))},
                  children=[], complete=False)
    output = directory / "results.json"
    cost.write(output, result)
    for case, files, extra, workers in (
        ("positive-default", ["pair.ts", "main.ts", "second.ts", "third.ts"], [], 4),
        ("positive-single", ["pair.ts", "main.ts", "second.ts", "third.ts"], ["--singleThreaded", "--checkers", "2"], 1),
        ("negative-two", ["negative.ts"], ["--checkers", "2"], 2),
        ("negative-no-check", ["negative.ts"], ["--noCheck", "--checkers", "2"], 2),
    ):
        config = directory / (case + ".json")
        cost.write(config, dict(compilerOptions=dict(strict=True, skipLibCheck=True, target="es2022"), files=files))
        command = ["--project", str(config), "--noEmit", "--incremental", "false", "--composite", "false", "--pretty", "false", *extra]
        expected = None
        for label, binary in (("normal", normal), ("off", probe), ("on", probe), ("native", native)):
            for key in list(os.environ):
                if key.startswith("TSR_"):
                    os.environ.pop(key)
            trace = directory / (case + ".tsv")
            if label == "on":
                os.environ["TSR_REFERENCE_ORIGINS"] = str(trace)
            child = cost.process([str(binary), *command, "--listFiles", "--extendedDiagnostics"], directory, 180)
            child.update(case=case, variant=label, diagnostics=cost.diagnostics(child["stdout"] + child["stderr"], directory),
                         counts=cost.counts(child))
            result["children"].append(child)
            cost.write(output, result)
            origins.require(not child["timed_out"] and child["exit_code"] in (0, 1), "incomplete public child")
            if label == "normal":
                expected = child
            if label != "native":
                origins.require(child["diagnostics"] == expected["diagnostics"] and child["counts"] == expected["counts"] and
                                cost.files(child) == cost.files(expected) and child["exit_code"] == expected["exit_code"], "observer changes public output/scope")
            else:
                child["complete_native_diagnostics_match"] = child["diagnostics"] == expected["diagnostics"]
                origins.require(child["diagnostics"]["count"] == (1 if case == "negative-two" else 0), "native positive/error control ineffective")
            if label == "on":
                child["probe"] = origins.read_probe(trace, child, cost.files(child), workers)
                origins.require(len(child["probe"]["checked_files"]) == (0 if case == "negative-no-check" else len(files)), "public performed scope mismatch")
            cost.write(output, result)
        print(json.dumps(dict(case=case, diagnostics=expected["diagnostics"]["count"], checked=expected["counts"].get("Checked files"),
                              complete_native_diagnostics_match=child["complete_native_diagnostics_match"])), flush=True)
    result["complete"] = True
    result["all_complete_native_diagnostics_match"] = all(child["complete_native_diagnostics_match"]
        for child in result["children"] if child["variant"] == "native")
    cost.write(output, result)


def main():
    parser = argparse.ArgumentParser()
    for name in ("normal", "probe", "native", "directory"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    run(args.normal.resolve(), args.probe.resolve(), args.native.resolve(), args.directory)


if __name__ == "__main__":
    main()
