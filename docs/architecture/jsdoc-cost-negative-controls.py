#!/usr/bin/env python3
"""Keep unsupported JSDoc diagnostics visible beside positive cost controls."""
import argparse
import importlib.util
import os
from pathlib import Path
import sys

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("cost", Path(__file__).with_name("jsdoc-cost-controls.py"))
cost = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cost)


def run(args):
    args.output.mkdir(parents=True, exist_ok=False)
    project = args.output / "project"
    cost.fixtures(project)
    checked = project / "checked.js"
    checked.write_text(checked.read_text() + '\n/** @type {number} */\n'
                       'export const wrongIdentity = identity("not number");\n'
                       '/** @type {RecordValue} */\n'
                       'export const wrongRecord = {value: "not number"};\n'
                       'export const wrongOverload = overloaded(true);\n')
    imports = project / "imports.js"
    imports.write_text(imports.read_text() + '\n/** @type {Item} */\n'
                       'export const wrongItem = {value: "not number"};\n')
    manifest = {path.name: cost.file_hash(path) for path in project.iterdir()
                if path.name != "fixture-manifest.json"}
    cost.write(project / "fixture-manifest.json", manifest)
    results = []
    for name, binary in (("normal", args.normal), ("probe-off", args.probe),
                         ("probe-on", args.probe), ("native", args.native)):
        for key in ("TSR_WORK_TRACE", "TSR_NATIVE_WORK_ACTIVITY", "TSR_JSDOC_COST_PROBE"):
            os.environ.pop(key, None)
        trace = args.output / "negative-doc.tsv"
        if name == "probe-on":
            os.environ["TSR_JSDOC_COST_PROBE"] = str(trace)
        command = [str(binary.resolve()), "--project", "tsconfig.json", "--noEmit", "--incremental", "false",
                   "--composite", "false", "--pretty", "false", "--listFiles"]
        if name != "native":
            command.append("--extendedDiagnostics")
        before = cost.snapshot([str(project / name) for name in manifest])
        binary_hash = cost.file_hash(binary)
        cost.require(all(cost.file_hash(project / name) == digest for name, digest in manifest.items()),
                     "negative fixture manifest changes")
        child = cost.process(command, project, 180)
        after = cost.snapshot([str(project / name) for name in manifest])
        cost.require(not child["timed_out"] and child["exit_code"] in (0, 1, 2), "negative process failed")
        cost.require(before == after, "negative inputs changed")
        cost.require(cost.file_hash(binary) == binary_hash, "negative binary changes")
        child["diagnostics"] = cost.diagnostics(child["stdout"] + child["stderr"], project)
        answer = {"mode": name, "child": child, "binary_sha256": binary_hash,
                  "inputs_before": before, "inputs_after": after}
        if name == "probe-on":
            answer["probe"] = cost.read_probe(trace, child)
        results.append(answer)
        cost.write(args.output / "results.json", {"manifest": manifest, "results": results})
    first = results[0]["child"]["diagnostics"]
    cost.require(all(result["child"]["diagnostics"] == first for result in results[1:3]), "observer changed diagnostics")
    native = results[-1]["child"]["diagnostics"]
    expected = [
        "checked.js(18,14): error TS2322: Type 'string' is not assignable to type 'number'.",
        "checked.js(20,29): error TS2322: Type 'string' is not assignable to type 'number'.",
        "checked.js(21,41): error TS2769: No overload matches this call.\n"
        "  The last overload gave the following error.\n"
        "    Argument of type 'boolean' is not assignable to parameter of type 'number'.",
        "imports.js(6,27): error TS2322: Type 'string' is not assignable to type 'number'.",
    ]
    cost.require(native["entries"] == expected, "complete native negative boundary changed")
    print({"tsr_errors": first["count"], "native_errors": native["count"], "equivalent": first == native})


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ("normal", "probe", "native", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    run(parser.parse_args())
