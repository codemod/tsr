"""Fail-closed controls for mapper observation receipts."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("mapper_pool", Path(__file__).with_name("checker-mapper-pool-controls.py"))
pool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pool)


class ReceiptTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.cwd = Path(self.temp.name)
        self.path = self.cwd / "trace.tsv"
        self.child = dict(pid=71, started_at_unix_ns=1000, wall_seconds=1, exit_code=0, timed_out=False)
        self.rows = ["schema\tmapper-pool-v1", "process\t71\t2\t0\t1001", "pool_joined\t90"]
        self.owner = [
            ["owner\t71\t0\t2\t0\t10\t80\t1", "check\t0\t20\t50\ta.ts"],
            ["owner\t71\t1\t2\t1\t11\t85\t1", "check\t1\t25\t55\tb.ts"],
        ]

    def inspect(self, counters=False):
        self.path.write_text("\n".join(self.rows) + "\n")
        for owner, rows in enumerate(self.owner):
            self.path.with_suffix(f".owner-{owner}.tsv").write_text("\n".join(rows) + "\n")
        return pool.inspect_trace(self.path, self.child, counters, self.cwd)

    def test_actual_peak_and_affinity(self):
        trace = self.inspect()
        self.assertEqual(trace["peak_worker_lifetimes"], 2)
        self.assertEqual(trace["peak_source_file_checks"], 2)
        self.assertEqual([(r["index"], r["owner"]) for r in trace["checks"]], [(0, 0), (1, 1)])
        self.assertEqual(trace["counters"], {})

    def test_wrong_pid_epoch_or_mode(self):
        for header in ("process\t72\t2\t0\t1001", "process\t71\t2\t0\t999", "process\t71\t2\t1\t1001"):
            with self.subTest(header=header):
                self.rows[1] = header
                with self.assertRaises(ValueError):
                    self.inspect()

    def test_failed_and_timed_out_children(self):
        for updates in (dict(exit_code=101), dict(timed_out=True)):
            with self.subTest(updates=updates):
                original = dict(self.child)
                self.child.update(updates)
                with self.assertRaises(ValueError):
                    self.inspect()
                self.child = original

    def test_uncompleted_owner_rejected(self):
        self.owner[1][0] = "owner\t71\t1\t2\t1\t11\t85\t0"
        with self.assertRaises(ValueError):
            self.inspect()

    def test_wrong_affinity_or_interval(self):
        for row in ("check\t2\t25\t55\tb.ts", "check\t1\t10\t55\tb.ts", "check\t1\t25\t86\tb.ts"):
            self.owner[1][1] = row
            with self.assertRaises(ValueError):
                self.inspect()

    def test_duplicate_check_rejected(self):
        self.owner[0].append(self.owner[0][1])
        with self.assertRaises(ValueError):
            self.inspect()

    def test_partial_or_missing_final_record(self):
        self.inspect()
        self.path.write_bytes(self.path.read_bytes()[:-1])
        with self.assertRaises(ValueError):
            pool.inspect_trace(self.path, self.child, False, self.cwd)
        self.rows[-1] = "count\tx\t0"
        with self.assertRaises(ValueError):
            self.inspect()

    def test_missing_owner_rejected(self):
        self.inspect()
        self.path.with_suffix(".owner-1.tsv").unlink()
        with self.assertRaises(FileNotFoundError):
            pool.inspect_trace(self.path, self.child, False, self.cwd)

    def counters(self):
        names = sorted(pool.COUNTER_NAMES)
        self.rows[1] = "process\t71\t2\t1\t1001"
        self.rows[2:2] = ["count\t" + name + "\t0" for name in names]
        for owner in self.owner:
            owner += ["owner_count\t" + name + "\t0" for name in names]

    def test_counts_are_once_after_pool_and_balanced(self):
        self.counters()
        trace = self.inspect(True)
        self.assertEqual(trace["counters"]["signature_worker_starts"], 0)
        self.rows.insert(2, "count\tsignature_requests\t0")
        with self.assertRaises(ValueError):
            self.inspect(True)

    def test_false_hit_or_incomplete_worker_rejected(self):
        self.counters()
        for name in ("literal_nonactive_hits", "signature_worker_starts", "reference_active_hits"):
            original = list(self.rows)
            self.rows = [row.replace("\t0", "\t1") if row.startswith("count\t" + name + "\t") else row for row in self.rows]
            with self.assertRaises(ValueError):
                self.inspect(True)
            self.rows = original

    def test_owner_sum_and_max_reconciliation(self):
        self.counters()
        self.rows = [row.replace("\t0", "\t10") if row.startswith("count\tliteral_max_key_items\t") else row for row in self.rows]
        for owner, value in zip(self.owner, (3, 10)):
            owner[:] = [row.replace("\t0", "\t" + str(value)) if row.startswith("owner_count\tliteral_max_key_items\t") else row for row in owner]
        self.inspect(True)
        self.owner[0] = [row.replace("\t0", "\t1") if row.startswith("owner_count\treference_key_items\t") else row for row in self.owner[0]]
        with self.assertRaises(ValueError):
            self.inspect(True)

    def test_missing_counter_schema_rejected(self):
        self.counters()
        self.rows = [row for row in self.rows if not row.startswith("count\tliteral_key_ns\t")]
        with self.assertRaises(ValueError):
            self.inspect(True)


if __name__ == "__main__":
    unittest.main()
