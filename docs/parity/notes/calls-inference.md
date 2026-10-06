# Lane notes: calls-inference (tsr-2zk.9)

Judgment calls made by the `calls-inference` parity box. Numbers are measured
with `verdictdump` / `diagverdictdump` against the baseline frozen at
`0d996e8` unless a section says otherwise.

## 1. REFUSED: untyped calls by callee provenance (tsr-2zk.16.5)

**Status: reverted** (48ce04a reverted at the integrator's review). Kept here
because a refused design deleted rather than recorded costs the next session
the same measurement.

**What it was.** `isUntypedFunctionCall`'s first disjunct is
`IsTypeAny(funcType)` (`checker.go:9933`). TSR's `is_untyped_call_target`
admits only an `any` the source wrote. 48ce04a added
`any_is_upstreams_implicit_any`: syntactic arms (unannotated parameter with no
contextual type, `undefined`/`null`-initialised var, `typeof globalThis`
property no global declares, member declared `: any`, getter returning only
`null`, element of `var x = []`) that guessed whether TSR's `any` was the same
`any` upstream computes. Measured 38 gap->right, 0 gap->wrong.

**Why refused.** It does not ask what upstream asks, on the same data:
upstream asks a type question; the arms inspect declaration syntax to
compensate for TSR producing `any` where upstream produces another type or the
error type. That is a heuristic, and the project rule is to port the
operation.

**The faithful fix** is on the type side: every TSR producer that answers
`any` for "could not compute" must answer the error type (or the right type),
after which `IsTypeAny(funcType)` can be asked literally. Measured at
`0d996e8`: the literal test (`callee_type == any`, error excluded) gives
42 gap->right and **30 gap->wrong**; the 30 wrongs name the producers that must
change first:

| Producer of a wrong `any` callee | Cases (gap->wrong lines) |
|---|---|
| UMD-global augmentation members (`v.reverse` on an augmented class) | umd-augmentation-1..4 (8) |
| private-name accessor read (`this.#fieldFunc2` typed `any`) | privateNameAccessorsCallExpression, privateNameStaticAccessorsCallExpression (8) |
| class-expression name resolves to the outer binding | classBlockScoping (4) |
| `/// <reference types>` / module augmentation members | typeReferenceDirectives9 (4), module_augmentUninstantiatedModule2 (1) |
| dynamic `import()` of a union of modules | dynamicImportsDeclaration (2) |
| recursive intersection return | returnInfiniteIntersection (2) |
| JS overload via JSDoc | jsFileMethodOverloads (1) |

None of these producers is in this lane's files.

## 2. `Function`-typed callees are untyped calls (tsr-2zk.16.5)

`isUntypedFunctionCall`'s third disjunct (`checker.go:9936`): a callee whose
apparent type is not a union, does not reduce to `never`, has no call and no
construct signatures, and is assignable to the global `Function` interface is
an untyped call answering `any` (`fn: Function; fn()`). Ported as
`Checker::is_untyped_function_typed_callee` (`calls.rs`) on the call and
tagged-template roads; the `new` road has no such disjunct upstream.

**Judgment.** Both signature lists must come from a *complete* query
(`signatures_of_type_kind` answering `Some`). `None` means "unresolved", not
"zero"; reading it as zero would turn every unresolved callee into `any`. The
module-clone special case in `check_call_expression_worker` already computed
the `Function` assignability; it now shares `is_assignable_to_global_function`.

Measured alone on top of the (since reverted) §1 commit: +4 gap→right, 0 gap→wrong, 0 losses
(`functionType`, `callWithSpreadES6` converted).

**Not ported:** the type-parameter disjunct
(`IsTypeAny(apparent) && funcType is TypeParameter`); TSR does not answer an
`any` apparent type for an unconstrained type parameter.

## 3. `reorderCandidates` on the call road (tsr-2zk.16.9)

**Forcing constraint.** `resolveCall` reorders candidates once before any pass
(`checker.go:8843` -> `reorderCandidates`, `:8957`): literal-typed
("specialized") signatures are hoisted ahead of the rest, and a later
declaration group of a merged symbol is spliced ahead of the earlier group.
TSR ported it only for `new` (`reorder_construct_candidates`); the call road
walked declaration order and, to stay sound, its subtype pass declined
(Undecidable) any set containing a specialized signature.

**Decision.** Rename the port `reorder_candidates` and apply it once at the
`choose_overload` entry and on the named-callee subtype pass; delete the
Undecidable guard. The construct road already passes reordered candidates, so
`choose_construct_overload` enters `choose_ordered_overload` directly — the
reorder is not idempotent (a second application splices merged groups back).

**Prerequisite fix found by the reorder.** The single-arity-survivor arm of
`choose_overload` related arguments to a *generic* survivor's uninstantiated
parameters. That answered NotRelated for `proxy<T, U>(fn: (options: T) => U)`
given `oneArg: (input: string) => string`, and the arm then returned the
order-sensitive "longest candidate" — after reordering, the 2-parameter
specialized overload (`declarationEmitOverloadedPrivateInference`, 2
right->gap). Upstream infers before `isSignatureApplicable`; a generic survivor
now flows to `check_generic_call` unchecked, as a single generic does.
Measured alone: +2 wrong->right, 0 losses.

**Measured** (types lines vs the frozen base, on top of the since-reverted §1 commit): reorder + survivor
fix +58 gap->right, +98 wrong->right, 0 right->other, **4 gap->wrong**, all in
`intersectionTypeInference3`: `Array.from(a)` now resolves through the
es2015.iterable overload (spliced first, as upstream), and the inferred
element prints `Nominal<"A", string>[]` where upstream keeps the alias `A[]`.
The selection is upstream's; the alias is lost in inference from `Set<A>`'s
iterator member. Accepted as an inference alias-retention gap, outside this
change.

**Not ported:** `getOptionalCallSignature` for call chains (`callChainFlags`).

## 4. TS2349 for object-shaped callees (tsr-2zk.9)

**Forcing constraint.** `resolveCallExpression`'s `len(callSignatures) == 0`
arm (`checker.go:8555-8571`) reports `invocationError` → TS2349 for any callee
whose apparent type has no call signatures, is not an untyped call, and (else
TS2348) has no construct signatures. TSR reported TS2349 only for primitives
(§318), declining every object-shaped callee because an incomplete signature
list would read as "not callable". 23 TS2349 lines were missing in the lane.

**Decision.** `check_callee_without_signatures` (`check.rs`) reports when
`signatures_of_type_kind` returns **complete** (`Some`) and empty lists for
both kinds, the callee is not `Function`-typed (§2), and TS2348 stays with
`check_class_called_without_new`. The target is the member name for a
property-access callee (`invocationErrorDetails`, `checker.go:9946`) — also
applied to the primitive arm. A zero-argument property-access call now
resolves the member: a `get` accessor heads with TS6234 (`checker.go:9983`),
an unresolved member declines, element access still declines.

Declines, each from a measured false positive (measured at the occurrence
level too, since an extra line inside an already-WRONG case changes no
verdict):

- **Union with a `Function` constituent.** `resolveUnionTypeMembers`
  (`checker.go:21056`) gives the global `Function` constituent the
  `unknownSignature`, so `Function | (() => object)` is callable (no report;
  the call types `error`). `resolved_union_signatures` (`union_signatures.rs`)
  now answers *unresolved* for such a union instead of an empty list, since
  `unknownSignature` is not modelled (`unionOfFunctionAndSignatureIsCallable`).
- **Generic context.** Inside a declaration with type parameters, a callee can
  be typed through mapped/indexed machinery this port types incompletely:
  `promises.map` on `{ [K in keyof T]: … }` with `T extends readonly
  unknown[]` read as `PromiseSettledResult<Awaited<T["map"]>>`
  (`dependentDestructuredVariablesFromNestedPatterns`). The arm declines when
  any ancestor declares type parameters — before any signature query, which
  itself forced recursive return types (TS7024 in
  `declarationsWithRecursiveInternalTypesProduceUniqueTypeParams`, caught by
  `tests/original_callable_entry.rs`). **How to know it is wrong:** lifting
  this decline once homomorphic mapped types over array type parameters are
  ported should measure zero new false positives.
- **Union callees.** Whether constituents share signatures depends on
  narrowing: `result()` under `result instanceof Function` stays the whole
  un-narrowed `(() => EffectResult) | Promise<EffectResult>` in this port
  (`unresolvableSelfReferencingAwaitedUnion`), which upstream narrows first.
  No conversion came from a union (`betterErrorForUnionCall` stays wrong).
- **JS files**, as `check_call_arity` (`typeTagNoErasure`).
- **`this` receiver on the zero-argument path.** Resolving `this.g` from the
  rule forces an object literal's `this` type early and manufactured a TS7023
  circularity (`thisTypeInObjectLiterals2`); declined.

**Measured** (diagnostics, vs the previous commit): 7 cases wrong→right
(`callOnInstance`, `constructorOverloads4`, `neverIntersectionNotCallable`,
`valuesMergingAcrossModules`, `esModuleInteropDefaultImports`,
`methodChainError`, `instancePropertyInClassType`), 0 losses; types
unchanged.

**Not ported:** the tagged-template road's TS2349
(`taggedTemplateWithConstructableTag01/02`, `templateStringInTaggedTemplate*`)
and the union sub-messages (head code only is compared).

## 5. `getNoInferType` at instantiation (tsr-2zk.44) — BLOCKED on one relate-lane line

**Status: not pushed as code.** The full patch is
`docs/parity/notes/calls-inference-noinfer.diff` (round 2, measured on top of
`15f1743`); it needs one hunk in `assignreport.rs`, which this box does not
own, so the integrator serializes it.

**Forcing constraint.** `compiler/contextuallyTypedJsxChildren2` reports two
false TS2345 at `Math.max(selected, 0)`: `selected` types `NoInfer<any>`.
`instantiateTypeWorker`'s substitution arm (`checker.go:22277`) instantiates
`NoInfer<T>` to `getNoInferType(T')` (`checker.go:27394`), which keeps the
wrapper only when `isNoInferTargetType(T')` (`:27401`); `any` is not a
target, so upstream's `selected` is `any`. TSR rebuilt the alias reference
unconditionally (the integration's relate lane then reports an "object"
`NoInfer<any>` against `number`).

**The patch, three pieces, each forced by the previous:**

1. *Port* — `instantiate_type_worker` (inference.rs) returns the
   instantiated base of a `NoInfer` reference unless
   `is_no_infer_target_type` (a port of `isNoInferTargetType`; TSR's only
   substitution type is the `NoInfer` reference, which is not a target).
   Converts the target case, but **loses `compiler/narrowingNoInfer1`**
   (EMPTY_RIGHT → TS2698 at `{ ..._ }`).
2. *The loss's cause* — the diagnostic walk reaches a callback body before
   anything resolves the call, so `_` is typed by `contextual.rs`'s stateless
   "fixing" road, which maps every type parameter to `unknown` even when an
   earlier argument fixes it. `NoInfer<unknown>` used to pass the spread
   check as an object; upstream's `unknown` does not. The base binary already
   reports the same false TS2698 for a plain `(a: A) => …` callback
   (`map7(m, (_) => ({ ..._ }))`), so the wrapper was masking it. The
   faithful order is upstream's: `checkCallExpression` → `resolveCall`
   assigns callback parameter types before the body is checked.
   `check_single_generic_candidate_arguments` (calls.rs, the diagnostic rule
   that runs on the call node before its children) now runs the call's type
   road first when a context-sensitive argument needs its published
   instantiation.
3. *Exposed by 2* — `tests/partial_inference.rs`'s
   `indexed_callbacks_retain_native_negative_diagnostics_and_concrete_values`
   pins native TS2322 on `const rejected: string = missingValue` where
   `missingValue: T["missing"]`, `T = { present: number }`. Upstream's
   instantiation answers `unknownType` for an absent key with no access node
   (`getIndexedAccessTypeEx`, `checker.go:26930`); TSR answered `error`.
   `indexed_access_is_certainly_absent` ports the not-found leg on certified
   inputs only (literal key, non-generic object with a complete property
   list, no apparent property, `Some(empty)` index infos); every other
   `None` stays "not computed" (`error`). TSR previously passed this test by
   coincidence: the stateless road typed the parameter `T["missing"]`.
   The calls.rs unit test that pinned `instantiate_signature` declining on
   that metadata now expects upstream's success.

**The blocker.** With 1–3, `missingValue` is upstream's `unknown`, and
`assignability_pair_is_reportable` (assignreport.rs) vetoes every `unknown`
side. The patch's hunk there exempts an *exact* `unknown` intrinsic source;
flagged look-alikes and targets keep the veto.

**Measured** (whole patch, unfiltered, vs the frozen base at `15f1743`):
diagnostics 6 cases WRONG→RIGHT (`contextuallyTypedJsxChildren2`,
`contextualTypingWithFixedTypeParameters1`,
`genericFunctionTypedArgumentsAreFixed`, `typeInferenceConflictingCandidates`,
`genericCallWithObjectTypeArgsAndConstraints2`,
`typeArgumentInferenceWithObjectLiteral`), 0 losses; types 7 WRONG→RIGHT,
0 losses. The assignreport hunk alone changes no corpus verdict. Workspace
tests pass with all hunks; without the assignreport hunk exactly the
`partial_inference` test above fails. CPU-median self-ratio (41 samples):
domain-model 1.004, generic-imports 1.003.

**Not ported:** the collapse at type-node creation (`NoInfer<any>` written
in source still builds the reference; `create_type_reference`, declared.rs,
type-refs lane), and `getNarrowableTypeForReference`'s NoInfer strip
(`checker.go:31492`; flow lane) — the remaining WRONG type lines of both
NoInfer cases.

**How to know it is wrong:** step 2 should never *add* a diagnostic whose
call upstream resolves identically; a new false positive inside a callback
body after merging names a call whose type road answers differently from
upstream, not a reason to restore the stateless road.

## 6. Object-callee TS2349 in the head (tsr-2zk.45)

Round 1's `check_callee_without_signatures` (§4) was superseded at
integration by main's `check_call_expression_head` (calls.rs). Its three
lost cases decline at the head's `is_untyped_signatureless_call` for two
different reasons, measured round 2:

- **`neverIntersectionNotCallable` — ported.** `isUntypedFunctionCall`
  (`checker.go:9937`) excludes a callee whose
  `getReducedType(apparentFuncType)` is `never`; the port tested only the raw
  `NEVER` flag, so `{ (x: string): number, a: "" } & { a: number }` (a
  never-reduced discriminant, `getReducedType` `checker.go:21830`) related to
  `Function` and read as an untyped call. The arm now also asks the shared
  `intersection_has_never_discriminant` (flow.rs, read not edited).
  +1 case, 0 losses.
- **`esModuleInteropDefaultImports` (6 lines), `valuesMergingAcrossModules`
  (1) — blocked, relate lane.** The callee is a module or namespace object
  (`typeof /mod`, `typeof A`). Relating it to the global `Function` answers
  `Unknown` with `reasons` mask `0b100` (row 3, `NoMembersTable`): the
  relater has no structural arm for an anonymous `typeof <namespace>` source.
  Upstream resolves such a type's members to its exports
  (`resolveAnonymousTypeMembers`) and answers NotRelated (no `apply`/`call`/
  `bind`). Round 1 read `Unknown` as "not assignable"; re-deriving the
  missing-property verdict in calls.rs is the §3a "side pass" pattern, so the
  faithful home is the relater's row-3 arm (`relater.rs`). A plain
  `namespace N { export const x = 1 } N();` shows the same miss.
