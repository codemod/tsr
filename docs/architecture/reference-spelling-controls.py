#!/usr/bin/env python3
"""Run site-aware native display controls; report Rust fidelity gaps explicitly."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("cost", Path(__file__).with_name("jsdoc-cost-controls.py"))
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)

DIRECTIVES = "// @strict: true\n// @target: esnext\n// @skipLibCheck: true\n"
FIXTURES = {
    "defaults": """export interface Box<T = number> { value: T }
export interface Pair<T, U = T> { left: T; right: U }
export declare const probe_bare: Box;
export declare const probe_full: Box<string>;
export declare const probe_partial: Pair<string>;
export function probe_fnBare(x: Box) { return x; }
export function probe_fnPartial(x: Pair<string>) { return x; }
export function pass<T>(x: Pair<T>): Pair<T> { return x; }
export const probe_computed = pass(probe_partial);
""",
    "nested": """export interface Box<T = number> { value: T }
export interface Outer<T> { child: T }
export declare const probe_nested: Outer<Box>;
export function probe_fnNested(x: Outer<Box>) { return x; }
""",
    "dependent": """export interface Chain<T = string, U = T, V = U> { one: T; two: U; three: V }
export declare const probe_chain: Chain<number, string>;
export function probe_fnChain(x: Chain<number, string>) { return x; }
""",
    "recursive": """export interface Recursive<T = number> { value: T; next?: Recursive<T> }
export interface Outer<T> { child: T }
export declare const probe_recursive: Outer<Recursive>;
export function probe_fnRecursive(x: Outer<Recursive>) { return x; }
""",
    "mapped": """export type Mapped<T = number> = { [P in 'key']: T };
export interface Outer<T> { child: T }
export declare const probe_mapped: Outer<Mapped>;
export function probe_fnMapped(x: Outer<Mapped>) { return x; }
""",
    "conditional": """export type Select<T> = T extends string ? {text: T} : {value: T};
export interface Outer<T> { child: T }
export declare const probe_conditional: Outer<Select<number>>;
export function probe_fnConditional(x: Outer<Select<number>>) { return x; }
""",
    "qualified": """export namespace Left { export interface Shape<T = string> { value: T } }
export namespace Right { export interface Shape<T = number> { value: T } }
import Alias = Left.Shape;
export declare const probe_qualified: Alias<string>;
export declare const probe_left: Left.Shape;
export declare const probe_right: Right.Shape;
export const probe_leftValue: Left.Shape = {value: 'ok'};
export const probe_rightValue: Right.Shape = {value: 1};
export function probe_fnQualified(x: Left.Shape<number>) { return x; }
""",
    "crossfile": """// @filename: base.ts
