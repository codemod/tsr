# Lane notes: r5-declared3 (`declared.rs` follow-ons, third box)

Round-5 cloud lane on epic `tsr-2zk`. It succeeds r5-declared2
([`r5-declared2.md`](r5-declared2.md)) as the single owner of
`crates/tsr-checker/src/declared.rs`. Items, in order: `tsr-2zk.1066` (the
unbounded conditional-alias recursion behind recursiveConditionalCrash3, and
`type_literal_key`'s flattening), a re-measure of r5-declared2's `.1061`
decline now that instantiation expressions have landed, the two small diffs
other lanes left for `declared.rs`, and the `.1078` leftovers. Native anchors
are `vendor/typescript-go` @ `5b1047d` (`internal/checker/checker.go` unless
noted).

**Base.** Integration head `d91243e` (batch AC, which carries r5-declared2
through `1405aae` and its fill-missing diff `03278c4`). Frozen dumps:

| dump | RIGHT | EMPTY_RIGHT | WRONG | EMPTY_WRONG | GAP |
|---|---|---|---|---|---|
| `diagverdictdump` | 5399 | 5584 | 1192 | 63 | |
| `verdictdump` | 545161 | | 6467 | | 905 |

**Setup.** The same as r5-operators3 §4: PyPI is blocked, so a stdlib-only
stand-in for `tomlkit.parse`/`inline_table`/`dumps`, kept outside the
repository, assembled the vendored crates. Dumps ran alone
(`RAYON_NUM_THREADS=3`), never beside a build. One trap for later boxes:
`cargo build -p tsr -p tsr-conformance --examples` builds the examples only, not
the `tsr` binary. Build the binary in its own `cargo build -p tsr` before
copying it as the perf reference.

## 1. `tsr-2zk.1066` — bounding and sharing conditional-alias evaluation

### 1.0 What the case does

`recursiveConditionalCrash3` (native: no diagnostics, checked in well under a
second). At `d91243e` the release CLI (`--singleThreaded --noEmit --target
es2015`) was still running at 120 s. A counter on `evaluate_conditional_alias`
showed about 23,000 evaluations a second. The recursion depth kept reaching
the 100 guard (`depth=89..99`, with 85–95 open binding frames, ~270 bindings in
all). So the work is an exponential tree that the depth guard cuts at every
leaf, not one infinite chain. `KeysCanBeExpanded_`'s
`N extends Depth['length']` bound never stops the recursion here. The mapped
template's `T[K]` reads do not resolve (`mapped.rs`, below), so every level
re-enters with new arguments until the guard fires.

r5-harness §4 found two missing pieces: the bound and the per-call
flattening. A third one turned out to matter more than either: **the port
had no `root.instantiations`**. The same alias with the same arguments was
evaluated again at every reference.

### 1.1 The bound (checker.go:22111)

instantiateTypeWithAlias stops at `instantiationDepth == 100 ||
instantiationCount >= 5_000_000`, reports TS2589 at `currentNode`, and answers
errorType. `instantiate_type` (`inference.rs`) already carries both legs.
`evaluate_conditional_alias` had only the depth leg and did not count. It
now checks both legs before evaluating and counts one instantiation per
evaluation it actually runs (a memo hit is not counted, below).

- **Stated divergence: no TS2589.** As at `instantiate_type`'s guard, this
  road declines (`None`) and its callers fall back to the named reference or
  to `error`. Reporting TS2589 needs native's `currentNode`, which the port
  does not track. Native reports it in 8 corpus cases
  (`limitDeepInstantiations`, `recursiveConditionalCrash4`,
  `recursiveMappedTypes`, …); not attempted here.
- **Stated divergence: the count is not reset per statement.** Native resets
  `instantiationCount` in checkSourceElement (`:2246`), checkDeferredNode
  (`:2509`) and checkExpressionEx (`:7563`). The port resets it only in
  `check_expression` and `check_property_access_expression`. The
  per-statement reset belongs in `check.rs`'s `check_node` (a hub function),
  so it is reported to the integrator rather than made here. No corpus line
  moves on it today: the budget is never reached in the corpus run (§1.4).

### 1.2 getConditionalTypeInstantiation's `root.instantiations` (checker.go:22485)

Native instantiates a conditional root once per (outer type arguments, alias,
forConstraint) and caches the result on the root. The port's
`evaluate_conditional_alias(symbol, arguments, result_alias)` is that
operation for an alias whose body is the conditional, but it re-walked the
body under a fresh frame every time.

`conditional_alias_instantiations` (on `Checker`) is the cache. Recorded per
the [checker port convention](../../conventions.md#checker-ports-preserve-ownership-and-work-boundaries):

- **Native operation.** getConditionalTypeInstantiation (`:22485`), reached
  from getTypeAliasInstantiation through instantiateTypeWithAlias. Its
  consumer is every reference to the alias.
- **Identity and owner.** The key is (alias symbol, the ordered type
  arguments, the result alias, whether a mapped-template frame is open), and
  the value is the evaluated `TypeId`. It is private to one `Checker` and lives
  for the Program. Native's key is the root's outer type parameters mapped
  through the mapper. For an alias root those are the alias's own parameters,
  so the ordered argument list is the same domain. The mapped-template flag
  partitions the cache as it partitions `type_literal_key`. `forConstraint`
  is not part of this road (the constraint captures call the evaluator
  differently).
- **Publication.** Only a completed evaluation (`Some`) is stored. A decline
  (`None`, "not computable here") is not, because some declines are
  context-dependent (the depth and count guards, in particular), and a
  stored one would turn a deep-context refusal into a refusal everywhere.
  The entry is written when the evaluation returns. Re-entry during the
  evaluation finds no entry and recurses as before, under the depth guard.
- **Not gated on `publishable_since`.** The first draft published only when
  no open resolution had observed a frame (`perf_links.rs`). In this case,
  almost every evaluation after the first ~800,000 ran inside an open
  resolution (the alias declaration being checked), so nothing published and
  the case got slower (260 s). Native caches the instantiation whatever
  resolutions are open. The full run (§1.4) is the evidence that no
  provisional answer escapes.
- **Consumer context.** Print mode (`identity_unmapped_type_parameters`) is
  never admitted, as for every frame-sensitive memo
  (`docs/architecture/mapper-mode.md`). The evaluation's own frame is pushed
  on top of whatever frames are open. An alias body names only its own
  parameters, so the result cannot depend on the outer frames' bindings,
  except for identity: `type_literal_key` and the conditional mint captures
  record every open frame. A shared result therefore carries the identity
  of the first context that built it. That is native's sharing, and the
  full run is the check.
- **Work boundary.** The worker is `evaluate_conditional_node` under the
  alias's frame. Measured on the case: of ~2.5 M calls, ~520 k ran the worker
  with the memo, and ~2/3 of those declined (not stored).

### 1.3 `type_literal_key`'s flattening

`type_literal_key`, `get_resolved_type_parameter_default`'s key and the two
conditional mint captures (`conditional_inference_nodes`,
`mapped_conditionals`) each collected every open frame into a fresh
`FxHashMap`, then (for the keys) drained and sorted it. With ~270 bindings in
~90 frames, that was the hash-table insert and iteration r5-harness measured
(34% inclusive at `type_literal_key`, ~35% of self cost in `HashMap` code).

`flattened_alias_bindings` replaces all four with one exact equivalent: one
vector of every entry, innermost frame first, a stable sort by symbol and a
dedup keeping each symbol's first (innermost) entry. No hash table is built,
and no frame (or only empty ones) allocates nothing. The key is
**identity-preserving in the strict sense**: it is the same sorted list the
`FxHashMap` collect produced, so every cache keyed on it keeps exactly its
old equivalence classes (unit test
`flattened_alias_bindings_match_the_hash_map_flattening`).

**Refused for now: native's narrower key.** Native keys an instantiated
anonymous type on the outer type parameters the node *possibly references*
(getObjectTypeInstantiation, isTypeParameterPossiblyReferenced), not on every
open frame. That key is cheaper still (a lookup per referenced parameter,
usually found in the innermost frame) and more faithful, but it merges
type-literal identities this port keeps apart today, so it is a behaviour
change with its own measurement. It was not attempted in this session.
**What would change it:** a profile where the flattening still dominates
after §1.2's memo. In this case it does not (§1.4).

### 1.4 Measured

recursiveConditionalCrash3, release CLI, same flags: at least 120 s at the
base (killed); **80 s** with this commit, exit 0, no diagnostics (native
reports none). The rest of the time is dominated by the per-mint printed
text (`written_type_text_flags`, `signatures.rs`, ~34% inclusive) and
`resolve_name_with_export_alias` (~10%), both outside `declared.rs`. The
recursion itself stays exponential: the mapped template's `T[K] extends
object` never resolves (`probefile` on a reduced `KeysCanBeExpanded_` prints
`"role" | T[K] extends object ? … : never`, where native prints the keys), so
`Depth` never bounds it. That is `mapped.rs` (r5-mapped4).

Unfiltered, against the frozen base `d91243e`:

| | base | after |
|---|---|---|
| diagnostics RIGHT + EMPTY_RIGHT | 10983 | 10983 |
| type lines RIGHT | 545161 | 545161 |
| losses (diag / types / base-RIGHT keys missing) | | 0 / 0 / 0 |

No verdict gains: the case is a known divergence and is in neither dump. Two
type lines moved GAP → WRONG, both in `genericCallInferenceInConditionalTypes1`
(0:21, 0:31). They used to print `error` and now print the deferred
`"ref" extends keyof P ? Omit<P, "ref"> : P` where native prints the
evaluated `Omit<any, "ref">`. A shared evaluation from the memo now reaches
a line that used to decline. Neither line was RIGHT before. Not traced
further.

`slowcases` on both dump pairs: clean (only the four KNOWN_SLOW cases, none
slower). Ir (callgrind, `--singleThreaded --pretty false`, base binary vs this
commit): generic-imports 342,918,439 → 342,902,017 (−0.005%); domain-model
1,198,111,634 → 1,153,360,167 (**−3.7%**, the memo's sharing). Median child CPU
vs the base binary, 21 samples: generic-imports 0.994, domain-model 0.962;
`diagnostics_match: true` on both. Workspace tests pass; clippy reports
nothing in `declared.rs`/`checker.rs`. Unit tests:
`conditional_alias_instantiations_are_shared_per_arguments_and_bounded`,
`flattened_alias_bindings_match_the_hash_map_flattening` (`declared.rs`).

### 1.5 For the integrator

- `check.rs` `check_node`: reset `instantiation_count` per source element, as
  checkSourceElement does (`:2246`), and in the deferred-node check (`:2509`).
  It is a hub function, so not changed here. It moves no corpus line today
  (§1.1).
- `tsr-2zk.1069` (r5-align's timing pass over known-divergence cases) had not
  landed at `d91243e`. When it does, recursiveConditionalCrash3 should time at
  ~80 s, not time out.
- What remains of the case's cost is outside this file: the mapped template's
  `T[K]` reads (`mapped.rs`, r5-mapped4), and the eager printed text
  (`written_type_text_flags`, `signatures.rs`) and `resolve_name_with_export_alias`
  (`symbols.rs`) on every mint.

## 2. `.1061`'s decline removed (r5-declared2 §1.2)

> **Corrected (§5).** This section's measurement was taken on `d91243e`, the
> batch-AC tip, which the integrator then reverted (`58ead272`). On that base
> `aliasInstantiationExpressionGenericIntersectionNoCrash2` was already
> WRONG, so dropping the decline looked free. Against the real integration
> head it is the loss that reverted batch AC. §5 restores the decline,
> narrowed to the piece that is actually missing. The numbers below are kept
> as measured.

r5-declared2 kept the alias's own name mint for an intersection-bodied alias
when a constituent was an alias reference whose body `evaluate_alias_body`
could not build. That covered `typeof Class<T>`, which a type query with type
arguments answered as `error` before `tsr-2zk.1006`. r5-instexpr's
instantiation expressions have landed (`get_type_from_type_query_node` now
calls `get_instantiation_expression_type`), and the decline's own falsifier
said to re-measure without it. It is removed, so the intersection road is
getTypeAliasInstantiation's (`:23641`) for every constituent.

**Measured** unfiltered, against the frozen base `d91243e` (with §1's commit
under it):

| | base | after |
|---|---|---|
| diagnostics RIGHT + EMPTY_RIGHT | 10983 | 10983 |
| type lines RIGHT | 545161 | 545162 (+1) |
| losses (diag / types / base-RIGHT keys missing) | | 0 / 0 / 0 |

Gain: `contextualTypeBasedOnIntersectionWithAnyInTheMix5:0:17`
(`this : ExtractComputedReturns<{}>`). Ir vs §1's commit is flat
(generic-imports 342,902,017 → 342,894,718; domain-model 1,153,360,167 →
1,153,379,821). Median child CPU vs the base binary, 21 samples:
generic-imports 1.019, domain-model 1.002; `diagnostics_match: true`.
`slowcases` flagged `ramdaToolsNoInfinite2` once (diag dump, 408 → 1,647 ms,
the 3× rule over 1,000 ms). That was one noisy sample. Filtered reruns read
545–597 ms, and the CLI on the case reads 766–781 ms against the base
binary's 806–922 ms.

**Not converted: `aliasInstantiationExpressionGenericIntersectionNoCrash2`.**
It was already WRONG at the base (TS2352 missing), with or without the
decline, so the decline was no longer holding a RIGHT. Two type lines in it
remain: the alias lines `ClassAlias`/`FnAlias` print the written
`typeof Class<T>` / `typeof fn<T>` where native prints the structure
(`{ new (): Class<T>; prototype: Class<any>; }`, `() => T`). The missing TS2352
needs that structural relation, so it is the same root.

## 3. The two small diffs other lanes left: already folded

`r5-errorsplit4-default-declared.diff` (getResolvedTypeParameterDefault
caches unconditionally, `:22007`) and `r5-typetriage-enum-member-ascii-escape.diff`
(`quote_ascii` at the two `(typeof {name})[…]` enum-member sites) were
both folded by r5-declared2 in `62d62f3`, which landed in batch AC. Nothing
to apply. Their measurements are in [`r5-declared2.md`](r5-declared2.md).

## 4. `.1078` leftovers

### 4.1 `.1034`(e): operands when only the extends type is generic

getConditionalType (`:24300`) defers a root when its check type *or* its
(inferred) extends type is generic. `conditional_inference_operands` (read by
inferToConditionalType's pairing in `inference.rs` and by the relater's
conditional arms) answered only for a generic check. r5-declared §1.4 gave
the reason: for a concrete check, reading extends again can resolve a
recursive `infer` target a second time.

**Port.** A concrete check now has operands when the root declares no
`infer` and its extends type is generic. A root declares `infer` exactly when
the binder filed an infer type parameter in the conditional's locals, as
bindTypeParameter does, so the test is `binder.locals(root)` being non-empty.
That hazard is the only reason the decline existed, and roots with `infer`
keep it.

**Measured** unfiltered, against the frozen base `d91243e` (§1–§2 under it):
no verdict and no printed text moved against §2's dumps, on either dump.
That makes it 0 losses and 0 gains. The relater's conditional arms do not yet
decide a pair through these operands in any corpus case. The arm does run:
domain-model Ir 1,153,379,821 → 1,154,017,055 (+0.055%), generic-imports
flat (342,894,718 → 342,894,711). Median child CPU vs the base binary, 21
samples: domain-model 1.011, generic-imports 0.990; `diagnostics_match: true`.
`slowcases` clean. It lands because it narrows a decline toward native at no
cost. The unit test `concrete_check_roots_have_operands_when_only_extends_is_generic`
pins both sides: `string extends T` has operands, and
`string extends [infer U, T]` keeps `None`.

### 4.2 Not converted

- **`singletonLabeledTuple:0:17`** (`AliasRest extends [unknown] ? true :
  false` with `type AliasRest = [...p: number[]]`; native `false`). The
  alias's tuple road (`get_type_from_tuple_type_node`, §79.1) mints the name
  `AliasRest` and copies the structural tuple's side tables onto it. Here
  the structural answer is an *array* normalization, which has no tuple side
  tables, so the name carries nothing a relation can read, and the
  conditional defers. Native's deferred reference resolves to a tuple
  target with one Rest element, which is not assignable to `[unknown]`. The
  fix is for the named copy to carry the array identity, through the
  completed array-alias body `index_signatures.rs` already reads
  (`completed_original_array_alias_body`). That file is not mine. Not
  attempted.
- **`namedTupleMembersErrors` 11/12** (`type RecusiveRestUnlabeled = [string,
  ...RecusiveRestUnlabeled]`; native TS2456 and declared `any`). The rest
  operand is a reference, so the element is variadic and native builds it
  with createNormalizedTupleType. That reads the operand's type, re-enters
  getDeclaredTypeOfTypeAlias, and pushTypeResolution reports the circularity.
  This port keeps a rest over an unresolved reference print-only
  (`tuple_type_node_structural`), so the declared-type computation never
  re-enters and prints `[string, ...RecusiveRestUnlabeled]`. The cause is in
  `declared.rs`, not `circular_alias.rs` (which handles import aliases). The
  case cannot convert here either way: it also misses five parser-grammar
  diagnostics (TS5085/5086/5087/17019/2574) and reports two extra TS1110.
  Not attempted in this session.
- **`namedTupleMembersErrors` 8** (`[first: string, rest: ...string[]?]`) is
  the parser's postfix-`...` form, as r5-declared2 §2.5 said.

## 5. Re-landing r5-declared2 on the integration head (batch AC's loss)

Batch AC (r5-declared2's merge and its fill-missing diff) failed the
integrator's gate with one diagnostics loss and was reverted on the
integration branch (`58ead272`, and `b52e2e5` for the `inference.rs` diff).
Sections 1–4 above were measured on `d91243e`, the batch-AC tip, so their base
already carried the loss. This branch is now rebuilt linearly on the
integration head `6cdb344` (batches AD–AG):

1. `git revert 58ead272` re-applies r5-declared2's merge;
2. §1, §2 and §4's commits, cherry-picked unchanged;
3. `git revert b52e2e5` re-applies the fill-missing diff
   (`r5-declared2-fill-missing-forward.diff`, r5-declared2 §3);
4. this section's fix.

### 5.1 The loss: `aliasInstantiationExpressionGenericIntersectionNoCrash2`

`wat as Wat<string>` with `type Wat<T> = ClassAlias<T> & FnAlias<T>`,
`type ClassAlias<T> = typeof Class<T>`, `type FnAlias<T> = typeof fn<T>`.
Native reports TS2352 through the `ClassAlias` constituent. RIGHT at the
integration head, WRONG (no TS2352) with r5-declared2 re-applied, with or
without r5-declared2's decline.

**Cause.** r5-declared2's `instantiate_intersection_alias` makes
`Wat<number>` an intersection of the instantiated constituents
`ClassAlias<number> & FnAlias<number>`. Native relates that structurally.
Here, a reference to an alias whose body is an instantiation expression is
still a **member-less name mint**. On a probe, `new a()` with
`a: ClassAlias<number>` answers `error`, and `f()` with `f: FnAlias<number>`
answers `error`. The constituent mints relate to each other only through
alias variance (`ClassAlias<number>` to `ClassAlias<string>` reports TS2352
alone). Across aliases (`FnAlias<number>` to `ClassAlias<string>`) they have
no members to compare, so the intersection comes out comparable and the
TS2352 is lost.

r5-declared2's decline tested `evaluate_alias_body(owner, args).is_none()`.
That was a proxy for this, from before `tsr-2zk.1006`. Since instantiation
expressions landed, the constituent bodies do evaluate (`{ new ():
Class<T>; prototype: Class<any>; }`, `() => T`), so the proxy never fires.
The piece that is missing is that a *reference* to such an alias does not
carry those members, not the body itself.

**Port (narrowed decline).** `instantiate_intersection_alias` keeps the
alias's own name mint (alias-argument relation, as at the integration head)
when a declared constituent references an alias whose body, parentheses
skipped, is a type query with type arguments. Any other intersection body
takes getTypeAliasInstantiation's road.

- **What would change it:** references to instantiation-expression aliases
  carrying the instantiated structure (the alias reference road, so that
  `new a()` types). Then the decline goes, and the TS2352 comes from the
  structural relation, as native's does.
- **How I would know it is wrong:** an intersection over such a constituent
  whose native print or relation differs from the alias-argument relation.
  None appears in the full run below.

**Also found, not changed:** the instantiation-expression cache
(`instantiation_expressions.rs`, keyed `(node, expression type)`) ignores
the alias frames. Under this port's frame-bound alias evaluation,
`typeof Class<T>` evaluated with `T = number` and then with `T = string`
returns the first result. Keying it on the frame (`flattened_alias_bindings`)
did not restore the TS2352 by itself, and the file is not mine, so it is
reported, not changed.

### 5.2 Measured

Frozen base: the integration head `6cdb344`. `diagverdictdump` RIGHT 5431,
EMPTY_RIGHT 5590, WRONG 1160, EMPTY_WRONG 57; `verdictdump` RIGHT 548747,
WRONG 6644, GAP 900. Unfiltered, the whole branch (r5-declared2 re-applied,
§1–§4, the fill-missing diff, §5.1) against it:

| | base | after |
|---|---|---|
| diagnostics RIGHT + EMPTY_RIGHT | 11021 | 11036 (+15) |
| type lines RIGHT | 548747 | 548865 (+118) |
| losses (diag / types / base-RIGHT keys missing) | | 0 / 0 / 0 |

Cases converted: `enumAssignmentCompat6`, `jsxChildWrongType`,
`namespaceDisambiguationInUnion`, `parenthesisDoesNotBlockAliasSymbolCreation`,
`spyComparisonChecking`, and all ten module configurations of
`arbitraryModuleNamespaceIdentifiers_module`. Type gains, by case:
`typeVariableConstraintIntersections` 19, `genericDefaults` 17 (the
fill-missing diff), `variadicTuples2` 9, `partiallyNamedTuples` 9,
`propTypeValidatorInference` 6, `intersectionsAndEmptyObjects` 4,
`declarationEmitGenericTypeParamerSerialization2` 4,
`namedTupleMembersErrors` 3, `ramdaToolsNoInfinite2` 3, two per
`arbitraryModuleNamespaceIdentifiers_module` configuration, and smaller ones.
That is r5-declared2's measured gains plus §2's and the fill-missing diff's.

`slowcases`: clean on both dump pairs. recursiveConditionalCrash3: 81 s,
exit 0. Ir (callgrind, `--singleThreaded --pretty false`, base binary vs
this branch): generic-imports 342,993,333 → 342,957,086 (−0.01%);
domain-model 1,190,055,384 → 1,146,632,065 (**−3.6%**). Median child CPU vs
the base binary: domain-model 0.986 (21 samples); generic-imports 1.040 at
21 samples, **1.028** at 41. `diagnostics_match: true` on both. Workspace
tests pass; clippy reports nothing in `declared.rs`, `checker.rs` or
`inference.rs`; fmt clean.
