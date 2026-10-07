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
from checker_work_trace import validate_trace


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
                      "exit_code": 1, "timed_out": False, "stderr": "", "stdout": "error TS2322: control\n",
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

    def native_trace_check(self, rows):
        from checker_work_trace import PINNED_NATIVE_SHA, validate_native_trace
        self.trace.write_text(json.dumps(rows))
        receipt = copy.deepcopy(self.context)
        receipt.update(oracle_sha=PINNED_NATIVE_SHA, trace_sha256=self.digest(self.trace))
        receipt["child"]["command"] += ["--generateTrace", str(self.root)]
        receipt["loaded_files"] = ["a.ts", "b.ts", str(self.input)]
        return validate_native_trace(self.trace, receipt)

    def test_native_full_workers_preserve_owner_paths_and_unsampled_duration(self):
        rows = [
            {"pid": 1, "tid": 2, "ph": "B", "cat": "check", "ts": 10,
             "name": "checkSourceFile", "args": {"checkerId": 0, "path": "a.ts"}},
            {"pid": 1, "tid": 3, "ph": "B", "cat": "check", "ts": 12,
             "name": "checkSourceFile", "args": {"checkerId": 1, "path": "b.ts"}},
            {"pid": 1, "tid": 3, "ph": "E", "cat": "check", "ts": 20,
             "name": "checkSourceFile", "args": {"checkerId": 1, "path": "b.ts"}},
            {"pid": 1, "tid": 2, "ph": "E", "cat": "check", "ts": 30,
             "name": "checkSourceFile", "args": {"checkerId": 0, "path": "a.ts"}},
        ]
        result = self.native_trace_check(rows)
        self.assertTrue(result["native_trace_valid"], result)
        self.assertEqual(result["highest_observed_native_worker"],
                         {"path": "a.ts", "checker_id": 0, "duration_ns": 20000})
        self.assertEqual(result["operation_counters"]["checkSourceFile"],
                         {"begins": 2, "completed": 2, "sampled": 0})
        for mutation in (rows[:-1], rows + rows,
                         rows[:3] + [{**rows[3], "args": {"checkerId": 1, "path": "a.ts"}}]):
            self.assertFalse(self.native_trace_check(mutation)["native_trace_valid"])
        self.assertFalse(result["actual_checked_work_verified"])

    def test_worker_only_trace_has_no_complete_program_work_envelope(self):
        begin = {"pid": 1, "tid": 2, "ph": "B", "cat": "check", "ts": 0,
                 "name": "checkSourceFile", "args": {"checkerId": 0, "path": "a.ts"}}
        result = self.native_trace_check([begin, {**begin, "ph": "E", "ts": 10}])
        self.assertTrue(result["native_trace_valid"])
        self.assertFalse(result["program_work_envelope_verified"])
        self.assertFalse(result["native_current_checkpoint_gates"]["program_work_envelope_verified"])
        self.assertFalse(result["actual_checked_work_verified"])

    def test_native_partial_bind_inventory_cannot_pass_as_loaded_scope(self):
        begin = {"pid": 1, "tid": 10, "ph": "B", "cat": "bind", "ts": 0,
                 "name": "bindSourceFile", "args": {"path": "a.ts"}}
        result = self.native_trace_check([begin, {**begin, "ph": "E", "ts": 10}])
        self.assertFalse(result["native_trace_valid"])
        self.assertFalse(result["bound_inventory_verified"])
        self.assertEqual(result["completed_bound_paths"], ["a.ts"])
        self.assertIn("bind scope differs", result["reasons"][0])

    def test_native_partial_parse_inventory_cannot_pass_as_loaded_scope(self):
        begin = {"pid": 1, "tid": 10, "ph": "B", "cat": "parse", "ts": 0,
                 "name": "createSourceFile", "args": {"path": "a.ts"}}
        result = self.native_trace_check([begin, {**begin, "ph": "E", "ts": 10}])
        self.assertFalse(result["native_trace_valid"])
        self.assertFalse(result["parsed_inventory_verified"])
        self.assertEqual(result["completed_parsed_paths"], ["a.ts"])
        self.assertIn("parse scope differs", result["reasons"][0])

    def test_native_trace_directory_mismatch_rejects_actual_receipt(self):
        from checker_work_trace import PINNED_NATIVE_SHA, validate_native_trace
        begin = {"pid": 1, "tid": 2, "ph": "B", "cat": "check", "ts": 0,
                 "name": "checkSourceFile", "args": {"checkerId": 0, "path": "a.ts"}}
        self.native_trace_check([begin, {**begin, "ph": "E", "ts": 10}])
        receipt = {**self.context, "oracle_sha": PINNED_NATIVE_SHA,
                   "trace_sha256": self.digest(self.trace)}
        receipt["child"] = {**self.context["child"],
                            "command": [str(self.binary), "--noEmit", "--generateTrace", str(self.root / "other")]}
        result = validate_native_trace(self.trace, receipt)
        self.assertFalse(result["native_trace_valid"])
        self.assertIn("outside invoked trace directory", result["reasons"][0])

    def test_native_private_owner_cannot_migrate_synthetic_threads(self):
        begin = {"pid": 1, "tid": 2, "ph": "B", "cat": "check", "ts": 0,
                 "name": "checkSourceFile", "args": {"checkerId": 0, "path": "a.ts"}}
        for mutated in ({**begin, "tid": 3}, {**begin, "pid": 2}):
            result = self.native_trace_check([mutated, {**mutated, "ph": "E", "ts": 10}])
            self.assertFalse(result["native_trace_valid"], result)
            self.assertTrue(result["reasons"])

    def test_native_worker_inventory_overlap_and_cross_owner_duplicate_reject(self):
        begin = {"pid": 1, "tid": 2, "ph": "B", "cat": "check", "ts": 0,
                 "name": "checkSourceFile", "args": {"checkerId": 0, "path": "a.ts"}}
        end = {**begin, "ph": "E", "ts": 10}
        outside = {**begin, "args": {"checkerId": 0, "path": "outside.ts"}}
        overlapping = {**begin, "ts": 1, "args": {"checkerId": 0, "path": "b.ts"}}
        other_owner = {**begin, "tid": 3, "ts": 11, "args": {"checkerId": 1, "path": "a.ts"}}
        for rows in ([outside, {**outside, "ph": "E", "ts": 10}],
                     [begin, overlapping, {**overlapping, "ph": "E", "ts": 5}, end],
                     [begin, end, other_owner, {**other_owner, "ph": "E", "ts": 20}]):
            result = self.native_trace_check(rows)
            self.assertFalse(result["native_trace_valid"], result)
            self.assertTrue(result["reasons"])
        incomplete = self.native_trace_check([begin, end, {**begin, "ts": 20}])
        self.assertFalse(incomplete["native_trace_valid"])
        self.assertEqual(incomplete["completed_full_workers"],
                         [{"path": "a.ts", "checker_id": 0, "duration_ns": 10000}])
        self.assertTrue(incomplete["observations_are_partial_on_failure"])

    def test_native_variance_completion_output_is_not_an_identity_change(self):
        begin = {"pid": 1, "tid": 2, "ph": "B", "cat": "checkTypes", "ts": 10,
                 "name": "getVariancesWorker", "args": {"checkerId": 0, "id": 99, "arity": 1}}
        end = {**begin, "ph": "E", "ts": 20,
               "args": {**begin["args"], "variances": ["out"]}}
        self.assertTrue(self.native_trace_check([begin, end])["native_trace_valid"])
        end["args"]["id"] = 100
        self.assertFalse(self.native_trace_check([begin, end])["native_trace_valid"])

    def test_native_inner_worker_ranking_preserves_private_identity_and_full_source(self):
        full = {"pid": 1, "tid": 2, "ph": "B", "cat": "check", "ts": 0,
                "name": "checkSourceFile", "args": {"checkerId": 0, "path": "a.ts"}}
        variance = {**full, "cat": "checkTypes", "ts": 10, "name": "getVariancesWorker",
                    "args": {"checkerId": 0, "id": 99, "arity": 1}}
        sampled = {**full, "ph": "X", "ts": 11, "dur": 500,
                   "name": "structuredTypeRelatedTo", "args": {"checkerId": 0, "sourceId": 1, "targetId": 2}}
        rows = [full, variance, sampled,
                {**variance, "ph": "E", "ts": 30, "args": {**variance["args"], "variances": ["out"]}},
                {**full, "ph": "E", "ts": 40}]
        result = self.native_trace_check(rows)
        self.assertTrue(result["native_trace_valid"], result)
        boundary = result["highest_observed_unsampled_inner_boundary"]
        self.assertEqual(boundary["operation"], "getVariancesWorker")
        self.assertEqual(boundary["duration_ns"], 20000)
        self.assertEqual(boundary["full_source_path"], "a.ts")
        self.assertEqual(boundary["args_begin"]["id"], 99)
        self.assertEqual(boundary["args_end"]["variances"], ["out"])
        self.assertEqual(result["sampled_native_operations"][0]["duration_ns"], 500000)
        self.assertFalse(result["native_current_checkpoint_gates"]["complete_checker_operation_coverage"])

    def test_native_emit_spans_do_not_claim_output_or_emission_equivalence(self):
        begin = {"pid": 1, "tid": 2, "ph": "B", "cat": "emit", "ts": 10,
                 "name": "emit", "args": {"path": "a.ts"}}
        end = {**begin, "ph": "E", "ts": 20}
        result = self.native_trace_check([begin, end])
        self.assertTrue(result["native_trace_valid"], result)
        self.assertEqual(result["completed_emit_operations"], [
            {"args_begin": {"path": "a.ts"}, "args_end": {"path": "a.ts"}, "duration_ns": 10000}])
        self.assertFalse(result["actual_checked_work_verified"])
        self.assertFalse(result["target_verified"])

    def test_native_sampled_and_empty_work_cannot_certify_complete_checking(self):
        row = {"pid": 1, "tid": 2, "ph": "X", "cat": "check", "ts": 10,
               "dur": 100, "name": "checkExpression", "args": {"checkerId": 0, "path": "a.ts"}}
        result = self.native_trace_check([row])
        self.assertTrue(result["native_trace_valid"], result)
        self.assertEqual(result["completed_full_workers"], [])
        self.assertIsNone(result["highest_observed_native_worker"])
        self.assertFalse(result["actual_checked_work_verified"])

    def test_cross_tool_same_count_different_paths_cannot_match_full_worker_scope(self):
        from checker_work_trace import compare_work_captures
        ours = {"artifact_integrity_valid": True, "worker_activity_valid": True, "program_files": [self.rows[2]],
                "checked_file_ids": [0], "operation_counters": {"source_file_check": {"begins": 1, "completed": 1}}}
        native = {"native_trace_valid": True, "artifact_integrity_valid": True,
                  "completed_full_workers": [{"path": "other.ts", "checker_id": 0, "duration_ns": 1}],
                  "operation_counters": {"checkSourceFile": {"begins": 1, "completed": 1}}}
        receipt = {**self.context, "trace_sha256": "0" * 64}
        result = compare_work_captures(ours, native, receipt, receipt)
        self.assertFalse(result["observed_full_worker_scope"]["identity_sets_match"])
        self.assertEqual(result["observed_full_worker_scope"]["native_only"], ["other.ts"])
        self.assertIsNone(result["operations"][1]["native_completed"])
        self.assertIsNone(result["operations"][0]["native_result_copy_bytes"])
        self.assertNotIn("symbol_type_query", result["counter_attribution"]["tsr"]["actual_workers"])
        self.assertIsNone(result["counter_attribution"]["native"]["queries"]["declared_type_query"])
        self.assertIsNone(result["counter_attribution"]["tsr"]["read_only_identifier_extra_flag_reads"])
        self.assertFalse(result["counter_attribution"]["assignment_branch_cost_observed"])
        self.assertFalse(result["actual_checked_work_verified"])
        native["completed_full_workers"][0]["path"] = str(self.input)
        matched = compare_work_captures(ours, native, receipt, receipt)
        self.assertTrue(matched["observed_full_worker_scope"]["identity_sets_match"])
        self.assertIsNone(matched["native_program_file_policy"])
        self.assertFalse(matched["operations"][0]["complete_operation_equivalence_verified"])
        native["native_trace_valid"] = False
        self.assertFalse(compare_work_captures(ours, native, receipt, receipt)
                         ["observed_full_worker_scope"]["identity_sets_match"])

    def test_comparison_keeps_ordinary_same_basename_library_prefix_paths_distinct(self):
        from checker_work_trace import compare_work_captures
        receipt = {**self.context, "trace_sha256": "0" * 64}
        ours = {"artifact_integrity_valid": True, "worker_activity_valid": True,
                "checked_file_ids": [0], "program_files": [], "operation_counters": {}}
        native = {"artifact_integrity_valid": True, "native_trace_valid": True,
                  "completed_full_workers": [], "operation_counters": {}}
        for left, right in (
            ("/a/typescript-go/internal/bundled/libs/model.ts",
             "/b/typescript-go/internal/bundled/libs/model.ts"),
            ("bundled:///libs/model.ts", "/a/typescript-go/internal/bundled/libs/model.ts"),
            ("bundled:///libs/model.d.ts", "/a/typescript-go/internal/bundled/libs/model.d.ts"),
        ):
            ours["program_files"] = [{"file_id": 0, "path": left}]
            native["completed_full_workers"] = [{"path": right, "checker_id": 0, "duration_ns": 1}]
            result = compare_work_captures(ours, native, receipt, receipt)
            self.assertFalse(result["observed_full_worker_scope"]["identity_sets_match"], result)
            self.assertEqual(result["observed_full_worker_scope"]["tsr_only"], [left])
            self.assertEqual(result["observed_full_worker_scope"]["native_only"], [right])
        ours["program_files"] = [{"file_id": 0, "path": "bundled:///libs/lib.es5.d.ts"}]
        native["completed_full_workers"] = [
            {"path": "/a/typescript-go/internal/bundled/libs/lib.es5.d.ts", "checker_id": 0, "duration_ns": 1}]
        canonical = compare_work_captures(ours, native, receipt, receipt)
        self.assertTrue(canonical["observed_full_worker_scope"]["identity_sets_match"])
        self.assertFalse(canonical["actual_checked_work_verified"])

    def test_comparison_cli_rejects_valid_base_trace_with_invalid_worker_activity(self):
        from checker_work_trace import PINNED_NATIVE_SHA
        native_path = self.root / "native.json"
        native_receipt_path = self.root / "native-receipt.json"
        receipt_path = self.root / "receipt.json"
        output_path = self.root / "comparison.json"
        begin = {"pid": 1, "tid": 2, "ph": "B", "cat": "check", "ts": 0,
                 "name": "checkSourceFile", "args": {"checkerId": 0, "path": str(self.input)}}
        native_path.write_text(json.dumps([begin, {**begin, "ph": "E", "ts": 10}]))
        native_receipt = {**self.context, "oracle_sha": PINNED_NATIVE_SHA,
                          "trace_sha256": self.digest(native_path)}
        native_receipt["child"] = {**self.context["child"],
                                   "command": [str(self.binary), "--noEmit", "--generateTrace", str(self.root)]}
        native_receipt_path.write_text(json.dumps(native_receipt))
        for index, key, value in ((3, "construction_started_at_ns", 31),
                                  (-1, "peak_observed_checkers", 2)):
            rows = self.activity_rows()
            rows[index][key] = value
            worker = self.worker_check(rows)
            self.assertTrue(worker["artifact_integrity_valid"], worker)
            self.assertFalse(worker["worker_activity_valid"], worker)
            receipt = {**self.context, "trace_sha256": self.digest(self.trace)}
            self.assertTrue(validate_trace(self.trace, receipt)["artifact_integrity_valid"])
            receipt_path.write_text(json.dumps(receipt))
            child = subprocess.run([sys.executable, str(Path(__file__).with_name("checker_work_trace.py")),
                                    "--trace", str(self.trace), "--receipt", str(receipt_path),
                                    "--compare-native-trace", str(native_path),
                                    "--compare-native-receipt", str(native_receipt_path),
                                    "--output", str(output_path)], capture_output=True, text=True)
            result = json.loads(output_path.read_text())
            self.assertEqual(child.returncode, 1, result)
            self.assertFalse(result["artifact_integrity_valid"])
            self.assertFalse(result["observed_full_worker_scope"]["identity_sets_match"])
            self.assertEqual(result["producer_reasons"]["tsr"], worker["reasons"])
            self.assertTrue(result["reasons"])
            self.assertFalse(result["target_verified"])

    def test_comparison_cli_rejects_valid_native_trace_with_different_completed_scope(self):
        from checker_work_trace import PINNED_NATIVE_SHA
        native_trace = self.root / "native.json"
        native_receipt_path = self.root / "native-receipt.json"
        tsr_receipt_path = self.root / "tsr-receipt.json"
        output = self.root / "comparison.json"
        self.worker_check(self.activity_rows())
        tsr_receipt_path.write_text(json.dumps({**self.context, "trace_sha256": self.digest(self.trace)}))
        begin = {"pid": 1, "tid": 2, "ph": "B", "cat": "check", "ts": 0,
                 "name": "checkSourceFile", "args": {"checkerId": 0, "path": str(self.source)}}
        native_trace.write_text(json.dumps([begin, {**begin, "ph": "E", "ts": 10}]))
        native_receipt = {**self.context, "oracle_sha": PINNED_NATIVE_SHA,
                          "loaded_files": [str(self.source)], "trace_sha256": self.digest(native_trace),
                          "child": {**self.context["child"], "command": [str(self.binary), "--noEmit",
                                                                        "--generateTrace", str(self.root)]}}
        native_receipt_path.write_text(json.dumps(native_receipt))
        child = subprocess.run([sys.executable, str(Path(__file__).with_name("checker_work_trace.py")),
                                "--trace", str(self.trace), "--receipt", str(tsr_receipt_path),
                                "--compare-native-trace", str(native_trace),
                                "--compare-native-receipt", str(native_receipt_path), "--output", str(output)],
                               capture_output=True, text=True)
        result = json.loads(output.read_text())
        self.assertEqual(child.returncode, 1, result)
        self.assertTrue(result["artifact_integrity_valid"])
        self.assertFalse(result["comparison_valid"])
        self.assertFalse(result["observed_full_worker_scope"]["identity_sets_match"])
        self.assertEqual(result["observed_full_worker_scope"]["native_only"], [str(self.source)])
        self.assertTrue(result["reasons"])

    def test_comparison_cli_malformed_receipt_does_not_hide_other_producer_failure(self):
        malformed, empty = self.root / "malformed.json", self.root / "empty.json"
        malformed.write_text('{"schema_version":')
        empty.write_text("{}")
        output = self.root / "comparison.json"
        for left, right in ((malformed, empty), (empty, malformed)):
            child = subprocess.run([sys.executable, str(Path(__file__).with_name("checker_work_trace.py")),
                                    "--trace", str(self.trace), "--receipt", str(left),
                                    "--compare-native-trace", str(self.trace),
                                    "--compare-native-receipt", str(right), "--output", str(output)],
                                   capture_output=True, text=True)
            self.assertEqual(child.returncode, 1, child.stderr)
            result = json.loads(output.read_text())
            self.assertTrue(result["producer_reasons"]["tsr"])
            self.assertTrue(result["producer_reasons"]["native"])
            self.assertEqual(result["qualifications"], {})
            self.assertFalse(result["comparison_valid"])
            self.assertFalse(result["target_verified"])

    def test_comparison_cli_empty_receipts_save_both_validation_failures(self):
        from checker_work_trace import PINNED_NATIVE_SHA
        native_trace = self.root / "native.json"
        native_receipt_path = self.root / "native-receipt.json"
        tsr_receipt_path = self.root / "tsr-receipt.json"
        output = self.root / "comparison.json"
        self.worker_check(self.activity_rows())
        tsr_receipt = {**self.context, "trace_sha256": self.digest(self.trace)}
        begin = {"pid": 1, "tid": 2, "ph": "B", "cat": "check", "ts": 0,
                 "name": "checkSourceFile", "args": {"checkerId": 0, "path": str(self.input)}}
        native_trace.write_text(json.dumps([begin, {**begin, "ph": "E", "ts": 10}]))
        native_receipt = {**self.context, "oracle_sha": PINNED_NATIVE_SHA,
                          "trace_sha256": self.digest(native_trace),
                          "child": {**self.context["child"], "command": [str(self.binary), "--noEmit",
                                                                        "--generateTrace", str(self.root)]}}
        for bad_tsr, bad_native in ((True, False), (False, True), (True, True)):
            tsr_receipt_path.write_text(json.dumps({} if bad_tsr else tsr_receipt))
            native_receipt_path.write_text(json.dumps({} if bad_native else native_receipt))
            child = subprocess.run([sys.executable, str(Path(__file__).with_name("checker_work_trace.py")),
                                    "--trace", str(self.trace), "--receipt", str(tsr_receipt_path),
                                    "--compare-native-trace", str(native_trace),
                                    "--compare-native-receipt", str(native_receipt_path),
                                    "--output", str(output)], capture_output=True, text=True)
            self.assertEqual(child.returncode, 1, child.stderr)
            result = json.loads(output.read_text())
            self.assertFalse(result["artifact_integrity_valid"])
            self.assertFalse(result["observed_full_worker_scope"]["identity_sets_match"])
            self.assertEqual(result["qualifications"], {})
            self.assertEqual(bool(result["producer_reasons"]["tsr"]), bad_tsr)
            self.assertEqual(bool(result["producer_reasons"]["native"]), bad_native)
            self.assertTrue(result["reasons"])
            self.assertNotIn("Traceback", child.stderr)

    def test_comparison_rejects_equal_workers_with_changed_options_inputs_or_loaded_scope(self):
        from checker_work_trace import compare_work_captures
        ours = {"artifact_integrity_valid": True, "worker_activity_valid": True,
                "checked_file_ids": [0], "program_files": [self.rows[2]]}
        native = {"artifact_integrity_valid": True, "native_trace_valid": True,
                  "completed_full_workers": [{"path": str(self.input), "checker_id": 0}]}
        receipt = {**self.context, "trace_sha256": "0" * 64}
        for key, value, flag in (("show_config", {"compilerOptions": {"noCheck": True}}, "captured_options_match"),
                                 ("inputs_before", [], "captured_inputs_match"),
                                 ("loaded_files", [str(self.input), str(self.source)], "captured_loaded_scope_match")):
            other = {**receipt, key: value}
            result = compare_work_captures(ours, native, receipt, other)
            self.assertTrue(result["observed_full_worker_scope"]["identity_sets_match"])
            self.assertFalse(result[flag])
            self.assertFalse(result["comparison_valid"])
            self.assertTrue(result["reasons"])

    def test_receipt_duplicate_loaded_paths_reject_before_worker_validation(self):
        context = copy.deepcopy(self.context)
        context["loaded_files"] *= 2
        result = self.check(context=context)
        self.assertFalse(result["artifact_integrity_valid"])
        self.assertIn("duplicate captured loaded-file identity", result["reasons"][0])
        self.assertEqual(result["checked_file_ids"], [])

    def test_receipt_duplicate_query_inputs_and_missing_invoked_config_reject(self):
        context = copy.deepcopy(self.context)
        context["inputs_before"] *= 2
        context["inputs_after"] *= 2
        self.reject(context=context)
        config = self.root / "tsconfig.json"
        config.write_text("{}")
        context = copy.deepcopy(self.context)
        context["child"]["command"] += ["--project", str(config)]
        rows = copy.deepcopy(self.rows)
        rows[0]["args"] = context["child"]["command"][1:]
        self.reject(rows=rows, context=context)
        context["inputs_before"] = snapshot([str(self.input), str(config)])
        context["inputs_after"] = context["inputs_before"]
        self.assertTrue(self.check(rows=rows, context=context)["artifact_integrity_valid"])

    def test_comparison_matching_workers_cannot_hide_changed_or_missing_diagnostics(self):
        from checker_work_trace import compare_work_captures
        ours = {"artifact_integrity_valid": True, "worker_activity_valid": True,
                "checked_file_ids": [0], "program_files": [self.rows[2]]}
        native = {"artifact_integrity_valid": True, "native_trace_valid": True,
                  "completed_full_workers": [{"path": str(self.input), "checker_id": 0}]}
        receipt = {**self.context, "trace_sha256": "0" * 64}
        for output in (None, "", "error TS2322: different detail\n"):
            other = {**receipt, "child": {**receipt["child"], "stdout": output}}
            result = compare_work_captures(ours, native, receipt, other)
            self.assertTrue(result["observed_full_worker_scope"]["identity_sets_match"])
            self.assertFalse(result["captured_output_match"])
            self.assertFalse(result["comparison_valid"])
            self.assertEqual(result["captured_stdout"]["native"], output)
            self.assertTrue(result["reasons"])

    def test_comparison_preserves_both_producers_rejection_reasons(self):
        from checker_work_trace import compare_work_captures
        receipt = {**self.context, "trace_sha256": "0" * 64}
        result = compare_work_captures({"reasons": ["bad construction"]},
                                      {"reasons": ["unfinished native span"]}, receipt, receipt)
        self.assertEqual(result["reasons"], ["tsr: bad construction", "native: unfinished native span"])
        self.assertFalse(result["artifact_integrity_valid"])

    def test_empty_cross_tool_checks_never_match_complete_work(self):
        from checker_work_trace import compare_work_captures
        receipt = {**self.context, "trace_sha256": "0" * 64}
        result = compare_work_captures({"artifact_integrity_valid": True},
                                      {"native_trace_valid": True}, receipt, receipt)
        self.assertFalse(result["observed_full_worker_scope"]["identity_sets_match"])
        self.assertFalse(result["target_verified"])

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
