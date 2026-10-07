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
PINNED_NATIVE_SHA = "5b1047d10d32e7d5b446be4de56b126ff42f82bb"


def qualified_source_paths(root: Path, producer: str) -> list[Path]:
    """Exact observed writers and policy consumers, not binary-build attestation."""
    native = root / "vendor/typescript-go/internal"
    if producer == "tsgo":
        return [native / path for path in (
            "compiler/program.go", "compiler/checkerpool.go", "checker/checker.go",
            "checker/tracer.go", "checker/relater.go", "tracing/tracing.go", "execute/tsc.go")]
    return [root / path for path in (
        "crates/tsr-execute/src/compile.rs", "crates/tsr-execute/src/checker_pool.rs",
        "crates/tsr-execute/src/work_trace.rs", "crates/tsr-execute/src/os_system.rs",
        "crates/tsr-checker/src/work_trace.rs", "crates/tsr-checker/src/checker.rs",
        "crates/tsr-checker/src/symbols.rs",
        "crates/tsr-checker/src/declared.rs")]


def validate_native_trace(path: Path, receipt: dict) -> dict:
    """Read pinned --generateTrace's unsampled full workers, not query counts.

    Synthetic pid=1/tid values are not OS identities. The supervising receipt
    binds this fresh file to the actual child. X events are sampled and cannot
    establish all worker executions or any cache hit/miss/publication policy.
    """
    result = empty_result()
    result.update(native_trace_valid=False, completed_full_workers=[], operation_counters={},
                  highest_observed_native_worker=None)
    try:
        child = validate_receipt(receipt, "Failed to")
        require(receipt["oracle_sha"] == PINNED_NATIVE_SHA, "native revision is not pinned")
        require("--generateTrace" in child["command"], "missing native trace invocation")
        require(file_hash(path) == receipt["trace_sha256"], "native trace artifact changed")
        with regular_file(path) as stream:
            rows = json.load(stream, object_pairs_hook=unique_object)
        require(isinstance(rows, list) and rows, "missing native trace records")
        stacks, spans, seen, counters = {}, [], set(), {}
        for row in rows:
            require(isinstance(row, dict), "invalid native trace event")
            phase = row["ph"]
            timestamp = row["ts"]
            require(type(timestamp) in (int, float) and timestamp >= 0
                    and timestamp < float("inf"), "invalid native timestamp")
            if phase == "M":
                continue  # Metadata timestamps are intentionally backdated.
            require(phase in ("B", "E", "X", "I"), "unsupported native event phase")
            require(integer(row["pid"]) and integer(row["tid"]), "invalid synthetic native identity")
            name, args = row.get("name"), row.get("args", {})
            require(isinstance(args, dict) and (name is None or isinstance(name, str)),
                    "invalid native operation/arguments")
            if phase == "X":
                duration = row["dur"]
                require(type(duration) in (int, float) and 0 <= duration < float("inf"),
                        "invalid sampled native duration")
            if name is not None:
                counter = counters.setdefault(name, {"begins": 0, "completed": 0, "sampled": 0})
                if phase == "B":
                    counter["begins"] += 1
                elif phase == "X":
                    counter["sampled"] += 1
            if phase not in ("B", "E"):
                continue
            key = (row["pid"], row["tid"])
            stack = stacks.setdefault(key, [])
            if phase == "B":
                stack.append(row)
                continue
            require(stack, "orphan native end event")
            begin = stack.pop()
            begin_args = begin.get("args", {})
            # relater.getVariancesWorker adds its completed output to end args.
            end_identity = {key: value for key, value in args.items()
                            if not (name == "getVariancesWorker" and key == "variances")}
            require(name == begin.get("name") and end_identity == begin_args
                    and row["cat"] == begin["cat"] and timestamp >= begin["ts"],
                    "native end differs from begun operation")
            counters[name]["completed"] += 1
            if name == "checkSourceFile":
                owner, source = args["checkerId"], args["path"]
                require(integer(owner) and isinstance(source, str) and source,
                        "missing private checker/source identity")
                require((owner, source) not in seen, "duplicate completed native full worker")
                seen.add((owner, source))
                spans.append({"path": source, "checker_id": owner,
                              "duration_ns": round((timestamp - begin["ts"]) * 1000)})
        require(all(not stack for stack in stacks.values()), "unfinished native spans")
        result.update(native_trace_valid=True, artifact_integrity_valid=True,
                      completed_full_workers=spans, operation_counters=counters,
                      highest_observed_native_worker=max(spans, key=lambda row: row["duration_ns"], default=None))
        result["limitations"].extend([
            "Native trace does not publish eligibility, cancellation, query/cache counters or Program diagnostics metadata.",
            "checkSourceFile duration includes nested checking; sampled X events cannot rank all inner workers.",
            "Separate trace invocation is not proof of actual work in timed invocations.",
        ])
    except (OSError, ValueError, KeyError, TypeError, AttributeError, RecursionError) as error:
        result["reasons"].append(f"{type(error).__name__}: {error}")
    return result


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
        "checked_file_ids": [], "program_files": [], "operation_counters": {},
        "checker_instances_created": 0,
        "unmapped_queries_observed": False,
    }


