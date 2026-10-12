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
  function, method, accessor or class (`literal_subtree_resolves`; §31 later
  replaced the call/`new`/tagged-template half by reusing the nested answer): native
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

## 7. INFER-NO-CANDIDATE-GUARD: a tuple of incompatible arity supplies nothing

`inferFromObjectTypes` returns at `typesDefinitelyUnrelated`
(`inference.go:715`, tuples at `:1195`) before inferring from any element of
a tuple whose arity rules the tuple parameter out. For
`all<T1, T2, T3>(x: [IPromise<T1>, IPromise<T2>, IPromise<T3>])` called with
a pair, all three parameters therefore keep no candidate, take their
default, and applicability moves on to the two-element overload.
`infer_from_tuple_types` already carried that predicate inline. It is now
`tuple_elements_definitely_unrelated` (with a `TypeId` front end,
`tuple_types_definitely_unrelated`), and `structural_source_supplied`
treats such an argument like §401's null/undefined: no structural source.
Before this, `$q.all([ps, pn])` declined the whole call (`error`).

Result (commit 7), against the `63a147ac` freeze: types +5 RIGHT
(`variadicTuples2` 4, `restTupleElements1` 1), 0 lost; diagnostics
unchanged. Falsifier: a tuple pair the predicate calls unrelated while
native's `TargetTupleType` flags disagree. Optional and rest elements are
the place to look, since `minLength` counts required elements only.

## 8. Held: re-checking literals that contain calls (ARRAY-LITERAL-TUPLE-CONTEXT)

`Promise.all([fa(), fb()])` prints `Promise<A[][]>` where native prints
`Promise<[A[], B[]]>`. §1's boundary skips a literal holding a call, so the
generic tuple overload infers from a check an earlier candidate left in
`node_types` (no tuple context). Native re-checks the literal per candidate
and reuses the nested call through its cached `links.resolvedSignature`.

[`r7-contextual-HELD-literal-call-boundary.diff`](r7-contextual-HELD-literal-call-boundary.diff)
admits such literals. Its eviction stops at a call, `new` or tagged template,
so their answers are reused. Literals holding a function, method, accessor or
class stay excluded. Admitting those too lost `reverseMappedContravariantInference`
(3 lines): a re-checked member's function type is re-derived here, and its
return widening follows the new context.

Measured against the freeze of commit 7 (`1c0a98d9`): types +20
(`mappedTypesGenericTuples2` 10, `correctOrderOfPromiseMethod` 5,
`awaitedType` 4, `deeplyNestedMappedTypes` 1), diagnostics +1
(`correctOrderOfPromiseMethod`), but **−6 types / −1 diagnostics**, so the
diff is not landed:

- `mappedTypesGenericTuples2` 0:20-27: `result` is now native's
  `[string, ...any[]]`, and `result.slice(1)` on that rest tuple answers
  `error`. That is a tuple member gap (Array methods of a variadic-rest
  tuple), not this lane's file.
- `deeplyNestedMappedTypes` TS2322 at (69,5) and (77,5): `Static<typeof
  Output>` changes from one wrong answer (`PropertiesReduce<…, unknown[] & []>`)
  to another (`{}`), which relates, so the TS2322 is lost. The root is the
  deep conditional/mapped evaluation of `PropertiesReduce` (r7-declared).

It lands once either root is fixed and its re-measurement shows no loss.

## 9. `inferToMultipleTypes`' intersection-only union arm (routed from r7-declared)

`inferToMultipleTypes` (`inference.go:448`) on a union target with no naked
type variable among its constituents returns after the per-target pass.
First, though, when every target is an intersection holding the same single
naked variable (`getSingleTypeVariableFromIntersectionTypes`, `:532`), it
infers the whole source to that variable at `NakedTypeVariable` priority
(`:481-490`). That arm is what infers `A | B` for `T` from `A | B` to
`T & (X | Y)`, which normalizes to `T & X | T & Y`. `infer_to_union` lacked
it, so only the per-constituent `T` candidates survived and the common
supertype picked one of them:

```ts
declare function foo<T, U>(obj: T & ({ a: U } | { b: U })): [T, U];
foo(ab /* { a: string } | { b: string } */); // native [{ a: string } | { b: string }, string]
```

Without it, `withRouter(MyComponent)` (`C & (FunctionComponent<P> | ComponentClass<P>)`)
reported a false TS2345. Both shapes appear in `unionAndIntersectionInference3`
once its aliases are unions (r7-declared's held "every union body" slice,
`r7-declared.md` §10.1). On main the aliases are print-only names, so the
commit is corpus-neutral: types 0 / 0 and diagnostics 0 / 0 against the
`5c037c96` freeze, jsTyping unchanged (127, new_false 0, lost_true 0). The
alias-free repro of both shapes now matches tsgo-pinned (no diagnostic). It
is a prerequisite for r7-declared's wider slice, not a gain of its own.

## 10. CONTEXTUAL-BINDING-PATTERN-INITIALIZER: three missing native arms

1. **`getTypeFromArrayBindingPattern`'s rest is only the last element**
   (`checker.go:17961`). A `...a` before the last element is an ordinary
   element (its own grammar error), so `var [...a, x] = [1, 2, 3]` implies
   `[any, any]` as the initializer's contextual type, which makes
   `[1, 2, 3]` a tuple. `array_pattern_implied_type` declined on any spread
   not in the last position.
2. **`getContextualTypeForInitializerExpression`'s pattern fallback**
   (`:29431`) applies to every variable-like declaration, not only to
   `VariableDeclaration`. A parameter with no annotation and no contextual
   parameter type, and a binding element with no contextual type, whose name
   is a non-empty pattern now give their initializer
   `getTypeFromBindingPattern(name, true, false)` too. The three arms share
   `initializer_binding_pattern_context`, and the existing TSR-only guards
   (own-element reference, rest/reference read) carry over unchanged.
