# r7-contextual — contextual typing, inference and literal lanes (`tsr-2zk.1275`)

Round-7 lane box. Owned files: `contextual.rs`, `inference.rs`,
`array_literals.rs`, `destructure.rs`, `binding_patterns.rs`, `objects.rs`,
`spreads.rs`. Native source is `vendor/typescript-go` @ `5b1047d`. Base:
`origin/main` `9020aa67`, frozen with `scripts/parity_gate.sh freeze`
(types 551,176 RIGHT / 4,508 WRONG / 673 GAP of 556,357; diagnostics 5,741
RIGHT + 5,610 EMPTY_RIGHT).

## 1. ARG-CONTEXT-RESOLVED-SIG: literal arguments re-checked under the instantiated signature

### Forcing constraint

`checkApplicableSignature` (`checker.go:9256`) checks every argument with
`checkExpressionWithContextualType(arg, paramType)` against the
*instantiated* candidate, and every later `checkExpression` of an argument
node (the type writer's, a diagnostic's) reads its contextual type from the
call's resolved signature (`getContextualTypeForArgumentAtIndex`,
`checker.go:29772`, `getResolvedSignature` at `:29789`). Literal widening
(`getWidenedLiteralLikeTypeForContextualType`, `isLiteralOfContextualType`
at `checker.go:25522`) therefore follows the *inferred* parameter type:

```ts
new Map([["", true]])      // [string, true]: V := boolean keeps `true`
f22(["foo", "bar"])        // T := [string, string] from [...T]
```

This port memoizes an argument's first check in `node_types`. The inference
pass checked `["", true]` under `readonly [K, V]` with `V` unresolved, widened
`true` to `boolean`, and that answer was what every later read saw. The r6
triage counted 18 types cases and one diagnostics case finished by this
alone (`r6-triage.md` §2 row 9).

### What was ported

`check_generic_call_worker` (`inference.rs`) now:

1. checks each array/object-literal argument **afresh** under the
   candidate's own parameter type (the inference context marked
   inferential), which is native's inference-pass check — not whatever an
   earlier candidate or an earlier resolution left in `node_types`;
2. after inference, re-checks the same arguments with the instantiated
   signature published through the existing per-call
   `call_inference_signatures` slot (saved and restored around the
   re-check), which is native's applicability check and the writer's view.

`calls.rs::recheck_literal_arguments_in_context` already did step 2 for a
*non-generic* overload candidate; this is the generic counterpart.

### Checker-port boundary record (docs/conventions.md)

- **Native operation:** `checkApplicableSignature` / `inferTypeArguments`
  (`checker.go:9256`, `:9390`) and the writer's
  `getContextualTypeForArgumentAtIndex` (`:29772`), pinned `5b1047d`.
- **Key identity and owner:** no new table. The re-check writes the
  existing `node_types` entries of the argument subtree (Checker-private,
  per node) and briefly occupies the existing `call_inference_signatures`
  entry of the call node.
- **Publication:** an argument's entry is replaced by the check under the
  last completed instantiation of this worker; a declined worker (any
  `return decline`) publishes nothing new. Re-entrant resolutions of the
  same call through the §56 `annotation_member_context` road
  (`narrow_value_stack` holds the call) neither evict nor re-check: that
  road is a contextual read, not the call's resolution.
- **Receiver/alias context:** the published instance is the worker's own
  `instantiate_signature` result with the signature's type parameters
  cleared — the same instance the `instantiated` out-slot returns.
- **Work boundary:** only array/object literals that are not
  context-sensitive and whose subtree holds no call, `new`, tagged template,
  function, method, accessor or class (`literal_subtree_resolves`): native
  caches a nested resolved signature and a function's type from their first
  check, while re-checking them here would redo that resolution (an
  exponential cost in nesting depth). The eviction
  (`evict_literal_subtree`) drops `node_types` only. It keeps the property
  symbols' `symbol_types`: native's re-check creates fresh property symbols,
  so a type built from the previous check — the reverse-mapped inference of
  `Readonly<Options>` from this very literal — still reads the old symbols.
  Dropping them (as `evict_subtree` does) made
  `reverseMappedTypeInferenceWidening1` report a false TS7022 at both
  `Readonly<Options>` calls.

### Alternatives taken seriously

