"""Negative controls for the archive-only receipt reader."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("origins", Path(__file__).with_name("reference-preparation-controls.py"))
origins = importlib.util.module_from_spec(spec)
spec.loader.exec_module(origins)

VALID = """tsr-reference-origins-v1\t42\t1001
activity\t2\t2
worker\t0\t0\t100
worker\t1\t10\t110
check\t0\t0\t2\t8
check\t0\t2\t20\t40
check\t1\t1\t21\t35
site\t1\t1\t1\t4
site\t2\t1\t1\t4
site\t3\t1\t1\t4
memory\t0\t8\t40\t6\t0\t0
memory\t1\t2\t10\t2\t0\t0
memory\t2\t2\t10\t2\t0\t0
memory\t3\t2\t10\t2\t0\t0
memory\t4\t2\t10\t2\t0\t0
spelling\t2\t3\t0\t5\t8\t1\t2\t4
"""


class ReceiptTests(unittest.TestCase):
    def read(self, data=VALID, workers=2, **overrides):
        child = dict(pid=42, started_at_unix_ns=1000, wall_seconds=1,
                     timed_out=False, exit_code=1, stdout="Checked files: 3\n")
        child.update(overrides)
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "receipt.tsv"
            path.write_text(data)
            return origins.read_probe(path, child, ["a.ts", "b.ts", "c.ts"], workers)

    def test_valid_activity_and_origin_union(self):
        result = self.read()
        self.assertEqual(result["peak_checking"], 2)
        self.assertEqual(result["checked_files"], [[0, "a.ts"], [1, "b.ts"], [2, "c.ts"]])
        self.assertEqual(result["memory"]["0"]["peak_requested_bytes"], 6)
        self.assertEqual(json.loads(json.dumps(result)), result)

    def test_mode_failure_pid_and_epoch_controls(self):
        for kwargs in (dict(workers=1), dict(timed_out=True), dict(exit_code=2), dict(pid=43),
                       dict(started_at_unix_ns=1002), dict(wall_seconds=0)):
            with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                self.read(**kwargs)

    def test_mutations_reject_unqualified_receipts(self):
        mutations = [
            VALID[:-1], VALID + "unknown\t0\n", VALID + "activity\t2\t2\n",
            VALID.replace("check\t1\t1", "check\t0\t1"),
            VALID.replace("check\t1\t1", "check\t1\t5"),
            VALID.replace("check\t0\t2", "check\t0\t0"),
            VALID.replace("check\t0\t2\t20\t40", "check\t0\t2\t3\t40"),
            VALID.replace("check\t1\t1\t21\t35", "check\t1\t1\t9\t35"),
            VALID.replace("activity\t2\t2", "activity\t2\t1"),
            VALID.replace("site\t2\t1\t1\t4\n", ""),
            VALID.replace("memory\t0\t8", "memory\t0\t9"),
            VALID.replace("memory\t0\t8\t40\t6", "memory\t0\t8\t40\t9"),
            VALID.replace("spelling\t2\t3\t0\t5\t8\t1", "spelling\t2\t3\t0\t5\t8\t2"),
            VALID.replace("site\t1\t1\t1", "site\t1\t-1\t1"),
            VALID.replace("check\t0\t0\t2\t8\n", ""),
        ]
        for index, data in enumerate(mutations):
            with self.subTest(index=index), self.assertRaises(ValueError):
                self.read(data)


if __name__ == "__main__":
    unittest.main()
