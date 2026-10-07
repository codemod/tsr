"""Controls for accepting benchmark evidence, including per-child resource use."""

from pathlib import Path
import json
import os
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from benchmark_inputs import load_manifest, snapshot, valid_snapshot
from whole_project_perf import ROOT, diagnostics, file_identity, option_differences, process, revision


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

    def test_frozen_compiler_remains_original_after_shared_target_replacement(self):
        from whole_project_perf import freeze_binary
        from benchmark_inputs import file_hash
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shared = root / "shared-tsr"
            shared.write_text(f"#!{sys.executable}\nprint('current-candidate')\n")
            shared.chmod(0o755)
            expected = file_hash(shared)
            frozen = root / "frozen-tsr"
            receipt = freeze_binary(shared, frozen, expected)
            shared.write_text(f"#!{sys.executable}\nprint('old-baseline')\n")
            child = process([str(frozen)], root, 10)
            self.assertEqual(child["stdout"].strip(), "current-candidate")
            self.assertEqual(file_hash(frozen), receipt["sha256"])
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                freeze_binary(shared, root / "wrong-build", expected)
            self.assertFalse(receipt["source_to_binary_provenance_verified"])

    def test_checkout_identity_distinguishes_unmerged_tracked_and_untracked_source(self):
        from whole_project_perf import checkout_identity
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def git(*args):
                subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True)
            git("init")
            git("config", "user.email", "fixture@example.invalid")
            git("config", "user.name", "Fixture")
            source = root / "source.rs"
            source.write_text("baseline")
            git("add", "source.rs")
            git("commit", "-m", "baseline")
            baseline = checkout_identity(root)
            source.write_text("unmerged AST candidate")
            candidate = checkout_identity(root)
            self.assertEqual(baseline["head"], candidate["head"])
            self.assertNotEqual(baseline["identity_sha256"], candidate["identity_sha256"])
            git("add", "source.rs")
            self.assertEqual(candidate["identity_sha256"], checkout_identity(root)["identity_sha256"])
            (root / "helper.rs").write_text("untracked source")
            self.assertNotEqual(candidate["identity_sha256"], checkout_identity(root)["identity_sha256"])
            self.assertFalse(candidate["build_provenance_verified"])
            self.assertFalse(candidate["causal_baseline_verified"])

    def test_ambient_trace_cannot_silently_serialize_timed_children(self):
        with patch.dict(os.environ, {"TSR_WORK_TRACE": "/ambient/trace", "TSR_WORK_TRACE_BINARY_SHA256": "ambient"}):
            sample = process([sys.executable, "-c",
                              "import os,json; print(json.dumps({k:v for k,v in os.environ.items() if k.startswith('TSR_WORK_TRACE')}))"],
                             Path.cwd(), 10)
        self.assertEqual(json.loads(sample["stdout"]), {})

    def test_matching_counts_and_caller_claims_cannot_discharge_work_certificate(self):
        from whole_project_perf import equivalence_certificate
        report = {"sampling_protocol_verified": True, "scope_match": True, "options_match": True,
                  "diagnostics_stable": True, "diagnostics_match": True, "inputs_unchanged": True,
                  "oracle_sha": "5b1047d10d32e7d5b446be4de56b126ff42f82bb",
                  "actual_checked_work_verified": True, "complete_input_equivalence_verified": True,
                  "build_provenance_verified": True, "flags": ["--noEmit"],
                  "tools": {name: {"effective_config": {"compilerOptions": {"noEmit": True}}}
                            for name in ("tsr", "tsgo")}}
        certificate = equivalence_certificate(report)
        self.assertFalse(certificate["verified"])
        self.assertEqual(certificate["proof_gap_count"], 9)
        self.assertIn("native_emit_eligibility_skipping_and_actual_work", certificate["unmet_constraints"])
        report["tools"]["tsgo"]["effective_config"]["compilerOptions"]["noEmit"] = False
        self.assertIn("matching_explicit_no_emit_options", equivalence_certificate(report)["unmet_constraints"])
        report["tools"]["tsgo"]["effective_config"]["compilerOptions"]["noEmit"] = True
        self.assertIn("actual_timed_full_worker_completion_and_cancellation", certificate["unmet_constraints"])
        report["sampling_protocol_verified"] = False
        self.assertIn("five_fresh_pairs_and_warmups", equivalence_certificate(report)["unmet_constraints"])

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

    def test_child_exit_and_resource_evidence_without_python39_wait_helper(self):
        # The self-hosted reporting runner uses Python 3.8. Exercise actual
        # children with its API surface, rather than mocking their wait status.
        with patch.dict(os.__dict__):
            os.__dict__.pop("waitstatus_to_exitcode", None)
            for code in (0, 2, 5):
                with self.subTest(exit_code=code):
                    sample = process([sys.executable, "-c", f"raise SystemExit({code})"],
                                     Path.cwd(), 10)
                    self.assertEqual(sample["exit_code"], code)
                    self.assertFalse(sample["timed_out"])
                    self.assertGreater(sample["peak_rss_bytes"], 0)
                    self.assertGreaterEqual(sample["user_seconds"], 0)
                    self.assertGreaterEqual(sample["system_seconds"], 0)
            sample = process([sys.executable, "-c",
                              "import os, signal; os.kill(os.getpid(), signal.SIGTERM)"],
                             Path.cwd(), 10)
            self.assertEqual(sample["exit_code"], -signal.SIGTERM)
            self.assertFalse(sample["timed_out"])
            sample = process([sys.executable, "-c", "import time; time.sleep(10)"],
                             Path.cwd(), 0.1)
            self.assertEqual(sample["exit_code"], -signal.SIGKILL)
            self.assertTrue(sample["timed_out"])


