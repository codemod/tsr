#!/usr/bin/env python3
"""Qualify every actual reference-cache call and its original key construction."""
from __future__ import annotations

import importlib.util
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location(
    "reference_hash", Path(__file__).with_name("checker-mapper-hash-controls.py"))
hash_probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(hash_probe)
pool = hash_probe.pool
hash_probe.RUST_SOURCE = "67cae54a96113085b635b1d61b4f5c87bd15df68"
SITES = (
    ("empty_lookup", "empty", 1), ("empty_insert", "reused", 2),
    ("deferred_lookup", "slice_copy", 1), ("deferred_insert", "reused", 2),
    ("string_mapping_insert", "move", 2), ("identity_mapping_insert", "move", 2),
    ("ordinary_lookup", "vec_clone", 1), ("keyword_insert", "move", 2),
    ("conditional_insert", "move", 2), ("alias_body_insert", "move", 2),
    ("template_insert", "move", 2), ("normalized_mapping_insert", "move", 2),
    ("named_insert", "vec_clone", 2), ("mapped_sequence_insert", "move", 2),
    ("nonnullable_lookup", "singleton", 1), ("nonnullable_insert", "singleton", 2),
    ("index_original_lookup", "borrowed", 1), ("index_body_lookup", "borrowed", 1),
    ("member_body_lookup", "borrowed", 1), ("deferred_target", "tuple_clone", 0),
    ("tuple_mapped_target", "slice_copy", 0), ("template_mapped_target", "move", 0),
    ("variadic_target", "move", 0), ("indexed_target", "vec_clone", 0),
    ("union_target", "vec_clone", 0), ("callable_target", "vec_clone", 0),
    ("named_target", "vec_clone", 0), ("nonnullable_target", "singleton", 0),
    ("unclassified", "unknown", 3),
)
FIELDS = (
    "constructions", "vector_items", "vector_capacity_items", "delegate_ns", "allocations",
    "requested_bytes", "usable_bytes", "lookup_operations", "insert_operations",
    "operation_vector_items", "nonempty_constructions",
)
TYPE_ID_BYTES = 4
pool.COUNTER_NAMES |= {f"reference_site_{name}_{field}" for name, _, _ in SITES for field in FIELDS}


def validate_counts(counts):
    hash_probe.validate_counts(counts)
    for name, kind, operation in SITES:
        get = lambda field: counts[f"reference_site_{name}_{field}"]
        pool.require(get("vector_capacity_items") >= get("vector_items"), "constructor capacity")
        pool.require(get("constructions") >= get("nonempty_constructions"), "constructor count omitted")
        pool.require(get("vector_items") >= get("nonempty_constructions"), "constructor item count")
        pool.require(get("usable_bytes") >= get("requested_bytes"), "allocator usable/requested")
        if operation != 1 and operation != 3:
            pool.require(get("lookup_operations") == 0, "wrong reference lookup site")
        if operation != 2 and operation != 3:
            pool.require(get("insert_operations") == 0, "wrong reference insert site")
        if kind in ("borrowed", "reused", "unknown"):
            pool.require(all(get(field) == 0 for field in FIELDS[:7]), "borrowed/reused constructor")
        elif operation:
            expected = get("lookup_operations" if operation == 1 else "insert_operations")
            pool.require(get("constructions") == expected, "operation/constructor mismatch")
            pool.require(get("operation_vector_items") == get("vector_items"), "operation/constructor key shape")
        if kind in ("move", "empty"):
            pool.require(get("allocations") == get("requested_bytes") == get("usable_bytes") == 0,
                         "empty/moved key allocated")
        if kind in ("slice_copy", "vec_clone", "tuple_clone", "singleton"):
            pool.require(get("allocations") == get("nonempty_constructions"), "copy allocation count")
            pool.require(get("requested_bytes") == TYPE_ID_BYTES * get("vector_items"),
                         "copy requested-storage count")
            pool.require(get("vector_capacity_items") == get("vector_items"), "copy capacity")
        if kind == "singleton":
            pool.require(get("vector_items") == get("constructions") == get("nonempty_constructions"),
                         "singleton construction shape")
        if kind == "empty":
            pool.require(get("vector_items") == get("vector_capacity_items") == get("nonempty_constructions") == 0,
                         "empty construction shape")
        if get("constructions") == 0:
            pool.require(all(get(field) == 0 for field in FIELDS[:7]), "cost without construction")
        if operation == 0:
            pool.require(get("operation_vector_items") == 0, "target outside reference map")
        if kind == "unknown":
            pool.require(all(get(field) == 0 for field in FIELDS), "unclassified reference operation")
    for operation in ("lookup", "insert"):
        total = sum(counts[f"reference_site_{name}_{operation}_operations"] for name, _, _ in SITES)
        pool.require(total == counts[f"table_reference_{operation}_operations"],
                     "reference map/site operation reconciliation")
    pool.require(counts["reference_lookup_requests"] == counts["reference_site_ordinary_lookup_constructions"],
                 "selected producer request reconciliation")
    pool.require(counts["reference_key_items"] == counts["reference_site_ordinary_lookup_vector_items"],
                 "selected producer shape reconciliation")
    for tag, sites in (("reference_key", (0, 2, 6, 14)),
                       ("reference_publication", tuple(i for i in range(19) if SITES[i][2] == 2)),
                       ("reference_target", tuple(range(19, 28)))):
        for field in ("allocations", "requested_bytes", "usable_bytes"):
            total = sum(counts[f"reference_site_{SITES[i][0]}_{field}"] for i in sites)
            pool.require(total == counts[f"{tag}_{field}"], "allocation tag/site reconciliation")


def inspect_trace(path, child, counters, cwd):
    trace = hash_probe.inspect_pool(path, child, counters, cwd, schema="mapper-reference-key-v1",
                                   max_suffixes=("_max_key_items", "_max_vector_items"))
    if counters:
        validate_counts(trace["counters"])
        for owner in trace["owners"]:
            validate_counts(owner["counters"])
    return trace


pool.inspect_trace = inspect_trace


def module_schema():
    text = Path(__file__).with_name("checker-reference-key-probe.rs").read_text()
    sites = tuple((name, kind, int(op)) for name, kind, op in re.findall(
        r'\("([a-z_]+)", "([a-z_]+)", (\d)\)', text.split("pub const FIELDS:", 1)[0]))
    fields = tuple(re.findall(r'"([a-z_]+)"',
        text.split("pub const FIELDS:", 1)[1].split("=", 1)[1].split(";", 1)[0]))
    pool.require(sites == SITES and fields == FIELDS, "module/reader constructor schema")


if __name__ == "__main__":
    module_schema()
    hash_probe.main()
