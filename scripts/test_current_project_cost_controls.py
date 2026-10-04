"""Controls for disjoint macOS stack-sample accounting, without a compiler run."""
import importlib.util
import json
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "cost_controls", ROOT / "docs/architecture/current-project-cost-controls.py"
)
cost = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cost)


class SampleAccountingTests(unittest.TestCase):
    def test_nested_samples_are_assigned_once_to_nearest_owner(self):
        graph = """Call graph:
    10 Thread_1 MainThread com.apple.main-thread
    + 6 tsr_checker::members::outer (in tsr)
    + ! 4 tsr_checker::members::inner (in tsr)
    + ! : 4 malloc (in libsystem_malloc.dylib)
    + 4 tsr_compiler::loader::load (in tsr)
    20 Thread_2 Worker
    + 20 tsr_checker::other_thread (in tsr)
Total number in stack
"""
        result = cost.parse_sample(graph)
        self.assertEqual(result["total_samples"], 10)
        self.assertEqual(result["phases"], {"checker": 6, "other": 4})
        self.assertEqual(dict(result["nearest_tsr_owner"]), {
            "tsr_checker::members::inner": 4,
            "tsr_compiler::loader::load": 4,
            "tsr_checker::members::outer": 2,
        })
        self.assertEqual(result["allocator_nearest_owner"], [
            ["tsr_checker::members::inner", 4]
        ])
        self.assertEqual(result["ignored_thread_roots"], 1)
        self.assertEqual(json.loads(json.dumps(result)), result)

    def test_child_overcount_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "child counts"):
            cost.parse_sample("""Call graph:
    2 Thread_1 com.apple.main-thread
    + 3 tsr_checker::work (in tsr)
Total number in stack
""")

    def test_missing_main_thread_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "no main-thread"):
            cost.parse_sample("""Call graph:
    2 Thread_2 Worker
    + 2 tsr_checker::work (in tsr)
Total number in stack
""")

    def test_unowned_and_waiting_samples_remain_in_total(self):
        result = cost.parse_sample("""Call graph:
    7 Thread_1 com.apple.main-thread
    + 7 semaphore_wait (in libsystem_kernel.dylib)
Total number in stack
""")
        self.assertEqual(result["nearest_tsr_owner"], [["<no TSR frame>", 7]])
        self.assertEqual(result["phases"], {"other": 7})
        self.assertEqual(result["allocator_nearest_owner"], [])


if __name__ == "__main__":
    unittest.main()
