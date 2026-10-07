# Alias semantic body: tsr-2zk.16.2

Pinned native: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

## Implemented boundary

`getDeclaredTypeOfTypeAlias` publishes the resolved body and seeds the alias's
ordered local-parameter instantiation with that body. `getTypeAliasInstantiation`
then maps that declared body through `instantiateTypeWithAlias`; it does not
manufacture an opaque object for constructors that never receive an alias.

This port fixes the reachable `keyof` and argument-free `typeof` constructors.
Their declared bodies already existed, but `create_type_reference_with_display`
previously returned a new `Named { flags: OBJECT }` instantiation. It now uses
`get_declared_type_of_symbol` before looking up the existing instantiation table,
constructs the native ordered parameter mapper, and calls `instantiate_type`.
The existing template constructor retains its separate worker: its constrained
intersection normalization is not interchangeable with the generic mapper.
Queries with written type arguments still require the external prerequisite below.
Written alias references enter the existing `qualified_written_text` channel;
signature annotation/constraint reuse is distinct from semantic alias identity.

Owner/key: the private `Checker` owns `declared_types[SymbolId]` and
`instantiations[(SymbolId, Vec<TypeId>)]`; IDs come from this checker's binder and
TypeStore, arguments remain ordered, and options remain fixed for its lifetime.
This slice never adds another cache. Native's additional alias key is not needed
for these constructors because they never accept an alias; it is required before
extending this route to alias-bearing bodies. Ambient alias evaluation bindings
are consumed by argument resolution, not substituted into a previously completed
body under another alias's context. The existing mapper applies semantic parameter
IDs and passes borrowed parameter names only for existing mapper APIs.

Publication: absent declared entry enters the existing DeclaredType resolution
frame; active resolution is not completed success. Successful resolution seeds
only supported alias-free bodies. Failed resolution retains the existing error
publication/fallback; no invented active-mapper cache or provisional semantic
success is published. The existing instantiation table receives a result only
after the mapper worker returns. Alias-bearing generic bodies remain unsupported
by this new route. No receiver or static/instance context is erased: `typeof`
returns the actual checked value type, and unchanged bodies retain its identity.

Work: declared-body AST resolution occurs before instantiation-table reuse;
`instantiate_type` owns the expensive semantic walk on cache misses. A completed
own-parameter seed avoids remapping that same declared body. No speed claim follows.
**Beads request to integrator before extending reuse:** record worker/query/hit/
active-repeat counts for this boundary under `tsr-2zk.16.2` and existing expensive-
work attribution issues; this box cannot write Beads. Counts are unmeasured.

## Native controls and regression

Native `tsgo --ignoreConfig --strict --declaration --emitDeclarationOnly` on a
private control inferred `{ item: string; }` for both `Value<number>` and
`Value<string>` where `type Value<T> = typeof value`; `Keys<{left;right}>` inferred
`"left" | "right"`, parenthesized `NestedKeys<{first}>` inferred `"first"`, and a
distinct argument inferred `"other"`. Native, frozen TSR, and candidate CLI
`--strict --noEmit` executions all completed without diagnostics for that control.
`alias_semantic_declared_body.rs` checks cold reverse lookup, warm lookup, distinct
key arguments, and actual value type identity rather than only printed alias text.

## Integration prerequisites: do not extend the route prematurely

1. `expressions.rs`: implement native `checkExpressionWithTypeArguments` and
   `getInstantiationExpressionType` (checker.go). The latter filters call and
   construct signatures by generic arity, checks type arguments, instantiates
   signatures, retains members/index infos, walks unions/intersections with native
   applicability state, and reports native TS2635 when no signature applies.
   Its cache key is `(AST NodeId, source TypeId)`, not an alias or printed string.
   Then replace `declared.rs::get_type_from_type_query_node`'s unconditional
   nonempty-argument refusal with that worker. This directly blocks
   `aliasInstantiationExpressionGenericIntersectionNoCrash1/2`; the existing
   diagnostic RIGHTs must remain while their six wrong type rows are converted.
2. `inference.rs::instantiate_type`, `instantiate_type_worker`, and
   `mentions_type_parameter_inner`: native `instantiateTypeWithAlias` must consider
   alias arguments even when semantic constituents contain no type variables,
   map alias arguments/origin, preserve the alias on union/intersection rebuilds,
   and distinguish active mapper identity from a completed result. Its native
   active mapper cache includes the source type and explicit new alias.
3. `checker.rs` / rendering owner: serialize generic alias metadata through the
   existing `alias_of` channel (or a coordinated TypeStore representation change)
   and preserve its distinction from `type_reference_targets`. The current
   `get_declared_type_of_type_alias` generic name mint cannot be cleanly removed
   until owned constructor changes and the external mapper consumers cut over
   together. Object/callable members, mapped types, deferred references, unions,
   intersections, conditional roots and normalized tuples have different native
   alias attachment points. No broad alias-body evaluator fallback was added.
4. Integration-owned `crates/tsr-conformance/tests/original_callable_entry.rs`:
   `recursive_declarations_original_entry_does_not_add_either_circular_return_occurrence`
   expects the erroneous TS2464 `(13,82)` although the current pinned corpus expects
   no diagnostic. The fix removes that diagnostic; full workspace tests stop at
   this stale expectation. Delete the incidental error expectation rather than
   repinning it. All remaining workspace tests passed with this one test excluded;
   that exclusion is not a clean full-workspace gate.

No `tsr-core` source contains the semantic type representation: it is
`crates/tsr-checker/src/types.rs`. No unowned files were edited.

## Evidence and limits

Baseline frozen before edits: source parent `5dd3bad84d12991e1ba169d2d5687321e1989740`,
release `tsr`, unfiltered diagnostic/type dumps. Corpus discovery: 12,444 cases.
Read-only full coverage uses every suite from coverage's driver through `run_suite`
and omits only snapshot writes (ownership forbids those writes).

Unfiltered type RIGHT lines: **469,765 -> 469,776 / 478,855**. Eleven gains:
two `recursiveTypeRelations`, one `keyofIntersection`, eight `mappedTypeAsClauses`.
Whole type cases remain **8,042 / 9,538**; no newly complete type case claimed.
Diagnostic cases: **4,221 -> 4,222 / 5,502**, clean cases **4,968 -> 4,969 / 5,068**.
Conversions: `mappedTypeAsClauses` diagnostic RIGHT and
`declarationsWithRecursiveInternalTypesProduceUniqueTypeParams` EMPTY_RIGHT.
Both loss checks are empty; no missing verdict keys. Dumps have two multiline
continuation rows that are not verdicts; comparisons include only recognized
verdicts. These scores do not verify full messages, lengths, or diagnostic order.
The broader assigned cluster and >=99.9% release goal remain blocked, not complete.

Full 16-suite read-only coverage completed: checker_types **8,042/9,538**,
diagnostics **4,222/5,502**. Workspace release tests encounter the single external
stale expectation above; all other tests pass with that test excluded. Workspace
clippy `--all-targets -- -D warnings` and workspace fmt check pass on Rust 1.96.0.

Fresh-process interleaved 21-pair measurements on domain-model and generic-imports
matched complete diagnostic text. Baseline/candidate and pinned-native performance
reports explicitly mark `work_comparable=false`, complete query-input equivalence
and actual checked work unverified. Therefore there is **no verified wall ratio**
and no <=0.50 claim. Timing details are recorded below after the final run.
