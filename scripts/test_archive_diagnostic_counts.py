import copy
import hashlib
import json
import unittest

import archive_diagnostic_counts as counts


class ArchiveDiagnosticCountsTests(unittest.TestCase):
    def fixture(self, entries):
        diagnostics = {"count": len(entries), "entries": entries,
                       "fingerprint": hashlib.sha256(json.dumps(sorted(entries)).encode()).hexdigest()}
        binding = {"source": "source", "sha256": "binary"}
        raw = {"complete": True, "bindings": {"normal": binding, "probe": binding},
               "cases": [{"mode": "default", "complete": True, "runs": [
                   {"timed_out": False, "exit_code": int(bool(entries)),
                    "scope": {"diagnostics": diagnostics}}]}]}
        report = {"source": "source", "bindings": copy.deepcopy(raw["bindings"]),
                  "application": [{"mode": "default", "compiler_children": 1,
                                   "diagnostic_count": len(entries)}]}
        return report, raw

    def check(self, report, raw):
        encoded = json.dumps(raw).encode()
        report["application"][0]["raw_report_sha256"] = hashlib.sha256(encoded).hexdigest()
        return counts.validate_archive(report, encoded)

    def test_empty_and_complete_multiline_diagnostics(self):
        for entries in ([], ["error TS1: one\n  continuation", "error TS2: two"]):
            with self.subTest(entries=entries):
                report, raw = self.fixture(entries)
                result = self.check(report, raw)
                self.assertEqual(result["application"][0]["diagnostic_count"], len(entries))

    def test_dictionary_length_packaging_mutation_is_rejected(self):
        report, raw = self.fixture(["error TS1: one"] * 46)
        report["application"][0]["diagnostic_count"] = len(raw["cases"][0]["runs"][0]["scope"]["diagnostics"])
        with self.assertRaisesRegex(ValueError, "archive diagnostic count"):
            self.check(report, raw)

    def test_count_and_fingerprint_disagreements_are_rejected(self):
        for key, value in (("count", 3), ("count", True), ("fingerprint", "stale"), ("entries", [None])):
            report, raw = self.fixture(["error TS1: one"])
            raw["cases"][0]["runs"][0]["scope"]["diagnostics"][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                self.check(report, raw)

    def test_stale_raw_hash_is_rejected(self):
        report, raw = self.fixture([])
        report["application"][0]["report_sha256"] = "stale"
        with self.assertRaisesRegex(ValueError, "application hash"):
            self.check(report, raw)

    def test_stale_source_and_binary_are_rejected(self):
        for key in ("source", "sha256"):
            report, raw = self.fixture([])
            raw["bindings"]["probe"][key] = "stale"
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "compiler binding"):
                self.check(report, raw)

    def test_partial_failed_and_cross_mode_receipts_are_rejected(self):
        for mutation in ("partial", "timeout", "signal", "mode", "missing", "duplicate"):
            report, raw = self.fixture([])
            if mutation == "partial":
                raw["complete"] = False
            elif mutation == "timeout":
                raw["cases"][0]["runs"][0]["timed_out"] = True
            elif mutation == "signal":
                raw["cases"][0]["runs"][0]["exit_code"] = -9
            elif mutation == "mode":
                raw["cases"][0]["mode"] = "single"
            elif mutation == "missing":
                raw["cases"] = []
            else:
                raw["cases"].append(copy.deepcopy(raw["cases"][0]))
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                self.check(report, raw)

    def test_changed_output_between_children_is_rejected(self):
        report, raw = self.fixture(["error TS1: one"])
        child = copy.deepcopy(raw["cases"][0]["runs"][0])
        child["scope"]["diagnostics"] = self.fixture(["error TS2: two"])[1]["cases"][0]["runs"][0]["scope"]["diagnostics"]
        raw["cases"][0]["runs"].append(child)
        report["application"][0]["compiler_children"] = 2
        with self.assertRaisesRegex(ValueError, "output changed"):
            self.check(report, raw)


if __name__ == "__main__":
    unittest.main()
