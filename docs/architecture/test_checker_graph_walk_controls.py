from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("graph_controls", Path(__file__).with_name("checker-graph-walk-controls.py"))
graph = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(graph)


def leaf_counts():
    result = dict.fromkeys(graph.NAMES, 0)
    for name in ("roots", "explicit_roots", "true_roots", "visits", "unique_visits",
                 "explicit_parameter_tests", "parameter_hits", "roots_visited_0"):
        result[name] = 1
    return result


class Counts(unittest.TestCase):
    def test_actual_partition_schema_accepts_a_parameter_leaf(self):
        graph.validate_counts(leaf_counts())

    def test_missing_extra_and_negative_fields_are_rejected(self):
        for action in ("missing", "extra", "negative", "bool"):
            counts = leaf_counts()
            if action == "missing": counts.pop("visited_comparisons")
            elif action == "extra": counts["unqualified"] = 0
            elif action == "negative": counts["visited_comparisons"] = -1
            else: counts["roots"] = True
            with self.subTest(action=action), self.assertRaises(ValueError): graph.validate_counts(counts)

    def test_omitted_root_visit_parameter_answer_and_histogram_counts_fail(self):
        for name in ("roots", "explicit_roots", "true_roots", "visits", "unique_visits",
                     "explicit_parameter_tests", "parameter_hits", "roots_visited_0"):
            counts = leaf_counts()
            counts[name] = 0
            with self.subTest(name=name), self.assertRaises(ValueError): graph.validate_counts(counts)

    def test_capacity_allocation_and_comparison_bounds_fail(self):
        for name in ("max_visited", "max_capacity", "scratch_requested_bytes", "scratch_allocations",
                     "printing_requested_bytes", "empty_names_fallbacks", "visited_hits", "visited_comparisons"):
            counts = leaf_counts()
            counts[name] = 1
            with self.subTest(name=name), self.assertRaises(ValueError): graph.validate_counts(counts)

    def test_duplicate_and_noninteger_rows_fail(self):
        rows = [["count", name, str(value)] for name, value in leaf_counts().items()]
        for altered in (rows + [rows[0]], [["count", "roots", "1.0"], *rows[1:]],
                        [["count", "roots", "-1"], *rows[1:]]):
            with self.assertRaises(ValueError): graph.count_rows(altered)


class Trace(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.cwd = Path(self.directory.name)
        self.path = self.cwd / "trace.tsv"
        self.child = {"pid": 321, "exit_code": 0, "timed_out": False}
        self.write_trace(True)

    def write_trace(self, counters):
        owner = leaf_counts()
        global_counts = {name: 2 * value for name, value in owner.items()}
        header = "schema\tgraph-walk-v1\nprocess\t321\t2\t" + str(int(counters)) + "\n"
        counts = "".join(f"count\t{name}\t{value}\n" for name, value in global_counts.items()) if counters else ""
        self.path.write_text(header + counts + "joined\n")
        for index in range(2):
            counts = "".join(f"count\t{name}\t{value}\n" for name, value in owner.items()) if counters else ""
            query = "query_shapes\t1\t3\t1\n" if counters else ""
            self.path.with_suffix(f".owner-{index}.tsv").write_text(
                f"owner\t321\t{index}\t2\t{int(counters)}\ncheck\t{index}\tfile{index}.ts\n" + counts + query + "complete\n")

    def inspect(self, counters=True):
        return graph.inspect_trace(self.path, self.child, counters, self.cwd)

    def test_enabled_and_disabled_direct_scope_agree(self):
        enabled = self.inspect()
        self.assertEqual(enabled["counts"]["roots"], 2)
        self.write_trace(False)
        disabled = self.inspect(False)
        self.assertEqual(enabled["checks"], disabled["checks"])
        self.assertEqual(disabled["counts"], {})

    def test_failure_and_timeout_rejected_before_reading(self):
        self.path.unlink()
        for change in ({"exit_code": -6}, {"timed_out": True}):
            self.child.update(change)
            with self.assertRaises(ValueError): self.inspect()

    def test_stale_pid_wrong_mode_and_unfinished_pool_rejected(self):
        before = self.path.read_text()
        for text in (before.replace("process\t321", "process\t322"),
                     before.replace("process\t321\t2\t1", "process\t321\t2\t0"),
                     before.replace("joined\n", ""), before[:-1]):
            self.path.write_text(text)
            with self.assertRaises(ValueError): self.inspect()

    def test_owner_counts_and_completion_reconcile(self):
        path = self.path.with_suffix(".owner-1.tsv")
        before = path.read_text()
        for text in (before.replace("count\ttrue_roots\t1", "count\ttrue_roots\t0"),
                     before.replace("complete\n", ""),
                     before.replace("owner\t321\t1", "owner\t321\t0"),
                     before.replace("query_shapes\t1\t3\t1", "query_shapes\t2\t3\t1")):
            path.write_text(text)
            with self.assertRaises(ValueError): self.inspect()

    def test_duplicate_and_wrong_affinity_checks_rejected(self):
        path = self.path.with_suffix(".owner-0.tsv")
        before = path.read_text()
        for text in (before.replace("check\t0\tfile0.ts", "check\t1\tfile0.ts"),
                     before.replace("check\t0\tfile0.ts\n", "check\t0\tfile0.ts\ncheck\t0\tfile0.ts\n")):
            path.write_text(text)
            with self.assertRaises(ValueError): self.inspect()

    def test_valid_local_partition_still_requires_independent_global_sum(self):
        before = self.path.read_text()
        counts = leaf_counts()
        for name, value in counts.items():
            before = before.replace(f"count\t{name}\t{2 * value}\n", f"count\t{name}\t{3 * value}\n")
        self.path.write_text(before)
        with self.assertRaisesRegex(ValueError, "owner/global"): self.inspect()


if __name__ == "__main__":
    unittest.main()
