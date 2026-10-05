"""Negative qualification controls for plain-comment subset attribution."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("eligible", Path(__file__).with_name("jsdoc-eligible-controls.py"))
eligible = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(eligible)


class EligibleReaderTests(unittest.TestCase):
    def read(self, *, header=None, values=None, name="/x.ts", child=None, terminated=True):
        if header is None:
            header = "tsr-jsdoc-eligible-v1\t123\t101"
        if values is None:
            values = [100, 100, 50, 50, 2, 2, 30, 2, 64, 20, 20, 1, 10]
        if child is None:
            child = {"pid": 123, "started_at_unix_ns": 100, "exit_code": 0,
                     "timed_out": False, "stdout": "Parsed files: 1\n"}
        row = "\t".join([name.encode().hex(), "100", "TypeScript", *map(str, values)])
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trace"
            path.write_text(header + "\n" + row + ("\n" if terminated else ""))
            return eligible.read_probe(path, child)

    def test_subset_is_reported_without_claiming_all_body_work(self):
        result = self.read()
        self.assertEqual(result["totals"]["plain_ns"], 20)
        self.assertEqual(result["totals"]["jsdoc_ns"], 50)

    def test_subset_cannot_exceed_its_enclosing_counters(self):
        for index, value in ((9, 51), (10, 51), (11, 3), (12, 31)):
            values = [100, 100, 50, 50, 2, 2, 30, 2, 64, 20, 20, 1, 10]
            values[index] = value
            with self.subTest(index=index), self.assertRaises(ValueError):
                self.read(values=values)

    def test_javascript_or_json_is_never_plain_eligible(self):
        for name in ("/x.js", "/x.jsx", "/x.mjs", "/x.cjs", "/x.json"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.read(name=name)

    def test_old_version_and_wrong_pid_are_rejected(self):
        for header in ("tsr-jsdoc-cost-v2\t123\t101", "tsr-jsdoc-eligible-v1\t124\t101"):
            with self.subTest(header=header), self.assertRaises(ValueError):
                self.read(header=header)

    def test_same_pid_stale_trace_is_rejected(self):
        with self.assertRaises(ValueError):
            self.read(header="tsr-jsdoc-eligible-v1\t123\t99")

    def test_incomplete_child_and_partial_rows_are_rejected(self):
        with self.assertRaises(ValueError):
            self.read(terminated=False)
        for exit_code, timed_out in ((-9, False), (0, True)):
            child = {"pid": 123, "started_at_unix_ns": 100, "exit_code": exit_code,
                     "timed_out": timed_out, "stdout": "Parsed files: 1\n"}
            with self.subTest(exit_code=exit_code), self.assertRaises(ValueError):
                self.read(child=child)

    def test_missing_parse_coverage_is_rejected(self):
        child = {"pid": 123, "started_at_unix_ns": 100, "exit_code": 0,
                 "timed_out": False, "stdout": "Parsed files: 2\n"}
        with self.assertRaises(ValueError):
            self.read(child=child)


if __name__ == "__main__":
    unittest.main()
