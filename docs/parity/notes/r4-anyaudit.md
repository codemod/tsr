# Lane notes: r4-anyaudit (tsr-2zk.31)

Spurious `any` producers: places where TSR answers `anyType` where tsgo
answers `errorType` or a concrete type. Single-owner box for the contract.
Pinned upstream: `vendor/typescript-go` @ `5b1047d`. Baseline frozen at
`af31cf9` (integration head): types 469,980 RIGHT / 959 GAP / 7,040 WRONG;
diagnostics 4,254 RIGHT / 4,970 EMPTY_RIGHT.

## §1 The contract, and what it lets this lane change

Native has one `errorType` (`checker.go:979`, `TypeFlagsAny`, prints `any`,
`IsTypeAny` true) and one `anyType`. TSR has the same two intrinsics
(`intrinsics.rs`), but its `error` is *also* the "could not compute" gap, and
the dumps print it `error` (ADR-0038). Three consequences decide what this
lane can do:

1. **Native `errorType`, TSR `any`.** Both print `any`; the line is RIGHT.
   Changing the producer to `error` turns it RIGHT → GAP, which the §5 gate
   rejects, and ADR-0038's ceiling explains why the score cannot tell. These
   producers are *recorded*, not changed, until the printing contract changes
   (an ADR superseding 0038). Their effect is on diagnostics only
   (`IsTypeAny`-gated reports), and the lanes that need the distinction keep
   their narrow trust rules (decls §1, implicit-any-widening §3) until then.
2. **Native concrete type, TSR `any`.** These are the WRONG lines and the
   ones this lane fixes: either by computing native's type, or — where the
   `any` stands for a port path that did not compute — by answering `error`
   (WRONG → GAP is the honest direction; it is not a loss).
3. **Native `any`, TSR `any` minted by a *different* rule.** Indistinguishable
   in the baseline; only matters when the minting rule is a "could not
   compute" fallback that also fires where native is concrete (row P1 below).

## §2 The audit

Instrument: `examples/any_audit.rs` with `TSR_ANY_DUMP=1` at the baseline.
Its LOST population (TSR prints `any`, native prints something else) is
**2,231 lines**; **1,457** of them are the writer printing a checker `error`
as `any` (`types_producer.rs`' SS183 / declaration-name arms — not an `any`
producer at all), and **774** are the checker answering `intrinsics.any`.
`WRONG` lines with TSR `any`: 2,216 (verdictdump, same baseline).

The classifier's controls do not read zero at this baseline
(`DISAGREEMENT` 1,942, `UNCLASSIFIED` 980): its syntactic attribution has
drifted from `type_at_location`. Every row below was therefore confirmed by
reading the producer and the case, not taken from the classifier's label.