def validate_receipt(receipt: dict, warning: str) -> dict:
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
    require(warning not in child["stderr"], "producer reported trace failure")
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
    return child


def validate_trace(path: Path, receipt: dict, *, inventory_only: bool = False) -> dict:
    """Stream records, retaining only file identities and currently active spans.

    Supports the shipped serial TSR producer. Native traces need their own
    source-qualified envelope; an unknown producer/schema is never guessed.
    Missing policy facts in an older schema-1 artifact are explicitly rejected.
    """
    result = empty_result()
    try:
        child = validate_receipt(receipt, "TSR work trace")
        command = child["command"]
        require(not inventory_only or "--listFilesOnly" in command, "inventory-only mode lacks command evidence")
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
                    result["program_files"].append(row)
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
                    counter = result["operation_counters"].setdefault(operation, {"begins": 0, "completed": 0})
                    counter["begins"] += 1
                elif event == "work_end":
                    require(phase == "work" and integer(row["span_id"]) and row["span_id"] in active,
                            "orphan or duplicate completion")
                    require(row["outcome"] == "returned", "work panicked or did not return")
                    operation = active.pop(row["span_id"])
                    result["operation_counters"][operation]["completed"] += 1
                    if operation == "source_file_check":
                        full_active -= 1
                elif event == "invocation_end":
                    require((phase == "files" if inventory_only else phase == "work")
                            and not active and full_active == 0, "incomplete invocation/spans")
                    require([file["path"] for file in files] == receipt["loaded_files"]
                            and library_count <= len(files), "incomplete final Program inventory")
                    require(row["state"] == "complete" and row["semantic_program_observed"] is True,
                            "no completed semantic Program")
                    for key, expected in (("exit_code", child["exit_code"]), ("checker_instances_created", 0 if inventory_only else 1),
                                          ("peak_full_checks", peak), ("unfinished_spans", 0)):
                        require(type(row[key]) is int and row[key] == expected, "incorrect cumulative " + key)
                    require(checked == (set() if inventory_only else
                            {file["file_id"] for file in files if file["full_check_eligible"]}),
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


class ActivityIntervals:
    """Integrate distinct private-checker unions, never sum nested span times."""

    def __init__(self):
        self.time = 0
        self.depths = {name: {} for name in ("constructing", "semantic", "full", "leased", "observed")}
        self.summary = {name: {"peak": 0, "wall_ns": 0, "checker_ns": 0} for name in self.depths}

    def advance(self, timestamp):
        require(integer(timestamp) and timestamp >= self.time, "non-monotonic activity timestamp")
        elapsed = timestamp - self.time
        for name, owners in self.depths.items():
            self.summary[name]["wall_ns"] += elapsed if owners else 0
            self.summary[name]["checker_ns"] += elapsed * len(owners)
        self.time = timestamp

    def change(self, owner, operation, delta):
        names = (["constructing", "observed"] if operation == "constructor" else
                 ["leased"] if operation == "lease" else
                 ["semantic", "observed", "full"] if operation == "source_file_check" else
                 ["semantic", "observed"])
        for name in names:
            owners = self.depths[name]
            depth = owners.get(owner, 0) + delta
            require(depth >= 0, "orphan activity interval")
            if depth:
                owners[owner] = depth
            else:
                owners.pop(owner, None)
            self.summary[name]["peak"] = max(self.summary[name]["peak"], len(owners))


def activity_records(path: Path, expected_hash: str):
    digest = hashlib.sha256()
    previous = 0
    with regular_file(path) as stream:
        for raw in stream:
            digest.update(raw)
            row = decode(raw)
            timestamp = row["recorded_at_ns"]
            require(integer(timestamp) and timestamp >= previous, "invalid worker record chronology")
            previous = timestamp
            yield row
    require(digest.hexdigest() == expected_hash, "worker artifact changed")


def validate_worker_activity(path: Path, receipt: dict, producer: str) -> dict:
    """Qualify only the two source-anchored lifecycle schemas (bd tsr-1yb.1.2.3.2.3).

    The trusted supervisor owns receipt freshness and provenance capture. These
    bounded observations establish neither required work nor safe admission.
    """
    result = empty_result()
    result.update(worker_activity_valid=False, safe_memory_admission_verified=False,
                  activity=None, idle_checker_instances=0, full_file_affinity=[])
    try:
        require(producer in ("tsr", "native"), "unsupported worker producer")
        child = validate_receipt(receipt, "TSR work trace" if producer == "tsr" else
                                 "native worker activity warning:")
        records = activity_records(path, receipt["trace_sha256"])
        header = next(records, None)
        require(header is not None and header["event"] == "invocation_start", "missing worker invocation start")
        end = None
        require(type(header["schema_version"]) is int and header["schema_version"] == 1,
                "unsupported worker schema")
        require(type(header["pid"]) is int and header["pid"] == child["pid"], "replayed worker PID")
        require(header["args"] == child["command"][1:], "worker arguments changed")
        for key in ("all_forcing_observed", "initialization_forcing_observed", "complete_provenance_verified"):
            require(header[key] is False, "unsupported worker coverage: " + key)
        activity = ActivityIntervals()
        activity.advance(header["recorded_at_ns"])
        if producer == "tsr":
            require(header["producer"] == "tsr-work-trace"
                    and type(header["worker_activity_schema_version"]) is int
                    and header["worker_activity_schema_version"] == 1
                    and header["activity_clock"] == "monotonic_elapsed_ns", "missing TSR activity schema")
            nonce = re.fullmatch(str(child["pid"]) + r"-([0-9]+)", header["invocation_id"])
            require(header["invocation_id"] == receipt["invocation_id"] and nonce is not None
                    and int(nonce[1]) >= child["started_at_unix_ns"], "stale TSR worker invocation")
            list_only = "--listFilesOnly" in header["args"]
            base = validate_trace(path, receipt, inventory_only=list_only)
            require(base["artifact_integrity_valid"], "TSR work artifact invalid: " + str(base["reasons"]))
            result.update(base)
            active, created, semantic = {}, set(), set()
            for row in records:
                require(end is None, "records follow worker invocation completion")
                event, timestamp = row["event"], row["recorded_at_ns"]
                if list_only:
                    require(event in ("program", "program_file", "invocation_end"), "work in list-only mode")
                if event == "invocation_end":
                    end = row
                if event == "checker_created":
                    start, finish = row["construction_started_at_ns"], row["construction_finished_at_ns"]
                    require(integer(start) and integer(finish) and activity.time <= start <= finish <= timestamp,
                            "invalid TSR constructor interval")
                    owner = row["checker_id"]
                    require(type(owner) is int and owner == 0 and owner not in created, "duplicate TSR owner")
                    activity.advance(start)
                    activity.change(owner, "constructor", 1)
                    activity.advance(finish)
                    activity.change(owner, "constructor", -1)
                    created.add(owner)
                activity.advance(timestamp)
                if event == "work_begin":
                    owner, operation = row["checker_id"], row["operation"]
                    require(owner in created and row["span_id"] not in active, "unknown TSR span owner")
                    active[row["span_id"]] = (owner, operation)
                    semantic.add(owner)
                    activity.change(owner, operation, 1)
                    if operation == "source_file_check":
                        result["full_file_affinity"].append([0, row["file_ids"][0], owner])
                elif event == "work_end":
                    owner, operation = active.pop(row["span_id"])
                    activity.change(owner, operation, -1)
            require(end is not None and not active, "missing TSR worker completion")
            require(end["state"] == "complete" and type(end["exit_code"]) is int
                    and end["exit_code"] == child["exit_code"], "incomplete TSR child")
            if list_only:
                require(end["semantic_program_observed"] is True and type(end["unfinished_spans"]) is int
                        and end["unfinished_spans"] == 0
                        and end["peak_full_checks"] == 0, "invalid list-only completion")
                for key in ("all_forcing_observed", "complete_provenance_verified", "actual_work_equivalence_verified"):
                    require(end[key] is False, "unsupported list-only verification claim")
                require(end["memory_admission_budget"] is None, "unsupported list-only memory claim")
            for name, key in (("constructing", "peak_constructing_checkers"),
                              ("semantic", "peak_covered_semantic_checkers"),
                              ("full", "peak_full_checkers"), ("observed", "peak_observed_checkers")):
                require(type(end[key]) is int and end[key] == activity.summary[name]["peak"],
                        "incorrect TSR activity peak: " + key)
            require(type(end["unfinished_constructions"]) is int and end["unfinished_constructions"] == 0
                    and end["construction_started_at_ns"] is None, "unfinished TSR construction")
            require(type(end["checker_instances_created"]) is int and end["checker_instances_created"] == len(created),
                    "incorrect TSR instance count")
            activity.summary["leased"] = None  # TSR does not observe exclusive leases.
        else:
            require(header["clock"] == "monotonic_elapsed_ns" and header["memory_admission_budget"] is None
                    and header["actual_work_equivalence_verified"] is False, "unsupported native coverage")
            require(re.fullmatch(r"[0-9a-f]{32}", header["nonce"]) is not None
                    and header["nonce"] == receipt["invocation_id"], "replayed native invocation")
            require(header["observed_operations"] == ["constructor", "lease", "source_file_check",
                    "symbol_type_query", "declared_type_query"], "unsupported native operations")
            pools, created, constructors, returned, native_ids = {}, set(), set(), set(), set()
            active, assignments, full, semantic = {}, {}, set(), set()
            next_token = 0
            for row in records:
                require(end is None, "records follow worker invocation completion")
                activity.advance(row["recorded_at_ns"])
                event = row["event"]
                if event == "invocation_end":
                    end = row
                    continue
                if event == "pool_selected":
                    pool, count, files = row["pool_id"], row["selected_count"], row["program_file_count"]
                    require(integer(pool) and pool == len(pools) and integer(count) and integer(files),
                            "invalid native pool count/identity")
                    requested, single = row["requested_checkers"], row["single_threaded"]
                    require((requested is None or type(requested) is int) and type(single) is bool
                            and requested == receipt["requested_checkers"]
                            and single is (receipt["requested_single_threaded"] is True)
                            and row["memory_admission_budget"] is None, "native worker options changed")
                    expected = max(min(1 if single else 4 if requested is None else requested, files, 256), 1)
                    require(count == expected, "incorrect native selection/clamp")
                    pools[pool] = {"count": count, "files": [], "file_count": files}
                elif event == "span_end":
                    token = row["token"]
                    require(integer(token) and token in active and row["aborted"] is False,
                            "orphan/aborted native interval")
                    owner, operation = active.pop(token)
                    activity.change(owner, operation, -1)
                    if operation == "constructor":
                        returned.add(owner)
                else:
                    pool = row["pool_id"]
                    require(integer(pool) and pool in pools, "unknown native pool")
                    selected = pools[pool]
                    if event == "program_file":
                        require(integer(row["file_id"]) and row["file_id"] == len(selected["files"])
                                and len(selected["files"]) < selected["file_count"]
                                and isinstance(row["path"], str) and row["path"], "invalid native file inventory")
                        selected["files"].append(row["path"])
                        continue
                    slot = row["slot"]
                    owner = (pool, slot)
                    require(integer(slot) and slot < selected["count"]
                            and selected["files"] == receipt["loaded_files"], "native slot/loaded inventory mismatch")
                    if event == "checker_created":
                        identity = row["native_checker_id"]
                        require(owner in returned and owner not in created and integer(identity)
                                and identity <= 0xffffffff and identity not in native_ids, "duplicate native checker ownership")
                        created.add(owner)
                        native_ids.add(identity)
                    elif event == "file_affinity":
                        file = row["file_id"]
                        require(integer(file) and file < selected["file_count"] and owner in created
                                and slot == file % selected["count"] and (pool, file) not in assignments,
                                "invalid/duplicate native file affinity")
                        require(sum(p == pool for p, _ in created) == selected["count"],
                                "native affinity published before all constructors return")
                        assignments[pool, file] = slot
                    elif event == "nonexclusive_access":
                        require(owner in created and row["exclusive_ownership_observed"] is False,
                                "unsupported nonexclusive ownership")
                    elif event == "span_begin":
                        token, operation, file = row["token"], row["operation"], row["file_id"]
                        require(integer(token) and token == next_token, "replayed native span identity")
                        next_token += 1
                        require(operation in header["observed_operations"] and type(file) is int
                                and (file == -1 or 0 <= file < selected["file_count"]), "invalid native operation/file")
                        if operation == "constructor":
                            require(owner not in constructors and file == -1, "duplicate native constructor")
                            constructors.add(owner)
                        else:
                            require(owner in created, "work before native constructor return")
                            if operation == "lease":
                                require(owner not in activity.depths["leased"] and file == -1,
                                        "overlapping exclusive native lease")
                            else:
                                semantic.add(owner)
                            if operation == "source_file_check":
                                require(file >= 0 and assignments.get((pool, file)) == slot
                                        and (pool, file) not in full, "wrong/duplicate full-file owner")
                                full.add((pool, file))
                                result["full_file_affinity"].append([pool, file, slot])
                        active[token] = (owner, operation)
                        activity.change(owner, operation, 1)
                    else:
                        raise ValueError("unknown native event: " + str(event))
            require(end is not None, "missing native worker completion")
            require(not active and returned == created == constructors, "incomplete native construction/work")
            for pool, selected in pools.items():
                count = sum(p == pool for p, _ in created)
                require(count in (0, selected["count"]) and len(selected["files"]) == selected["file_count"]
                        and selected["files"] == receipt["loaded_files"], "incomplete native pool/inventory")
                require(sum(p == pool for p, _ in assignments) == (selected["file_count"] if count else 0),
                        "incomplete native affinity")
            require(end["complete"] is True and type(end["status"]) is int
                    and end["status"] == child["exit_code"] and type(end["unfinished_spans"]) is int
                    and end["unfinished_spans"] == 0 and end["all_forcing_observed"] is False,
                    "incomplete native invocation")
            require(end["peaks"] == {name: summary["peak"] for name, summary in activity.summary.items()}
                    and all(type(value) is int for value in end["peaks"].values()), "incorrect native activity peaks")
        result.update(worker_activity_valid=True, artifact_integrity_valid=True, checker_instances_created=len(created),
                      idle_checker_instances=len(created - semantic), activity=activity.summary)
        result["full_file_affinity"].sort()
    except (OSError, ValueError, KeyError, TypeError, AttributeError, RecursionError) as error:
        result["reasons"].append(f"{type(error).__name__}: {error}")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trace", required=True, type=Path)
    parser.add_argument("--receipt", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--worker-producer", choices=("tsr", "native"))
    parser.add_argument("--native-trace", action="store_true", help="read pinned native --generateTrace JSON")
    args = parser.parse_args()
    try:
        with regular_file(args.receipt) as stream:
            receipt = decode(stream.read())
        result = (validate_native_trace(args.trace, receipt) if args.native_trace else
                  validate_worker_activity(args.trace, receipt, args.worker_producer)
                  if args.worker_producer else validate_trace(args.trace, receipt))
    except (OSError, ValueError, TypeError, RecursionError) as error:
        result = empty_result()
        result["reasons"].append(f"Invalid supervising receipt: {type(error).__name__}: {error}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    return 0 if result.get("worker_activity_valid", result["artifact_integrity_valid"]) else 1


if __name__ == "__main__":
    raise SystemExit(main())
