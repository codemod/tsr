# Concrete member-query observation

This is a locating measurement for `tsr-1yb.11` and the concrete builder
handoff `tsr-1yb.4.2`. It measures repeated name answers, structured walks and
returned string payloads. It establishes no CPU saving or native cache hits.
The [member-cache contract](checker-member-cache-contract.md) defines the
semantic prerequisite for reuse.

## Source and workload

The temporary opt-in probe was applied to TSR
`074f60a720667ad0cb0215072a5945f88ccc5c84`. The first counter binary SHA256 was
`573f8952776bbcb5ebbc987db606ef02c1ce741bef50d8545e265991af9dd9d0`.
The repeat added type labels and used binary SHA256
`5a035936a4a8d6c74f373dc8e4e3330f22118698907f72677280c8b6969d9f40`.
Both produced identical aggregate and path counts on the local Next.js app.

Each process used `--project tsconfig.json --noEmit --incremental false
--composite false --pretty false --extendedDiagnostics` from the app directory.
The baseline, disabled probe and enabled probe retained all 123 normalized
diagnostics, 13,097 loaded files, 1,341 checked files and 13,560 parsed files.
The complete diagnostic fingerprint was
`cdb777a1930fee9c86ab978511539234e820c77a69a4586d522409488e074074`.
Content hashed using the prior loaded-file inventory remained unchanged:
`395a32b083501c1cac01a17c6585385849f997fc10a3faab37b2f3d2d7bd9b16`.
These controls verify output and count preservation; they do not newly verify
every checked-file identity or establish workload equivalence with native.

The probe observed one CLI checker's private TypeIds and never reused an answer.
It retained the previous result to compare ordered name lists. A repeated
successful observation required the same path and no detected guard on either
the previous or current query. Guard flags propagated to containing queries;
walk counts stayed with the immediate query to avoid double counting.

## Observed work

| Metric | Count |
|---|---:|
| Distinct queried TypeIds | 13,959 |
| Name queries | 404,793 |
| Queries returning names (`Some`) | 227,419 |
| Unsupported queries (`None`) | 177,374 |
| Identical successful repeats outside detected guards | 216,884 |
| Structured owner walks | 232,788 |
| Walks during those repeats | 131,729 |
| Member table entries examined | 3,569,558 |
| Returned names | 2,042,495 |
| Returned string payload bytes | 16,063,739 |
| Payload bytes returned during those repeats | 15,821,033 |

The named-owner path made 125,915 queries, including 25,104 guarded queries
and 96,445 identical successful repeats outside guards. Export-table queries
contributed 87,203 repeats and authoritative overlays another 33,203; those
paths performed no structured owner walks. Static and mapped-literal-key paths
contributed 32 and one repeats respectively.

The largest named repeat source was the **String wrapper**, with 19,838
queries, 19,837 repeats, 39,674 repeat walks and 7,557,897 repeat payload bytes.
Its label was inspected directly; it is not an Array attribution. TypeId 75
identified it only within this checker and cannot serve as a cross-worker key.

There were 25,101 failed-base events and three visited-guard prunes. A prune
can arise from a shared diamond base, so it is not necessarily a cycle.
No active name-query or late-bound resolver re-entry was observed in these
app runs. This absence does not make provisional answers safe to cache.
The 3,028 alias value-forcing queries are calls, not expensive cache misses.

## Controls and limits

Before/probe complete diagnostics matched on repeated generic, mapped and
conditional, Flatten/MCP, diamond, computed-member, cyclic, unfollowable-base
and cross-file generic fixtures. The mapped/conditional fixture was clean and
recorded 120 structured walks during identical repeats. Generic, Flatten/MCP
and cross-file fixtures emitted existing TSR errors; output equality on those
fixtures does not establish native-positive generic or recursive behavior.
Known imported-generic and augmentation gaps remain `tsr-6.48` and `tsr-6.49`.

Equal names can still describe different member types, mapper contexts,
receivers or publication states. The successful-result count therefore does
not count native-completed member tables. Unsupported and guarded observations
cannot be treated as eligible reuse. Returned payload bytes exclude vector
capacity, allocator metadata and the probe's own retained copies. A name cache
that returns deep-cloned `Vec<String>` values can retain this copy cost.

Raw observations are in `/tmp/tsr-member-query-nextjs.json`,
`/tmp/tsr-member-query-repeat.json` and
`/tmp/tsr-member-query-controls/results.json`. The diagnostic-only patch and
source hashes were archived under
`.context/compound-engineering/ce-optimize/tsr-whole-project/member-query-probe/`;
patch replay was checked against the pinned source before restoring the exact
original production files. The instrumentation is not installed in the CLI.

The next experiment needs current exclusive CPU/allocation attribution and
actual expensive execution counts. Traversal reuse belongs to the concrete
member builder; immutable name projections have a separate measured task,
`tsr-1yb.7.5`, after that builder contract. Broad mapper and relation audits
remain unfinished. The verified whole-project TSR/tsgo wall target remains
0.50; this observation supplies no speed ratio.
