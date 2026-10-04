"""End-to-end controls for the observational CI reporting entry point."""
from pathlib import Path
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/report_whole_project_perf.py"


class CIReportTests(unittest.TestCase):
    def compiler(self, root, name, behavior="pass", listed="main.ts", options=None,
                 preflight_behavior=None):
        config_behavior = preflight_behavior or f"print(json.dumps({options or {'compilerOptions': {'noEmit': True}}!r}))"
        path = root / name
        path.write_text(
            f"#!{sys.executable}\nimport json, pathlib, sys, time\n"
            f"p=pathlib.Path({str(root)!r})\n"
            "with (p/'launched.log').open('a') as f: f.write(json.dumps(sys.argv)+'\\n')\n"
            "if '--showConfig' in sys.argv:\n"
            f" {config_behavior}\n"
            "elif '--listFilesOnly' in sys.argv:\n"
            f" print(p / {listed!r})\n"
            "else:\n"
            f" {behavior}\n"
        )
        path.chmod(0o755)
        return path

    def run_report(self, root, behavior="pass", native_behavior=None, listed="main.ts",
                   native_options=None, ready=True, missing=False, stale=False,
                   preflight_behavior=None):
        (root / "tsconfig.json").write_text('{"files":["main.ts"]}')
        (root / "main.ts").write_text('export const value=1;')
        (root / "other.ts").write_text('export const other=2;')
        ours = self.compiler(root, "tsr", behavior, preflight_behavior=preflight_behavior)
        native = self.compiler(root, "tsgo", native_behavior or behavior, listed, native_options)
        out = root / "results"
        out.mkdir()
        if stale:
            (out / "default.json").write_text('{"status":"completed","target_verified":true,"observed_wall_ratio":0.01}')
            (out / "single.json").write_text((out / "default.json").read_text())
        if missing:
            native.unlink()
        args = [sys.executable, str(SCRIPT), "--project", str(root / "tsconfig.json"),
                "--tsr", str(ours), "--tsgo", str(native), "--output-dir", str(out),
                "--samples", "1", "--timeout", "0.3", "--builds-ready", str(ready).lower()]
        child = subprocess.run(args, capture_output=True, text=True, timeout=30,
                               env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"})
        self.assertTrue((out / "observations.json").exists(), child.stderr)
        report = json.loads((out / "observations.json").read_text())
        markdown = (out / "summary.md").read_text()
        self.assertFalse(report["release_target_verified"])
        self.assertIn("Smoke workload", markdown)
        self.assertIn("0.50", markdown)
        self.assertIn("not evaluated", markdown)
        return child, report, markdown

    def test_complete_checks_publish_resources_hashes_and_both_worker_requests(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            child, report, markdown=self.run_report(root)
            self.assertEqual(child.returncode, 0, child.stderr)
            self.assertEqual(set(report["runs"]), {"default", "single"})
            self.assertEqual(report["workload_kind"], "smoke-plumbing")
            self.assertIsNotNone(report["project_config_sha256"])
            for mode, run in report["runs"].items():
                self.assertEqual(run["state"], "completed_incomparable")
                self.assertFalse(run["report"]["work_comparable"])
                self.assertFalse(run["report"]["actual_checked_work_verified"])
                self.assertIn("Actual performed", markdown)
                for tool in run["report"]["tools"].values():
                    self.assertEqual(len(tool["binary_sha256"]), 64)
                    self.assertEqual(len(tool["input_fingerprint"]), 64)
                    sample=tool["samples"][0]
                    self.assertGreater(sample["peak_rss_bytes"], 0)
                    self.assertGreater(sample["pid"], 0)
                    self.assertIn("--incremental", sample["command"])
                    self.assertIn("--composite", sample["command"])
                    if mode=="single":
                        self.assertIn("--singleThreaded", sample["command"])
                    else:
                        self.assertNotIn("--singleThreaded", sample["command"])
            self.assertIn("CPU", markdown)
            self.assertIn("RSS", markdown)
            self.assertIn("stddev", markdown)

    def test_compiler_diagnostics_are_completed_checks_not_tool_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            child, report, markdown=self.run_report(Path(directory),
                "print('main.ts(1,1): error TS2322: intentional control');sys.exit(1)")
            self.assertEqual(child.returncode, 0, child.stderr)
            for run in report["runs"].values():
                self.assertTrue(run["report"]["diagnostics_match"])
                self.assertEqual(run["report"]["tools"]["tsr"]["samples"][0]["diagnostics"]["count"], 1)
            self.assertIn("diagnostic", markdown)

    def test_mismatched_options_scope_and_full_diagnostics_remain_incomparable(self):
        cases = [("other.ts", None, "pass"),
                 ("main.ts", {"compilerOptions":{"strict":True}}, "pass"),
                 ("main.ts", None, "print('main.ts(1,1): error TS2322: different detail');sys.exit(1)")]
        for listed, options, behavior in cases:
            with self.subTest(listed=listed, options=options), tempfile.TemporaryDirectory() as directory:
                child, report, markdown=self.run_report(Path(directory),native_behavior=behavior,
                                                       listed=listed,native_options=options)
                self.assertEqual(child.returncode, 0, child.stderr)
                for run in report["runs"].values():
                    self.assertEqual(run["state"], "completed_incomparable")
                    self.assertFalse(run["report"]["work_comparable"])
                self.assertIn("false", markdown)

    def test_timeout_and_crash_have_no_completed_ratio(self):
        for behavior,state in [("time.sleep(10)","timed_out"),("sys.exit(5)","tool_failed")]:
            with self.subTest(state=state), tempfile.TemporaryDirectory() as directory:
                child, report, markdown=self.run_report(Path(directory),behavior)
                self.assertNotEqual(child.returncode, 0)
                for run in report["runs"].values():
                    self.assertEqual(run["state"], state)
                    self.assertIn(state, markdown)
                self.assertNotIn("Observed wall ratio:", markdown)

    def test_input_mutation_is_preserved_as_rejected_measurement(self):
        with tempfile.TemporaryDirectory() as directory:
            child, report, markdown=self.run_report(Path(directory), "(p/'main.ts').write_text((p/'main.ts').read_text()+' changed')")
            self.assertNotEqual(child.returncode, 0)
            for run in report["runs"].values():
                self.assertEqual(run["state"], "inputs_changed")
                self.assertFalse(run["report"]["inputs_unchanged"])
            self.assertNotIn("Observed wall ratio:",markdown)

    def test_preflight_timeout_and_failure_preserve_child_evidence(self):
        for behavior, expected in [("time.sleep(10)", "timed_out"),
                                   ("sys.exit(5)", "tool_failed"),
                                   ("print('bad config');sys.exit(1)", "tool_failed")]:
            with self.subTest(expected=expected), tempfile.TemporaryDirectory() as directory:
                child, report, markdown = self.run_report(
                    Path(directory), preflight_behavior=behavior)
                self.assertNotEqual(child.returncode, 0)
                for run in report["runs"].values():
                    self.assertEqual(run["state"], expected)
                    failure = run["report"]["rejected_preflight"]
                    self.assertIn("--showConfig", failure["command"])
                    self.assertGreater(failure["pid"], 0)
                    self.assertEqual(failure["timed_out"], expected == "timed_out")
                self.assertNotIn("Observed wall ratio:", markdown)

    def test_setup_failure_cannot_publish_stale_report_or_launch_compilers(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            child, report, markdown=self.run_report(root,ready=False,stale=True)
            self.assertNotEqual(child.returncode, 0)
            self.assertFalse((root/"launched.log").exists())
            for run in report["runs"].values():
                self.assertEqual(run["state"], "setup_failed")
                self.assertIsNone(run["report"])
            self.assertFalse((root/"results/default.json").exists())
            self.assertNotIn("Observed wall ratio:",markdown)

    def test_missing_binary_still_publishes_failure_and_summary(self):
        with tempfile.TemporaryDirectory() as directory:
            child, report, markdown=self.run_report(Path(directory),missing=True,stale=True)
            self.assertNotEqual(child.returncode,0)
            for run in report["runs"].values():
                self.assertEqual(run["state"],"harness_failed")
                self.assertIsNone(run["report"])
                self.assertIsNotNone(run["error"])
            self.assertNotIn("Observed wall ratio:",markdown)

    def test_forged_fast_smoke_cannot_show_release_target_success(self):
        spec=importlib.util.spec_from_file_location("ci_report",SCRIPT)
        module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        summary=module.render_summary({"source_sha":"abc", "oracle_sha":"def", "runs":{
            "default":{"state":"completed", "harness_exit_code":0, "error":None,
                       "report":{"status":"completed","target_verified":True,
                                 "work_comparable":False,"observed_wall_ratio":0.001,"tools":{}}}}})
        self.assertIn("not evaluated",summary)
        self.assertNotIn("Observed wall ratio:",summary)
        self.assertNotIn("verified success",summary)


if __name__ == "__main__":
    unittest.main()
