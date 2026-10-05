"""Failure controls for completed setup receipts and separate owner accounting."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "setup", Path(__file__).with_name("jsdoc-setup-controls.py")
)
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)


class SetupReceiptTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "trace.tsv"
        self.child = {"pid": 42, "started_at_unix_ns": 100, "exit_code": 1,
                      "timed_out": False, "stdout": "Parsed files: 2\n"}
        self.rows = ["tsr-jsdoc-setup-v1\t42\t101"]
        self.rows += [f"timer\t{name}\t2\t10\t5" for name in sorted(setup.TIMER_NAMES)]
        self.rows += ["declined\t3", "maps\t1\t2\t3\t2\t3"]
        self.rows += [f"memory\t{i}\t0\t{8 if i in (0, 1) else 0}\t{1 if i in (0, 1) else 0}\t0\t{16 if i in (0, 1) else 0}"
                      for i in sorted(setup.MEMORY_IDS)]
        self.rows += ["complete"]

    def read(self, workers=1):
        self.path.write_text("\n".join(self.rows) + "\n")
        return setup.read_probe(self.path, self.child, workers)

    def replace(self, prefix, value):
        self.rows = [value if row.startswith(prefix) else row for row in self.rows]

    def test_complete_json_round_trip_keeps_qualification_false(self):
        result = self.read()
        self.assertEqual(json.loads(json.dumps(result)), result)
        self.assertFalse(result["speed_qualified"])

    def test_partial_and_failed_children_are_rejected(self):
        self.rows.pop()
        with self.assertRaises(ValueError):
            self.read()
        self.rows.append("complete")
        self.child["timed_out"] = True
        with self.assertRaises(ValueError):
            self.read()

    def test_stale_or_wrong_pid_is_rejected(self):
        for row in ("tsr-jsdoc-setup-v1\t41\t101", "tsr-jsdoc-setup-v1\t42\t99"):
            self.rows[0] = row
            with self.assertRaises(ValueError):
                self.read()

    def test_boolean_child_fields_and_unsupported_owner_domains_are_rejected(self):
        self.child["exit_code"] = True
        with self.assertRaises(ValueError):
            self.read()
        self.child["exit_code"] = 1
        for workers in (True, 0, 5):
            with self.assertRaises(ValueError):
                self.read(workers)

    def test_duplicate_or_missing_domain_is_rejected(self):
        self.rows.insert(1, "timer\tparse\t2\t10\t5")
        with self.assertRaises(ValueError):
            self.read()
        self.rows.pop(1)
        self.rows = [row for row in self.rows if not row.startswith("timer\tattach\t")]
        with self.assertRaises(ValueError):
            self.read()

    def test_missing_parse_or_private_worker_is_rejected(self):
        with self.assertRaises(ValueError):
            self.read(workers=2)
        self.child["stdout"] = "Parsed files: 3\n"
        with self.assertRaises(ValueError):
            self.read()

    def test_leaked_or_impossible_simultaneous_storage_is_rejected(self):
        self.replace("memory\t1\t", "memory\t1\t1\t8\t1\t8\t16")
        with self.assertRaises(ValueError):
            self.read()
        self.replace("memory\t1\t", "memory\t1\t0\t8\t1\t0\t16")
        self.replace("memory\t0\t", "memory\t0\t0\t9\t1\t0\t16")
        with self.assertRaises(ValueError):
            self.read()

    def test_subset_and_capacity_impossibilities_are_rejected(self):
        self.replace("timer\tplain\t", "timer\tplain\t3\t11\t6")
        with self.assertRaises(ValueError):
            self.read()
        self.replace("timer\tplain\t", "timer\tplain\t2\t10\t5")
        self.replace("maps\t1\t", "maps\t1\t4\t3\t2\t3")
        with self.assertRaises(ValueError):
            self.read()


if __name__ == "__main__":
    unittest.main()
