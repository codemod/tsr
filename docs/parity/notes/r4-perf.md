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

## §3 C9 — heritage entity symbols and instantiated bases

**Forcing measurement.** After C7, `instantiated_heritage_base` was 2.43% and
`heritage_entity_symbol` 1.32% of inclusive Ir on the 100-module project, now
reached mostly from `generic_heritage_member` (29,913 calls of each per
member lookup through a generic base) rather than from the signature walk.

### `PerfLinks::heritage_entity_symbols`

**Native operation.** `resolveEntityName` (`checker.go:15772`) on a heritage
clause's expression, whose answer native keeps in the name's
`links.resolvedSymbol` (read again by `getTypeFromClassOrInterfaceReference`
and `resolveBaseTypesOfInterface`/`resolveBaseTypesOfClass` through
`getBaseTypes`, `:19167`). Consumers: `base_symbol_of_heritage_entry`
(members), base-type and heritage-conformance walks, `symbols.rs`' qualified
receiver.

**Identity and owner.** Key: the heritage expression's `NodeId` and the
requested `SymbolFlags` meaning (`TYPE` for the entry, `NAMESPACE` for each
qualifier). Value: the merged, alias-resolved `SymbolId`. Private `Checker`,
Program lifetime.

**Publication.** Only `Some`. The resolution is binder scope lookup plus
`resolve_alias`, whose own table (`alias_targets`) is final once `Resolved`;
an alias still `Resolving` answers `None` here (its chain stops at the alias
symbol, which carries no `TYPE`/`NAMESPACE` meaning), so a `Some` was computed
through finished aliases only and is final. `None` (unresolvable name, a
circular alias, the wrong meaning) is recomputed every time, as before.
No mapper, receiver or alias-evaluation context is read: binder lookups are
syntax-directed.

**Work boundary.** `BindResult::resolve_name` and the alias chain;
42,734 requests, 42,530 hits after C7.

### `PerfLinks::heritage_bases`

**Native operation.** `getTypeFromClassOrInterfaceReference` with
`fillMissingTypeArguments` for one heritage entry, computed once into the
derived type's `resolvedBaseTypes` by `resolveBaseTypesOfInterface`
(`:19498`) / `resolveBaseTypesOfClass`. Consumers: `generic_heritage_member`,
C7's signature walk, index-signature and base-type walks, heritage
conformance, the JSX component class lookup.

**Identity and owner.** Key: base `SymbolId`, the location `NodeId` (it
decides the JavaScript default rule), and the written argument slice named by
its first node and its length (one contiguous node list, so these identify
it). A slice whose first node has no id is never memoised. Value: the
reference `TypeId`. Private `Checker`, Program lifetime.

**Publication.** Same gates as C7: admitted by `memo_frames` over the base's
declarations plus the first argument (or the location), so an
alias-evaluation frame whose parameter scope encloses the reference keeps the
old frame-sensitive computation; mapped-template and print frames are never
admitted; the computation runs with the admitted frames taken; publication
needs no active resolution or flow loop and a non-error result. `None` (arity
mismatch, an argument that resolved to error) is recomputed.

**Work boundary.** Argument type resolution, default instantiation and
`create_type_reference`; 32,518 requests, 32,043 hits, 203 published
computations after C7.

### `memo_frames` cost

The admission test is not free. On the first C9 build, `memo_frames` was
0.92% self Ir plus 0.89% for copying each symbol's declaration list into a
`Vec` for it. The shipped form borrows the declarations from the binder (a
`&'a` reference, so no clone is needed while `self` is mutably borrowed) and
reuses one scratch buffer for the scope owners; the remaining 0.92% self is
the ancestor walk itself, run for the about two thirds of requests made under an
alias-evaluation frame. A cheaper containment test (a pre-order node-range
check) would remove most of it; it needs a node-range table this port does not
have, so it is not built.

**Measured** (§1 method, output identical to base on every row):

| | Ir | Δ vs previous | Δ vs `c02dbbd` |
|---|---:|---:|---:|
| C7 (`65847cb`) | 3,664,043,246 | — | −9.56% |
| + C9 heritage tables, cloning admission | 3,588,368,878 | −2.07% | −11.43% |
| + borrowed declarations, scratch buffer (shipped) | 3,530,032,122 | −1.63% | **−12.87%** |


