#!/usr/bin/env python3
"""Extend actual-pool receipts with original mapper-key hash/equality accounting."""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location(
    "hash_mapper_pool", Path(__file__).with_name("checker-mapper-pool-controls.py"))
pool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pool)
DOMAINS = ("object", "signature", "reference", "predicate")
OPERATIONS = ("lookup", "insert", "unscoped")
FIELDS = (
    "operations", "hash_invocations", "hash_vector_items", "hash_identity_fields",
    "hash_delegate_ns", "eq_key_comparisons", "eq_true", "eq_false", "eq_delegate_ns",
    "map_ns", "max_vector_items", "eq_vector_items_supplied",
)
RUST_SOURCE = "1ccf5337b91d248784c51a95043c92ec86acf8cc"
NATIVE_SOURCE = "5b1047d10d32e7d5b446be4de56b126ff42f82bb"
NATIVE_BINARY_SHA = "b3cd1909b5dbc6582681e0a9d2641c7eefd9b7b6c3401cfad82b5e8b935b8033"
pool.COUNTER_NAMES |= {f"table_{domain}_{operation}_{field}"
                       for domain in DOMAINS for operation in OPERATIONS for field in FIELDS}
inspect_pool = pool.inspect_trace


def validate_counts(counts):
    for domain in DOMAINS:
        factor = 2 if domain in ("object", "signature") else 1
        for operation in OPERATIONS:
            prefix = f"table_{domain}_{operation}_"
            get = lambda field: counts[prefix + field]
            pool.require(get("hash_identity_fields") == get("hash_invocations") +
                         factor * get("hash_vector_items"), "hash identity-field reconciliation")
            pool.require(get("eq_key_comparisons") == get("eq_true") + get("eq_false"),
                         "key equality outcome reconciliation")
            pool.require(get("max_vector_items") <= get("hash_vector_items"),
                         "hash maximum exceeds supplied vector items")
            if get("hash_invocations") == 0:
                pool.require(get("hash_delegate_ns") == get("hash_vector_items") ==
                             get("max_vector_items") == 0, "hash cost without invocation")
            if get("eq_key_comparisons") == 0:
                pool.require(get("eq_delegate_ns") == get("eq_vector_items_supplied") == 0,
                             "equality cost without comparison")
            if operation == "unscoped":
                pool.require(get("operations") == get("map_ns") == 0, "unscoped map operation")
            else:
                pool.require(get("map_ns") >= get("hash_delegate_ns") + get("eq_delegate_ns"),
                             "delegates outside observed map interval")
                if get("operations") == 0:
                    pool.require(get("hash_invocations") == get("eq_key_comparisons") ==
                                 get("map_ns") == 0, "callbacks without map operation")
                if operation == "lookup":
                    # Empty tables may return before hashing. Equality can test
                    # multiple colliding keys, so it is not bounded by requests.
                    pool.require(get("hash_invocations") <= get("operations"),
                                 "multiple query hashes in one lookup")
                else:
                    # Table growth can rehash stored keys inside the same insert.
                    pool.require(get("hash_invocations") >= get("operations"),
                                 "insertion without its original key hash")


def inspect_trace(path, child, counters, cwd):
    trace = inspect_pool(path, child, counters, cwd, schema="mapper-pool-hash-v1",
                         max_suffixes=("_max_key_items", "_max_vector_items"))
    if counters:
        validate_counts(trace["counters"])
        for owner in trace["owners"]:
            validate_counts(owner["counters"])
    return trace


pool.inspect_trace = inspect_trace


def validate_bindings(bindings):
    for role, source in (("normal", RUST_SOURCE), ("probe", RUST_SOURCE), ("native", NATIVE_SOURCE)):
        pool.require(isinstance(bindings.get(role), dict), "missing compiler binding")
        pool.require(bindings[role].get("source") == source, "source outside qualified archive")
    pool.require(bindings["native"].get("sha256") == NATIVE_BINARY_SHA,
                 "native binary outside qualified worker-activity receipt")


def main():
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--bindings", type=Path)
    args, _ = parser.parse_known_args()
    if args.bindings is not None:
        validate_bindings(json.loads(args.bindings.read_text()))
    pool.main()


if __name__ == "__main__":
    main()
