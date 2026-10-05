#!/usr/bin/env python3
"""Extend existing mapper-pool receipts with hit-only clone reconciliation."""
from __future__ import annotations

import importlib.util
import argparse
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location(
    "literal_mapper_pool", Path(__file__).with_name("checker-mapper-pool-controls.py"))
pool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pool)
FIELDS = (
    "calls", "property_images", "property_elements", "property_owned_strings",
    "property_string_len_bytes", "property_string_capacity_bytes", "property_vec_capacity_bytes",
    "property_allocations", "property_requested_bytes", "property_usable_bytes",
    "signature_images", "signature_elements", "signature_type_parameter_elements",
    "signature_value_parameter_elements", "signature_this_parameters", "signature_owned_strings",
    "signature_string_len_bytes", "signature_string_capacity_bytes", "signature_vec_capacity_bytes",
    "signature_arc_clones", "signature_allocations", "signature_requested_bytes",
    "signature_usable_bytes", "origin_refusals", "metadata_refusals",
)
BUCKETS = ("hit", "active", "error", "miss", "refusal")
RUST_SOURCE = "09b65ead8d72894b0b95369413abf0ae4b55fc94"
NATIVE_SOURCE = "5b1047d10d32e7d5b446be4de56b126ff42f82bb"
pool.COUNTER_NAMES |= {f"literal_copy_{bucket}_{field}" for bucket in BUCKETS for field in FIELDS}
inspect_pool = pool.inspect_trace


def validate_counts(counts):
    def get(bucket, field):
        return counts[f"literal_copy_{bucket}_{field}"]

    for bucket, base in (("hit", "nonactive_hits"), ("active", "active_hits"),
                         ("error", "nonactive_error_hits"), ("miss", "worker_starts")):
        pool.require(get(bucket, "calls") == counts[f"literal_{base}"], "literal hit-copy classification")
    pool.require(get("refusal", "calls") == get("refusal", "origin_refusals") +
                 get("refusal", "metadata_refusals"), "unclassified prelookup refusal")
    for kind in ("property", "signature"):
        for field in ("allocations", "requested_bytes", "usable_bytes"):
            pool.require(sum(get(bucket, f"{kind}_{field}") for bucket in BUCKETS) ==
                         counts[f"literal_prelookup_{kind}_{field}"], "clone allocation reconciliation")
        for bucket in BUCKETS:
            pool.require(get(bucket, f"{kind}_images") <= get(bucket, "calls"), "duplicate image")
            pool.require(get(bucket, f"{kind}_string_len_bytes") <=
                         get(bucket, f"{kind}_string_capacity_bytes"), "string capacity below payload")
            pool.require(get(bucket, f"{kind}_string_len_bytes") <=
                         get(bucket, f"{kind}_requested_bytes"), "copied payload exceeds allocations")
    pool.require(all(get("refusal", field) == 0 for field in FIELDS
                     if field not in ("calls", "origin_refusals", "metadata_refusals")),
                 "refused before lookup but copied images")
    pool.require(all(get(bucket, field) == 0 for bucket in BUCKETS if bucket != "refusal"
                     for field in ("origin_refusals", "metadata_refusals")), "refusal outside refusal bucket")


def inspect_trace(path, child, counters, cwd):
    trace = inspect_pool(path, child, counters, cwd)
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


def main():
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--bindings", type=Path)
    args, _ = parser.parse_known_args()
    if args.bindings is not None:
        validate_bindings(json.loads(args.bindings.read_text()))
    # The existing driver checks executable SHA-256 and native receipt bindings.
    pool.main()


if __name__ == "__main__":
    main()
