"""Reject incomplete constructor attribution and borrowed/moved misclassification."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "reference_controls", Path(__file__).with_name("checker-reference-key-controls.py"))
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


class ConstructionControlTests(unittest.TestCase):
    def setUp(self):
        self.counts = dict.fromkeys(probe.pool.COUNTER_NAMES, 0)
        self.prefix = "reference_site_ordinary_lookup_"
        values = dict(constructions=2, nonempty_constructions=1, vector_items=3,
                      vector_capacity_items=3, delegate_ns=20, allocations=1,
                      requested_bytes=12, usable_bytes=16, lookup_operations=2,
                      operation_vector_items=3)
        self.counts.update({self.prefix + k: v for k, v in values.items()})
        self.counts.update(reference_lookup_requests=2, reference_key_items=3,
                           reference_key_allocations=1, reference_key_requested_bytes=12,
                           reference_key_usable_bytes=16,
                           table_reference_lookup_operations=2,
                           table_reference_lookup_hash_invocations=2,
                           table_reference_lookup_hash_vector_items=3,
                           table_reference_lookup_hash_identity_fields=5,
                           table_reference_lookup_hash_delegate_ns=10,
                           table_reference_lookup_max_vector_items=3,
                           table_reference_lookup_map_ns=100)

    def test_valid_original_clone_and_empty_counts(self):
        probe.validate_counts(self.counts)
        probe.validate_counts(dict.fromkeys(probe.pool.COUNTER_NAMES, 0))

    def test_actual_constructor_count_omission_is_rejected(self):
        self.counts[self.prefix + "constructions"] = 0
        with self.assertRaisesRegex(ValueError, "constructor count omitted"):
            probe.validate_counts(self.counts)

    def test_actual_allocation_count_or_bytes_omission_is_rejected(self):
        for field in ("allocations", "requested_bytes", "usable_bytes"):
            counts = dict(self.counts)
            counts[self.prefix + field] = 0
            with self.subTest(field=field), self.assertRaises(ValueError):
                probe.validate_counts(counts)

    def test_reference_operation_cannot_escape_site_coverage(self):
        self.counts["table_reference_lookup_operations"] += 1
        with self.assertRaisesRegex(ValueError, "site operation reconciliation"):
            probe.validate_counts(self.counts)

    def test_borrowed_and_reused_keys_cannot_claim_construction(self):
        for name in ("index_original_lookup", "index_body_lookup", "member_body_lookup",
                     "empty_insert", "deferred_insert"):
            counts = dict(self.counts)
            counts[f"reference_site_{name}_constructions"] = 1
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "borrowed/reused"):
                probe.validate_counts(counts)

    def test_wrong_operation_and_unknown_production_calls_are_rejected(self):
        counts = dict(self.counts)
        counts["reference_site_ordinary_lookup_insert_operations"] = 1
        with self.assertRaisesRegex(ValueError, "wrong reference insert"):
            probe.validate_counts(counts)
        counts = dict(self.counts)
        counts["reference_site_unclassified_lookup_operations"] = 1
        with self.assertRaisesRegex(ValueError, "unclassified reference"):
            probe.validate_counts(counts)

    def test_original_allocator_tags_must_reconcile_to_sites(self):
        self.counts["reference_key_requested_bytes"] += 4
        with self.assertRaisesRegex(ValueError, "allocation tag/site"):
            probe.validate_counts(self.counts)

    def test_moved_vectors_retain_shape_with_zero_new_storage(self):
        counts = dict.fromkeys(probe.pool.COUNTER_NAMES, 0)
        prefix = "reference_site_keyword_insert_"
        counts.update({prefix + k: v for k, v in dict(constructions=1,
            nonempty_constructions=1, vector_items=8, vector_capacity_items=16,
            delegate_ns=5, insert_operations=1, operation_vector_items=8).items()})
        counts.update(table_reference_insert_operations=1,
                      table_reference_insert_hash_invocations=1,
                      table_reference_insert_hash_vector_items=8,
                      table_reference_insert_hash_identity_fields=9,
                      table_reference_insert_hash_delegate_ns=10,
                      table_reference_insert_max_vector_items=8,
                      table_reference_insert_map_ns=100)
        probe.validate_counts(counts)
        counts[prefix + "requested_bytes"] = 32
        counts[prefix + "usable_bytes"] = 32
        with self.assertRaisesRegex(ValueError, "moved key allocated"):
            probe.validate_counts(counts)

    def test_module_reader_schema_is_identical(self):
        probe.module_schema()
        self.assertEqual(len(probe.pool.COUNTER_NAMES), 563)


if __name__ == "__main__":
    unittest.main()
