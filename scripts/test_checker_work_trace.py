"""Adversarial controls for one qualified trace artifact, not speed comparability."""

import copy
import hashlib
import json
import os
from pathlib import Path
import tempfile
import subprocess
import sys
import unittest

from benchmark_inputs import snapshot
from checker_work_trace import validate_trace, validate_worker_activity


class TraceIntegrityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.binary = self.root / "compiler"
        self.binary.write_text("qualified binary bytes")
        self.source = self.root / "producer.rs"
        self.source.write_text("qualified producer bytes")
        self.input = self.root / "a.ts"
        self.input.write_text("const value: number = 'wrong';")
        self.trace = self.root / "trace.ndjson"
        self.options = {"compilerOptions": {"skipLibCheck": True, "noEmit": True}}
        self.rows = [
            {"event": "invocation_start", "schema_version": 1,
             "producer": "tsr-work-trace", "pid": 1234, "invocation_id": "1234-99",
             "args": ["--noEmit"], "source_sha_claim": None,
             "binary_sha256_claim": None, "complete_provenance_verified": False,
             "all_forcing_observed": False, "initialization_forcing_observed": False,
             "observed_operations": ["source_file_check", "symbol_type_query",
                                     "declared_type_query", "variable_type_worker"]},
            {"event": "program", "current_directory": str(self.root),
             "case_sensitive": True, "root_files": [str(self.input)],
             "compiler_options_debug": "qualified options", "show_config": json.dumps(self.options),
             "full_check_options": {"no_check": None, "skip_lib_check": True,
                                    "skip_default_lib_check": None, "check_js": None},
             "default_library_file_count": 0,
             "complete_input_equivalence_verified": False,
             "native_eligibility_equivalence_verified": False},
            {"event": "program_file", "file_id": 0, "path": str(self.input),
             "source_node_id": 10, "text_bytes": self.input.stat().st_size,
             "full_check_eligible": True, "full_check_exclusion": None,
             "declaration_file": False, "javascript_source": False,
             "json_source": False, "check_js_directive": None},
            {"event": "checker_created", "checker_id": 0, "requested_checkers": None,
             "requested_single_threaded": None, "effective_serial_checker_limit": 1,
             "requested_checkers_matches_actual_instances": None,
             "worker_options_applied_by_driver": False, "memory_admission_budget": None,
             "initialization_forcing_observed": False},
            {"event": "work_begin", "span_id": 0, "checker_id": 0,
             "operation": "source_file_check", "file_ids": [0],
             "unmapped_source_node_ids": []},
            {"event": "work_begin", "span_id": 1, "checker_id": 0,
             "operation": "symbol_type_query", "file_ids": [],
             "unmapped_source_node_ids": [999]},
            {"event": "work_end", "span_id": 1, "outcome": "returned"},
            {"event": "work_end", "span_id": 0, "outcome": "returned"},
            {"event": "invocation_end", "exit_code": 1, "state": "complete",
             "semantic_program_observed": True, "checker_instances_created": 1,
             "peak_full_checks": 1, "unfinished_spans": 0, "all_forcing_observed": False,
             "complete_provenance_verified": False, "actual_work_equivalence_verified": False,
             "memory_admission_budget": None},
        ]
        self.context = {
            "schema_version": 1,
            "current_directory": str(self.root),
            "child": {"pid": 1234, "command": [str(self.binary), "--noEmit"],
                      "exit_code": 1, "timed_out": False, "stderr": "",
                      "started_at_unix_ns": 98},
            "invocation_id": "1234-99", "source_sha": None,
            "binary_sha256": self.digest(self.binary),
            "source_files_sha256": {str(self.source): self.digest(self.source)},
            "inputs_before": snapshot([str(self.input)]),
            "inputs_after": snapshot([str(self.input)]),
            "show_config": self.options, "loaded_files": [str(self.input)],
            "requested_checkers": None, "requested_single_threaded": None,
        }

    @staticmethod
    def digest(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def check(self, rows=None, context=None, raw=None):
        self.trace.write_text(raw if raw is not None else
                              "".join(json.dumps(row) + "\n" for row in (rows or self.rows)))
        binding = copy.deepcopy(self.context if context is None else context)
        binding["trace_sha256"] = self.digest(self.trace)
        return validate_trace(self.trace, binding)

    def parallel_fixture(self):
        other = self.root / "b.ts"
        other.write_text("export const value = 1;")
        rows = copy.deepcopy(self.rows[:3])
        rows[0].update(schema_version=2, worker_activity_schema_version=2,
                       activity_clock="monotonic_elapsed_ns")
        second = copy.deepcopy(rows[2])
        second.update(file_id=1, path=str(other), source_node_id=11, text_bytes=other.stat().st_size)
        rows.append(second)
        rows.extend([
            {"event": "pool_selected", "selected_count": 2, "program_file_count": 2,
             "requested_checkers": 2, "requested_single_threaded": None,
             "worker_options_applied_by_driver": True, "memory_admission_budget": None,
             "affinity": "program_file_index_modulo_selected_count"},
            {"event": "checker_construction_begin", "checker_id": 0},
            {"event": "checker_construction_begin", "checker_id": 1},
            {"event": "checker_created", "checker_id": 0, "initialization_forcing_observed": False},
            {"event": "work_begin", "span_id": 0, "checker_id": 0,
             "operation": "source_file_check", "file_ids": [0], "unmapped_source_node_ids": []},
            {"event": "work_begin", "span_id": 1, "checker_id": 0,
             "operation": "symbol_type_query", "file_ids": [1], "unmapped_source_node_ids": []},
            {"event": "checker_created", "checker_id": 1, "initialization_forcing_observed": False},
            {"event": "work_begin", "span_id": 2, "checker_id": 1,
             "operation": "source_file_check", "file_ids": [1], "unmapped_source_node_ids": []},
            {"event": "work_end", "span_id": 1, "checker_id": 0, "outcome": "returned"},
            {"event": "work_end", "span_id": 0, "checker_id": 0, "outcome": "returned"},
            {"event": "work_end", "span_id": 2, "checker_id": 1, "outcome": "returned"},
            {**self.rows[-1], "checker_instances_created": 2, "peak_full_checks": 2,
             "peak_constructing_checkers": 2, "peak_covered_semantic_checkers": 2,
             "peak_full_checkers": 2, "peak_observed_checkers": 2, "unfinished_constructions": 0},
        ])
        starts = {}
        for time, row in enumerate(rows):
            row["recorded_at_ns"] = time
            if row["event"] == "checker_construction_begin":
                starts[row["checker_id"]] = time
                row["construction_started_at_ns"] = time
            elif row["event"] == "checker_created":
                row.update(construction_started_at_ns=starts[row["checker_id"]],
                           construction_finished_at_ns=time)
        context = copy.deepcopy(self.context)
        context["show_config"]["compilerOptions"]["checkers"] = 2
        rows[1]["show_config"] = json.dumps(context["show_config"])
        context.update(loaded_files=[str(self.input), str(other)], requested_checkers=2,
                       inputs_before=snapshot([str(self.input), str(other)]),
                       inputs_after=snapshot([str(self.input), str(other)]))
        return rows, context

    def test_parallel_pool_and_activity_are_qualified_without_claiming_equivalence(self):
        rows, context = self.parallel_fixture()
        result = self.check(rows, context)
        self.assertTrue(result["artifact_integrity_valid"], result["reasons"])
        self.assertEqual(result["checked_file_ids"], [0, 1])
        self.assertEqual(result["checker_instances_created"], 2)
        context["trace_sha256"] = self.digest(self.trace)
        activity = validate_worker_activity(self.trace, context, "tsr")
        self.assertTrue(activity["worker_activity_valid"], activity["reasons"])
        for name in ("constructing", "semantic", "full", "observed"):
            self.assertEqual(activity["activity"][name]["peak"], 2)
        self.assertEqual(activity["full_file_affinity"], [[0, 0, 0], [0, 1, 1]])
        self.assertFalse(activity["actual_checked_work_verified"])
        self.assertFalse(activity["target_verified"])

    def test_parallel_wrong_affinity_missing_constructors_and_foreign_completions_fail(self):
        for index, key, value in [(4, "selected_count", 1), (7, "checker_id", 1),
                                  (8, "checker_id", 1), (12, "checker_id", 1),
                                  (0, "schema_version", 1), (15, "checker_instances_created", 1)]:
            rows, context = self.parallel_fixture()
            rows[index][key] = value
            with self.subTest(index=index, key=key):
                self.assertFalse(self.check(rows, context)["artifact_integrity_valid"])
        rows, context = self.parallel_fixture()
        rows.pop(6)
        self.assertFalse(self.check(rows, context)["artifact_integrity_valid"])

    def test_parallel_peak_cannot_count_nested_queries_as_extra_checkers(self):
        rows, context = self.parallel_fixture()
        rows[-1]["peak_covered_semantic_checkers"] = 3
        self.check(rows, context)
        context["trace_sha256"] = self.digest(self.trace)
        result = validate_worker_activity(self.trace, context, "tsr")
        self.assertFalse(result["worker_activity_valid"])

    def test_parallel_same_owner_completions_follow_nested_entry_order(self):
        rows, context = self.parallel_fixture()
        rows[12], rows[13] = rows[13], rows[12]
        for timestamp, row in enumerate(rows):
            row["recorded_at_ns"] = timestamp
        self.assertFalse(self.check(rows, context)["artifact_integrity_valid"])

    def test_parallel_worker_request_cannot_contradict_effective_options(self):
        rows, context = self.parallel_fixture()
        context["show_config"]["compilerOptions"]["checkers"] = 1
        rows[1]["show_config"] = json.dumps(context["show_config"])
        self.assertFalse(self.check(rows, context)["artifact_integrity_valid"])

    def test_parallel_worker_options_omitted_by_show_config_use_trusted_request(self):
        rows, context = self.parallel_fixture()
        context["show_config"]["compilerOptions"].pop("checkers")
        rows[1]["show_config"] = json.dumps(context["show_config"])
        self.assertTrue(self.check(rows, context)["artifact_integrity_valid"])

    def test_parallel_constructor_cannot_reintroduce_serial_policy_facts(self):
        rows, context = self.parallel_fixture()
        rows[7].update(effective_serial_checker_limit=1, worker_options_applied_by_driver=False)
        self.assertFalse(self.check(rows, context)["artifact_integrity_valid"])

    def test_parallel_nonpositive_requests_clamp_to_one_native_owner(self):
        for requested in (0, -1):
            rows, context = self.parallel_fixture()
            context["requested_checkers"] = requested
            context["show_config"]["compilerOptions"]["checkers"] = requested
            rows[1]["show_config"] = json.dumps(context["show_config"])
            rows[4].update(requested_checkers=requested, selected_count=1)
            # One returned constructor, then two sequential full-file workers.
            rows = [*rows[:6], rows[7], rows[8], rows[9], rows[12], rows[13],
                    rows[11], rows[14], rows[15]]
            rows[11]["checker_id"] = 0
            rows[12]["checker_id"] = 0
            rows[11]["span_id"] = rows[12]["span_id"] = 2
            rows[-1].update(checker_instances_created=1, peak_full_checks=1,
                           peak_constructing_checkers=1, peak_covered_semantic_checkers=1,
                           peak_full_checkers=1, peak_observed_checkers=1)
            for timestamp, row in enumerate(rows):
                row["recorded_at_ns"] = timestamp
            rows[5]["construction_started_at_ns"] = 5
            rows[6].update(construction_started_at_ns=5, construction_finished_at_ns=6)
            with self.subTest(requested=requested):
                self.assertTrue(self.check(rows, context)["artifact_integrity_valid"])

    def reject(self, rows=None, context=None, raw=None):
        result = self.check(rows, context, raw)
        self.assertFalse(result["artifact_integrity_valid"], result)
        self.assertTrue(result["reasons"], result)
        self.assertFalse(result["actual_checked_work_verified"])

    def test_complete_artifact_keeps_all_broader_gates_false(self):
        result = self.check()
        self.assertTrue(result["artifact_integrity_valid"], result)
        self.assertEqual(result["checked_file_ids"], [0])
        self.assertEqual(result["checker_instances_created"], 1)
        self.assertTrue(result["unmapped_queries_observed"])
        for gate in ("actual_checked_work_verified", "complete_input_equivalence_verified",
                     "complete_provenance_verified", "target_verified"):
            self.assertIs(result[gate], False)

    def activity_rows(self):
        rows = copy.deepcopy(self.rows)
        rows[0].update(worker_activity_schema_version=1, activity_clock="monotonic_elapsed_ns")
        for i, row in enumerate(rows):
            row["recorded_at_ns"] = i * 10
        rows[3].update(construction_started_at_ns=21, construction_finished_at_ns=29)
        rows[-1].update(peak_constructing_checkers=1, peak_covered_semantic_checkers=1,
                        peak_full_checkers=1, peak_observed_checkers=1,
                        unfinished_constructions=0, construction_started_at_ns=None)
        return rows

    def worker_check(self, rows, producer="tsr", context=None):
        from checker_work_trace import validate_worker_activity
        self.trace.write_text("".join(json.dumps(row) + "\n" for row in rows))
        binding = copy.deepcopy(self.context if context is None else context)
        binding["trace_sha256"] = self.digest(self.trace)
        return validate_worker_activity(self.trace, binding, producer)

    def test_worker_nested_queries_count_one_checker_and_union_duration(self):
        result = self.worker_check(self.activity_rows())
        self.assertTrue(result["worker_activity_valid"], result)
        self.assertEqual(result["activity"]["semantic"], {"peak": 1, "wall_ns": 30, "checker_ns": 30})
        self.assertEqual(result["activity"]["constructing"], {"peak": 1, "wall_ns": 8, "checker_ns": 8})
        self.assertIsNone(result["activity"]["leased"])
        self.assertFalse(result["safe_memory_admission_verified"])
        self.assertFalse(result["actual_checked_work_verified"])

    def test_worker_missing_corrupt_clock_construction_and_peak_reject(self):
        for index, key, value in [(0, "worker_activity_schema_version", None),
                                  (0, "activity_clock", "wall_clock"),
                                  (4, "recorded_at_ns", 20), (4, "recorded_at_ns", True),
                                  (3, "construction_started_at_ns", 31),
                                  (3, "construction_finished_at_ns", 40),
                                  (-1, "peak_observed_checkers", 2),
                                  (-1, "unfinished_constructions", 1)]:
            rows = self.activity_rows()
            rows[index][key] = value
            with self.subTest(key=key, value=value):
                result = self.worker_check(rows)
                self.assertFalse(result["worker_activity_valid"], result)
                self.assertTrue(result["reasons"], result)

    def native_rows(self):
        self.context["invocation_id"] = "a" * 32
        self.context["requested_checkers"] = 2
        self.context["loaded_files"] = [str(self.input), str(self.source)]
        rows = [
            {"event": "invocation_start", "schema_version": 1, "pid": 1234,
             "nonce": "a" * 32, "args": ["--noEmit"], "clock": "monotonic_elapsed_ns",
             "observed_operations": ["constructor", "lease", "source_file_check", "symbol_type_query", "declared_type_query"],
             "all_forcing_observed": False, "initialization_forcing_observed": False,
             "complete_provenance_verified": False, "actual_work_equivalence_verified": False,
             "memory_admission_budget": None},
            {"event": "pool_selected", "pool_id": 0, "selected_count": 2, "program_file_count": 2,
             "requested_checkers": 2, "single_threaded": False, "memory_admission_budget": None},
            {"event": "program_file", "pool_id": 0, "file_id": 0, "path": str(self.input)},
            {"event": "program_file", "pool_id": 0, "file_id": 1, "path": str(self.source)},
            {"event": "span_begin", "token": 0, "pool_id": 0, "slot": 0, "operation": "constructor", "file_id": -1},
            {"event": "span_begin", "token": 1, "pool_id": 0, "slot": 1, "operation": "constructor", "file_id": -1},
            {"event": "span_end", "token": 0, "aborted": False},
            {"event": "checker_created", "pool_id": 0, "slot": 0, "native_checker_id": 11},
            {"event": "span_end", "token": 1, "aborted": False},
            {"event": "checker_created", "pool_id": 0, "slot": 1, "native_checker_id": 12},
            {"event": "file_affinity", "pool_id": 0, "file_id": 0, "slot": 0},
            {"event": "file_affinity", "pool_id": 0, "file_id": 1, "slot": 1},
            {"event": "span_begin", "token": 2, "pool_id": 0, "slot": 0, "operation": "lease", "file_id": -1},
            {"event": "span_begin", "token": 3, "pool_id": 0, "slot": 0, "operation": "source_file_check", "file_id": 0},
            {"event": "span_begin", "token": 4, "pool_id": 0, "slot": 0, "operation": "symbol_type_query", "file_id": -1},
            {"event": "span_end", "token": 4, "aborted": False},
            {"event": "span_end", "token": 3, "aborted": False},
            {"event": "span_end", "token": 2, "aborted": False},
            {"event": "invocation_end", "complete": True, "status": 1, "unfinished_spans": 0,
             "all_forcing_observed": False,
             "peaks": {"constructing": 2, "semantic": 1, "full": 1, "leased": 1, "observed": 2}},
        ]
        for i, row in enumerate(rows):
            row["recorded_at_ns"] = i * 10
        return rows

    def test_native_worker_constructor_overlap_and_idle_instance(self):
        result = self.worker_check(self.native_rows(), "native")
        self.assertTrue(result["worker_activity_valid"], result)
        self.assertEqual(result["activity"]["constructing"], {"peak": 2, "wall_ns": 40, "checker_ns": 50})
        self.assertEqual(result["checker_instances_created"], 2)
        self.assertEqual(result["idle_checker_instances"], 1)
        self.assertEqual(result["activity"]["semantic"]["checker_ns"], 30)
        self.assertEqual(result["full_file_affinity"], [[0, 0, 0]])

    def test_native_worker_identity_affinity_lease_and_completion_reject(self):
        for index, key, value in [(0, "nonce", "b" * 32), (0, "pid", 1235),
                                  (1, "selected_count", 1), (1, "selected_count", True),
                                  (9, "native_checker_id", 11), (10, "slot", 1),
                                  (13, "slot", 1), (15, "aborted", True),
                                  (-1, "status", 0), (-1, "complete", False)]:
            rows = self.native_rows()
            rows[index][key] = value
            with self.subTest(key=key, value=value):
                self.assertFalse(self.worker_check(rows, "native")["worker_activity_valid"])
        rows = self.native_rows()
        overlap = {**rows[12], "token": 3, "recorded_at_ns": 125}
        rows.insert(13, overlap)
        self.assertFalse(self.worker_check(rows, "native")["worker_activity_valid"])
        rows = self.native_rows()
        rows[6], rows[7] = rows[7], rows[6]
        self.assertFalse(self.worker_check(rows, "native")["worker_activity_valid"])
        rows = self.native_rows()
        for key, value in [("exit_code", -9), ("timed_out", True),
                           ("stderr", "native worker activity warning: controlled")]:
            context = copy.deepcopy(self.context)
            context["child"][key] = value
            self.assertFalse(self.worker_check(rows, "native", context)["worker_activity_valid"])

    def test_worker_list_only_and_initialization_only_remain_distinct(self):
        rows = self.activity_rows()
        del rows[4:8]
        rows[-1].update(peak_full_checks=0, peak_covered_semantic_checkers=0, peak_full_checkers=0)
        context = copy.deepcopy(self.context)
        context["show_config"]["compilerOptions"]["noCheck"] = True
        rows[1]["show_config"] = json.dumps(context["show_config"])
        rows[1]["full_check_options"]["no_check"] = True
        rows[2].update(full_check_eligible=False, full_check_exclusion="no_check")
        result = self.worker_check(rows, context=context)
        self.assertTrue(result["worker_activity_valid"], result)
        self.assertEqual(result["idle_checker_instances"], 1)
        rows = self.activity_rows()
        rows = [*rows[:3], rows[-1]]
        rows[0]["args"].append("--listFilesOnly")
        rows[-1].update(semantic_program_observed=True, checker_instances_created=0, peak_full_checks=0,
                        peak_constructing_checkers=0, peak_covered_semantic_checkers=0,
                        peak_full_checkers=0, peak_observed_checkers=0)
        context = copy.deepcopy(self.context)
        context["child"]["command"].append("--listFilesOnly")
        result = self.worker_check(rows, context=context)
        self.assertTrue(result["worker_activity_valid"], result)
        self.assertEqual(result["checker_instances_created"], 0)
        rows.append(copy.deepcopy(rows[-1]))
        self.assertFalse(self.worker_check(rows, context=context)["worker_activity_valid"])

    def test_native_worker_truncated_partial_and_replayed_spans_reject(self):
        rows = self.native_rows()
        for mutated in [rows[:-1], rows + [rows[-1]], rows[:9] + rows[10:],
                        rows[:15] + [{**rows[15], "token": 3}] + rows[16:],
                        rows[:12] + [{**rows[12], "token": 0}] + rows[13:]]:
            self.assertFalse(self.worker_check(mutated, "native")["worker_activity_valid"])

    def test_unknown_schema_duplicate_keys_and_truncated_json_reject(self):
        for schema in (2, "1", True):
            rows = copy.deepcopy(self.rows)
            rows[0]["schema_version"] = schema
            with self.subTest(schema=schema):
                self.reject(rows)
        self.reject(raw='{"event":"invocation_start","event":"invocation_end"}\n')
        self.reject(raw='{"event":"invocation_start"')
        self.reject(raw='[]\n')
        self.reject(raw='{"event":NaN}\n')

    def test_replayed_process_invocation_binary_options_and_input_reject(self):
        for key, value in (("pid", 1235), ("invocation_id", "1234-98"),
                           ("source_sha_claim", "unqualified-source"),
                           ("binary_sha256_claim", "f" * 64), ("args", ["--noCheck"])):
            rows = copy.deepcopy(self.rows)
            rows[0][key] = value
            with self.subTest(key=key):
                self.reject(rows)
        rows = copy.deepcopy(self.rows)
        config = copy.deepcopy(self.options)
        config["compilerOptions"]["noCheck"] = True
        rows[1]["show_config"] = json.dumps(config)
        self.reject(rows)
        for path in (self.binary, self.source, self.input):
            before = path.read_bytes()
            path.write_bytes(before + b" changed")
            with self.subTest(path=path.name):
                self.reject()
            path.write_bytes(before)
        context = copy.deepcopy(self.context)
        context["inputs_after"] = []
        self.reject(context=context)

    def test_receipt_boolean_cannot_override_actual_binding_failure(self):
        context = copy.deepcopy(self.context)
        context.update(actual_checked_work_verified=True, complete_provenance_verified=True)
        context["binary_sha256"] = "0" * 64
        self.reject(context=context)
        context = copy.deepcopy(self.context)
        context.update(actual_checked_work_verified=True, complete_provenance_verified=True)
        result = self.check(context=context)
        self.assertTrue(result["artifact_integrity_valid"], result)
        self.assertIs(result["actual_checked_work_verified"], False)

    def test_signal_timeout_failure_warning_and_flush_warning_reject(self):
        for changes in ({"exit_code": -9}, {"timed_out": True}, {"exit_code": 5},
                        {"stderr": "warning: TSR work trace incomplete: final flush\n"},
                        {"stderr": "warning: TSR work trace not started: stale file\n"}):
            context = copy.deepcopy(self.context)
            context["child"].update(changes)
            with self.subTest(changes=changes):
                self.reject(context=context)

    def test_missing_eligible_file_and_duplicate_full_workers_reject(self):
        rows = [row for row in self.rows if row.get("span_id") != 0]
        rows[-1] = {**rows[-1], "peak_full_checks": 0}
        self.reject(rows)
        rows = copy.deepcopy(self.rows)
        rows[-1:-1] = [
            {**rows[4], "span_id": 2},
            {"event": "work_end", "span_id": 2, "outcome": "returned"},
        ]
        self.reject(rows)

    def test_orphan_reused_missing_and_panicking_completions_reject(self):
        for index, update in ((6, {"span_id": 77}), (5, {"span_id": 0}),
                              (6, {"outcome": "panicking"}), (5, {"checker_id": 1}),
                              (4, {"file_ids": [1]}), (4, {"file_ids": [True]})):
            rows = copy.deepcopy(self.rows)
            rows[index].update(update)
            with self.subTest(index=index, update=update):
                self.reject(rows)
        self.reject(self.rows[:-1])
        self.reject(self.rows[:6] + self.rows[7:])
        self.reject(self.rows[:4] + [self.rows[3]] + self.rows[4:])
        self.reject(self.rows + [self.rows[-1]])

    def test_cumulative_counts_and_worker_request_claims_are_verified(self):
        for key, value in (("checker_instances_created", 2), ("peak_full_checks", 0),
                           ("unfinished_spans", 1), ("exit_code", 0),
                           ("semantic_program_observed", False), ("state", "incomplete"),
                           ("actual_work_equivalence_verified", True)):
            rows = copy.deepcopy(self.rows)
            rows[-1][key] = value
            with self.subTest(key=key):
                self.reject(rows)
        rows = copy.deepcopy(self.rows)
        rows[3]["requested_checkers"] = 2
        self.reject(rows)
        rows = copy.deepcopy(self.rows)
        rows[3]["requested_single_threaded"] = True
        self.reject(rows)

    def test_exclusion_facts_do_not_hide_required_full_work(self):
        for update in ({"full_check_eligible": False, "full_check_exclusion": "file_no_check"},
                       {"full_check_exclusion": "invented"}, {"source_node_id": None},
                       {"file_id": 1}):
            rows = copy.deepcopy(self.rows)
            rows[2].update(update)
            with self.subTest(update=update):
                self.reject(rows)
        rows = copy.deepcopy(self.rows)
        del rows[1]["full_check_options"]
        self.reject(rows)
        rows = copy.deepcopy(self.rows)
        rows[1]["full_check_options"]["no_check"] = True
        self.reject(rows)

    def test_no_check_zero_workers_still_requires_one_created_checker(self):
        rows = copy.deepcopy(self.rows[:4] + [self.rows[-1]])
        rows[1]["full_check_options"]["no_check"] = True
        config = copy.deepcopy(self.options)
        config["compilerOptions"]["noCheck"] = True
        rows[1]["show_config"] = json.dumps(config)
        rows[2].update(full_check_eligible=False, full_check_exclusion="no_check")
        rows[-1]["peak_full_checks"] = 0
        context = copy.deepcopy(self.context)
        context["show_config"] = config
        result = self.check(rows, context)
        self.assertTrue(result["artifact_integrity_valid"], result)
        self.assertEqual(result["checked_file_ids"], [])
        self.assertEqual(result["checker_instances_created"], 1)

    def test_special_trace_file_does_not_block(self):
        os.mkfifo(self.trace)
        result = validate_trace(self.trace, self.context)
        self.assertFalse(result["artifact_integrity_valid"])

    def test_reused_pid_cannot_accept_an_invocation_older_than_the_child(self):
        context = copy.deepcopy(self.context)
        context["child"]["started_at_unix_ns"] = 100
        self.reject(context=context)
        for nonce in ("1234", "1234-0", "9999-99", "1234-future", "1234-99-extra"):
            rows = copy.deepcopy(self.rows)
            rows[0]["invocation_id"] = nonce
            context = copy.deepcopy(self.context)
            context["invocation_id"] = nonce
            with self.subTest(nonce=nonce):
                self.reject(rows, context)
        context = copy.deepcopy(self.context)
        del context["child"]["started_at_unix_ns"]
        self.reject(context=context)

    def test_captured_trace_digest_cannot_be_replaced_by_a_parseable_artifact(self):
        self.check()
        context = copy.deepcopy(self.context)
        context["trace_sha256"] = self.digest(self.trace)
        self.trace.write_bytes(self.trace.read_bytes().replace(b'"qualified options"', b'"other options"'))
        result = validate_trace(self.trace, context)
        self.assertFalse(result["artifact_integrity_valid"], result)
        self.assertIn("captured trace bytes", result["reasons"][0])

    def test_reader_can_be_loaded_from_a_scratch_driver(self):
        module = Path(__file__).with_name("checker_work_trace.py")
        child = subprocess.run([sys.executable, "-c", f"import runpy; assert callable(runpy.run_path({str(module)!r})['validate_trace'])"],
                               cwd=self.root, capture_output=True, text=True, timeout=10)
        self.assertEqual(child.returncode, 0, child.stderr)

    def test_cli_invalid_receipt_saves_failure_with_broader_gates_false(self):
        receipt, output = self.root / "receipt.json", self.root / "result.json"
        receipt.write_text('{"schema_version":')
        child = subprocess.run([sys.executable, str(Path(__file__).with_name("checker_work_trace.py")),
                                "--trace", str(self.trace), "--receipt", str(receipt), "--output", str(output)],
                               capture_output=True, text=True, timeout=10)
        self.assertEqual(child.returncode, 1, child.stderr)
        result = json.loads(output.read_text())
        self.assertFalse(result["artifact_integrity_valid"])
        self.assertFalse(result["actual_checked_work_verified"])
        self.assertIn("Invalid supervising receipt", result["reasons"][0])


if __name__ == "__main__":
    unittest.main()
