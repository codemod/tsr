"""Reject cache replay and skipped semantic work as full-check evidence."""

import argparse
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

from check_cache_isolation import (
    POISON, SOURCE_NAMES, build_state, full_check_gates,
    normalize_diagnostics, poison_build_info, run,
)


def sample():
    entries = [f"<fixture>/{name}(1,1): error TS2322: Type 'string' is not assignable to type 'number'."
               for name in SOURCE_NAMES]
    return {
        "diagnostics": {"count": 3, "entries": entries, "fingerprint": "reference"},
        "statistics": {"Checked files": 3, "Instantiations": 238},
        "build_state_before": {}, "build_state_after": {},
        "pid": 42, "started_at_unix_ns": 1,
        "exit_code": 1, "timed_out": False,
    }


class CacheIsolationTests(unittest.TestCase):
    def test_identical_diagnostics_cannot_hide_incremental_replay(self):
        reference = sample()
        replay = copy.deepcopy(reference)
        replay["statistics"]["Instantiations"] = 0
        gates = full_check_gates(replay, reference, "tsgo")
        self.assertTrue(gates["complete_diagnostics_unchanged"])
        self.assertFalse(gates["native_instantiations_repeated"])
        self.assertFalse(all(gates.values()))

    def test_skipped_sources_and_changed_work_counts_fail(self):
        reference = sample()
        for tool, key, value in (("tsr", "Checked files", 2), ("tsgo", "Instantiations", 237)):
            candidate = copy.deepcopy(reference)
            candidate["statistics"][key] = value
            with self.subTest(tool=tool):
                self.assertFalse(all(full_check_gates(candidate, reference, tool).values()))
        for change in ("missing", "same_file_twice", "unknown_path", "poison"):
            candidate = copy.deepcopy(reference)
            if change == "missing":
                candidate["diagnostics"]["entries"].pop()
            elif change == "same_file_twice":
                candidate["diagnostics"]["entries"][1] = candidate["diagnostics"]["entries"][0]
            elif change == "unknown_path":
                candidate["diagnostics"]["entries"][0] = "outside.ts(1,1): error TS2322: unrelated"
            else:
                candidate["diagnostics"]["entries"][0] += POISON
            with self.subTest(change=change):
                self.assertFalse(all(full_check_gates(candidate, reference, "tsr").values()))

    def test_timeouts_and_build_state_writes_are_not_valid_fast_checks(self):
        reference = sample()
        for key, value in (("timed_out", True), ("exit_code", 0), ("pid", 0),
                           ("stderr", "unexpected compiler failure"),
                           ("build_state_after", {"cache.tsbuildinfo": {"sha256": "new"}})):
            candidate = copy.deepcopy(reference)
            candidate[key] = value
            with self.subTest(key=key):
                self.assertFalse(all(full_check_gates(candidate, reference, "tsr").values()))

    def test_poison_preserves_versions_options_and_diagnostic_location(self):
        info = {
            "version": "pinned", "fileInfos": [{"version": "unchanged-source"}],
            "options": {"strict": True}, "semanticDiagnosticsPerFile": [1, [2, [{
                "code": 2322, "pos": 3, "end": 4,
                "messageKey": "Type_0_is_not_assignable_to_type_1_2322",
                "messageArgs": ["string", "number"],
            }]]],
        }
        changed = json.loads(poison_build_info(json.dumps(info).encode()))
        self.assertEqual(changed["fileInfos"], info["fileInfos"])
        self.assertEqual(changed["options"], info["options"])
        diagnostic = changed["semanticDiagnosticsPerFile"][1][1][0]
        self.assertEqual(diagnostic["messageArgs"], [POISON, "number"])
        diagnostic["messageArgs"][0] = "string"
        self.assertEqual(changed, info)
        with self.assertRaises(ValueError):
            poison_build_info(b'{"semanticDiagnosticsPerFile": [1]}')

    def test_message_normalization_preserves_continuations_and_foreign_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory)
            (workspace / "index.ts").touch()
            detail = "\n  Nested relation detail.\n"
            absolute = f"{workspace}/index.ts(2,3): error TS2322: mismatch" + detail
            relative = "index.ts(2,3): error TS2322: mismatch" + detail
            normalized = normalize_diagnostics(absolute, workspace)
            self.assertEqual(normalized, normalize_diagnostics(relative, workspace))
            self.assertIn("Nested relation detail", normalized["entries"][0])
            second = "other.ts(1,2): error TS2322: second\n"
            self.assertEqual(normalize_diagnostics(relative + second, workspace),
                             normalize_diagnostics(second + relative, workspace))
            foreign = "/another-project/index.ts(2,3): error TS2322: mismatch" + detail
            self.assertNotEqual(normalized["fingerprint"], normalize_diagnostics(foreign, workspace)["fingerprint"])

    def test_default_and_configured_build_state_are_both_observed(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory)
            self.assertEqual(build_state(workspace), {})
            for name in ("cache.tsbuildinfo", "tsconfig.tsbuildinfo"):
                (workspace / name).write_bytes(b"malformed")
            observed = build_state(workspace)
            self.assertEqual(set(observed), {"cache.tsbuildinfo", "tsconfig.tsbuildinfo"})
            (workspace / "cache.tsbuildinfo").write_bytes(b"changed")
            self.assertNotEqual(build_state(workspace), observed)

    def test_existing_workspace_is_never_modified(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory)
            protected = workspace / "tsconfig.json"
            protected.write_text("user configuration")
            args = argparse.Namespace(tsr=Path(sys.executable), tsgo=Path(sys.executable),
                                      work_dir=workspace, output=workspace / "report.json")
            with self.assertRaises(FileExistsError):
                run(args)
            self.assertEqual(protected.read_text(), "user configuration")
            self.assertFalse(args.output.exists())


if __name__ == "__main__":
    unittest.main()
