#!/usr/bin/env python3
"""Validate reference states in real pool receipts; never infer member readiness."""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    "reference_state_pool", Path(__file__).with_name("checker-mapper-pool-controls.py"))
pool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pool)
inspect_pool = pool.inspect_trace
RUST_SOURCE = "96520fe712cdbd3ef76f8a73b7778a285aaa33bd"
NATIVE_SOURCE = "5b1047d10d32e7d5b446be4de56b126ff42f82bb"
NATIVE_BINARY_SHA = "b3cd1909b5dbc6582681e0a9d2641c7eefd9b7b6c3401cfad82b5e8b935b8033"
SITES = (
    "empty_lookup", "empty_insert", "deferred_lookup", "deferred_insert",
    "string_mapping_insert", "identity_mapping_insert", "ordinary_lookup",
    "keyword_insert", "conditional_insert", "alias_body_insert", "template_insert",
    "normalized_mapping_insert", "named_insert", "mapped_sequence_insert",
    "nonnullable_lookup", "nonnullable_insert", "index_original_lookup",
    "index_body_lookup", "member_body_lookup",
)
FIELDS = ("calls", "key_items", "misses", "errors", "values", "reserved_answers",
          "framed_answers", "new_entries", "replacement_entries", "replaced_errors")
REQUESTS = ("ordinary", "nonnullable", "empty", "deferred")
REQUEST_FIELDS = ("requests", "reentries", "returned_error", "returned_value", "unreturned",
                  "reservations", "reservation_closed", "cache_hit_returns", "other_returns")
WORKERS = ("string_mapping", "identity_mapping", "conditional", "alias_body", "template",
           "normalized_mapping", "mapped_sequence", "nonnullable_body")
WORKER_FIELDS = ("calls", "refused", "returned_error", "returned_value")
STATE_NAMES = {
    f"{prefix}_{site}_{field}"
    for prefix, sites, fields in (("state_site", SITES, FIELDS),
                                 ("state_request", REQUESTS, REQUEST_FIELDS),
                                 ("state_worker", WORKERS, WORKER_FIELDS))
    for site in sites for field in fields
}
pool.COUNTER_NAMES |= STATE_NAMES


def validate_counts(counts):
    pool.require(STATE_NAMES <= counts.keys(), "reference state schema incomplete")
    pool.require(all(isinstance(counts[k], int) and counts[k] >= 0 for k in STATE_NAMES),
                 "invalid state count")
    def group(prefix, site):
        return lambda field: counts[f"{prefix}_{site}_{field}"]
    for site in SITES:
        get = group("state_site", site)
        pool.require(get("calls") == get("misses") + get("errors") + get("values"),
                     "reference answer partition")
        pool.require(get("reserved_answers") <= get("framed_answers") <= get("errors") + get("values"),
                     "reservation/frame is not an answered lookup")
        if site.endswith("lookup"):
            pool.require(get("new_entries") == get("replacement_entries") == get("replaced_errors") == 0,
                         "lookup reported publication")
        else:
            pool.require(get("misses") == get("reserved_answers") == get("framed_answers") == 0,
                         "publication reported lookup state")
            pool.require(get("calls") == get("new_entries") + get("replacement_entries"),
                         "publication replacement partition")
            pool.require(get("replaced_errors") <= get("replacement_entries"), "replaced error without entry")
    for kind in REQUESTS:
        get = group("state_request", kind)
        pool.require(get("requests") == get("returned_error") + get("returned_value") + get("unreturned"),
                     "request return partition")
        pool.require(get("unreturned") == 0, "unfinished reference request")
        pool.require(get("requests") == get("cache_hit_returns") + get("other_returns"),
                     "request origin partition")
        pool.require(get("reentries") <= get("requests"), "reentry without request")
        pool.require(get("reservations") == get("reservation_closed") <= get("requests"),
                     "unclosed reservation")
        if kind != "ordinary":
            pool.require(get("reservations") == 0, "reservation outside named route")
        lookup = group("state_site", kind + "_lookup")
        pool.require(get("cache_hit_returns") == lookup("errors") + lookup("values"),
                     "cache answer/return reconciliation")
        if kind in ("empty", "deferred"):
            pool.require(get("requests") == lookup("calls"), "admitted request/lookup reconciliation")
        else:
            pool.require(get("requests") >= lookup("calls"), "lookup outside request")
    pool.require(counts["state_request_ordinary_reservations"] == counts["state_site_named_insert_calls"],
                 "named publication/reservation reconciliation")
    pool.require(counts["state_site_ordinary_lookup_calls"] == counts["reference_lookup_requests"],
                 "old/new selected lookup reconciliation")
    for kind in WORKERS:
        get = group("state_worker", kind)
        pool.require(get("calls") == get("refused") + get("returned_error") + get("returned_value"),
                     "worker result partition")
    pool.require(counts["state_worker_nonnullable_body_calls"] == counts["state_site_nonnullable_lookup_misses"],
                 "nonnullable miss/worker reconciliation")


def inspect_trace(path, child, counters, cwd):
    trace = inspect_pool(path, child, counters, cwd, schema="mapper-reference-state-v1")
    if counters:
        validate_counts(trace["counters"])
        for owner in trace["owners"]:
            validate_counts(owner["counters"])
    return trace


pool.inspect_trace = inspect_trace


def module_schema():
    text = Path(__file__).with_name("checker-reference-state-probe.rs").read_text()
    for name, expected in (("SITES", SITES), ("FIELDS", FIELDS), ("REQUESTS", REQUESTS),
                           ("REQUEST_FIELDS", REQUEST_FIELDS), ("WORKERS", WORKERS),
                           ("WORKER_FIELDS", WORKER_FIELDS)):
        section = text.split(f"pub const {name}:", 1)[1].split("=", 1)[1].split(";", 1)[0]
        actual = tuple(re.findall(r'"([a-z_]+)"', section))
        pool.require(actual == expected, "module/reader state schema differs")


def main():
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--bindings", type=Path)
    args, _ = parser.parse_known_args()
    if args.bindings is not None:
        bindings = json.loads(args.bindings.read_text())
        for role, source in (("normal", RUST_SOURCE), ("probe", RUST_SOURCE), ("native", NATIVE_SOURCE)):
            pool.require(bindings.get(role, {}).get("source") == source, "source outside qualified archive")
        pool.require(bindings["native"].get("sha256") == NATIVE_BINARY_SHA, "unqualified native binary")
    module_schema()
    pool.main()


if __name__ == "__main__":
    main()
