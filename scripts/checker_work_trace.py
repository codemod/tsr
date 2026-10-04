#!/usr/bin/env python3
"""Validate a TSR trace against a supervising process receipt (bd tsr-1yb.1.2.4.1).

The receipt is trusted capture evidence, not something supplied by the compiler.
Rehash its bound files; never promote caller booleans or compiler hash claims to
build provenance, complete inputs, semantic equivalence or speed acceptance.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re

INPUT_SPEC = importlib.util.spec_from_file_location(
    "benchmark_inputs", Path(__file__).with_name("benchmark_inputs.py"))
inputs = importlib.util.module_from_spec(INPUT_SPEC)
INPUT_SPEC.loader.exec_module(inputs)
file_hash, regular_file = inputs.file_hash, inputs.regular_file
snapshot, valid_snapshot = inputs.snapshot, inputs.valid_snapshot

OPERATIONS = {"source_file_check", "symbol_type_query", "declared_type_query",
              "variable_type_worker"}
SHA256 = re.compile(r"[0-9a-f]{64}\Z")


def require(condition: bool, reason: str) -> None:
    if not condition:
        raise ValueError(reason)


def integer(value: object) -> bool:
    return type(value) is int and value >= 0


def optional_bool(value: object) -> bool:
    return value is None or type(value) is bool


def unique_object(pairs: list) -> dict:
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON key: " + key)
        result[key] = value
    return result


def decode(raw: bytes | str) -> dict:
    def invalid_constant(value: str) -> None:
        raise ValueError("non-JSON numeric constant: " + value)
    value = json.loads(raw, object_pairs_hook=unique_object, parse_constant=invalid_constant)
    require(isinstance(value, dict), "JSON record must be an object")
    return value


def exclusion(options: dict, file: dict, library_count: int) -> str | None:
    if options["no_check"] is True:
        return "no_check"
    if options["skip_lib_check"] is True and file["declaration_file"]:
        return "skip_lib_check"
    if options["skip_default_lib_check"] is True and file["file_id"] < library_count:
        return "skip_default_lib_check"
    if file["check_js_directive"] is False:
        return "file_no_check"
    if file["json_source"]:
        return "json_source"
    if file["javascript_source"] and file["check_js_directive"] is None \
            and options["check_js"] is False:
        return "check_js_false"
    if file["source_node_id"] is None:
        return "missing_source_node"
    return None


def empty_result() -> dict:
    return {
        "schema_version": 1, "artifact_integrity_valid": False, "reasons": [],
        "actual_checked_work_verified": False, "complete_input_equivalence_verified": False,
        "complete_provenance_verified": False, "target_verified": False,
        "limitations": ["Supervising-process receipt authenticity is outside this reader.",
                        "Source file hashes do not prove a binary was built from those files.",
                        "Input snapshots are partial and cannot detect all transient mutations.",
                        "Initialization/all forcing and native worker admission remain unverified."],
        "checked_file_ids": [], "checker_instances_created": 0,
        "unmapped_queries_observed": False,
    }


def validate_trace(path: Path, receipt: dict) -> dict:
    """Stream records, retaining only file identities and currently active spans.

    Supports the shipped serial TSR producer. Native traces need their own
    source-qualified envelope; an unknown producer/schema is never guessed.
    Missing policy facts in an older schema-1 artifact are explicitly rejected.
    """
    result = empty_result()
    try:
        require(type(receipt.get("schema_version")) is int and receipt["schema_version"] == 1,
                "unsupported receipt schema")
        child = receipt["child"]
        require(integer(child["pid"]) and child["pid"] > 0, "invalid actual child PID")
        require(integer(child["started_at_unix_ns"]) and child["started_at_unix_ns"] > 0,
                "missing actual child start time")
        require(type(child["timed_out"]) is bool and not child["timed_out"], "child timed out")
        require(type(child["exit_code"]) is int and child["exit_code"] in (0, 1, 2),
                "child failed or was signaled")
        require(isinstance(child["stderr"], str), "missing child stderr")
        require("TSR work trace" not in child["stderr"], "producer reported trace failure")
        command = child["command"]
        require(isinstance(command, list) and command
                and all(isinstance(arg, str) and arg for arg in command), "invalid child command")
        require(SHA256.fullmatch(receipt["binary_sha256"]) is not None
                and file_hash(command[0]) == receipt["binary_sha256"], "executed binary changed")
        sources = receipt["source_files_sha256"]
        require(isinstance(sources, dict) and sources, "missing qualified source/patch files")
        for source, digest in sources.items():
            require(isinstance(source, str) and SHA256.fullmatch(digest) is not None
                    and file_hash(source) == digest, "qualified source/patch changed")
        before, after = receipt["inputs_before"], receipt["inputs_after"]
        require(isinstance(before, list) and before and before == after
                and valid_snapshot(before), "input capture missing, invalid or changed")
        require(snapshot([row["path"] for row in before]) == before,
                "captured inputs no longer match")
        require(isinstance(receipt["loaded_files"], list)
                and all(isinstance(name, str) and name for name in receipt["loaded_files"]),
                "missing ordered loaded identities")
        files, nodes, active, checked = [], set(), {}, set()
        phase, options, library_count = "new", {}, 0
        next_span, full_active, peak = 0, 0, 0
        digest = hashlib.sha256()
        with regular_file(path) as stream:
            for line_number, raw in enumerate(stream, 1):
                digest.update(raw)
                row = decode(raw)
                event = row.get("event")
                require(phase != "ended", "records follow invocation completion")
                if event == "invocation_start":
                    require(phase == "new", "duplicate or reordered invocation start")
                    require(type(row["schema_version"]) is int and row["schema_version"] == 1
                            and row["producer"] == "tsr-work-trace", "unsupported trace producer/schema")
                    require(type(row["pid"]) is int and row["pid"] == child["pid"], "replayed child PID")
                    require(isinstance(receipt["invocation_id"], str) and receipt["invocation_id"]
                            and row["invocation_id"] == receipt["invocation_id"], "replayed invocation")
                    # This producer uses PID + Unix nanoseconds. A PID can be
                    # reused; compare age against the supervising process too.
                    nonce = re.fullmatch(str(child["pid"]) + r"-([0-9]+)", row["invocation_id"])
                    require(nonce is not None and int(nonce[1]) >= child["started_at_unix_ns"],
                            "invalid or stale invocation timestamp")
                    require(row["args"] == command[1:], "invocation command/options changed")
                    for key, expected in (("source_sha_claim", receipt["source_sha"]),
                                          ("binary_sha256_claim", receipt["binary_sha256"])):
                        require(row[key] is None or row[key] == expected, "unqualified " + key)
                    require(isinstance(row["observed_operations"], list)
                            and len(row["observed_operations"]) == len(OPERATIONS)
                            and set(row["observed_operations"]) == OPERATIONS,
                            "unsupported operation coverage")
                    for key in ("all_forcing_observed", "complete_provenance_verified",
                                "initialization_forcing_observed"):
                        require(row[key] is False, "unsupported verification claim: " + key)
                    phase = "started"
                elif event == "program":
                    require(phase == "started", "duplicate or reordered Program")
                    require(row["current_directory"] == receipt["current_directory"],
                            "process working directory changed")
                    require(decode(row["show_config"]) == receipt["show_config"], "effective options changed")
                    options = row["full_check_options"]
                    for field, config_key in (("no_check", "noCheck"), ("skip_lib_check", "skipLibCheck"),
                                              ("skip_default_lib_check", "skipDefaultLibCheck"),
                                              ("check_js", "checkJs")):
                        require(optional_bool(options[field]) and options[field] is
                                receipt["show_config"].get("compilerOptions", {}).get(config_key),
                                "eligibility option contradicts resolved configuration: " + field)
                    library_count = row["default_library_file_count"]
                    require(integer(library_count), "invalid default-library population")
                    for key in ("complete_input_equivalence_verified", "native_eligibility_equivalence_verified"):
                        require(row[key] is False, "unsupported verification claim: " + key)
                    phase = "files"
                elif event == "program_file":
                    require(phase == "files", "file outside Program inventory")
                    file_id = row["file_id"]
                    require(integer(file_id) and file_id == len(files), "duplicate or missing file identity")
                    require(isinstance(row["path"], str) and row["path"], "invalid file path")
                    require(integer(row["text_bytes"]), "invalid source byte count")
                    node = row["source_node_id"]
                    require(node is None or (integer(node) and node <= 0xffffffff and node not in nodes),
                            "invalid or duplicate Program source node")
                    if node is not None:
                        nodes.add(node)
                    for field in ("declaration_file", "javascript_source", "json_source", "full_check_eligible"):
                        require(type(row[field]) is bool, "missing/invalid file policy fact: " + field)
                    require(optional_bool(row["check_js_directive"]), "invalid parsed directive fact")
                    reason = exclusion(options, row, library_count)
                    require(row["full_check_exclusion"] == reason and row["full_check_eligible"] is (reason is None),
                            "file eligibility/exclusion contradicts qualified policy facts")
                    files.append(row)
                elif event == "checker_created":
                    require(phase == "files", "duplicate or reordered checker construction")
                    require([file["path"] for file in files] == receipt["loaded_files"],
                            "Program inventory differs from captured loaded identities")
                    require(library_count <= len(files), "default-library population exceeds Program")
                    require(type(row["checker_id"]) is int and row["checker_id"] == 0
                            and type(row["effective_serial_checker_limit"]) is int
                            and row["effective_serial_checker_limit"] == 1, "unsupported worker lifetime/budget")
                    requested = row["requested_checkers"]
                    require(requested is None or integer(requested), "invalid requested checker count")
                    require(requested == receipt["requested_checkers"]
                            and optional_bool(row["requested_single_threaded"])
                            and row["requested_single_threaded"] is receipt["requested_single_threaded"],
                            "requested worker settings changed")
                    require(row["requested_checkers_matches_actual_instances"] is
                            (None if requested is None else requested == 1), "incorrect request/instance agreement")
                    require(row["worker_options_applied_by_driver"] is False
                            and row["memory_admission_budget"] is None
                            and row["initialization_forcing_observed"] is False, "unsupported worker policy claim")
                    result["checker_instances_created"] = 1
                    phase = "work"
                elif event == "work_begin":
                    require(phase == "work", "work outside checker lifetime")
                    token, operation = row["span_id"], row["operation"]
                    require(integer(token) and token == next_span, "reused or missing span identity")
                    next_span += 1
                    require(operation in OPERATIONS, "unknown work operation")
                    require(type(row["checker_id"]) is int and row["checker_id"] == 0, "unknown checker identity")
                    ids, unmapped = row["file_ids"], row["unmapped_source_node_ids"]
                    require(isinstance(ids, list) and all(integer(i) and i < len(files) for i in ids)
                            and len(set(ids)) == len(ids), "invalid work file references")
                    require(isinstance(unmapped, list) and all(integer(i) and i <= 0xffffffff for i in unmapped)
                            and len(set(unmapped)) == len(unmapped) and not nodes.intersection(unmapped),
                            "invalid unmapped source references")
                    if operation == "source_file_check":
                        require(len(ids) == 1 and not unmapped and files[ids[0]]["full_check_eligible"]
                                and ids[0] not in checked, "duplicate or ineligible full worker")
                        checked.add(ids[0])
                        result["checked_file_ids"].append(ids[0])
                        full_active += 1
                        peak = max(peak, full_active)
                    result["unmapped_queries_observed"] |= bool(unmapped)
                    active[token] = operation
                elif event == "work_end":
                    require(phase == "work" and integer(row["span_id"]) and row["span_id"] in active,
                            "orphan or duplicate completion")
                    require(row["outcome"] == "returned", "work panicked or did not return")
                    if active.pop(row["span_id"]) == "source_file_check":
                        full_active -= 1
                elif event == "invocation_end":
                    require(phase == "work" and not active and full_active == 0, "incomplete invocation/spans")
                    require(row["state"] == "complete" and row["semantic_program_observed"] is True,
                            "no completed semantic Program")
                    for key, expected in (("exit_code", child["exit_code"]), ("checker_instances_created", 1),
                                          ("peak_full_checks", peak), ("unfinished_spans", 0)):
                        require(type(row[key]) is int and row[key] == expected, "incorrect cumulative " + key)
                    require(checked == {file["file_id"] for file in files if file["full_check_eligible"]},
                            "eligible source did not complete a full worker")
                    for key in ("all_forcing_observed", "complete_provenance_verified", "actual_work_equivalence_verified"):
                        require(row[key] is False, "unsupported verification claim: " + key)
                    require(row["memory_admission_budget"] is None, "unsupported memory budget claim")
                    phase = "ended"
                else:
                    raise ValueError("unknown/reordered event: " + str(event))
        require(phase == "ended", "missing normal invocation completion")
        require(digest.hexdigest() == receipt["trace_sha256"], "artifact differs from captured trace bytes")
        result["artifact_integrity_valid"] = True
    except (OSError, ValueError, KeyError, TypeError, AttributeError, RecursionError) as error:
        result["reasons"].append(f"{type(error).__name__}: {error}")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trace", required=True, type=Path)
    parser.add_argument("--receipt", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        with regular_file(args.receipt) as stream:
            receipt = decode(stream.read())
        result = validate_trace(args.trace, receipt)
    except (OSError, ValueError, TypeError, RecursionError) as error:
        result = empty_result()
        result["reasons"].append(f"Invalid supervising receipt: {type(error).__name__}: {error}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    return 0 if result["artifact_integrity_valid"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