3. **`checkArrayLiteral`'s destructuring arm takes a spread at any
   position** (`:8034-8065`, Variadic for an array-like operand, else Rest).
   `[...a, x] = [1, 2, 3]` with `a: [number, number, number]` is
   `[number, number, number, number]`. The arm's exclusion of a non-final
   spread dated from a tuple mint without element flags. It now builds
   through `normalize_variadic_tuple`, which places both kinds.

Result (commit 9), against the commit-8 freeze: types +5 RIGHT
(`restElementMustBeLast` 3, `declarationEmitDestructuring2` 2), 0 lost;
diagnostics unchanged; jsTyping unchanged (127, new_false 0, lost_true 0).
Still open in the cluster: `intraBindingPatternReferences` (a pattern default
that reads a sibling binding), `objectBindingPatternContextuallyTypesArgument`
(an argument contextually typed by the pattern), `literalTypesAndTypeAssertions`
and the optional flags in `declarationEmitDestructuring2` (`{ x?: …; y?: … }`,
`padObjectLiteralType`).

## 11. BINDING-ELEMENT-COMPUTED-NAME-INDEXED-ACCESS: a certain miss is `errorType`

`let {[foo]: bar} = {bar: "bar"}` with `foo: string`: in
`getBindingElementTypeFromParentType`, the computed-name arm indexes the
parent with `getIndexedAccessTypeEx(parent, string, …, name)`
(`checker.go:17741`). A binding element is not an access expression, so
`getPropertyTypeForIndexType` takes its no-signature leg
(`:27100-27130`): it reports TS2537 (already reported here) and answers nil,
and `getIndexedAccessTypeEx` (`:26927`) turns that into `errorType` for its
access node. That is native's any-flagged error, printed `any`, not this
port's gap, so the assignment-pattern literals built from the bound names
(`{ [foo]: any; }`, `[{ [x: string]: any; }]`) now type. The arm answered the
gap. It now answers `native_error` when `computed_key_certainly_unmatched`
certifies the miss: a non-literal `string`/`number` key, and a non-generic
object whose member table is complete and has no index signature. Anything
else keeps the gap.

Result (commit 10): types +21 RIGHT (`computedPropertiesInDestructuring1` 10,
`computedPropertiesInDestructuring1_ES6` 10, `objectRest` 1), 0 lost;
diagnostics unchanged; jsTyping unchanged.

## 12. CONTEXTUAL-ARG-SUPER-CALL: `super(...)` arguments read the instantiated base constructors

In resolveCallExpression's `super` arm (`checker.go:8471`), a super call
resolves against the base constructor type's construct signatures,
instantiated with the `extends` clause's type arguments
(`getInstantiatedConstructorsForTypeArguments`). An `any` super type is
`resolveUntypedCall`, every argument `any`. The argument's contextual type
reads that resolution (`getContextualTypeForArgumentAtIndex`), so
`super(value => String(value<string>()))` under `extends A<number, string>`
types `value` as `number`, and the TS2349 for calling it follows. TSR's
resolving road asked the `super` expression's type for *call* signatures and
found none. `super_call_argument_context` (contextual.rs) now takes the
candidates from `calls.rs::instantiated_constructors_for_type_arguments`
(made `pub(crate)`, granted, visibility only). It reads a single candidate's
parameter, or the overload set through the road's existing arity/agreement
step (factored as `agreed_candidate_argument_type`).

Result (commit 11): types +18 RIGHT (`superCallParameterContextualTyping1` 6,
`…3` 6, `…2` 3, `targetTypeBaseCalls` 3), diagnostics +2
(`targetTypeBaseCalls`, `superCallParameterContextualTyping2`), 0 lost;
jsTyping unchanged.

## 13. ARG-CONTEXT-RESOLVED-SIG: the overload-failure candidate is re-checked too

When no candidate applies, `getCandidateForOverloadFailure` resolves the
call to `inferSignatureInstantiationForOverloadFailure`'s candidate. For a
single generic candidate, that is the instance inferred with
`SkipContextSensitive`. The writer reads literal arguments against that
resolved signature, as for an applicable one: in
`_.all([true, 1, null, 'yes'], _.identity)` (TS2345 on the second argument)
the literal prints `(string | number | true | null)[]`. §1's fresh check and
re-check skipped `check_generic_call_worker`'s overload-failure retry. They
now run there too (the retry's `skip_context_sensitive` still governs
context-sensitive arguments only).

Result (commit 12), against the `2abb9ccd` freeze: types +1 RIGHT
(`genericTypeArgumentInference1` 0:16), 0 lost; diagnostics unchanged;
jsTyping unchanged. `genericArgumentCallSigAssignmentCompat`'s two lines stay
WRONG: the expression-statement form reaches the error candidate through
another road. Not yet traced.

## 14. GET-NON-UNDEFINED-TYPE-GENERIC-CONSTRAINT

`getNonUndefinedType` (`checker.go:31548`) removes `undefined` only after
replacing each instantiable constituent with its base constraint, whenever
some constituent is a generic type whose constraint may be `undefined`
(`isGenericTypeWithUndefinedConstraint`, `:31561`). The destructuring default
strips in `get_binding_element_type` (the annotated root, `:17782`, and the
unannotated union, `:17789`) used `getTypeWithFacts(NEUndefined)` alone, so a
`T extends string | undefined` element kept `T` and its `undefined`. It is
now `get_non_undefined_type`, built on the existing `base_constraint_of_type`.
No cache is added.

Result (commit 13), against the batch-17 merge freeze: types +6 RIGHT
(`destructuringWithConstraint` 2, `strictNullNotNullIndexTypeNoLib` 2,
`strictNullNotNullIndexTypeShouldWork` 2), diagnostics +2 (the latter two),
0 lost.