class InputEvidenceTests(unittest.TestCase):
    def test_manifest_fifo_is_rejected_without_reading(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "manifest-pipe"
            os.mkfifo(path)
            with self.assertRaises(OSError):
                load_manifest(path, Path(directory))

    def test_capture_errors_invalidate_observation_instead_of_becoming_missing(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "source.ts"
            path.touch()
            with patch("benchmark_inputs.file_hash", side_effect=PermissionError("unreadable")):
                rows = snapshot([str(path)])
            self.assertEqual(rows[0]["kind"], "file")
            self.assertIn("unreadable", rows[0]["error"])
            self.assertFalse(valid_snapshot(rows))

    def test_scratch_drivers_can_load_harness_without_scripts_on_sys_path(self):
        with tempfile.TemporaryDirectory() as directory:
            source = ("import runpy; "
                      f"h = runpy.run_path({str(ROOT / 'scripts/whole_project_perf.py')!r}); "
                      "assert callable(h['process']); assert callable(h['inputs'].snapshot)")
            result = subprocess.run([sys.executable, "-c", source], cwd=directory,
                                    capture_output=True, text=True, timeout=10,
                                    env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"})
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_manifest_preserves_logical_paths_and_requires_provenance(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "inputs.json"
            value = {"schema_version": 1, "paths": ["link.ts", "./link.ts"],
                     "provenance": {"source_sha": "native-pin", "producer": "query probe"}}
            manifest.write_text(json.dumps(value))
            paths, evidence = load_manifest(manifest, root)
            self.assertEqual(paths, sorted([str(root / "link.ts"), str(root) + "/./link.ts"]))
            self.assertEqual(evidence["coverage"], "caller-supplied observed paths; partial")
            value.pop("provenance")
            manifest.write_text(json.dumps(value))
            with self.assertRaises(ValueError):
                load_manifest(manifest, root)

    def test_symlink_parent_and_file_dot_segments_keep_filesystem_meaning(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "target" / "nested").mkdir(parents=True)
            (root / "target" / "value.ts").write_text("target value")
            (root / "value.ts").write_text("different root value")
            (root / "alias").symlink_to("target/nested", target_is_directory=True)
            spelling = "alias/../value.ts"
            identity = file_identity(spelling, root)
            self.assertEqual(identity, str(root) + "/" + spelling)
            row = snapshot([identity])[0]
            self.assertEqual(row["realpath"], os.path.realpath(root / "target" / "value.ts"))
            self.assertEqual(row["sha256"], snapshot([str(root / "target" / "value.ts")])[0]["sha256"])
            self.assertNotEqual(row["sha256"], snapshot([str(root / "value.ts")])[0]["sha256"])
            for suffix in ("/.", "/"):
                invalid = str(root / "value.ts") + suffix
                self.assertEqual(snapshot([invalid])[0]["kind"], "missing")

    def test_each_resolution_input_mutation_changes_observation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "package.json"
            extended = root / "extended.json"
            missing = root / "candidate.ts"
            real = root / "real.ts"
            alternate = root / "other.ts"
            link = root / "link.ts"
            for path in (manifest, extended, real, alternate):
                path.write_text("same bytes")
            link.symlink_to(real.name)
            for label, paths, mutate in (
                ("manifest", [manifest], lambda: manifest.write_text("new bytes")),
                ("missing candidate", [missing], lambda: missing.write_text("export {}")),
                ("symlink", [link], lambda: (link.unlink(), link.symlink_to(alternate.name))),
                ("extended config", [extended], lambda: extended.write_text("new config")),
                ("directory addition", [root], lambda: (root / "entry.ts").touch()),
                ("directory removal", [root], lambda: (root / "entry.ts").unlink()),
            ):
                with self.subTest(label=label):
                    before = snapshot([str(path) for path in paths])
                    self.assertTrue(valid_snapshot(before))
                    mutate()
                    after = snapshot([str(path) for path in paths])
                    self.assertTrue(valid_snapshot(after))
                    self.assertNotEqual(before, after)

    def test_missing_kind_and_symlinked_parent_remain_observable(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "one").mkdir()
            (root / "two").mkdir()
            (root / "alias").symlink_to("one", target_is_directory=True)
            name = str(root / "alias" / "missing.ts")
            before = snapshot([name])
            self.assertEqual(before[0]["kind"], "missing")
            self.assertEqual(before[0]["symlinks"][-1]["target"], "one")
            (root / "alias").unlink()
            (root / "alias").symlink_to("two", target_is_directory=True)
            self.assertNotEqual(before, snapshot([name]))

    def test_fifo_and_broken_link_are_not_read_as_source_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fifo = root / "pipe"
            os.mkfifo(fifo)
            (root / "broken").symlink_to("missing")
            rows = snapshot([str(fifo), str(root / "broken")])
            self.assertTrue(valid_snapshot(rows))
            by_path = {row["path"]: row for row in rows}
            self.assertEqual(by_path[str(fifo)]["kind"], "other")
            self.assertNotIn("sha256", by_path[str(fifo)])
            self.assertEqual(by_path[str(root / "broken")]["kind"], "missing")
            self.assertTrue(by_path[str(root / "broken")]["symlinks"])

    def run_harness(self, root, mutation="", warmups=0, require_comparable=False,
                    listed_path="main.ts", config_fifo=False):
        project = root / "project"
        project.mkdir()
        (project / "tsconfig.json").write_text('{"extends":"./extended.json"}')
        (project / "extended.json").write_text("{}")
        (project / "package.json").write_text('{"types":"index.ts"}')
        (project / "main.ts").write_text("export const x = 1;")
        (project / "other.ts").write_text("export const x = 1;")
        (project / "link.ts").symlink_to("main.ts")
        (project / "entries").mkdir()
        (project / "entries" / "existing.ts").touch()
        if config_fifo:
            (project / "tsconfig.json").unlink()
            os.mkfifo(project / "tsconfig.json")
        if listed_path == "pipe":
            os.mkfifo(project / "pipe")
        binary = root / "compiler"
        binary.write_text(
            f"#!{sys.executable}\n"
            "import json, pathlib, sys\n"
            f"p = pathlib.Path({str(project)!r})\n"
            "if '--showConfig' in sys.argv:\n"
            " print(json.dumps({'compilerOptions': {'noEmit': True}}))\n"
            "elif '--listFilesOnly' in sys.argv:\n"
            f" print(p / {listed_path!r})\n"
            "else:\n"
            f" {mutation or 'pass'}\n"
            " print('main.ts(1,1): error TS2322: intentional control')\n"
            " sys.exit(1)\n"
        )
        binary.chmod(0o755)
        manifest = root / "inputs.json"
        manifest.write_text(json.dumps({
            "schema_version": 1,
            "provenance": {"source_sha": revision(ROOT), "producer": "public mutation fixture"},
            "paths": ["package.json", "extended.json", "missing.ts", "link.ts", "entries"],
        }))
        output = root / "report.json"
        command = [sys.executable, str(ROOT / "scripts/whole_project_perf.py"),
                   "--project", str(project / "tsconfig.json"), "--tsr", str(binary),
                   "--tsgo", str(binary), "--input-manifest", str(manifest),
                   "--samples", "1", "--warmups", str(warmups), "--output", str(output)]
        if require_comparable:
            command.append("--require-comparable")
        result = subprocess.run(command, capture_output=True, text=True, timeout=15,
                                env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"})
        self.assertTrue(output.exists(), result.stderr)
        return result, json.loads(output.read_text())

    def test_loaded_special_file_is_rejected_without_reading_it(self):
        with tempfile.TemporaryDirectory() as directory:
            result, report = self.run_harness(Path(directory), listed_path="pipe")
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertEqual(report["status"], "invalid_loaded_inputs")
            self.assertFalse(report["target_verified"])
            self.assertIsNone(report["verified_wall_ratio"])

    def test_special_config_is_rejected_before_launching_a_compiler(self):
        with tempfile.TemporaryDirectory() as directory:
            result, report = self.run_harness(Path(directory), config_fifo=True)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertEqual(report["status"], "invalid_inputs")
            self.assertEqual(report["input_observations"], [])
            self.assertFalse(report["target_verified"])

    def test_cli_rejects_all_mutations_and_preserves_failed_sample(self):
        mutations = {
            "manifest": "(p / 'package.json').write_text('{}')",
            "missing": "(p / 'missing.ts').write_text('export {}')",
            "symlink": "(p / 'link.ts').unlink(); (p / 'link.ts').symlink_to('other.ts')",
            "extended": "(p / 'extended.json').write_text('{\"compilerOptions\":{}}')",
            "directory add": "(p / 'entries' / 'new.ts').touch()",
            "directory remove": "(p / 'entries' / 'existing.ts').unlink()",
            "root config": "(p / 'tsconfig.json').write_text('{}')",
            "binary": "(p.parent / 'compiler').write_text('# modified compiler')",
        }
        for label, mutation in mutations.items():
            with self.subTest(label=label), tempfile.TemporaryDirectory() as directory:
                result, report = self.run_harness(Path(directory), mutation)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertEqual(report["status"], "inputs_changed")
                self.assertFalse(report["inputs_unchanged"])
                self.assertFalse(report["target_verified"])
                self.assertIsNone(report["verified_wall_ratio"])
                self.assertEqual(len(report["tools"]["tsr"]["samples"]), 1)
                self.assertFalse(report["rejected_measurement"]["input_validation"]["stable"])
                self.assertEqual(report["rejected_measurement"]["diagnostics"]["count"], 1)

    def test_warmup_input_change_cannot_disappear_before_timing(self):
        with tempfile.TemporaryDirectory() as directory:
            result, report = self.run_harness(
                Path(directory), "(p / 'package.json').write_text('{}')", warmups=1)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertEqual(report["tools"]["tsr"]["samples"], [])
            self.assertIn("rejected_measurement", report)
            self.assertFalse(report["target_verified"])

    def test_failed_warmup_preserves_child_failure_before_aborting(self):
        with tempfile.TemporaryDirectory() as directory:
            result, report = self.run_harness(Path(directory), "sys.exit(5)", warmups=1)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(report["status"], "tool_failed")
            self.assertEqual(report["tools"]["tsr"]["samples"], [])
            self.assertEqual(report["rejected_measurement"]["exit_code"], 5)
            self.assertFalse(report["rejected_measurement"]["timed_out"])
            self.assertFalse(report["target_verified"])
            self.assertIsNone(report["verified_wall_ratio"])

    def test_partial_inputs_and_loaded_scope_cannot_verify_speed(self):
        for require_comparable in (False, True):
            with self.subTest(require_comparable=require_comparable), tempfile.TemporaryDirectory() as directory:
                result, report = self.run_harness(Path(directory), require_comparable=require_comparable)
                self.assertEqual(result.returncode, int(require_comparable), result.stderr)
                self.assertEqual(report["status"], "completed")
                self.assertTrue(report["inputs_unchanged"])
                self.assertTrue(report["scope_match"])
                self.assertTrue(report["options_match"])
                self.assertTrue(report["diagnostics_match"])
                self.assertFalse(report["complete_input_equivalence_verified"])
                self.assertFalse(report["actual_checked_work_verified"])
                self.assertFalse(report["work_comparable"])
                self.assertIsNone(report["verified_wall_ratio"])
                self.assertFalse(report["target_verified"])
                samples = [tool["samples"][0] for tool in report["tools"].values()]
                self.assertNotEqual(samples[0]["pid"], samples[1]["pid"])
                self.assertTrue(all(sample["input_validation"]["stable"] for sample in samples))
                self.assertTrue(all(sample["input_validation"]["before_capture_seconds"] >= 0
                                    for sample in samples))


if __name__ == "__main__":
    unittest.main()
