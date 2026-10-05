"""Reject misleading private-owner byte observations."""
import importlib.util
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("memory", Path(__file__).with_name("checker-memory-controls.py"))
memory = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(memory)


class MemoryReaderTests(unittest.TestCase):
    def setUp(self):
        rows = ["MEMORY_HEADER\t71\t2000"]
        for phase in ("initialized", "checked", "released"):
            rows += [f"MEMORY_OWNER\t{phase}\t1\t10\t20\t3\t18",
                     f"MEMORY_OWNER\t{phase}\t2\t15\t30\t4\t23",
                     f"MEMORY_TOTAL\t{phase}\t25\t35\t7\t41"]
        self.child = {"pid": 71, "started_at_unix_ns": 1000, "exit_code": 0,
                      "timed_out": False, "stderr": "\n".join(rows) + "\n"}

    def test_coherent_live_sum_does_not_sum_distinct_peaks(self):
        result = memory.read_memory(self.child, 2)
        self.assertEqual(result["checked"]["total"]["peak_requested_bytes"], 35)
        self.assertEqual(sum(row["peak_requested_bytes"] for row in result["checked"]["owners"].values()), 50)

    def test_same_pid_replay_rejected(self):
        self.child["stderr"] = self.child["stderr"].replace("71\t2000", "71\t999")
        with self.assertRaisesRegex(ValueError, "stale"):
            memory.read_memory(self.child, 2)

    def test_signaled_child_rejected(self):
        self.child["exit_code"] = -9
        with self.assertRaisesRegex(ValueError, "incomplete memory child"):
            memory.read_memory(self.child, 2)

    def test_missing_boundary_rejected(self):
        self.child["stderr"] = self.child["stderr"].replace("MEMORY_TOTAL\treleased\t25\t35\t7\t41\n", "")
        with self.assertRaisesRegex(ValueError, "incomplete memory boundaries"):
            memory.read_memory(self.child, 2)

    def test_quiescent_sum_mismatch_rejected(self):
        self.child["stderr"] = self.child["stderr"].replace("MEMORY_TOTAL\tchecked\t25", "MEMORY_TOTAL\tchecked\t26")
        with self.assertRaisesRegex(ValueError, "owner sum"):
            memory.read_memory(self.child, 2)

    def test_duplicate_owner_rejected(self):
        self.child["stderr"] = self.child["stderr"].replace("MEMORY_OWNER\tchecked\t2", "MEMORY_OWNER\tchecked\t1")
        with self.assertRaisesRegex(ValueError, "duplicate"):
            memory.read_memory(self.child, 2)

    def test_peak_below_live_rejected(self):
        self.child["stderr"] = self.child["stderr"].replace("\t25\t35\t7", "\t25\t24\t7")
        with self.assertRaisesRegex(ValueError, "peak below"):
            memory.read_memory(self.child, 2)

    def test_padding_below_payload_rejected(self):
        self.child["stderr"] = self.child["stderr"].replace("\t25\t35\t7\t41", "\t25\t35\t7\t24")
        with self.assertRaisesRegex(ValueError, "padding below"):
            memory.read_memory(self.child, 2)


if __name__ == "__main__":
    unittest.main()