## 15. ARG-CONTEXT-RESOLVED-SIG: conditional and parenthesized arguments (builder.ts `append`, routed)

jsTyping's builder.ts:1304 calls the overloaded generic
`append<T>(to, value)` with a conditional whose branch is a tuple literal:

```ts
sigs = append(sigs, emitSignature === undefined ? fileId : [fileId, cond ? emptyArray as [] : emitSignature]);
```

Native checks every argument with `checkExpressionWithContextualType` and
the inference context (`checker.go:9485`), so the branch literal's context is
`T | undefined` instantiated through the return mapper. From the assignment
target `ES[]` that is `ES`, which has a tuple constituent, so the literal is
a tuple (traced with an instrumented pinned tsgo: T's value candidate is
`[FileId, string | [] | [string]] | FileId`). `getCommonSupertype` then keeps
`ES`. In TSR the overload walk had already cached the argument from a probe
without that context, so the literal was an array. Its union
`([] | EmitSig | FileId)[] | FileId` is a supertype of `ES`, which made the
inferred T the value's union and caused a false TS2322 on the assignment.

§1's fresh check and re-check (`resolved_literal_argument_recheck`) now also
admit conditional and parenthesized arguments, under the same boundary: not
context-sensitive, and no call, `new`, tagged template, function, method,
accessor or class anywhere below them. The eviction covers the whole argument
subtree (condition included), and a re-check repeats its flow reads; there is
no new table.

Result (commit 14), against the batch-22 freeze (`2e0a7363`): types and
diagnostics 0 / 0, jsTyping unchanged (125, new_false 0, lost_true 0), Ir
1.00000 / 0.99996. The cut-down repro now matches tsgo-pinned. In jsTyping,
builder.ts:1304 still reports, because its condition holds a call
(`isString(emitSignature)`), which the boundary above excludes. That needs §8's
held call boundary.

## 16. Array-element context of an intersection reads `getTypeOfPropertyOfContextualType`

`getContextualTypeForElementExpression` reads a known element position
through `getTypeOfPropertyOfContextualType` (`checker.go:30555`). On an
intersection, that function collects each object constituent's property and
replaces an `any` with `unknown` (`appendContextualPropertyTypeConstituent`,
`:30671`): "`any` doesn't provide any contextual information but could spoil
the overall result". So `const a: [any] & [1] = [1]` keeps `[1]`, and
`any[] & 1[]` gives `1[]`. `contextual_type_for_element_expression` used the
plain property read (`any & 1` is `any`). For an intersection context it now
asks `contextual_property_type` (symbols.rs), the existing port of
`getTypeOfPropertyOfContextualTypeEx`. Other contexts keep their current read.
No new table.

## 17. `inferFromTypes`' default arm for primitive sources, with `typesDefinitelyUnrelated`

`inferFromTypes`' default arm (`inference.go:262-274`) replaces the source by
`getApparentType(source)`. A non-object apparent type restarts inference;
an object apparent type becomes `inferFromObjectTypes`' source. So
`Object.groupBy('a string', c => c)` reads `String`'s `[Symbol.iterator]`
and infers `T := string` (TSR answered `unknown`, with a false TS2322 against
`PropertyKey`). `inferFromObjectTypes` infers from members only when the two
types are possibly related (`typesDefinitelyUnrelated`, `:715` and `:1185`):
`String` against `A[]` (in `direct<A extends string>(a: A | A[])` called with
`"z"`) has unmatched required properties both ways, so nothing is inferred
there, and the naked `A` keeps `"z"`.

Ported:
- the default arm for **primitive** sources;
- the object half of `typesDefinitelyUnrelated` (`object_types_definitely_unrelated`,
  with `getUnmatchedProperty`, `relater.go:984`). It is decided only for two
  plain object types whose property lists this port enumerates completely,
  and is otherwise "possibly related". A generic alias instance is compared
  through its body (`binding_type_alias_body`): its own name list is
  complete but empty, which made `Number` look related to
  `RecArray<T> = Array<…>`. "Complete" also requires every member name to
  be literal or a well-known `Symbol.x`, and no heritage clause
  (`member_names_are_literal`). A late-bound name like `[observable]()`
  with `observable: typeof Symbol.obs` can be missing from the list (this
  port types `typeof Symbol.obs` as `symbol`), which made `symbolProperty61`'s
  class look unrelated to `InteropObservable<T>`.

Because a definitely unrelated object argument infers nothing natively, it
joins §7's tuple exemption from the INFER-NO-CANDIDATE decline. Without
that, `numberSet.intersection([])` (`never[]` against `ReadonlySetLike<U>`)
declined the call where native answers `Set<number>`.

Held: the instantiable and intersection half of the default arm. On the same
base it lost `classVarianceResolveCircularity2` (`new Foo(this)` reads
`Foo<any>`: the apparent type of a class's `this` re-enters the class) and,
as measured in §4, `voidReturnIndexUnionInference` (the mapped receiver's
apparent property is another lane's).

## 18. INFER-NO-CANDIDATE-GUARD: a recursive certifier for "this source supplies no candidate"

The INFER-NO-CANDIDATE decline (`structural_source_supplied`) refuses a call
when a parameter with no candidates sits where an argument could have
supplied one, because this collector's misses are not native's. §4, §7 and
§17 each certified one source kind with no native candidate.
`source_could_supply_candidate` (inference.rs) gathers them and adds the
signature arm. It follows `inferFromTypes`' and `inferFromSignatures`' own
exits:
- a target that does not mention the parameter supplies nothing;
- so does a null/undefined, `any` or `unknown` source (§4);
- so does a pair that is `typesDefinitelyUnrelated`: tuple arities (§7), or
  objects (§17), with a primitive source read through its apparent type;
- two single non-generic signatures with no rest parameters are visited as
  `applyToParameterTypes` does: parameter pairs below the shorter arity,
  the `this` pair only when the source declares a `this` parameter, then the
  return pair. Each pair is certified recursively (depth 4; deeper is
  "could").

Everything else stays "could supply", which keeps the decline. Newly
certified: a callback written with fewer parameters than the target's
mentioning position (`(options?: T) => U` against `() => string`); the
target's `this: This` against a source without `this` (`flatMap`'s `This`);
`number` against `IPromise<U>` in a callback's return (ipromise2's first
`then` overload, after which overload resolution moves on); `1` against `T[]`.