| # | Producer (function, file) | TSR answers | Native answers | Cases / lines | Status |
|---|---|---|---|---|---|
| P1 | `get_type_for_variable_like_declaration` (`symbols.rs`) falling through when `get_contextually_typed_parameter_type` (`contextual.rs`) returns `None` for an *unresolved* lookup | implicit `any` (or `any[]` for a rest) | the contextual parameter type (`getContextuallyTypedParameterType`, `checker.go:29458`) | 239 direct lines (parameter name / reference) in ~80 cases, plus ~157 member-access lines on the `any` receiver; top cases `tsxStatelessFunctionComponents2`, `contextualTypeFunctionObjectPropertyIntersection`, `jsdocSignatureOnReturnedFunction` | measured, blocked: §4 |
| P2 | `has_no_contextual_type`'s `ReturnStatement` arm (`signatures.rs`) had no get-accessor arm, so a function returned from an unannotated getter could not show its context absent and its parameters gapped / the getter's type answered `any` through the member | `any` / `error` | nil return context → `(a: any, ...b: any[]) => void` (`getContextualReturnType`, `checker.go:29665`) | `privateNameAccessorsCallExpression`, `privateNameStaticAccessorsCallExpression`, `contextualTypingOfAccessors`: 26 lines | **fixed** (§3) |
| P3 | `negated_truthiness_type` (`expressions.rs`) | `error` for an `error` operand (prints `any` through the writer) | `boolean`: `getTypeFacts(errorType)` has both truthy and falsy facts | 6 lines in `reactDefaultPropsInferenceSuccess`, *exposed* by P1 | owner: operators (r4-operators finished); not changed — the operand is TSR's gap, so `boolean` would be a guess for a TSR gap whose native type may be a literal |
| P4 | `get_type_of_alias` (`symbols.rs`, the five `module_specifier_unfindable` / missing-export arms) | `any` when the resolution stack pops cleanly | `errorType` (`resolveExternalModuleName` → `unknownSymbol`) except shorthand ambient modules | RIGHT lines (both print `any`) | §1.1: recorded, not changed |
| P5 | unresolved identifiers on the writer's `any` paths (`types_producer.rs`) | `error`, printed `any` | `errorType` | 4,411 banked lines | faithful (ADR-0039) |
| P6 | `report_circularity_error` (`symbols.rs`) | `any` | `anyType` (`reportCircularityError`, `checker.go:18822`) | — | faithful |
| P7 | `get_type_of_accessors_worker` last arm (`symbols.rs`) | `any` | `anyType` (`checker.go:18545`) | — | faithful |
| P8 | catch variable, shorthand ambient module, non-strict null/undefined widening, `autoType`/`autoArrayType` (`symbols.rs`) | `any` / `any[]` | the same `anyType` | — | faithful |
| P9 | property / element access on an `any` receiver (`members.rs`, `indexed.rs`) | `any` | `any` when the receiver is native `any` | 157 LOST lines | propagation, not a producer: each follows its receiver's producer (mostly P1) |
| P10 | `inferred_return_type` and the return aggregate in `return_type_from_body` (`signatures.rs`, §437) | `any` return for a body whose single return is TSR's `error` | `errorType` aggregate (prints `any`) when native's own return errored; otherwise the computed type | §437 measured 462 GAP→WRONG against 167 gained lines and +29 cases; re-surfaces in §5 (`nestedRecursiveLambda`) | §1.1: kept by its own recorded case calculus; owner: signatures (main) |
| P11 | `parameter_of` (`signatures.rs`) for an annotation that does not resolve | `any` (stand-in for `errorType` at a printing position) | `errorType`, printed through the reused annotation node | — | §1.1: recorded |
| P12 | `type_parameter_of` (`signatures.rs`) for a constraint that does not resolve | `any` constraint | the constraint's type, or `errorType` | — | recorded; not measured |

### Rows that look like `any` producers and are not

- `interfaceExtendsObjectIntersection` (16 lines, `Constructor<I1>()` in an
  `extends` clause answers `any`): the callee `Constructor` resolves to the
  type alias instead of the merged function (the callee line prints
  `Constructor`, native `<T>() => Constructor<T>`). Name resolution, not an
  `any` producer.
- `classPropInitializationInferenceWithElementAccess` and its accessor twin
  (16 lines): §687's `constructor_assignment_types` does not match
  element-access assignments `this["x"] = …`; the `any` is P8's genuine
  implicit any reached because that approximation missed.

## §3 P2: a getter's return context

**Forcing constraint.** `getContextualReturnType` (`checker.go:29665`) reads
`getReturnTypeFromAnnotation` first — for a get accessor that is the getter's
annotation or the paired setter's parameter annotation
(`getAnnotatedAccessorTypeNode`) — then
`getContextualSignatureForFunctionLikeDeclaration`, which is nil for anything
but a function expression, arrow or object-literal method. So a `return` in an
unannotated getter whose setter (if any) is unannotated has no contextual
type. `has_no_contextual_type`'s `ReturnStatement` arm delegated to
`declaration_takes_no_contextual_return`, which has no `GetAccessor` arm and
answered "not shown", so `get #f() { return function (a, ...b) {} }` gapped
the function and printed its parameters through P1's `any`.

**Decision.** The arm asks `getter_takes_no_contextual_return` for a getter
owner. A late-bound (`__computed`) pair is split across symbols in this port
(§523 in `symbols.rs`), so its setter cannot be shown unannotated and the
answer stays "not shown". `declaration_takes_no_contextual_return` itself is
unchanged: it gates return-type *inference* in five places, and widening it is
a separate measurement.

**Measured.** +23 WRONG → RIGHT, +3 GAP → RIGHT, both loss checks empty,
diagnostics unchanged.

**Falsifier.** A getter whose returned function native types from a context
this arm says is absent (a getter inside an object literal with a contextual
type: native still answers nil — the object literal's context reaches
property *values*, not accessor bodies).

## §4 P1: an unresolved contextual parameter is not upstream's nil

`contextual.rs`' module documentation records the original decision: `None`
from the contextual lookup becomes the implicit `any`, because answering
`errorType` would turn RIGHT lines (native also `any`) into gaps. That trade
was taken before `contextual_signature_result` separated an *unresolved*
lookup (outer `None`) from native's computed nil (`ContextualSignature::Absent`).
With that split the faithful rule is expressible: answer the implicit `any`
for native's nil sources only (no contextual type, `Absent`, a too-short
signature), and `errorType` otherwise.

