#!/usr/bin/env python3
"""Source-qualified archived JSDoc attribution; never qualifies deferral speed."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from benchmark_inputs import file_hash, regular_file, snapshot, valid_snapshot
from whole_project_perf import diagnostics, fingerprint, process


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")
    require(json.loads(path.read_text()) == value, "persisted result differs")


def files(child):
    return [line for line in child["stdout"].splitlines()
            if line.startswith(("/", "bundled:///")) and "error TS" not in line]


def counts(child):
    result = {}
    for line in child["stdout"].splitlines():
        for label in ("Files", "Checked files", "Parsed files"):
            if line.startswith(label + ":"):
                result[label] = int(line.split(":", 1)[1].strip())
    return result


def read_probe(path, child):
    require(not child["timed_out"] and child["exit_code"] in (0, 1), "incomplete trace child")
    with regular_file(path) as stream:
        data = stream.read()
    require(data.endswith(b"\n"), "partial final trace record")
    lines = data.decode("utf-8").splitlines()
    header = lines[0].split("\t") if lines else []
    require(len(header) == 3 and header[:2] == ["tsr-jsdoc-cost-v2", str(child["pid"])],
            "trace PID/version mismatch")
    require(int(header[2]) >= child["started_at_unix_ns"], "stale same-PID trace")
    names = ["parse_ns", "parse_arena_requested_bytes", "jsdoc_ns", "jsdoc_arena_requested_bytes",
             "eligible_leading_calls", "comments", "comment_range_bytes", "documented_nodes", "table_capacity_bytes"]
    rows = []
    for line in lines[1:]:
        values = line.split("\t")
        require(len(values) == 12, "invalid trace row width")
        name = bytes.fromhex(values[0]).decode("utf-8")
        require(values[0] == name.encode().hex(), "noncanonical filename encoding")
        require(values[2] in ("TypeScript", "Tsx", "Json"), "unknown parser dialect")
        numbers = [int(value) for value in values[3:]]
        require(all(value >= 0 for value in numbers) and int(values[1]) >= 0, "negative attribution")
        row = {"file": name, "source_bytes": int(values[1]), "parser_kind": values[2], **dict(zip(names, numbers))}
        require(row["jsdoc_ns"] <= row["parse_ns"], "JSDoc time exceeds enclosing parse")
        require(row["jsdoc_arena_requested_bytes"] <= row["parse_arena_requested_bytes"], "JSDoc bytes exceed parse")
        require(row["table_capacity_bytes"] >= row["documented_nodes"], "impossible sparse table capacity")
        rows.append(row)
    require(len(rows) == counts(child).get("Parsed files"), "trace does not cover every compiler file parse")
    return {"trace_sha256": hashlib.sha256(data).hexdigest(), "rows": rows,
            "totals": {key: sum(row[key] for row in rows) for key in names},
            "categories": {category: {key: sum(row[key] for row in rows if category_of(row["file"]) == category)
                                      for key in names}
                           for category in ("declaration", "tsx", "js", "ts", "json")}}


def category_of(name):
    name = name.lower()
    if name.endswith((".d.ts", ".d.mts", ".d.cts")):
        return "declaration"
    if name.endswith(".tsx"):
        return "tsx"
    if name.endswith((".js", ".jsx", ".mjs", ".cjs")):
        return "js"
    if name.endswith(".json"):
        return "json"
    return "ts"


def measure(binary, project, output, probe=False, repeats=3):
    output.parent.mkdir(parents=True, exist_ok=True)
    for key in ("TSR_WORK_TRACE", "TSR_NATIVE_WORK_ACTIVITY", "TSR_JSDOC_COST_PROBE"):
        os.environ.pop(key, None)
    command = [str(binary.resolve()), "--project", "tsconfig.json", "--noEmit", "--incremental", "false",
               "--composite", "false", "--pretty", "false", "--extendedDiagnostics", "--listFiles"]
    show = process([str(binary.resolve()), "--project", "tsconfig.json", "--showConfig", "--incremental", "false",
                    "--composite", "false", "--noEmit"], project, 180)
    require(show["exit_code"] == 0 and not show["timed_out"], "showConfig failed")
    config = json.loads(show["stdout"])
    children = []
    result = {"binary_sha256": file_hash(binary), "project": str(project.resolve()), "config": config,
              "config_fingerprint": fingerprint(config), "command": command, "children": children}
    preflight = process(command, project, 180)
    require(not preflight["timed_out"] and preflight["exit_code"] in (0, 1), "scope preflight failed")
    names = files(preflight)
    observed_paths = [name for name in names if name.startswith("/")] + [str(project / "tsconfig.json")]
    result["preflight"] = preflight
    result["loaded_input_snapshot_before"] = snapshot(observed_paths)
    require(valid_snapshot(result["loaded_input_snapshot_before"]) and
            all(row["kind"] == "file" for row in result["loaded_input_snapshot_before"]),
            "unreadable or missing observed source/config")
    expected = None
    for index in range(repeats):
        trace = output.parent / (output.stem + f"-{index}.tsv") if probe else None
        if trace:
            os.environ["TSR_JSDOC_COST_PROBE"] = str(trace)
        child = process(command, project, 180)
        require(not child["timed_out"] and child["exit_code"] in (0, 1), "compiler process failed")
        child["diagnostics"] = diagnostics(child["stdout"] + child["stderr"], project)
        child["loaded"] = files(child)
        child["counts"] = counts(child)
        require(child["loaded"] and child["counts"].get("Checked files", 0) > 0, "no observed checking")
        require(child["loaded"] == names, "measured loaded order differs from input preflight")
        signature = {key: child[key] for key in ("diagnostics", "loaded", "counts", "exit_code")}
        if expected is None:
            expected = signature
        require(signature == expected, "repeat changes complete output or reported scope")
        if trace:
            child["probe"] = read_probe(trace, child)
        children.append(child)
        # Raw process evidence is persisted before another sample starts.
        write(output, result)
    os.environ.pop("TSR_JSDOC_COST_PROBE", None)
    summary = {key: statistics.median(child[key] for child in children)
               for key in ("wall_seconds", "user_seconds", "system_seconds", "peak_rss_bytes")}
    summary.update(complete_process=True, outputs_preserved=True)
    result["summary"] = summary
    result["wall_range"] = [min(child["wall_seconds"] for child in children), max(child["wall_seconds"] for child in children)]
    # This fingerprint is loaded-file coverage only; missing candidates/query inputs remain separate.
    physical = [name for name in children[0]["loaded"] if name.startswith("/")]
    result["loaded_input_snapshot_after"] = snapshot(physical + [str(project / "tsconfig.json")])
    require(result["loaded_input_snapshot_before"] == result["loaded_input_snapshot_after"], "observed inputs changed")
    require(file_hash(binary) == result["binary_sha256"], "binary changed during measurements")
    by_path = {row["path"]: row for row in result["loaded_input_snapshot_after"]}
    result["ordered_physical_source_identity"] = [
        [by_path[name]["realpath"], by_path[name]["sha256"]]
        if name in by_path else [name, None] for name in children[0]["loaded"]]
    write(output, result)
    return result


def fixtures(directory):
    directory.mkdir(parents=True, exist_ok=False)
    contents = {
        "tsconfig.json": json.dumps({"compilerOptions": {"strict": True, "allowJs": True, "checkJs": True,
                                                         "jsx": "preserve", "skipLibCheck": True, "target": "es2022"},
                                      "files": ["ordinary.ts", "links.ts", "view.tsx", "ambient.d.ts", "checked.js", "imports.js"]}, indent=2) + "\n",
        "base.ts": "export interface Item { value: number }\n",
        "ordinary.ts": "/** Ordinary TS documentation with @param ignored text. */\nexport function plain(x: number) { return x; }\n",
        "links.ts": "/** @deprecated use NewMarker */\nexport interface Marker { x: number }\n/** {@link Marker} */\nexport interface Linked { marker: Marker }\n",
        "view.tsx": "/** JSX doc text. */\ndeclare namespace JSX { interface IntrinsicElements { span: { id?: string } } }\nconst view = <span id=\"doc\" />;\n",
        "ambient.d.ts": "\n".join(f"/** Documented interface {i}; @param value numeric. */\ndeclare interface Documented{i} {{ /** Value docs. */ value: number; }}" for i in range(512)) + "\n",
        "checked.js": "/** @template T\n * @param {T} x\n * @returns {T} */\nexport function identity(x) { return x; }\n/** @typedef {{value: number}} RecordValue */\n/** @type {RecordValue} */\nexport const record = {value: 1};\n/** @overload\n * @param {string} x\n * @returns {string} */\n/** @overload\n * @param {number} x\n * @returns {number} */\n/** @param {string | number} x */\nexport function overloaded(x) { return x; }\n",
        "imports.js": "/** @import {Item} from './base' */\n/** @type {Item} */\nexport const item = {value: 1};\n",
    }
    for name, text in contents.items():
        (directory / name).write_text(text)
    manifest = {name: file_hash(directory / name) for name in contents}
    write(directory / "fixture-manifest.json", manifest)
    return manifest


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="action", required=True)
    generate = sub.add_parser("fixtures")
    generate.add_argument("directory", type=Path)
    measurement = sub.add_parser("measure")
    measurement.add_argument("--binary", type=Path, required=True)
    measurement.add_argument("--project", type=Path, required=True)
    measurement.add_argument("--output", type=Path, required=True)
    measurement.add_argument("--probe", action="store_true")
    measurement.add_argument("--repeats", type=int, default=3)
    args = parser.parse_args()
    if args.action == "fixtures":
        print(json.dumps(fixtures(args.directory)))
    else:
        require(args.repeats > 0, "repeat count must be positive")
        result = measure(args.binary, args.project, args.output, args.probe, args.repeats)
        print(json.dumps(result["summary"]))


if __name__ == "__main__":
    main()