Result (commit 19), against the `a470d2280` freeze: types +42 RIGHT
(`acceptSymbolAsWeakType` 10, `dissallowSymbolAsWeakType` 10, `ipromise4` 7,
`ipromise2` 5, `declarationEmitOverloadedPrivateInference` 4, `arrayFlatMap` 2,
`couldNotSelectGenericOverload` 2, two more), diagnostics +1, 0 lost; exact
+5, 0 lost; jsTyping unchanged; Ir 1.00002 / 0.99983.

Commit 20 adds `inferToTemplateLiteralType`'s exit (`inference.go:566`): a
string literal that a template literal with text does not match infers
nothing (`"hello"` against `` `*${V}*` ``, `templateLiteralTypes3`). For a
string-literal source, `match_template_parts` is the direct port, so its
`None` is a certain non-match.

Commit 21 adds the union-target arm. From one non-union source, every
inference road into a union target (`inferFromMatchingTypes`' pairs, the
lower-priority remainder, `inferToMultipleTypes`) infers into some
constituent. So the source supplies a candidate only if it could for one of
them, with the target read through its alias body. `g([[]] as [void[]])`
against `B<T, U> = [T] | [T, U]` then certifies no candidate for U: `[T]`
does not mention it, and `[T, U]` is arity-unrelated to a one-tuple
(`tupleTypeInference2`).

## 19. Binding elements over an `unknown` parent

A binding element destructuring an `unknown` (a `catch` variable under
`useUnknownInCatchVariables`, or a written `unknown`) is natively:
- in an object pattern, `getIndexedAccessTypeEx(unknown, "x", …, name)`
  (`checker.go:17741`): `getPropertyTypeForIndexType` finds no property on
  the apparent `{}`, reports TS2339 at the name, and the access answers
  `errorType`, printed `any`;
- in an array pattern, `checkIteratedTypeOrElementType` (`:17766`): it
  reports TS2488 and answers `anyType`.

`get_binding_element_type` answered the gap for both. It now answers
`native_error` and `any` respectively; rest elements keep their own arms.
The two reports belong to `nonexistent_property.rs` /
`index_access_reports.rs` (r7-flow) and are not emitted yet for an `unknown`
parent (routed).

## 20. ARG-CONTEXT-RESOLVED-SIG: a concise literal body of an arrow argument

`f(() => [1, "hi", true])` with `f<T>(cb: () => T): T` under
`const [e1, e2, e3] = …`: native caches the arrow's signature return from
its first (inference-pass) check, but the type writer re-reads the return
expression with its contextual type read through the resolved signature
(`() => [number, string, boolean]`), so the literal prints
`[number, string, true]`. `recheck_literal_arguments_in_resolved_signature` now
also re-checks the concise array/object-literal body of an arrow argument
that is not context-sensitive, under §1's boundary. The function node and its
symbol's type are untouched (only the body's `node_types` are evicted), so
the function's type stays its first check's, as natively. Block bodies'
`return` expressions would need `signatures.rs`' private
`return_expressions_of` and are not covered.

## 21. The end-of-inference re-check admits literals holding functions

`withKeyedObj({ b: { data: true, onSuccess: (dataArg) => {…} } })` against
`Results<T> & Errors<E>`: the argument is context-sensitive, so §1 skipped
it. Natively the writer still re-reads `data: true` under the resolved
`T["b"] = boolean` and keeps `true`. §8 refused literals holding functions
because evicting a function re-derived its type (and its return widening)
in the new context, while native keeps a function's first-check type.
`evict_literal_subtree` now stops at arrows and function expressions, so
their `node_types` and their symbols' types survive and only the literal's
own nodes are re-read. With that, the end-of-inference re-check (not the
inference-pass fresh check) admits array/object literal arguments,
context-sensitive or not, whose subtree holds functions but no call, `new`,
tagged template, class, method or accessor
(`resolved_literal_argument_late_recheck`), provided the literal is
context-sensitive. The deferred pass re-checks such a literal under the
inference context before inferring from it, so no later inference reads the
re-check. Two variants were measured and refused:
- admitting non-context-sensitive literals that hold functions: a later
  worker run read the re-checked type from the cache, an exact-oracle loss
  in `computedPropertyNamesContextualType6_ES5/ES6` (T `… | true | …`
  instead of native's `boolean`);
- also fresh-checking those literals in the inference pass: the arrow's first
  check then happened under the inferential context and widened its return
  (`reverseMappedContravariantInference` 0:7-9, 3 lines lost).
Methods and accessors stay out:
`objects.rs` builds their member types through
`get_signature_from_declaration`, not from `node_types`.

Commit 23 extends this to a generic mapped context. `getTypeOfPropertyOfContextualType`'s
generic-mapped arm substitutes the key (`substituteIndexedMappedType`), so
`create([() => 1, [() => ""]])` against `Definition<T> = { [K in keyof T]:
(() => T[K]) | Definition<T[K]> }` gives the inner literal
`(() => T["1"]) | Definition<T["1"]>`, a homomorphic mapped constituent and
so a tuple context (`reverseMappedTupleContext`). The element road answered
`None` for a generic mapped context.

## 22. `contextualTypeHasPattern` for literals with nested patterns and non-primitive defaults

An object literal whose contextual type is a binding pattern's implied
type copies each implied property's optionality (`checkObjectLiteral`,
`checker.go:13253`; §489 ported it syntactically through
`contextual_binding_pattern`). The parameter-default arm gated it with
`binding_default_pattern_context_available`, which refused a pattern
element with a nested pattern name or a non-primitive default, because no
nested pattern context was available for the property value. Since §10, the
initializer contextual road supplies that context (the value `[2, 4, 6, 8]`
of `y: [a, b, c, d] = [1, 2, 3, 4]` already types as a tuple), so the
refusal cost only the optional flags. `function f({x = 10, y: [a, b, c, d] =
[1, 2, 3, 4]} = { x: 10, y: [2, 4, 6, 8] })` now prints
`{ x?: number; y?: [number, number, number, number]; }`. The certifier keeps
its other refusals: a computed key, non-property members, and the array arm.

