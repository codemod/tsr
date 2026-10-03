"""Controls for accepting benchmark evidence, including per-child resource use."""

from pathlib import Path
import sys
import tempfile
import unittest

from whole_project_perf import diagnostics, file_identity, option_differences, process


class BenchmarkEvidenceTests(unittest.TestCase):
    def test_diagnostics_retain_continuations_and_ignore_order(self):
        first = "a.ts(1,1): error TS2322: incompatible\n  nested detail\nb.ts(2,2): error TS2345: argument\n"
        second = "b.ts(2,2): error TS2345: argument\na.ts(1,1): error TS2322: incompatible\n  nested detail\n"
        self.assertEqual(diagnostics(first, Path("/repo"))["fingerprint"],
                         diagnostics(second, Path("/repo"))["fingerprint"])
        self.assertNotEqual(diagnostics(first, Path("/repo"))["fingerprint"],
                            diagnostics(first.replace("nested detail", "different"), Path("/repo"))["fingerprint"])

    def test_scope_does_not_collapse_symlink_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "real.ts").touch()
            (root / "link.ts").symlink_to("real.ts")
            self.assertNotEqual(file_identity("real.ts", root), file_identity("link.ts", root))
            self.assertNotEqual(file_identity("lib.es5.d.ts", root),
                                file_identity("bundled:///libs/lib.es5.d.ts", root))

    def test_option_differences_preserve_missing_semantics(self):
        self.assertEqual(option_differences(
            {"compilerOptions": {"lib": ["ES2022"]}},
            {"compilerOptions": {"lib": ["es2022"]}}), {})
        self.assertIn("skipLibCheck", option_differences(
            {"compilerOptions": {}}, {"compilerOptions": {"skipLibCheck": True}}))

    def test_resources_and_diagnostics_belong_to_the_child(self):
        sample = process([sys.executable, "-c", "print('error TS2322: control'); raise SystemExit(1)"],
                         Path.cwd(), 10)
        self.assertEqual(sample["exit_code"], 1)
        self.assertFalse(sample["timed_out"])
        self.assertGreater(sample["peak_rss_bytes"], 0)
        self.assertEqual(diagnostics(sample["stdout"], Path.cwd())["count"], 1)

    def test_timeout_is_not_a_successful_fast_check(self):
        sample = process([sys.executable, "-c", "import time; time.sleep(10)"], Path.cwd(), 0.1)
        self.assertTrue(sample["timed_out"])
        self.assertLess(sample["exit_code"], 0)


if __name__ == "__main__":
    unittest.main()
