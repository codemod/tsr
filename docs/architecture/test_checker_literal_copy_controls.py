"""Reject falsely eligible clone attribution while reusing pool receipt checks."""
import importlib.util
from pathlib import Path
import re
import unittest

SPEC = importlib.util.spec_from_file_location(
    "literal_copy", Path(__file__).with_name("checker-literal-copy-controls.py"))
copy = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(copy)
BASE_SPEC = importlib.util.spec_from_file_location(
    "base_receipts", Path(__file__).with_name("test_checker_mapper_pool_controls.py"))
base_receipts = importlib.util.module_from_spec(BASE_SPEC)
BASE_SPEC.loader.exec_module(base_receipts)
base_receipts.pool = copy.pool


class ExtendedReceiptTests(base_receipts.ReceiptTests):
    """Run the existing process, pool, affinity and truncation controls."""


class CopyTests(unittest.TestCase):
    def setUp(self):
        self.counts = dict.fromkeys(copy.pool.COUNTER_NAMES, 0)
        self.counts.update(literal_requests=1, literal_nonactive_hits=1,
            literal_copy_hit_calls=1, literal_copy_hit_property_images=1,
            literal_copy_hit_property_elements=1, literal_copy_hit_property_owned_strings=3,
            literal_copy_hit_property_string_len_bytes=8,
            literal_copy_hit_property_string_capacity_bytes=8,
            literal_copy_hit_property_vec_capacity_bytes=160,
            literal_copy_hit_property_allocations=4,
            literal_copy_hit_property_requested_bytes=168,
            literal_copy_hit_property_usable_bytes=192,
            literal_prelookup_property_allocations=4,
            literal_prelookup_property_requested_bytes=168,
            literal_prelookup_property_usable_bytes=192)

    def test_eligible_and_zero_copy_hits(self):
        copy.validate_counts(self.counts)
        copy.validate_counts(dict.fromkeys(copy.pool.COUNTER_NAMES, 0))

    def test_hit_moved_into_miss_bucket(self):
        self.counts.update(literal_copy_hit_calls=0, literal_copy_miss_calls=1)
        with self.assertRaisesRegex(ValueError, "classification"):
            copy.validate_counts(self.counts)

    def test_lost_or_double_charged_allocation(self):
        for field in ("allocations", "requested_bytes", "usable_bytes"):
            with self.subTest(field=field):
                changed = dict(self.counts)
                changed[f"literal_copy_miss_property_{field}"] = 1
                with self.assertRaisesRegex(ValueError, "reconciliation"):
                    copy.validate_counts(changed)

    def test_refusal_reason_and_impossible_copies(self):
        self.counts["literal_copy_refusal_calls"] = 1
        with self.assertRaisesRegex(ValueError, "refusal"):
            copy.validate_counts(self.counts)
        self.counts["literal_copy_refusal_origin_refusals"] = 1
        copy.validate_counts(self.counts)
        self.counts["literal_copy_refusal_property_images"] = 1
        with self.assertRaises(ValueError):
            copy.validate_counts(self.counts)

    def test_capacity_and_requested_bytes_are_not_interchangeable(self):
        self.counts["literal_copy_hit_property_string_capacity_bytes"] = 7
        with self.assertRaisesRegex(ValueError, "capacity"):
            copy.validate_counts(self.counts)

    def test_owner_cannot_pass_on_global_agreement_alone(self):
        copy.validate_counts(self.counts)
        changed = dict(self.counts, literal_copy_hit_calls=2)
        with self.assertRaises(ValueError):
            copy.validate_counts(changed)

    def test_rust_and_reader_use_the_same_complete_counter_schema(self):
        text = Path(__file__).with_name("checker-literal-copy-probe.rs").read_text()
        fields = text.split("const FIELDS:", 1)[1].split("const BUCKETS:", 1)[0]
        buckets = text.split("const BUCKETS:", 1)[1].split("const WIDTH:", 1)[0]
        self.assertEqual(tuple(re.findall(r'"([a-z_]+)"', fields)), copy.FIELDS)
        self.assertEqual(tuple(re.findall(r'"([a-z_]+)"', buckets)), copy.BUCKETS)

    def test_wrong_source_bindings_are_rejected(self):
        bindings = {role: {"source": source} for role, source in
                    (("normal", copy.RUST_SOURCE), ("probe", copy.RUST_SOURCE), ("native", copy.NATIVE_SOURCE))}
        copy.validate_bindings(bindings)
        for role in bindings:
            changed = {key: dict(value) for key, value in bindings.items()}
            changed[role]["source"] = "0" * 40
            with self.subTest(role=role), self.assertRaisesRegex(ValueError, "source"):
                copy.validate_bindings(changed)

    def test_missing_compiler_binding_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "missing"):
            copy.validate_bindings({})


if __name__ == "__main__":
    unittest.main()