**Measured patch** (`contextually_typed_parameter_is_unresolved` in
`contextual.rs`, consulted by `get_type_for_variable_like_declaration` before
the initializer road), on top of §3: 89 WRONG → GAP, 7 WRONG → RIGHT,
**22 RIGHT → GAP and 8 RIGHT → WRONG**. Not shipped. Each loss is a
contextual path that answers "unresolved" where native's answer is nil or
`any`:

| Loss | Lines | Native's answer | Missing piece (owner) |
|---|---|---|---|
| `nestedRecursiveLambda` | 11 | nil: `void` operand, arrow body of a context-free arrow, `any`-typed argument | `has_no_contextual_type` has no `ArrowFunction`-body arm (`getContextualType` routes it with `ReturnStatement`, `checker.go:29358`) and no `void`/`typeof`/`delete` arm (no dispatch arm → nil) — `signatures.rs` |
| `contextualTypingOfTooShortOverloads` | 3 | `any` via `getContextualCallSignature`'s arity filter over an overload set | overloaded callee contextual signature — `contextual.rs` |
| `contextuallyTypedIife` 200/201 | 2 | nil: argument of an IIFE whose parameter has no type yet (`resolvingSignature`) | `getContextualTypeForArgumentAtIndex`'s resolving-signature arm — `contextual.rs` |
| `signatureCombiningRestParameters2` | 1 | `any` | rest parameter under an intersected signature — `contextual.rs` |
| `dependentDestructuredVariablesFromNestedPatterns` 46, `esDecorators-contextualTypes.2` 71, `variadicTuples2` 252/254, `parsingDeepParenthensizedExpression` (3) | 8 | various | downstream of an `error` parameter |
| `reactDefaultPropsInferenceSuccess` | 6 | `boolean` | P3 |

The patch is kept as `docs/parity/notes/r4-anyaudit-p1.diff`. It ships when
these paths answer native's nil; the declines that exist only because P1
answers `any` go with it:
`implicit_any::retained_return_position_report` (implicit-any-widening §3),
decls §1's written-`any` trust gate for TS2403, and calls-inference §1's
callee-provenance question.

## §5 Two `getContextualType` arms for `has_no_contextual_type` (measured, not shipped)

Both are faithful arms of `getContextualType`'s dispatch (`checker.go:29343`)
missing from `has_no_contextual_type` (`signatures.rs`), and both were
measured on the merged head `512083b` against its own dumps. Patch:
`docs/parity/notes/r4-anyaudit-context-arms.diff` (both arms).

- **`void` / `typeof` / `delete` operand** — no dispatch arm, so nil. Alone:
  0 RIGHT losses, but **4 GAP → WRONG** (`nestedRecursiveLambda` 3/4/20/21):
  the outer arrow of `void (r => (r => r))` now shows no context, its
  inner arrow (the outer's *body*) still cannot, so the inner gaps and the
  outer's return aggregate turns that `error` into `any` through P10. A
  confident wrong where a gap was; not shipped alone.
- **Arrow expression body** (`case ast.KindArrowFunction,
  ast.KindReturnStatement`): the body is a return expression, so the
  `ReturnStatement` arm's question is asked of the arrow. With the `void` arm:
  **+8 GAP → RIGHT, +21 WRONG → RIGHT** (`nestedRecursiveLambda`,
  `fatarrowfunctionsOptionalArgs`, `parseErrorIncorrectReturnToken`,
  `reactReduxLikeDeferredInferenceAllowsAssignment`,
  `asyncArrowFunctionCapturesArguments_es6`), 1 GAP → WRONG
  (`instantiateTemplateTagTypeParameterOnVariableStatement` 3), and
  **16 RIGHT → WRONG in `conditionalTypeDoesntSpinForever`**: the generic
  outer arrow `<SO_FAR>(soFar) => (… ? {} : { name: <TYPE>(…) => x as T })`
  now shows no context, and its inferred return prints the inner method's
  return as `error` (`=> error`) although the inner arrow's own line stays
  RIGHT. The inner signature is rebuilt on the no-contextual-return road
  during the outer's return inference and loses its `as`-typed return there
  — a return-inference defect in `signatures.rs`, not in the arm.

**Next step.** Root-cause the inner-signature rebuild in
`conditionalTypeDoesntSpinForever`; with it fixed, both arms ship together
(+29) and P1's `nestedRecursiveLambda` losses (11 of 30) disappear with them.
