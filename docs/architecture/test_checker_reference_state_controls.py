#!/usr/bin/env python3
"""Reader falsifiers; compiler-hook mutations are separate executed controls."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "reference_states", Path(__file__).with_name("checker-reference-state-controls.py"))
states = importlib.util.module_from_spec(spec)
spec.loader.exec_module(states)


class ReferenceStateControls(unittest.TestCase):
    def counts(self):
        counts = dict.fromkeys(states.pool.COUNTER_NAMES, 0)
        counts.update({
            "state_site_ordinary_lookup_calls": 1,
            "state_site_ordinary_lookup_misses": 1,
            "reference_lookup_requests": 1,
            "state_site_named_insert_calls": 1,
            "state_site_named_insert_values": 1,
            "state_site_named_insert_new_entries": 1,
            "state_request_ordinary_requests": 1,
            "state_request_ordinary_returned_value": 1,
            "state_request_ordinary_other_returns": 1,
            "state_request_ordinary_reservations": 1,
            "state_request_ordinary_reservation_closed": 1,
        })
        return counts

    def test_module_schema_and_valid_counts(self):
        states.module_schema()
        self.assertEqual(len(states.STATE_NAMES), 258)
        states.validate_counts(self.counts())

    def test_mutations_are_rejected(self):
        for key, value in (
            ("state_site_ordinary_lookup_misses", 0),
            ("state_site_ordinary_lookup_errors", 1),
            ("state_site_ordinary_lookup_reserved_answers", 1),
            ("state_site_named_insert_new_entries", 0),
            ("state_site_named_insert_replaced_errors", 1),
            ("state_request_ordinary_reservation_closed", 0),
            ("state_request_ordinary_unreturned", 1),
            ("state_request_ordinary_cache_hit_returns", 1),
            ("state_request_nonnullable_reservations", 1),
            ("state_worker_nonnullable_body_calls", 1),
            ("state_worker_string_mapping_returned_error", 1),
            ("state_site_empty_lookup_calls", 1),
            ("state_site_named_insert_calls", -1),
        ):
            with self.subTest(key=key):
                counts = self.counts()
                counts[key] = value
                with self.assertRaises(ValueError):
                    states.validate_counts(counts)

    def test_missing_state_counter_is_rejected(self):
        counts = self.counts()
        del counts["state_site_named_insert_values"]
        with self.assertRaises(ValueError):
            states.validate_counts(counts)

    def test_reserved_error_is_an_independent_lookup_dimension(self):
        counts = dict.fromkeys(states.pool.COUNTER_NAMES, 0)
        counts.update({
            "state_site_member_body_lookup_calls": 1,
            "state_site_member_body_lookup_errors": 1,
            "state_site_member_body_lookup_reserved_answers": 1,
            "state_site_member_body_lookup_framed_answers": 1,
        })
        states.validate_counts(counts)


if __name__ == "__main__":
    unittest.main()