- **Re-check only in the callers that publish `resolved_call_signatures`**
  (`calls.rs`, `expressions.rs`): that is where the signature is known to be
  the *chosen* one. Rejected for this lane because those files are not
  owned, and because the worker is also the inference pass: without step 1,
  a re-checked literal's narrower type fed the next candidate's inference
  (`Object.entries({ a: true, b: 2 })` inferred `T = number | true` on a
  later pass and fell to the `any` overload; measured, 4 RIGHT lines lost in
  `useObjectValuesAndEntries1`). It would win if the callers ran the
  re-check once on the chosen signature *and* the worker always checked
  arguments under its own context.
- **Evict at the worker's start without marking the context inferential**:
  the argument then read the stateless resolving road, which declines on a
  same-arity overload pair and lost `variadicTuples1:779/780`
  (`f22(["foo", "bar"])` printed `string[]`).

### Prerequisite fixed in the same commit

`mentions_type_parameter_inner` had no arm for `variadic_tuple_elements`, so
`[...T[K]]` did not mention `K` (native: `couldContainTypeVariablesWorker`'s
reference arm, `checker.go:22193`). A mapped member's template
`[...HandleOptions<T[K]>]` instantiated by `K := "prop"` with no printed
names kept `K`. Neutral on its own (measured: 0 gained, 0 lost); the
re-check exposed it as 10 lost lines in
`homomorphicMappedTypeWithNonHomomorphicInstantiationSpreadable1`.

### Result (commit 1)

Against the `9020aa67` freeze: types +86 RIGHT, 0 lost, 0 missing;
diagnostics unchanged. Cases fully converted (14):
`homomorphicMappedTypeWithNonHomomorphicInstantiationSpreadable1`,
`reverseMappedTypeInferenceWidening1`, `reverseMappedTypeInferenceWidening2`,
`unknownLikeUnionObjectFlagsNotPropagated`,
`destructuringParameterProperties3`, `for-of37`, `for-of38`, `for-of40`,
`for-of46`, `for-of49`, `for-of50`, `iterableArrayPattern29`,
`iterableArrayPattern30`, `keyofInferenceLowerPriorityThanReturn`.

### Still open in the cluster, and why

- `inferTupleFromBindingPattern`, `neverReturningFunctions1` (`parse()
  { return [true] }`, `this` in methods): the literal sits inside a function
  or method, outside the work boundary above. Native re-reads the return
  expression's context through the resolved signature; porting that needs a
  re-check of return expressions without re-resolving the function's own
  signature.
- `genericArgumentCallSigAssignmentCompat`, `genericTypeArgumentInference1`
  (`_.all([true, 1, null, 'yes'], _.identity)`): the generic-function
  argument sends the worker down the overload-failure retry
  (`skip_context_sensitive`), where no re-check runs.
- `reverseMappedIntersectionInference1`,
  `jsDeclarationsWithDefaultAsNamespaceLikeMerge`: not yet triaged.

### How to know this is wrong

A RIGHT line whose literal argument is checked by a later candidate after
the worker published a different candidate's instance (the last worker run
wins here; native's writer reads the chosen one). The falsifier is an
overload set whose *earlier* generic candidate is applicable at inference
but rejected later by `calls.rs`'s selection, printing the rejected
candidate's literal widening.

## 2. Intersection-target inference at `InferencePriorityNakedTypeVariable` (routed from r7-perf)

### Forcing constraint

r7-perf's jsTyping equivalent-work delta (`docs/parity/notes/r7-perf.md` §4
on `box/r7-perf`, cluster 1): 164 extra TS2769 on `visitNode`/`visitNodes`/
`nodeVisitor` calls whose visitor parameter is `(n: NonNullable<TIn>) => …`.
Cut down:

```ts
declare function v3<TIn extends Node | undefined>(node: TIn, visitor: (n: NonNullable<TIn>) => void): TIn;
v3(t /* Node | undefined */, visitor /* (n: Node) => void */); // TSR: false TS2345, TIn := Node
```

Native `inferFromTypes` (`inference.go:126-146`) matches identical
intersection constituents first and, when the target is still an
intersection, reaches `inferToMultipleTypes` (`inference.go:401`), which infers
to the single naked type variable with `InferencePriorityNakedTypeVariable`.
The contravariant `Node` from the visitor therefore loses to the direct
priority-0 candidate `Node | undefined` from the first argument
(`inference.go:183`'s priority reset). TSR's
`intersection_inference_source` shortcut inferred the remaining source to the
variable at the *current* priority, so both candidates sat at priority 0 and
`getInferredType`'s contravariant-preference arm picked `Node`.

### What was ported

`intersection_inference_source` now also answers whether the target is still
an intersection after matching — a union source (which skips matching), or
more than one unmatched target constituent — and the caller applies
`NAKED_TYPE_VARIABLE` exactly then. When matching removes everything but the
variable, native's target is the bare variable and the TypeVariable arm
infers at the current priority; that case keeps the current priority.
Identity is the existing `TypeId` equality the shortcut already used for
`isTypeIdenticalTo`. No cache, table or traversal is added.

### Result (commit 2)

Corpus-neutral against commit 1's freeze (types 0 gained / 0 lost;
diagnostics 0 / 0). jsTyping (`tsconfig.perf.json`, `--pretty false`): TSR
423 → 259 diagnostics; all 164 TS2769, 4 TS2345 and 2 TS2322 removed, none
added; tsgo-matching diagnostics unchanged at 84 of tsgo's 86.

