# Parity lane `r4-perf` — checker memo tables (round 4)

Lane issue `tsr-2zk.910` (parent `tsr-2zk.17`). Box protocol:
`docs/parity/box-protocol.md`. Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Release target (CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on
equivalent complete work. Round 3's attribution and candidate list:
[`perf-r3.md`](perf-r3.md) §13 (C7–C9 there are this lane's brief).

Sections are numbered so code can cite them (`r4-perf.md` §N). Every number
names the source commit and measurement it came from.

## §1 Method

- Source base `c02dbbd` (integration branch, C1/C2/C5 of round 3 already
  landed). 4-vCPU cloud container, Linux 6.18.
- **Ir**: `valgrind --tool=callgrind` total instructions of the release
  `tsr` binary on `python3 scripts/generate_perf_project.py --modules 100`,
  `--singleThreaded --pretty false`. Deterministic; every variant's complete
  CLI output is compared with the base's.
- **Wall/CPU**: `scripts/whole_project_perf.py` against the previous binary
  (self-comparison, box protocol §5) and against native tsgo built from the
  pinned submodule (`scripts/offline-cargo/build-tsgo.sh`).

## §2 C7 — interface/type-literal signature lists (`PerfLinks::interface_signatures`)

**Forcing measurement.** At `c02dbbd`,
`signature_candidates_of_interface_symbol` was 11.05% of inclusive Ir
(`signature_candidates_of_named_type` above it 12.17%): 246,353 requests on
the 100-module project, each rebuilding the declared list, re-resolving every
heritage entry (`heritage_entity_symbol` → `resolve_name`), re-instantiating
each base (`instantiated_heritage_base`) and re-mapping the bases' signatures.

**Native operation.** `resolveObjectTypeMembers` (`checker.go:19106`) fills
`callSignatures`/`constructSignatures` on the declared interface type once
(`resolveStructuredTypeMembers`), declared members first, then each base's
`getSignaturesOfType` (`:18959`) result; the receiver mapper is applied after,
per instantiated reference. Consumer here:
`signature_candidates_of_named_type`, which applies the receiver mapper to the
memoised list, as before.

**Identity and owner.** Key: the *merged* interface or type-literal
`SymbolId` and the `SignatureKind` (three slots: call, construct, abstract
construct). Value: the list before the receiver's mapper. Private to the
owning `Checker` (`Checker::perf_links`), Program lifetime. No option is read
that can differ within one checker.

**Publication.** Absent = uncomputed. Published only when all hold:
(1) the worker answered `Some` (a `None` is a heritage cycle, a gap base, an
unresolved base type or an unbuilt declaration — recomputed every time, as
before); (2) no resolution is on the stack and no flow loop is active
(`signature_links_publishable`; a circular read under an active resolution
answers `errorType`, which would be frozen); (3) no member's return is
`errorType` (`signatures_decided`; a lazy return's pending sentinel reads as
error and its completion rewrites `signature_types`, not this table).
A published list is **read in any context** the admission below allows,
including under an active resolution: native reads resolved members wherever
they are requested, so reading the final answer there is closer to native
than recomputing a provisional one.

**The `visiting` stack.** The worker's cycle guard depends on the caller's
`visiting` list, so a recursive call is not obviously the same question as a
top-level one. It is, for every published answer: a list is published only
when the symbol's own heritage walk met no cycle, and every symbol already on
the stack has the requested symbol among its ancestors, so if one of them
recurred in the requested symbol's walk there would be a cycle through the
requested symbol itself. A non-empty `visiting` can therefore only turn a
published `Some` into the same `Some`, and the memo is read and written at
every depth (the brief said top-level only; this argument is why the lane
widened it).

**Consumer context (the alias-evaluation frames).** The first version gated on
"no alias-evaluation frame open", as round 3's C6 did. Probe on the 100-module
project: of 246,353 requests, 179,453 were refused by that gate (166,004 for
an open alias frame, 96,445 under an active resolution, overlapping), and Ir
fell only 3.2% (4,051.6 M → 3,921.4 M). An alias frame binds type-parameter
symbols (an alias's own parameters, `infer` declarations, a distributed check
parameter) to arguments; the worker can read a bound parameter only from a
declaration inside that parameter's scope. `interface_signature_frames`
therefore admits a request when no bound parameter's scope owner (its
declaring node; for `infer`, the enclosing conditional type; JSDoc
`@template` is never admitted) is an ancestor-or-self of one of the symbol's
declarations, and then **takes** the frames for the computation, the idiom
`get_resolved_type_parameter_default` already uses for target-owned work
(`declared.rs`). Bases need no check: a heritage name resolves lexically, and
nothing declared inside a type-parameter scope (a function body, an alias
body, a conditional type) can be named from outside it. Print mode
(`identity_unmapped_type_parameters`) and a mapped-template frame are never
admitted (`docs/architecture/mapper-mode.md`). A type literal written inside
a generic alias (`type F<T> = { (x: T): T }`) is exactly the case the
ancestor test refuses, and it keeps the old frame-sensitive computation.

**Work boundary.** The worker is the full heritage walk; the hit cost is one
hash lookup plus a `Vec<Signature>` clone (native returns a shared slice; the
clone remains because every consumer takes ownership).

**Measured** (§1 method):

| | Ir | Δ |
|---|---:|---:|
| base `c02dbbd` | 4,051,562,646 | — |
| C7, C6-style gate (not shipped) | 3,921,358,215 | −3.2% |
| C7 as shipped | 3,664,043,246 | **−9.56%** |

Probe counts with C7 as shipped (temporary counters, not committed): 185,963
requests (the recursion now stops at the first published base), 4 refused
admission, 182,842 completed hits, 2,657 published computations, 460
computations not published (under an active resolution or flow loop), none
refused for an `errorType` return.

**Correction.** An intermediate build read 3,604,691,762 (−11.0%). It returned
cached lists without restoring the caller's alias-evaluation frames, so the
rest of the caller's evaluation ran frameless; that number measured a bug and
is not C7's. Its 100-module output happened to be identical, which is why the
corpus run, not the bench output, is the gate.

**Corpus.** Both unfiltered dumps (`diagverdictdump`, 10,570 rows;
`verdictdump`, 477,985 rows) are `cmp`-identical to the `c02dbbd` baseline, so
both §5 loss checks are empty. `cargo test --workspace --release
--no-fail-fast`: the same two failures as the base
(`iteration::optional_tuple_check_types_preserve_named_enum_identity_and_reads`,
`mapped_tuple_inference::tuple_slice_optional_arguments_follow_null_and_exact_optional_options`),
none new.

**Wall and CPU** (default scheduling, 21 samples, medians; `self` is C7
against the `c02dbbd` binary, `tsgo` against native):

| Project | self wall | self CPU | vs tsgo wall | vs tsgo CPU |
|---|---:|---:|---:|---:|
| generic-imports | 86.7 / 86.4 ms (1.003) | 0.994 | 88.0 / 85.7 ms (1.028) | 0.555 |
| domain-model | 186.5 / 188.7 ms (0.988) | 0.954 | 194.4 / 238.4 ms (0.816) | 0.529 |
| domain-model-large | 718.7 / 736.9 ms (0.975) | 0.953 | 714.0 / 822.2 ms (**0.868**) | 0.593 |

`--singleThreaded` domain-model-large, 11 samples: 1,611.7 vs 1,737.1 ms wall
(0.928), CPU 0.935. generic-imports checks three files and does not reach
this code; its row is the noise floor.

**How we would know it is wrong.** A §5 loss under the patch, or a case where
a type literal or interface declared inside a function or alias prints
different signatures depending on whether it was first requested inside an
alias evaluation. The ancestor test is the piece to suspect first.
