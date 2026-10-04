# Mapper-key allocation attribution

The measured key constructors are a small part of this application's remaining
cost. Keep the existing production representation. Settle native mapper and
completion contracts before widening reuse, and pursue a larger demonstrated
cost next. This report supplies locating evidence for `tsr-1yb.16.1`; it does
not complete that task, ship a cache, or prove the TSR/tsgo wall ratio of 0.50.

The [counts](checker-mapper-key-counts.json),
[temporary probe patch](checker-mapper-key-probe.patch), and
[public control runner](checker-mapper-key-controls.py) preserve the evidence.
The patch is diagnostic tooling and must not be applied to production.

## Source and measurement boundary

The measurement source is `bc27fae238aa21ae338abb79c67dd1fedd32380c`.
Normal CLI SHA256 is
`7f41b6f6257c51cbfb3c5ea6c5f634237448c02fca37c6469b83712cabb3c04d`;
expanded probe SHA256 is
`1f04c3da238d6969f6752dd3f1da3b6c780c83e5d585cb149297087c2706682c`.
The JSON records original source-file hashes and the patch hash. Replaying the
patch on those exact source bytes passes. All production source was restored.
Subsequent incoming changes are not included in these app counts.

The real app runs use `--noEmit --incremental false --composite false
--pretty false`, an explicit project, and pinned bundled libs. Normal,
disabled-probe, enabled-probe, and repeated enabled-probe processes produce
the same complete normalized 118 diagnostics. Both enabled runs record the
same 1,341 actual checked identities, order, and checked-input hash. Non-timing
counters repeat exactly. Raw private application diagnostics and file names
are retained locally and excluded from the exported evidence.

Checked identities are emitted only with counters enabled. This does not prove
off/on checked-scope equality independently of output. Complete effective
configuration hashes were not captured by this probe. These are remaining
measurement gaps, not waived equivalent-work gates. The fixed public controls
record their source/configuration hashes and verify one actual checked root
per enabled run.

The temporary allocator delegates to `System`. Constant TLS tags surround
specific existing copies; successful allocation events record requested layout
and macOS `malloc_size` usable bytes. These are cumulative events, including
reallocations, not live heap or RSS. Usable bytes exclude zone metadata and
page fragmentation. The allocator observer is diagnostic-only unsafe tooling
under ADR-0011; no production allocator or unsafe code changes are retained.
Diagnostic active-worker frames copy keys outside allocation tags and use
checker addresses only during overlapping calls, without dereferencing or
retaining addresses afterward.

## App decomposition

| Observed path | Lookup requests | Nonactive value hits | Miss/worker blocks | Key allocations | Requested bytes | Usable bytes |
|---|---:|---:|---:|---:|---:|---:|
| Captured anonymous object | 11 | 4 | 7 | 11 | 88 | 176 |
| Baked signature | 115,765 | 102,491 | 13,274 | 115,765 | 1,767,184 | 2,136,240 |
| Mapped type | 10 | 6 | 4 | 10 | 80 | 160 |
| Type literal | 5,481 | 610 | 4,870 | 5,481 | 124,880 | 165,264 |
| Reference after early mapped branches | 351,696 | 322,124 | 29,571 | 351,696 | 2,701,612 | 5,628,992 |

The literal path additionally records one nonactive error hit. The reference
path records one active hit; its nonactive count includes all returned values
without a separate error classification. Signature workers publish 13,226
successful images and refuse 48; literal workers publish 4,868 successful
images and fail twice. A worker count is execution of the named source block,
not an assertion that it equals a native instantiation or completed members.

The five constructors request 4,593,844 bytes in 472,963 allocation events.
Their measured construction intervals total about 15.414 ms in one instrumented
run: reference 11.140 ms, signature 4.053 ms, literal 0.219 ms, and the other
two less than 0.002 ms together. Timers include observer overhead. They omit
later key destruction and do not separately isolate hashing from lookup.
This is a useful scale estimate, not a statistical upper bound on normal
saved wall. An eligible whole-project improvement has not been established.

The top-level containment predicate constructs 27,461 keys, with 22,839 hits
and 4,622 worker executions. Recursive calls additionally construct 1,040
unused keys: 7,992 requested / 16,640 usable bytes. That unused construction
alone is too small to justify a throughput candidate in this workload.

Literal properties are cloned before eligibility/cache lookup. That boundary
performs 67,661 allocation events, requesting 3,488,477 bytes with 4,103,696
usable bytes. Both misses and hits contribute; this total cannot all be called
avoidable. Signature cloning at this particular prelookup boundary allocates
zero bytes in the app. Reserved literal/mapped publication adds 4,874 key
copies, 114,496 requested / 152,448 usable bytes.

Reference publication adds 29,430 argument copies, 230,544 requested bytes;
recording reverse reference targets adds 29,444 copies, 230,652 requested bytes.
The reference table ends with 29,651 entries, 57,344 bucket capacity, and
231,572 bytes of retained key-vector capacity. Other methods and early mapped
branches also populate it. The object/signature tables retain 114,552/229,008
key-vector capacity bytes. These exclude bucket storage, values, targets, and
other semantic tables; none is a total cache heap measurement.

## Public controls and native semantics

Fourteen fixed fixtures run as 70 serialized fresh processes: normal TSR,
disabled probe, two enabled probes, and pinned native. All TSR instrumentation
variants preserve complete diagnostics and repeat non-timing counts.
Twelve fixtures match native complete diagnostics. The two mismatches are
explicitly failed fidelity controls, not performance passes.