## 23. A non-const assertion default is context-independent

`binding_default_is_context_independent` gates the binding-pattern
implied type (`getTypeFromBindingElement`, which checks the default with
no context). A default whose type does not depend on the outer contextual
type can be checked through the shared `node_types` memo without poisoning
a later contextual check. `getContextualType` (`checker.go:29368`) gives an
`as`/`<T>` operand the asserted type, and the assertion's type is that type,
so neither depends on the outer context. Only `as const` forwards the outer
context to its operand, so it stays refused. `let { b = "foo" as "foo" } = {
b: "bar" }` now reads the implied `{ b?: "foo" }`: the literal `"bar"` keeps
its literal type under the literal context, and `b : "bar" | "foo"`
(literalTypesAndTypeAssertions). `satisfies` also takes its operand's
context from its own type (`:29396`) but is not admitted: it has no corpus
line and has not been measured.

## 24. Array-literal subtype reduction admits instantiable constituents

`checkArrayLiteral` builds its element union with `UnionReductionSubtype`
(`checker.go:8096`). The port ran `union_with_subtype_reduction` only
when the union held two or more object types (`object_constituent_count`),
because `removeSubtypes` (`:25955`) tests a source only if it is
structured or instantiable. A type-parameter constituent is such a
candidate: `[a, b, c]` with `C extends B extends A` is `A[]` natively, and the
port printed `(A | B | C)[]` (typeArgumentInferenceTransitiveConstraints,
three lines). The gate (`subtype_reduction_may_apply`) now also admits any
intersection or instantiable constituent; the reducer already ports the
type-parameter arms (the union-constraint test included) and answers
`None` → gap when the relation is undecidable. A lone object beside
primitives still skips reduction, as before: primitives are no candidates
without an empty object type. The contextual/literal gate in front of it
(an uncontextual position or a literal-free union) is unchanged.

## 25. The benign-return shortcut does not skip a literal argument's re-read

`check_generic_call_worker` answers a call whose return mentions no
type parameter (`f<T>(x: T): string`) without running inference, unless an
argument is context-sensitive (SS135). Native always resolves the call, and
checkApplicableSignature (`checker.go:9256`) re-checks every argument
against the instantiated candidate. The writer then reads an array or
object literal argument under the inferred parameter type
(`getContextualTypeForArgumentAtIndex`, `:29772`). `all<T>(list: T[], …):
boolean` called with `[true]` infers `T = boolean` (tsgo:
`{ t: boolean }` for a `{ t: T }` return), and `[true]` reads as `true[]`
under `boolean[]`. The shortcut left the uninstantiated `T[]` check's
`boolean[]` in place (genericArgumentCallSigAssignmentCompat 0:15, 0:26).
The shortcut now also declines when an argument passes
`resolved_literal_argument_recheck`, the same certifier and work boundary
§1's re-check uses. For a benign return every decline still answers
`returned`, so the call's own type cannot change. The cost is running
inference for such calls, which the native checker already pays.

Routed, not ported: a self-mention inside a union-bodied alias body
(`type A = "number" | "null" | A[]`) is §29's OBJECT-flagged NAME
placeholder (`alias_placeholders`, declared.rs). `complete_alias_placeholder`
completes it only for a type-literal body, so `A[]`'s element context is
`string | <object A>`, which has no literal constituent, and
`const g: A = ["number"]` widens to `string[]` (recursiveTupleTypeInference
0:5–0:7). Owner: r7-declared.

## 26. INFER-NO-CANDIDATE: inferFromObjectTypes' member walks certify an absence

`source_could_supply_candidate` (§4, §7, §17, §18) certified that a source
supplies no candidate only through `inferFromTypes`' early exits. Between two
plain object types it answered "could", so the no-candidate arm declined
(`error`) where native infers `unknown`: `bar<T>(param: { x?: T }): T` called
with `{}`, and `getNumberIndexValue<T>(map: { [x: number]: T }): T` called with
`{ a: "x" }` or a string enum object (objectLiteralContextualTyping 0:41,
0:42, 0:45; implicitIndexSignatures 0:153, 0:154; tsgo d.ts emit agrees:
`unknown` for all four probes). `object_members_could_supply` ports the
member walks of `inferFromObjectTypes` (`inference.go:714-809`). The target
must be a type literal (no reference, mapped type or alias instance, whose
type-argument roads come first), and both property lists must be complete
(`member_names_are_literal`). A candidate can then only come from:

- a target property whose source namesake exists (`inferFromProperties`,
  recursing on the pair). A name the global `Object` declares is assumed
  present, since `getPropertyOfType` reads it there.
