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

    def test_diagnostic_summary_and_phase_rows_are_not_message_continuations(self):
        output = "a.ts(1,1): error TS2322: incompatible\n  nested detail\n"
        expected = diagnostics(output, Path("/repo"))
        for suffix in (
            "\nFound 1 error in a.ts\x1b[90m:1\x1b[0m\n\n",
            "\nFound 2 errors in the same file, starting at: a.ts:1\n\n",
            "\nFound 2 errors in 2 files.\n\nErrors  Files\n     1  a.ts:1\n     1  b.ts:2\n",
            "\nErrors  Files\n     1  a.ts:1\n",
            "\nFiles: 123\n  Total time: 1.0s\n",
        ):
            with self.subTest(suffix=suffix):
                self.assertEqual(diagnostics(output + suffix, Path("/repo")), expected)

    def test_diagnostic_ansi_paths_and_related_information_remain_significant(self):
        plain = ("/repo/a.ts(1,1): error TS2322: incompatible\n"
                 "  /repo/b.ts(2,3): The expected type comes from this declaration.\n")
        colored = plain.replace("error", "\x1b[91merror\x1b[0m")
        colored = colored.replace("declaration", "\x1b[1mdeclaration\x1b[0m")
        expected = diagnostics(plain, Path("/repo"))
        self.assertEqual(diagnostics(colored, Path("/repo")), expected)
        self.assertIn("  <project>/b.ts(2,3):", expected["entries"][0])
        for original, replacement in (
            ("a.ts", "c.ts"), ("(1,1)", "(1,2)"), ("TS2322", "TS2345"),
            ("incompatible", "different"), ("declaration", "other declaration"),
        ):
            with self.subTest(original=original):
                self.assertNotEqual(diagnostics(plain.replace(original, replacement), Path("/repo"))
                                    ["fingerprint"], expected["fingerprint"])

    def test_error_text_inside_a_continuation_does_not_start_another_diagnostic(self):
        output = ("error TS2322: incompatible\n"
                  "  This message quotes error TS2345: argument.\n")
        result = diagnostics(output, Path("/repo"))
        self.assertEqual(result["count"], 1)
        self.assertIn("quotes error TS2345:", result["entries"][0])

    def test_missing_or_truncated_diagnostics_change_the_fingerprint(self):
        output = "a.ts(1,1): error TS2322: incompatible\n  nested detail\n"
        expected = diagnostics(output, Path("/repo"))["fingerprint"]
        for truncated in ("", "a.ts(1,1): error TS2322: incom", output.splitlines()[0]):
            with self.subTest(truncated=truncated):
                self.assertNotEqual(diagnostics(truncated, Path("/repo"))["fingerprint"], expected)

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

    def test_process_identity_belongs_to_the_executed_child(self):
        command = [sys.executable, "-c", "import os; print(os.getpid())"]
        first = process(command, Path.cwd(), 10)
        second = process(command, Path.cwd(), 10)
        for sample in (first, second):
            self.assertEqual(sample["pid"], int(sample["stdout"].strip()))
            self.assertEqual(sample["command"], command)
        self.assertLess(first["started_at_unix_ns"], second["started_at_unix_ns"])

    def test_timeout_is_not_a_successful_fast_check(self):
        sample = process([sys.executable, "-c", "import time; time.sleep(10)"], Path.cwd(), 0.1)
        self.assertTrue(sample["timed_out"])
        self.assertLess(sample["exit_code"], 0)


if __name__ == "__main__":
    unittest.main()