### How to know this is wrong

An intersection target whose matching leaves exactly the naked variable plus
a constituent identical to a source constituent under `isTypeIdenticalTo` but
not under `TypeId` equality: native infers at the current priority there,
this port at the lower one.

## 3. Const-context literal images keep their literal markers (routed from r7-calls)

### Forcing constraint

`unionObjectAndArrayLiteralCandidates` (`inference.go:1470`) unions the
candidates flagged `ObjectFlagsObjectLiteral | ObjectFlagsArrayLiteral` with
subtype reduction. Native's const-context candidate *is* such a literal:
`checkObjectLiteral` flags it (`checker.go:13205`) and `checkArrayLiteral`
wraps the readonly tuple in `createArrayLiteralType` (`checker.go:8103`).
This port builds the const view afterwards, in
`const_inference.rs::const_literal_inference_source`, and minted it without
the markers. `widening.rs` then saw two plain object types, and the first
candidate won:

```ts
declare function f5<const T>(obj: { x: T, y: T }): T;
f5({ x: [1, 'x'], y: [2, 'y'] });   // native readonly [1, "x"] | readonly [2, "y"]; TSR readonly [1, "x"]
```

### What was ported

The tuple image passes through `create_array_literal_type` when its source
was an array-literal image. The object image copies the source's
`object_literal_spread_flags` entry, which is this port's
`ObjectFlagsObjectLiteral`, together with the source's printed members
(`object_literal_members`) and index infos. Normalization reads the members:
with the marker but without them, the union normalized to
`{ readonly b?: undefined; } | …`, which dropped the real members. All three
are existing side tables keyed by the new image's `TypeId`. Each is written
once, when the image is minted, and holds the same kind of value the source's
entry holds. No new table and no new traversal.

### Result (commit 3)

Against the merged base (main `7e9f37eb` + `3c301c08`): types +16 RIGHT,
0 lost (`typeParameterConstModifiers` 8, `jsdocTemplateTag6` 8); diagnostics
unchanged. This unblocks r7-calls' temporary const-type-parameter decline
(`typeParameterConstModifiers`).

### How to know this is wrong

A const image whose members differ from the source's printed members. The
copy is taken from the *source* literal, while the image's own properties are
rewritten readonly. `widening.rs` would then normalize with the non-readonly
member text. No corpus line shows this, because the members are re-rendered
from `anonymous_properties` on print. A falsifier is a normalized const union
member that prints a mutable property.

## 4. INFER-NO-CANDIDATE-GUARD: an `any`/`unknown` argument supplies no structural candidate

### Forcing constraint

When a type parameter collects no candidate, `getInferredType`
(`inference.go:1317`, no-candidate arm `:1358-1377`) answers its default,
`unknown`, or `any` under `AnyDefault` (JS files). `check_generic_call_worker`
instead declines the whole call (`structural_source_supplied`) whenever some
argument sits at a position mentioning the parameter. The reason is that this
collector is incomplete, so finding no candidate does not prove that native
finds none either. `inferingFromAny` (a JS file: `f2(a)` with `a: any`
against `t: T[]` is `any`) printed `error` on 17 lines.

### Measured before choosing

Dropping the decline outright, against the `90967e42` freeze: types +125 /
−4, diagnostics +8 / −3. Each loss is a collector or argument-type gap in
another lane's file, so the decline cannot go yet:

| loss | root cause | owner |
|---|---|---|
| `voidReturnIndexUnionInference` (TS2345 ×2) | `props.onFoo` on `Readonly<P>` reads `P["onFoo"] \| undefined`. Native reads the property of `getResolvedApparentTypeOfMappedType`, i.e. `Readonly<Props>`. | `mapped.rs`/`members.rs` |
| `symbolProperty61` (TS2345) | `typeof Symbol.obs` is `symbol`, not `unique symbol`, so inference through the `[Symbol.obs]` key finds nothing | unique-symbol typing |
| `jsFileImportPreservedWhenUsed` (4 lines) | `T[keyof T]` at `T := object` is `any`; native gives `never` (`keyof object` is `never`) | `indexed.rs` |
| `contravariantOnlyInferenceFromAnnotatedFunction` (TS2322) | native infers `A := string` from the annotated `fn: (a: string) => {}` inside a homomorphic `Funcs<A, B>`. This reading did not find the native arm that does it. | open (REVERSE-MAPPED family) |