With 1 versus 32 identical signature-return calls, TSR executes one signature
worker in each fixture. Literal requests grow 1 to 32, but literal workers
remain one; native total instantiations remain five. With nested literal
returns, requests grow 2 to 33 while workers remain two; native instantiations
remain four. Copies before lookup still grow with requests. These observations
demonstrate reuse of these exercised blocks, without proving all native work
or mapper equivalence.

Distinct number/string substitutions, swapped arguments, reversed query
order, composed `List<Box<T>>` receivers, fresh shadowed generic parameters,
mapped properties, recursive `Link<T>` receivers, and simple contextual
inference retain their positive assignments and reject the negative assignment
with native complete diagnostics. The composed and recursive controls initially
missed the four mapper sites; the expanded reference probe observes 8/9
requests, 2/6 nonactive hits, and 6/3 miss blocks respectively. Instrumentation
coverage must follow the consuming path rather than inferred fixture names.

Pinned native is `5b1047d10d32e7d5b446be4de56b126ff42f82bb`;
clean binary SHA256 is
`6340b230f887095fb7c82fedb41eecadd7dc49e4baa65a05ef4abb8ab4b74bc4`.
It was built from the exact archived source described in the existing
[native publication audit](checker-symbol-completion-contract.md). The source
manifest hash is exported. The older `/tmp/tsr-95-tsgo` binary has a different
reported revision and modified-build metadata; it is not this oracle.

Native `checker.go:22104` keys an active mapper's local cache by type plus
alias, identifies the mapper by pointer, counts actual worker executions on
miss, and clears its cache on pop (`22146–22180`). Persistent object reuse
(`22304`) computes effective arguments through mapper composition, includes
the resulting alias and single-signature state, and owns instantiations on the
target object. `instantiateSignatureEx` (`20619`) creates fresh own parameters
and deliberately defers resolved return/predicate computation to avoid fixing
inferences early. TSR's eager baked-signature worker and ordered vectors
cannot be certified equivalent solely from these diagnostic controls.

TSR inventory at the measured revision:

| Path | Existing key and owner | Publication/lifetime boundary |
|---|---|---|
| `inference.rs:instantiate_anonymous_properties` | TypeId plus ordered parameter/image vector; checker object table | Successful image published after rebuilding |
| `inference.rs:instantiate_signature_type` | Same shape; checker signature table | Eager fresh/signature rebuilding; refuses before publication |
| `mapped.rs:instantiate_mapped_type` | Same shape; checker object table | Provisional error sentinel, then final overwrite |
| `declared.rs:instantiate_type_literal` | Same shape; shared checker object table | Reserved identity before recursion; final metadata or error |
| `declared.rs:create_type_reference_with_display` | SymbolId plus ordered arguments; checker reference table | Early branches bypass measured lookup; reference may precede metadata |
| `inference.rs:target_could_contain_parameter` | TypeId plus ordered searched parameters; checker predicate table | Only top-level lookup/publication; nested keys unused |

The probe fields called `completed_hits` mean only that the same exact key
was absent from its observed active-worker stack. They do not certify native
`MembersResolved`, alias completeness, fixing context, or metadata stability.
Other conditional/template/identity alias caches and member substitutions
remain outside this count inventory. `tsr-1yb.4.1.2` owns the wider native
identity/lifetime contract; `tsr-1yb.16.1` remains in progress.

## Fidelity follow-ups and resource limits

The two declaration symbols named `Token` remain distinct and the negative
assignment is rejected. TSR omits native's explanation that the private
`brand` properties have separate declarations. This extends existing
diagnostic-chain task `tsr-6.55`.

Recursive `Flatten<T>` over a readonly array conditional incorrectly accepts
an inferred nested string result assigned to number. Native reports TS2322;
TSR reports no diagnostic. This is `tsr-6.58`. Both failures reproduce with a
separately built normal CLI at current-main checkpoint
`437af60e805c58bc2c90e8e55b413eef4a9c8df9`; those source/binary observations
are separate from the earlier app measurements. The Flatten fixture fails
equivalent-work acceptance even if its time is short.

Probe resource controls run one warmup and five rotated samples per mode,
serially with builds and other owned profiling stopped. Normal/disabled/enabled
median wall is 4.586009/4.563475/4.634252 s. Enabled wall is about 48 ms above
normal; ranges overlap. This is observer overhead evidence, not an accepted
speed change. Peak RSS medians are 1,060,814,848 / 1,128,579,072 /
1,107,443,712 bytes, with overlapping ranges. RSS does not isolate key storage,
and independent maxima cannot establish worker memory. Every sample retains
the 118 complete diagnostics; enabled samples also retain checked/input hashes.

A 3-second normal-process CPU sample starting 0.2 s after launch records
2,323 samples: 1,208 in checker ancestry, 1,115 elsewhere. Disjoint nearest
TSR owners include file lookup 270, loader task 111, package-JSON lookup 83,
parameter traversal 81, and binder name resolution 62. This partial interval
mixes loading and checking; its counts are not full-project wall percentages.
It favors fresh attribution of filesystem/loading work over a new semantic
cache based on these small key-construction intervals.

Remaining measurement work is independent off/on checked-scope telemetry,
complete configuration identity, hit-only avoidable literal-copy accounting,
and wider conditional/composed/fixing-context coverage. A production candidate
still needs a measured normal-work benefit, native identity/publication
contract, complete corpus preservation, and confirmed whole-project samples.