- a signature kind both sides have (`inferFromSignatures`).
- a target index info with any source index info, or a source property
  applicable to its key (`inferFromIndexTypes`). A numeric-literal name
  counts for a `number` key; any property counts for any other key.

`Some(false)` needs at least one enumerated member mentioning the parameter.
Otherwise the mention is on another road and the answer stays undecided.
`member_names_are_literal` now also admits an enum object (`typeof E`,
`TypeData::Anonymous` over a symbol declared only by enum declarations). A
merged namespace would add exports that its declaration walk does not see.

## 27. A binding pattern's implied object is tuple-like through its `"0"` member

`checkArrayLiteral`'s `inTupleContext` (`checker.go:8029`) asks
`isTupleLikeType` of the contextual type, whose second disjunct is
`getPropertyOfType(t, "0") != nil`. The type an object binding pattern implies
(`getTypeFromObjectBindingPattern`, `:17917`) has synthetic properties with no
declarations. This port keeps them in `anonymous_properties` with no symbol
(`binding_pattern_object`), so `get_property_of_type` answered none and
`var { 0: a, 1: b } = [10, "hello"]` widened the literal to
`(string | number)[]`. Native gives `[number, string]`, and `a : number`,
`b : string` follow (declarationsAndAssignments 0:210).
`binding_pattern_object_has_index_zero` reads that table for a symbol-less
`"0"` property of a member-less named object, which is exactly
`binding_pattern_object`'s shape. It is consulted beside `is_tuple_like_type`
in `array_literal_has_a_tuple_contextual_type` only. Generalizing the lookup
in `get_property_of_type` (members.rs) would serve every reader, but that file
is not this lane's.

## 28. An array-literal default supplies context to a nested-pattern element

`binding_default_pattern_context_available`'s array arm (the twin of §22's
object arm) refused a default whose matching pattern element is itself a
pattern, because no per-element pattern context was available for the
literal's element. Since §10 that context arrives through
`initializer_binding_pattern_context` on the BindingElement arm. In
`var [a, [b, { x, y: c }] = ["abc", { x: 10, y: false }]] = [1, ["hello", { x: 5, y: true }]]`
the default types as `[string, { x: number; y: boolean; }]` under its
`[any, { x: any; y: any; }]` pattern context. The outer literal's
`{ x: 5, y: true }` then keeps `true` under `y: boolean`
(isLiteralOfContextualType), as native does (declarationsAndAssignments
0:228–0:235). The arm still refuses a pattern element with its own
initializer, and any rest element. tsgo's d.ts emit agrees with the port on
five probes (`[a, {x, y}] = [1, {x: 1, y: true}]`, a nested array pattern, an
object root holding an array pattern, an excess member, and a short tuple).

## 29. An empty tuple spreads `never`

`checkSpreadExpression` reads `checkIteratedTypeOrElementType(Spread, …)`,
and a tuple iterates through its base type (`getTupleBaseType`:
`Array<union of element types>`). For `[]` that is `Array<never>`, so
`[x, y, z, ...a0] = v` with `a0: []` records `...a0 : never`
(declarationsAndAssignments 0:486, 0:535). `array_spread_element_type` fell
to `for_of_element_type`, which treats the empty yield list of an empty
tuple as an absent yield and answers `any`; the spread line printed `error`.
`array_spread_element_type` now answers `never` for an empty fixed tuple
before that road. Routed: the for-of reader (`for_of_yield_types`,
symbols.rs) still gives `for (const q of a0)` `q : any`, where native gives
`never`.

## 30. Phase 1 checks every object/array literal argument under the inference context

`inferTypeArguments` checks each ordinary argument with
`checkExpressionWithContextualType(arg, paramType, context)`
(`checker.go:9485`, `:7486`). The argument's contextual type is the
uninstantiated parameter, mapped only by the return mapper
(`instantiateContextualType`), and its apparent type is the constraint. The
port marked the active context `inferential` in phase 1 only for a §1
fresh-literal argument or under a return-literal map. Any other literal read
its context through the stateless argument road, which fixes the call's type
parameters to `unknown` first. `test2<T extends Test<"bar">>(arg: T)` with
`{ foo: true, bar() {} }` read `unknown` (probed), where native reads
`Test<"bar">`, whose `foo` is `true` (inferenceAndSelfReferentialConstraint:
native `T = { foo: true; … }`). Object and array literal arguments whose subtree holds no
function, method, accessor or class now read the active context in phase 1
(`literal_subtree_holds_function`). Function and conditional arguments keep
the return-literal-map gate.

The function boundary is §8's work boundary, measured again here. Admitting
every literal (first run, against b55) gained 49 lines but lost 3:
`reverseMappedContravariantInference` 0:7–0:9. Under the active context, the
member `(v: string) => false` gets the generic contextual signature
`(val: T["foo"]) => boolean`, and the port's return widening
(`inferred_return_type`, signatures.rs) then widens to `boolean`. Native keeps
`false` because `boolean` is a literal context. With the boundary (verdicts
on the same tree): deeplyNestedMappedTypes 12→1, genericRestParameters2 9→1,
genericRestParameters1 27→23, trailingCommasInFunctionParametersAndArguments
1→0 non-RIGHT lines, and reverseMappedContravariantInference stays 0. The
boundary gives up intraExpressionInferences (26→17 without it) and
jsxChildrenGenericContextualTypes (10→7), whose literals hold functions.
Those return once the function-member return widening follows a generic
contextual signature's return type (routed with §25's signatures.rs items).

That case still needs the mapped-type member resolution routed in the report:
the port does not resolve `{ [P in "bar" | "foo"]: P extends "foo" ? true :
() => any }` into members at all.

## 31. The held literal-call boundary (§8) lands: nested calls are reused, not excluded

