#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "alias_body_states", Path(__file__).with_name("checker-alias-body-state-controls.py"))
alias = importlib.util.module_from_spec(spec)
spec.loader.exec_module(alias)


class AliasBodyStateControls(unittest.TestCase):
    def counts(self):
        counts = dict.fromkeys(alias.pool.COUNTER_NAMES, 0)
        counts.update({
            "alias_body_other_requests": 2,
            "alias_body_other_cache_missing": 1,
            "alias_body_other_cache_error": 1,
            "alias_body_other_conditional_calls": 1,
            "alias_body_other_conditional_none": 1,
            "alias_body_other_body_calls": 1,
            "alias_body_other_body_error": 1,
            "alias_body_other_published_error": 1,
            "alias_body_other_returned_none": 2,
        })
        return counts

    def test_schema_and_existing_error_reuse(self):
        alias.module_schema()
        self.assertEqual(len(alias.NAMES), 75)
        self.assertEqual(len(alias.pool.COUNTER_NAMES), 433)
        alias.validate_counts(self.counts())

    def test_omissions_and_misclassifications_are_rejected(self):
        for key, value in (
            ("alias_body_other_cache_error", 0),
            ("alias_body_other_body_calls", 0),
            ("alias_body_other_body_error", 0),
            ("alias_body_other_published_error", 0),
            ("alias_body_other_conditional_none", 0),
            ("alias_body_other_returned_none", 1),
            ("alias_body_other_returned_error", 1),
            ("alias_body_other_reject_depth", 1),
            ("alias_body_other_reject_parameter", 1),
            ("alias_body_nonnullable_requests", 1),
            ("alias_body_other_cache_error", -1),
        ):
            with self.subTest(key=key):
                counts = self.counts()
                counts[key] = value
                with self.assertRaises(ValueError):
                    alias.validate_counts(counts)

    def test_cached_error_cannot_be_relabelled_value_with_same_returns(self):
        counts = self.counts()
        counts["alias_body_other_cache_error"] = 0
        counts["alias_body_other_cache_value"] = 1
        with self.assertRaisesRegex(ValueError, "None provenance"):
            alias.validate_counts(counts)

    def test_missing_counter_is_rejected(self):
        counts = self.counts()
        del counts["alias_body_other_cache_error"]
        with self.assertRaises(ValueError):
            alias.validate_counts(counts)


if __name__ == "__main__":
    unittest.main()
