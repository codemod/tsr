#!/usr/bin/env python3
"""Separate alias-body cache answers, guard refusals and actual body evaluations."""
from __future__ import annotations
import importlib.util
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    "alias_reference_states", Path(__file__).with_name("checker-reference-state-controls.py"))
states = importlib.util.module_from_spec(spec)
spec.loader.exec_module(states)
states.RUST_SOURCE = "3e2073fc90c1646b9c4284e9ab1c5739c3bfb99e"
pool = states.pool
ORIGINS = ("nonnullable", "ordinary", "other")
FIELDS = (
    "requests", "key_items", "cache_missing", "cache_error", "cache_value",
    "conditional_calls", "conditional_none", "conditional_error", "conditional_value",
    "body_calls", "body_error", "body_value", "published_error", "published_value",
    "returned_none", "returned_error", "returned_value", "reject_not_alias",
    "reject_no_declaration", "reject_not_alias_node", "reject_no_body",
    "reject_conditional", "reject_depth", "reject_arity", "reject_parameter",
)
NAMES = {f"alias_body_{origin}_{field}" for origin in ORIGINS for field in FIELDS}
pool.COUNTER_NAMES |= NAMES


def validate_counts(counts):
    states.validate_counts(counts)
    pool.require(NAMES <= counts.keys(), "alias body schema incomplete")
    for origin in ORIGINS:
        get = lambda field: counts[f"alias_body_{origin}_{field}"]
        pool.require(all(isinstance(get(f), int) and get(f) >= 0 for f in FIELDS), "invalid alias body count")
        rejected = sum(get(f) for f in FIELDS if f.startswith("reject_"))
        pool.require(get("requests") == get("cache_missing") + get("cache_error") + get("cache_value"),
                     "alias cache answer partition")
        pool.require(get("requests") == get("returned_none") + get("returned_error") + get("returned_value"),
                     "alias method return partition")
        pool.require(get("cache_missing") == get("conditional_calls"), "conditional delegate after cache miss")
        pool.require(get("conditional_calls") == get("conditional_none") + get("conditional_error") + get("conditional_value"),
                     "conditional delegate result partition")
        pool.require(get("conditional_none") == rejected + get("body_calls"), "first guard rejection or body entry")
        pool.require(get("body_calls") == get("body_error") + get("body_value"), "body result partition")
        pool.require(get("published_error") == rejected + get("body_error") + get("conditional_error"),
                     "error publication provenance")
        pool.require(get("published_value") == get("body_value") + get("conditional_value"),
                     "value publication provenance")
        pool.require(get("cache_missing") == get("published_error") + get("published_value"),
                     "cache miss/publication reconciliation")
        pool.require(get("returned_none") == get("cache_error") + rejected + get("body_error"),
                     "None provenance includes cached refusal")
        pool.require(get("returned_error") == get("conditional_error"), "error return provenance")
        pool.require(get("returned_value") == get("cache_value") + get("body_value") + get("conditional_value"),
                     "value return provenance")
        if origin != "other":
            outer = "nonnullable_body" if origin == "nonnullable" else "alias_body"
            pool.require(get("requests") == counts[f"state_worker_{outer}_calls"], "direct delegate admission")
            for inner, field in (("none", "refused"), ("error", "returned_error"), ("value", "returned_value")):
                pool.require(get("returned_" + inner) == counts[f"state_worker_{outer}_{field}"],
                             "direct inner/outer delegate return")


def inspect_trace(path, child, counters, cwd):
    trace = states.inspect_pool(path, child, counters, cwd, schema="mapper-alias-body-state-v1")
    if counters:
        validate_counts(trace["counters"])
        for owner in trace["owners"]:
            validate_counts(owner["counters"])
    return trace


pool.inspect_trace = inspect_trace


def module_schema():
    text = Path(__file__).with_name("checker-alias-body-state-probe.rs").read_text()
    for name, expected in (("ORIGINS", ORIGINS), ("FIELDS", FIELDS)):
        section = text.split(f"pub const {name}:", 1)[1].split("=", 1)[1].split(";", 1)[0]
        pool.require(tuple(re.findall(r'"([a-z_]+)"', section)) == expected, "alias module/reader schema")


if __name__ == "__main__":
    module_schema()
    states.main()