### What was ported

Narrowing the decline toward native for one source kind the collector handles
completely. In `inferFromTypes` (`inference.go:65-275`) an `any` or `unknown`
source gets a candidate only at a naked type variable: directly, or through
the union, intersection, conditional and indexed-access arms, which this
collector has. Its default arm finds no object in `getApparentType(any)`.
Only `wildcardType` propagates further, and no argument is ever that type. So
such an argument joins §401's null/undefined exemption. This port's
any-flagged `error` is a gap, not `any` (ADR-0048), and keeps the refusal.

`tests/signature_position_prerequisites.rs` pinned the old decline for
`arrayOrUnknown(uncertain)`. tsgo-pinned answers `"unknown"` in both
strictness modes (overload 1 infers `T := unknown` and `unknown[]` rejects
the argument). Strict now matches. Non-strict still declines, which is a gap,
not a manufactured winner, and the test now says so.

### Result (commit 4)

Against the `90967e42` freeze: types +24 RIGHT, 0 lost; diagnostics +1 case
(`computedPropertyBindingElementDeclarationNoCrash1`), 0 lost. Cases converted:
`inferingFromAny`, `computedPropertyBindingElementDeclarationNoCrash1`.

### How to know this is wrong

A call whose `any`/`unknown` argument meets a target arm this collector lacks
but native reaches with a naked variable, for example a template-literal
target with an `any` hole. The call would then answer the default where
native has the `any` candidate.

### Still declined (the other +101 lines measured above)

They wait on the four rows above. A shape-by-shape certification, like this
one, is the way to take more of them without the losses.

## 5. `getIntersectedSignatures`: non-identical type-parameter lists give no contextual signature (routed from r7-reports)

`getContextualCallSignature` (`checker.go:10305`) keeps the call signatures
that are not arity-smaller than the function. When more than one survives,
`getIntersectedSignatures` (`:10314`) folds them only while
`compareTypeParametersIdentical` holds, which first requires lists of equal
length. Otherwise the answer is nil and the function is uncontextual, so
`noImplicitAny` reports TS7006 on its parameters. In
`contextual_call_signature`, `combine_contextual_overload_signatures`
declined (`None`, unknown) on any generic member, so a generic overload next
to a non-generic one answered "unknown" where native answers "absent". Now
lists of different lengths answer `Absent`. Equal non-empty lists, which
native unifies through `combineUnionOrIntersectionMemberSignatures`, stay a
decline.

Result (commit 5), against the `249f9754` freeze: types +10 RIGHT
(`contextualTypeCaching` 6, `contextualTypingWithGenericAndNonGenericSignature`
4), diagnostics +1 (`contextualTypingWithGenericAndNonGenericSignature`, TS7006
×4), 0 lost. Falsifier: two generic overloads with *different* type-parameter
counts where native still finds a contextual signature. It cannot, since
`compareTypeParametersIdentical` returns false on the length check.

## 6. `getFlowTypeOfDestructuring` at the binding-element sites (with r7-flow)

`getBindingElementTypeFromParentType` narrows the object arm's indexed access
(`checker.go:17743`) and the array-like positional access (`:17771`) through
`getFlowTypeOfDestructuring` (`:17849`), a flow walk of the synthetic
reference `parent["name"]`. r7-flow ported that walk as
`flow.rs::get_flow_type_of_destructuring` (its note §7). This commit wires its
two callers in `destructure.rs`, after the element type's error check. Rest
elements are excluded, as natively (their arms do not call it). The diff is
r7-flow's `r7-flow-destructuring-call-site.diff` unchanged, plus removal of
the entry's `#[allow(dead_code)]`, which the integrator granted. No new cache:
one flow walk per element read, as native's `getFlowTypeOfReference`.

Result (commit 6), against the `e5e0494b` freeze: types +20 RIGHT
(`destructuringTypeGuardFlow` 8, `destructuringControlFlow` 7,
`dependentDestructuredVariables` 3, `narrowingDestructuring` 2), diagnostics
+2, 0 lost. jsTyping's shape (`if (!state.cache) return; const { cache } =
state; cache.orig();`) no longer reports TS18048, matching tsgo-pinned.
