"""Fail closed on lost hash/equality observations and incomplete pool receipts."""
import importlib.util
from pathlib import Path
import re
import unittest

SPEC = importlib.util.spec_from_file_location(
    "mapper_hash", Path(__file__).with_name("checker-mapper-hash-controls.py"))
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)
BASE_SPEC = importlib.util.spec_from_file_location(
    "base_hash_receipts", Path(__file__).with_name("test_checker_mapper_pool_controls.py"))
base = importlib.util.module_from_spec(BASE_SPEC)
BASE_SPEC.loader.exec_module(base)
base.pool = probe.pool


class ExtendedReceiptTests(base.ReceiptTests):
    def setUp(self):
        super().setUp()
        self.rows[0] = "schema\tmapper-pool-hash-v1"


class HashCounterTests(unittest.TestCase):
    def setUp(self):
        self.counts = dict.fromkeys(probe.pool.COUNTER_NAMES, 0)
        self.prefix = "table_object_lookup_"
        self.counts.update({self.prefix + k: v for k, v in dict(
            operations=2, hash_invocations=2, hash_vector_items=3, hash_identity_fields=8,
            hash_delegate_ns=10, eq_key_comparisons=3, eq_true=1, eq_false=2,
            eq_delegate_ns=12, map_ns=100, max_vector_items=2,
            eq_vector_items_supplied=9).items()})

    def test_valid_collisions_empty_tables_and_all_zero_counters(self):
        probe.validate_counts(self.counts)
        probe.validate_counts(dict.fromkeys(probe.pool.COUNTER_NAMES, 0))
        self.counts[self.prefix + "operations"] = 4  # Two empty-table queries need no hash.
        probe.validate_counts(self.counts)

    def test_omitted_hash_invocation(self):
        self.counts[self.prefix + "hash_invocations"] -= 1
        with self.assertRaisesRegex(ValueError, "identity-field"):
            probe.validate_counts(self.counts)

    def test_omitted_equality_comparison_or_outcome(self):
        for field in ("eq_key_comparisons", "eq_true", "eq_false"):
            changed = dict(self.counts)
            changed[self.prefix + field] -= 1
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "outcome"):
                probe.validate_counts(changed)

    def test_scalar_identity_factor_is_specific_to_the_actual_table_key(self):
        for domain, factor in (("reference", 1), ("predicate", 1), ("signature", 2)):
            changed = dict.fromkeys(probe.pool.COUNTER_NAMES, 0)
            p = f"table_{domain}_lookup_"
            changed.update({p + k: self.counts[self.prefix + k] for k in probe.FIELDS})
            changed[p + "hash_identity_fields"] = 2 + factor * 3
            probe.validate_counts(changed)
            changed[p + "hash_identity_fields"] += 1
            with self.assertRaises(ValueError):
                probe.validate_counts(changed)

    def test_delegate_times_cannot_escape_the_map_interval(self):
        self.counts[self.prefix + "map_ns"] = 21
        with self.assertRaisesRegex(ValueError, "interval"):
            probe.validate_counts(self.counts)

    def test_table_growth_rehashes_are_inserts_and_unscoped_callbacks_are_explicit(self):
        changed = dict.fromkeys(probe.pool.COUNTER_NAMES, 0)
        p = "table_reference_insert_"
        changed.update({p + k: v for k, v in dict(operations=1, hash_invocations=3,
            hash_vector_items=6, hash_identity_fields=9, max_vector_items=2,
            hash_delegate_ns=4, map_ns=10).items()})
        probe.validate_counts(changed)
        changed[p + "operations"] = 4
        with self.assertRaisesRegex(ValueError, "insertion"):
            probe.validate_counts(changed)
        changed = dict.fromkeys(probe.pool.COUNTER_NAMES, 0)
        p = "table_predicate_unscoped_"
        changed.update({p + k: v for k, v in dict(hash_invocations=1,
            hash_vector_items=2, hash_identity_fields=3, max_vector_items=2,
            hash_delegate_ns=4).items()})
        probe.validate_counts(changed)
        changed[p + "operations"] = 1
        with self.assertRaisesRegex(ValueError, "unscoped"):
            probe.validate_counts(changed)

    def test_wrong_source_or_native_binary_binding_rejected(self):
        bindings = {role: {"source": source} for role, source in (
            ("normal", probe.RUST_SOURCE), ("probe", probe.RUST_SOURCE), ("native", probe.NATIVE_SOURCE))}
        bindings["native"]["sha256"] = probe.NATIVE_BINARY_SHA
        probe.validate_bindings(bindings)
        for role in bindings:
            changed = {k: dict(v) for k, v in bindings.items()}
            changed[role]["source"] = "0" * 40
            with self.assertRaises(ValueError):
                probe.validate_bindings(changed)
        bindings["native"]["sha256"] = "0" * 64
        with self.assertRaises(ValueError):
            probe.validate_bindings(bindings)

    def test_archive_module_and_reader_have_identical_counter_schema(self):
        text = Path(__file__).with_name("checker-mapper-hash-probe.rs").read_text()
        for name, expected in (("DOMAINS", probe.DOMAINS), ("OPERATIONS", probe.OPERATIONS),
                               ("FIELDS", probe.FIELDS)):
            content = text.split(f"const {name}:", 1)[1].split("=", 1)[1].split(";", 1)[0]
            self.assertEqual(tuple(re.findall(r'"([a-z_]+)"', content)), expected)


if __name__ == "__main__":
    unittest.main()