**Corpus.** Both unfiltered dumps `cmp`-identical to the `c02dbbd` baseline;
both §5 loss checks empty. Tests: the same two pre-existing failures as §2,
none new.

**Wall and CPU** (default scheduling, 21 samples, medians; `self` is C9
against the C7 binary `65847cb`):

| Project | self wall | self CPU | vs tsgo wall | vs tsgo CPU |
|---|---:|---:|---:|---:|
| generic-imports | 0.997 | 0.985 | 84.9 / 79.0 ms (1.076) | 0.569 |
| domain-model | 1.002 | 1.004 | 177.9 / 229.0 ms (0.777) | 0.522 |
| domain-model-large | 1.021 | 1.002 | 716.6 / 839.6 ms (**0.854**) | 0.571 |

Default-mode self ratios are inside this box's noise (§1 of `perf-r3.md`:
A/A up to ±2.6% wall). `--singleThreaded` domain-model-large, where the
checker is the whole run: against C7, 15 samples, wall 0.982, CPU 0.990;
against `c02dbbd`, 11 samples, 1,549.8 vs 1,690.7 ms (**0.917**), CPU 0.917.

**How we would know it is wrong.** A §5 loss; or a heritage reference inside
a generic function whose base type differs between a first request inside an
alias evaluation and one outside it (the admission test is the suspect).

## §4 C8 — `couldContainTypeVariables` in front of `mentions_type_parameter`: refused this round

**Measured ceiling.** After C7+C9, every `mentions_type_parameter` walk
together is 1.66% of inclusive Ir (`mentions_type_parameter_inner` via
`mentions_type_parameter`), plus 0.23% for `access_member_lookup`'s this-type
walk (round 3's C5 already cut the latter from 11.1%). A perfect negative
filter cannot save more than ~1.9%.

**Why it is not a cheap exact filter here.** Native's
`couldContainTypeVariables` (`checker.go:22184`) is *shallow*: it reads the
type's flags, a reference's node and type arguments, an anonymous type's
symbol flags and a union's members, and caches the bit in `objectFlags` on
the type itself, whose identity and shape are fixed at creation. TSR's walk is
*deep* (signature parameters, property types, mapped and deferred side
tables), and two of its inputs are not final when first read:
`TypeStore::complete_object` rewrites a reserved id's data in place, and
`peek_property_type`/`peek_parameter_type` skip edges whose types are not yet
published. A cached "cannot contain" for such a type could later become wrong.
The walk also falls back to printing the type and testing names when `names`
is non-empty, so a "reaches no type parameter" bit alone is not exact for
those callers. Making it exact needs either an "edges final" bit on types, or
a two-bit answer (reaches a type parameter / reaches a print fallback) that is
only cached for types built complete. That is more machinery than a 1.9%
ceiling pays for while larger items remain (§5).

**What would change this:** `mentions_type_parameter` growing back above
~5% of Ir, or a type-store change that marks types whose edges are final.

## §5 Attribution after C7+C9 (inclusive Ir, 100-module project)

| TSR function (native operation) | Share | Note |
|---|---:|---|
| `evaluate_conditional_alias` | 15.5% | conditional alias bodies re-evaluated per reference |
| `get_type_of_symbol` | 11.0% | — |
| `get_property_names_of_type` / `collect_structured_property_names` | 10.8% / 9.9% | property-name lists rebuilt per query; native keeps resolved members |
| `check_type_alias_circularity` | 9.2% | — |
| `get_signature_from_declaration` (`getSignatureFromDeclaration`, `links.resolvedSignature`) | 8.7% | uncached |
| `instantiate_for_reference_with_this` (round 3's C3, not landed) | 7.5% | — |
| `compare_symbols` + the `(String, SymbolId)` sort | 4.3% + ~4% | main's `c71b2e3` already caches the sort key |
| `signature_candidates_of_named_type` | 2.4% | receiver mapper over the C7 list per query; native caches resolved members per instantiated reference |
| malloc/free | ~25% self | allocation count |

None of these is in this lane's owned functions. The next memo in the same
shape is `signature_candidates_of_named_type` per `(receiver TypeId, kind)`
under the C7 gates, then `get_signature_from_declaration` per declaration
(native `links.resolvedSignature`), which needs the lazy-return publication
states in `signatures.rs` respected.