export interface Box<T = number> { value: T }
export interface Outer<T> { child: T }
// @filename: contract.ts
import { Box, Outer } from './base';
export declare const probe_cross: Outer<Box>;
export function probe_fnCross(x: Outer<Box>) { return x; }
""",
    "unique-symbol": "export declare const probe_symbol: unique symbol;\n",
    "negative": "export const probe_error: number = 'wrong';\n",
    "long-union": "export function probe_long(x: " + " | ".join(f"'s{i:03}'" for i in range(64)) + ") { return x; }\n",
}
PROTOCOLS = ("baseline", "plain", "missing-enclosing", "missing-flags")


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def native_maps(value, protocol, names):
    require(value["schema"] == 1 and value["protocol"] == protocol, "wrong native protocol")
    require(len(value["orders"]) == 2, "incomplete native query orders")
    outputs = []
    for order, reverse in zip(value["orders"], (False, True)):
        require(order["reverse"] is reverse, "wrong native query order")
        rows = order["types"]
        maps = {}
        for phase in ("before-check", "after-check"):
            selected = [row for row in rows if row["phase"] == phase]
            require(len(selected) == len(names), "incomplete native probe coverage")
            require([row["name"] for row in selected] == (list(reversed(names)) if reverse else names), "native probe order differs")
            require(all(type(row[key]) is int and row[key] >= 0 for row in selected
                        for key in ("query_instantiation_delta", "print_instantiation_delta")), "invalid executed-work count")
            maps[phase] = {(row["file"], row["name"]): row["type"] for row in selected}
            require(len(maps[phase]) == len(names), "duplicate native probes")
        require(len(rows) == 2 * len(names), "unknown native phase")
        require(maps["before-check"] == maps["after-check"], "native display changes after checking")
        require(order["loaded_inputs_sha256"], "missing native input coverage")
        require(not order["emit_skipped"] and order["declarations"], "native declaration emit incomplete")
        outputs.append(maps["after-check"])
    require(outputs[0] == outputs[1], "native display depends on query order")
    for key in ("loaded_inputs_sha256", "diagnostics", "declarations", "emit_diagnostics"):
        require(value["orders"][0][key] == value["orders"][1][key], "native order changes " + key)
    return outputs[0]


def rust_maps(text, names):
    units, phases = {}, {key: {} for key in ("producer", "forward", "reverse")}
    for line in text.splitlines():
        fields = line.split("\t", 3)
        if fields[0] == "input":
            require(len(fields) == 3 and fields[1] not in units, "invalid Rust unit")
            data = bytes.fromhex(fields[2])
            require(data.hex() == fields[2], "noncanonical unit encoding")
            units[fields[1]] = data.decode("utf-8")
        else:
            require(len(fields) == 4 and fields[0] in phases, "unknown Rust record")
            key = (fields[1], fields[2])
            require(key not in phases[fields[0]], "duplicate Rust probe")
            phases[fields[0]][key] = fields[3]
    require(units, "missing Rust inputs")
    for phase, rows in phases.items():
        require(len(rows) == len(names) and {key[1] for key in rows} == set(names), "incomplete Rust probes: " + phase)
    require(phases["producer"] == phases["forward"] == phases["reverse"], "Rust display depends on query order")
    return units, phases["producer"]


def build_identity(binary, receipt_path, helper):
    receipt = json.loads(receipt_path.read_text())
    require(receipt["terminal"] is True and receipt["exit_code"] == 0, "incomplete helper build")
    require(receipt["binary_sha256"] == cost.file_hash(binary), "binary differs from build receipt")
    require(receipt["helper_sha256"] == cost.file_hash(helper), "helper differs from build receipt")
    return receipt


def summarize(result, raw_path):
    require(result["complete"] and all(case["complete"] for case in result["cases"]), "partial oracle receipt")
    report = {"schema": 1, "kind": "reference-display-contract", "complete": True, "speed_claim": False,
              "native_sha": result["builds"]["native"]["native_sha"], "rust_sha": result["builds"]["rust"]["source"],
              "builds": result["builds"], "driver_sha256": result["driver_sha256"],
              "native_options": dict(strict=True, skipLibCheck=True, target="esnext", singleThreaded=True,
                                     declaration=True, emitDeclarationOnly=True, newLine="lf"),
              "rust_options": dict(strict=True, skipLibCheck=True, target="esnext"),
              "raw_receipt": {"path": str(raw_path), "sha256": cost.file_hash(raw_path)},
              "all_unit_bytes_match": result["all_unit_bytes_match"],
              "all_rust_displays_match": result["all_rust_displays_match"],
              "loaded_native_bundled_inputs": {}, "cases": [], "children": []}
    for case in result["cases"]:
        base = case["native"]["baseline"]
        units = {u["Name"]: u["Text"] for u in base["units"]}
        loaded = base["orders"][0]["loaded_inputs_sha256"]
        libs = {key: value for key, value in loaded.items() if key not in {"/" + name for name in units}}
        if not report["loaded_native_bundled_inputs"]:
            report["loaded_native_bundled_inputs"] = libs
        require(report["loaded_native_bundled_inputs"] == libs, "fixture library input mismatch")
        record = {key: case[key] for key in ("family", "fixture_sha256", "names", "unit_bytes_match",
                                            "rust_display_differences", "protocol_differences", "complete")}
        record.update(units=units, native_baseline=[{key: row[key] for key in ("file", "name", "type")}
                      for row in base["orders"][0]["types"] if row["phase"] == "after-check"],
                      rust_baseline=case["rust"]["types"], native_diagnostics=base["orders"][0]["diagnostics"],
                      native_declarations=base["orders"][0]["declarations"],
                      native_emit_diagnostics=base["orders"][0]["emit_diagnostics"],
                      native_emit_skipped=base["orders"][0]["emit_skipped"],
                      native_fixture_input_sha256={key: value for key, value in loaded.items() if key not in libs},
                      nonzero_native_instantiation_deltas=[])
        for protocol, value in case["native"].items():
            for order in value["orders"]:
                for row in order["types"]:
                    if row["query_instantiation_delta"] or row["print_instantiation_delta"]:
                        record["nonzero_native_instantiation_deltas"].append(dict(protocol=protocol, reverse=order["reverse"], **row))
        report["cases"].append(record)
    for child in result["children"]:
        require(child["exit_code"] == 0 and not child["timed_out"], "partial helper child")
        record = {key: child[key] for key in ("family", "protocol", "pid", "command", "exit_code", "timed_out", "started_at_unix_ns")}
        record.update(stdout_sha256=hashlib.sha256(child["stdout"].encode()).hexdigest(),
                      stderr_sha256=hashlib.sha256(child["stderr"].encode()).hexdigest())
        report["children"].append(record)
    return report


def run(native, rust, native_build, rust_build, directory):
    directory.mkdir(parents=True, exist_ok=False)
    builds = {"native": build_identity(native, native_build, Path(__file__).with_name("reference-spelling-native.go")),
              "rust": build_identity(rust, rust_build, Path(__file__).with_name("reference-spelling-contract.rs"))}
    require(builds["native"]["native_sha"] == "5b1047d10d32e7d5b446be4de56b126ff42f82bb", "wrong native pin")
    result = {"schema": 1, "builds": builds, "driver_sha256": cost.file_hash(Path(__file__)),
              "cases": [], "children": [], "complete": False, "speed_claim": False}
    output = directory / "results.json"
    cost.write(output, result)
    for family, body in FIXTURES.items():
        fixture = directory / (family + ".ts")
        fixture.write_text(DIRECTIVES + ("" if body.startswith("// @filename:") else "// @filename: contract.ts\n") + body)
        names = re.findall(r"(?:const|function)\s+(probe_\w+)", body)
        require(names and len(names) == len(set(names)), "invalid named fixture probes")
        case = {"family": family, "fixture": str(fixture), "fixture_sha256": cost.file_hash(fixture),
                "names": names, "native": {}, "rust": None, "complete": False}
        result["cases"].append(case)
        cost.write(output, result)
        for protocol in (*PROTOCOLS, "rust"):
            for key in list(os.environ):
                if key.startswith("TSR_"):
                    os.environ.pop(key)
            command = [str(rust), str(fixture)] if protocol == "rust" else [str(native), str(fixture), protocol]
            child = cost.process(command, directory, 90)
            child.update(family=family, protocol=protocol)
            result["children"].append(child)
            cost.write(output, result)
            require(child["exit_code"] == 0 and not child["timed_out"], "incomplete helper child")
            if protocol == "rust":
                units, rows = rust_maps(child["stdout"], names)
                case["rust"] = {"units": units, "types": [{"file": key[0], "name": key[1], "type": value} for key, value in rows.items()]}
            else:
                value = json.loads(child["stdout"])
                native_maps(value, protocol, names)
                case["native"][protocol] = value
            cost.write(output, result)
        base = case["native"]["baseline"]
        base_map = native_maps(base, "baseline", names)
        native_units = {u["Name"]: u["Text"] for u in base["units"]}
        case["unit_bytes_match"] = native_units == case["rust"]["units"]
        rust_map = {(row["file"], row["name"]): row["type"] for row in case["rust"]["types"]}
        case["rust_display_differences"] = [{"file": key[0], "name": key[1], "native": value, "rust": rust_map.get(key)}
                                            for key, value in base_map.items() if rust_map.get(key) != value]
        case["protocol_differences"] = {}
        for protocol in PROTOCOLS[1:]:
            other = case["native"][protocol]
            other_map = native_maps(other, protocol, names)
            require(other["units"] == base["units"], "protocol input mismatch")
            for key in ("loaded_inputs_sha256", "diagnostics", "declarations", "emit_diagnostics"):
                require(other["orders"][0][key] == base["orders"][0][key], "render protocol changes " + key)
            case["protocol_differences"][protocol] = [{"file": key[0], "name": key[1], "baseline": value, "other": other_map[key]}
                                                       for key, value in base_map.items() if value != other_map[key]]
        codes = [d["code"] for d in base["orders"][0]["diagnostics"]]
        require(codes == ([2322] if family == "negative" else []), "native diagnostic control ineffective")
        require(not base["orders"][0]["emit_diagnostics"], "native declaration errors")
        case["complete"] = True
        cost.write(output, result)
        print(json.dumps({"family": family, "unit_bytes_match": case["unit_bytes_match"],
                          "rust_differences": len(case["rust_display_differences"]),
                          "broken_protocol_differences": {key: len(rows) for key, rows in case["protocol_differences"].items()}}), flush=True)
    by_name = {case["family"]: case for case in result["cases"]}
    require(by_name["defaults"]["protocol_differences"]["missing-enclosing"], "missing enclosing node control ineffective")
    require(by_name["unique-symbol"]["protocol_differences"]["missing-flags"], "wrong flags control ineffective")
    result["complete"] = True
    result["all_rust_displays_match"] = all(not case["rust_display_differences"] for case in result["cases"])
    result["all_unit_bytes_match"] = all(case["unit_bytes_match"] for case in result["cases"])
    cost.write(output, result)
    cost.write(directory / "summary.json", summarize(result, output))


def main():
    parser = argparse.ArgumentParser()
    for name in ("native", "rust", "native-build", "rust-build", "directory"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    run(*(getattr(args, name.replace("-", "_")).resolve() for name in ("native", "rust", "native-build", "rust-build", "directory")))


if __name__ == "__main__":
    main()
