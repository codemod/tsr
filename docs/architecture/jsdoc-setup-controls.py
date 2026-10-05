#!/usr/bin/env python3
"""Validate archive-only JSDoc setup receipts; no speed-equivalence verdict."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "jsdoc_cost", Path(__file__).with_name("jsdoc-cost-controls.py")
)
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)

TIMER_NAMES = {"parse", "leading", "range", "body", "plain", "list", "attach",
               "nested_attach", "bind_copy", "set_jsdoc", "constructor", "typedef_scan"}
MEMORY_IDS = {0, 1, 2, 3, 4, 128, 129}


def read_probe(path, child, workers=1):
    cost.require(type(workers) is int and 1 <= workers <= 4, "unsupported worker domain")
    cost.require(type(child["pid"]) is int and child["pid"] > 0 and
                 type(child["started_at_unix_ns"]) is int and child["started_at_unix_ns"] > 0,
                 "invalid supervising child identity")
    cost.require(child["timed_out"] is False and type(child["exit_code"]) is int and
                 child["exit_code"] in (0, 1), "incomplete child")
    with cost.regular_file(path) as stream:
        data = stream.read()
    cost.require(data.endswith(b"\n"), "partial final setup record")
    rows = data.decode().splitlines()
    header = rows.pop(0).split("\t") if rows else []
    cost.require(len(header) == 3 and header[:2] == ["tsr-jsdoc-setup-v1", str(child["pid"])],
                 "setup PID/version mismatch")
    cost.require(int(header[2]) >= child["started_at_unix_ns"], "stale setup trace")
    cost.require(rows and rows.pop() == "complete", "missing setup completion")
    timers, memory, maps = {}, {}, {}
    declined = None
    for row in rows:
        fields = row.split("\t")
        values = [int(value) for value in fields[2:]]
        cost.require(all(value >= 0 for value in values), "negative setup value")
        if fields[0] == "timer":
            cost.require(len(fields) == 5 and fields[1] in TIMER_NAMES and fields[1] not in timers,
                         "duplicate or malformed timer")
            timers[fields[1]] = dict(zip(("calls", "ns", "arena_requested_bytes"), values))
        elif fields[0] == "memory":
            owner = int(fields[1])
            cost.require(len(fields) == 7 and owner in MEMORY_IDS and owner not in memory,
                         "duplicate or malformed owner")
            memory[owner] = dict(zip(("live", "peak", "requests", "padded_live", "cumulative_requested"), values))
            cost.require(values[0] == 0 and values[3] == 0, "tagged allocation leak after invocation")
            cost.require(values[1] <= values[4], "peak exceeds cumulative allocation requests")
        elif fields[0] == "maps":
            owner = int(fields[1])
            cost.require(len(fields) == 6 and owner not in maps and 1 <= owner <= workers,
                         "duplicate or unexpected private map owner")
            maps[owner] = dict(zip(("entries", "entry_capacity", "hosts", "host_capacity"), values))
            cost.require(values[1] >= values[0] and values[3] >= values[2], "impossible map capacity")
        elif fields[0] == "declined":
            cost.require(len(fields) == 2 and declined is None, "duplicate declined count")
            declined = int(fields[1])
            cost.require(declined >= 0, "negative declined count")
        else:
            raise ValueError("unknown setup record")
    cost.require(set(timers) == TIMER_NAMES and set(memory) == MEMORY_IDS and
                 set(maps) == set(range(1, workers + 1)) and declined is not None,
                 "incomplete setup domains")
    cost.require(timers["plain"]["calls"] <= timers["body"]["calls"] and
                 timers["plain"]["ns"] <= timers["body"]["ns"] and
                 timers["plain"]["arena_requested_bytes"] <= timers["body"]["arena_requested_bytes"],
                 "plain subset exceeds comment bodies")
    cost.require(sum(memory[i]["cumulative_requested"] for i in range(1, 5)) ==
                 memory[0]["cumulative_requested"], "private total accounting mismatch")
    cost.require(sum(memory[i]["requests"] for i in range(1, 5)) == memory[0]["requests"],
                 "private request event accounting mismatch")
    cost.require(memory[0]["peak"] <= sum(memory[i]["peak"] for i in range(1, 5)),
                 "simultaneous peak exceeds sum of owner peaks")
    parsed = cost.counts(child).get("Parsed files")
    if parsed is not None:
        cost.require(timers["parse"]["calls"] == parsed, "missing compiler parse boundary")
    return {"sha256": hashlib.sha256(data).hexdigest(), "timers": timers,
            "memory": {str(key): value for key, value in memory.items()},
            "maps": {str(key): value for key, value in maps.items()}, "declined": declined,
            "speed_qualified": False, "lazy_first_repeat_materialization": "not implemented; eager bodies only"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("child", type=Path)
    parser.add_argument("--workers", type=int, default=1)
    args = parser.parse_args()
    print(json.dumps(read_probe(args.trace, json.loads(args.child.read_text()), args.workers)))


if __name__ == "__main__":
    main()