§8 held this diff (`r7-contextual-HELD-literal-call-boundary.diff`) at
+20 / −6 types, −1 diagnostics. It is rebased here onto the current
boundary helpers. §1's re-check
(`resolved_literal_argument_recheck`) now admits a literal holding a call,
`new` or tagged template. `literal_subtree_holds_function_outside_calls`
refuses only a function, method, accessor or class outside such a call.
`evict_literal_subtree` stops at the nested call, so its answer is reused, as
native's per-candidate re-check reuses the call's cached
`links.resolvedSignature`. `Promise.all([fa(), fb()])` now keeps
`Promise<[number[], string[]]>` (probed). `literal_subtree_resolves` had no
remaining caller and is removed.

The first gate (against b57) gained 27 types (mappedTypesGenericTuples2 10,
awaitedType 8, correctOrderOfPromiseMethod 5, awaitedTypeStrictNull 4), plus
1 diagnostics case, with no corpus loss. §8's two blockers no longer reproduce.
The exact oracle, which checks diagnostics before the type walk, lost
genericContextualTypes2 and 3: `entry` and its `assign(…)` printed `error`.
`createMachine<{ count: number }>({ context, entry: assign({…}) })` writes its
type argument, but phase 1's argument loop runs before the written-arguments
arm. Its literal now passed the boundary, so the loop marked the active
context `inferential`. The literal then read the uninstantiated
`Config<TContext>`, and the nested `assign` call cached its answer under that
context. Native does no inference with written type arguments: the argument
is checked against the instantiated signature (`checkApplicableSignature`).
Phase 1 now never marks the context `inferential` when the call writes type
arguments (§30's object/array arm included). Instead it publishes the
written-argument instance (`written_type_argument_map`, the
`fillMissingTypeArguments` fill the written arm already used, now shared)
through `call_inference_signatures` while such an argument is checked. A
first cut that only dropped the `inferential` mark sent those literals to the
stateless argument road, and the exact oracle lost tupleTypeInference:
`$q.all<string, number>([$q.when<string>(), $q.when<number>()])` lost its
`[IPromise<string>, IPromise<number>]` tuple context. With the instance
published, genericContextualTypes2 and 3 are EXACT and tupleTypeInference is
RIGHT under the oracle runner, no `Inference`-named case changes EXACT
status, and the four gains above still hold on the verdict dump.

A call with a spread argument keeps the previous `inferential` mark and
publishes no instance. checkApplicableSignature reads such a call's
non-array rest through `getSpreadArgumentType`, an indexed access into the
rest type per element, which the instance's positional context does not
port. The gate measured that boundary: publishing the instance there lost
circularInlineMappedGenericTupleTypeNoCrash 0:17–0:18.
`new Foo<[...Elements, "abc"]>(...this.elements, { bar: "abc" })` read
`{ bar: "abc" }` where native prints `{ bar: string }`. With the boundary it is
back to its b57 state (one non-RIGHT line, unchanged).

## 32. A unique-symbol member of an object literal reads its context by the escaped name, then by name type

`getContextualTypeForObjectLiteralElement` (`checker.go:29924`) looks the
member up by its symbol's name. For a late-bound unique-symbol key that name
is `__@A@<id>` (`getPropertyNameFromType`), and the lookup carries the key as
`nameType`. The port spells the literal's member `[A]`
(`late_bound_symbol_member_name`), but a resolved mapped type over
unique-symbol keys lists `__@A@<id>`, so `{ [A]: ap => … }` against
`{ [K in Action["type"]]: (p: K) => void }` (resolved) found no member, and `ap`
was `any`. The lookup now takes the escaped name
(`unique_symbol_property_name`) when the context lists only that spelling.
When no member exists at all, it falls back to the index infos read with the
name type, as `getTypeFromIndexInfosOfContextualType` (`:30646`) does:
`const x: { [sym: symbol]: (p: string) => void } = { [A]: s => s.length }` now
types `s : string`. contextuallyTypedSymbolNamedProperties: 20 → 3 non-RIGHT
lines on the verdict dump. The last three wait on mapped.rs:
`generic_mapped_contextual_property_type_of_key` reads the generic
`{ [K in T["type"]]: … }` with the constraint `T["type"]` unreduced
(`contextual_mapped_key_base_constraint`). Native's `getBaseConstraintOrType`
gives `string | symbol`, which admits the key, so during inference the
`[A]` arrow gets no contextual signature and caches `error`.

## 33. A conditional contextual type is apparent through its default constraint

`getApparentTypeOfContextualType` maps the instantiated contextual type
through `getApparentType`, which takes an instantiable type's base
constraint. For a conditional, that is `computeBaseConstraint`'s
conditional arm: the base of `getConstraintFromConditionalType`, i.e. the
distributive constraint when the check type is a type parameter, and
otherwise `getDefaultConstraintOfConditionalType`, the union of the two
branches (`checker.go:17263`). `base_constraint_of_type` (constraints.rs)
declines for such a conditional, and `apparent_contextual_type` answered
`unknown`. `bad<P extends Props>(attrs: string extends keyof P ? { [K in keyof P]: P[K] } : …)`
called with `{ when: value => false }` had no member context, so the arrow
answered `error`. `apparent_contextual_type` now falls back to
`default_constraint_of_conditional_type`, then the apparent type of that
union, for a conditional whose check type is not a type parameter. A
distributive conditional keeps `unknown`: its distributive constraint is not
ported here. So does a default constraint that is itself conditional, so the
fallback cannot recurse. conditionalTypeContextualTypeSimplificationsSuceeds
8 → 0 and complicatedIndexesOfIntersectionsAreInferencable 7 → 0 non-RIGHT
lines on the verdict dump.

## 34. An object binding pattern supplies the return mapper, and an annotated arrow default is context-independent

`inferTypeArguments` (`checker.go:9405`) takes a contextual type that comes
from a binding pattern into `context.returnMapper` only (isFromBindingPattern),
which `instantiateContextualType` reads for a context-sensitive argument with
no inferences yet. `binding_pattern_return_context` computed that slice for
array patterns of plain identifiers only. An object pattern now answers its
implied type through `initializer_binding_pattern_context`, the road the
initializer's own contextual read already takes. The array slice keeps its
hand-built tuple.

The implied type itself needed one more certified default.
`getTypeFromBindingElement` checks a default with the `unknown` contextual
type. An arrow that is not context-sensitive (every parameter annotated, or
type parameters, and no context-sensitive concise body) reads no contextual
signature, and its type is its symbol's in both checks. So
`binding_default_is_context_independent` admits it. A function expression
without an explicit `this` parameter is context-sensitive natively
(hasContextSensitiveParameters) and stays refused. A concise body that is
itself a function, literal, conditional, binary, parenthesized or JSX form
is refused unless a return type is written, since such a body may be
context-sensitive. `const { f = (x: string) => x.length } = id({ f: x => x.charAt })`
now types `x : string`, and `f` is
`((x: string) => number) | ((x: string) => (pos: number) => string)`
(objectBindingPatternContextuallyTypesArgument 9 → 0 non-RIGHT lines).

## 35. A for-of head's binding element default copies its pattern's optionality

`getContextualTypeForInitializerExpression` (`checker.go`) gives a binding
element's default its parent's projected type
(`getContextualTypeForBindingElement`). That is the root's annotation, else
its initializer's type (`checkDeclarationInitializer`). Only with neither does
it fall back to `getTypeFromBindingPattern`, whose optional members the
literal copies (`contextualTypeHasPattern`). `contextual_binding_pattern`'s
BindingElement arm certified only a parameter root. A variable root with no
annotation and no initializer, as in `for (let { skills: { primary: p = "a" }
= { primary: "x" } } of arr)`, now also yields the element's pattern, so the
default prints `{ primary?: string; }`
(sourceMapValidationDestructuringForOfObjectBindingPatternDefaultValues 6 → 0
non-RIGHT lines). A JS root keeps the old answer, since its JSDoc `@type` is an
annotation. A root with an initializer keeps no `?`, as native does
(`declarationsAndAssignments`' `{ x: 10, y: false } : { x: number; y: boolean; }`).
Not fixed here: such a default under an initialized root
(`const { skills: { primary: p = "a" } = { primary: "x" } } = obj`) prints
`{ primary: "x"; }`, where the projected `obj.skills` context should widen it.

## 36. Const type parameters: per-constituent mutability, and the resolved re-read is no const context

Two `isConstContext`/`checkArrayLiteral` details for `const` type parameters
(typeParameterConstModifiers 3 → 0 non-RIGHT lines on the verdict dump, no
loss among the verdict dump's `*onst*` cases):

- `checkArrayLiteral`'s readonly test is `someType(contextualType,
  isMutableArrayLikeType)` over the *apparent* contextual type
  (`checker.go:8087`). For a type parameter that is its constraint, tested per
  union constituent. `const_context_is_mutable_array_like` resolved the
  parameter to its union constraint but then tested the union whole, so
  `<const T extends string[] | readonly number[]>` made `["hello", "world"]`
  readonly. That candidate failed the constraint and `T` fell back to it. A
  union base now recurses per constituent, so `string[]` keeps the tuple
  mutable and `fd2(["hello", "world"])` is `["hello", "world"]`.
- `literal_has_const_contextual_target` read the uninstantiated parameter
  always. That is right during inference (`instantiateContextualType` without
  ContextFlagsSignature leaves `T`), but §1's re-read under the resolved
  signature sees the instantiated parameter natively. Under `readonly
  [unknown, 2, 3, 4, unknown]` there is no const type variable, so
  `inners2([1,2,3,4,5])` prints `[number, 2, 3, 4, number]`. When the call's
  `call_inference_signatures` slot holds an instance (type parameters
  cleared, as §1 and §31 publish), the target is read instantiated.

Routed while triaging: §142's literal-self `this` mint (expressions.rs)
prints `this` where native prints the widened literal type
(thisTypeInObjectLiterals, this_inside-object-literal-getters-and-setters).
An interface extending a tuple alias (`interface I extends EmptyTuple`,
`type EmptyTuple = []`) inherits no tuple members (`m.length` is `error`), so
it is not tuple-like (contextualTypeWithTuple 0:95). An inline conditional
parameter with function branches instantiates to `error` (`h2<T>(x: T, y: T
extends E ? (e: 1) => void : (e: 2) => void)`).

## 37. A resolved re-read skips the pre-inference tuple-context read

`array_literal_has_a_tuple_contextual_type` has a pre-inference fallback.
Native's first inference pass reads the written parameter, which
`instantiateContextualType` maps only through the return mapper, and whose
apparent type is the constraint, so `all<T extends readonly unknown[] |
[]>([a, b])` is a tuple context through `[]`. That fallback ran during §1's
re-read under the resolved signature too, where it re-read the written `T`
with no live return mapper and found the constraint's `[]`.
`const f1: F = () => Promise.all([{…}, {…}])` with `F = () => Promise<Player[]>`
infers `T = Player[]` (the return mapper) natively, and the writer reads the
literal under the instantiated `(… | …)[]` as an array. The port's last read
made it a tuple (inferFromGenericFunctionReturnTypes3 0:206). Like §36, the
fallback no longer runs when the call's `call_inference_signatures` slot
holds a resolved instance (type parameters cleared). `Promise.all([fa(),
fb()])` keeps its tuple (its inferred `T` is the tuple itself), and the §31
gains hold on the verdict dump.
