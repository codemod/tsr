# Lane notes: r7-flow (tsr-2zk.1274)

Round-7 lane box. Owned: `flow.rs`, `nonexistent_property.rs`,
`index_access_reports.rs`, `readonly_target.rs`, `truthiness.rs`,
`this_expression.rs`, and the parse-error gates outside `calls.rs`. Native is
`vendor/typescript-go` @ `5b1047d`. Base: origin/main `9020aa67`, frozen
unfiltered:

- types 556,357 aligned lines: 551,176 RIGHT / 673 GAP / 4,508 WRONG;
- diagnostics 12,238 cases: 5,741 RIGHT / 5,610 EMPTY_RIGHT / 852 WRONG /
  35 EMPTY_WRONG.

## §1 `getExplicitTypeOfSymbol`'s for-of arm, and the access-site lift (`tsr-2zk.1264`)

**Forcing constraint.** r6-parsegate's access-site lift
(`r6-parsegate-4-lift-access.diff`, landed as `9b9180c1`, reverted in
`3f8e0868`) failed
`semantic_parse_error_gates::recovery_misses_require_checked_value_roles_and_complete_receiver_ownership`:

```ts
function untyped(values: Base[]) {
  for (let receiver of values) { receiver.assertDerived(); receiver.z; }
}
```

reported a false TS2339 on `receiver.z`. The receiver was never narrowed to
`Derived`, in any file; the parse-error gate in `check_nonexistent_property`
had only hidden the miss in files with parse errors, and the gate itself
had grown a stand-in decline ("`getExplicitTypeOfSymbol` … unsupported
for-of/mapped origins cannot certify a miss").

**Native.** `getTypeOfDottedName` types an assertion call's callee without
flow (`flow.go:2122`). For an identifier it asks `getExplicitTypeOfSymbol`
(`flow.go:2155`), which for a variable declared by a `for..of` head with no
annotation types the statement's expression by `getTypeOfDottedName` again
and answers `checkIteratedTypeOrElementType(use, t, undefinedType, nil)`
(`flow.go:2176-2187`). The port had every arm but that one.

**Port.** `explicit_for_of_iterated_type` (flow.rs): `any` input answers
itself, `never` answers `any` (no yield type), otherwise the iteration
engine's yield type, `any` when absent. The nil error node reports nothing.

Judgment calls:

- **Array-like road declined.** Without a global `Iterable` native takes
  `getIteratedTypeOrElementType`'s array-like road (`checker.go:6141`). The
  port answers `None` there, as `for_of_iterated_type` does; that is the
  previous behaviour (no explicit type, no effects signature). An ES5 corpus
  case asserting through a for-of variable would show it as a missing
  narrowing. Not measured as a loss (none on the base).
- **`resolvingExplicitTypeOfSymbol` is a call-stack set, not a Checker
  field.** The for-of arm recurses (`for (const x of x)` would not
  terminate). Native's set lives on the Checker (`flow.go:2157`). Here it is
  a `Vec<SymbolId>` threaded through `get_type_of_dotted_name_in` and
  `get_explicit_type_of_symbol`, owned by the outermost query and empty on
  return. It covers every cycle the for-of arm introduces. It does not cover
  a re-entry through `getTypeOfSymbol` (a function whose return inference
  walks reachability into an effects query on itself): native answers nil
  there, the port the in-progress type, unchanged from before any guard
  existed. Would be wrong if a corpus case shows a self-referential effects
  query diverging; the fix is the Checker field (checker.rs, integrator).
- **Checker port convention.** No cache, side table, mapper or member image.
  The traversal is native's own dotted-name walk, bounded by the dotted name's
  length and the resolving set; each step is a memoized name resolution, a
  property lookup, or one iteration-types query (uncached here, as
  `get_iteration_types_of_iterable` is everywhere in the port).

**The lift.** With the arm ported, `9b9180c1` re-applies unchanged except a
stale comment ("retain its existing parse-error decline") removed with the
gate it described. Native `checkPropertyAccessExpressionOrQualifiedName`,
`checkElementAccessExpression` and the other 24 sites report with ungated
`c.error` (r6-parsegate §4, diff 4).

**Measured** on `9020aa67`, unfiltered: diagnostics +5, 0 lost
(`compiler/extension`, `identifierStartAfterNumericLiteral`, `libMembers`,
`conformance/objectSpreadNegativeParse`, `parserRealSource7`); types +6
WRONG→RIGHT, 0 lost (`assertionTypePredicates1` 320-322, 392-394: the
for-of arm itself, in a parse-clean file).

Still missing (outside this lane): TS2775 ("Assertions require every name in
the call target to be declared with an explicit type annotation") on
`for (const r of inferred) r.assertDerived()`; native reports it from
`checkCallExpression` when `getEffectsSignature` is nil (`checker.go:8353-8359`,
with TS2776 for a non-dotted callee). Test:
`crates/tsr-conformance/tests/r7_flow_explicit_for_of.rs`.

## §2 The declaration/relation parse-error lift (`tsr-2zk.1264`, r6-parsegate diff 5)

r6-parsegate's WIP diff 5 (`r6-parsegate-5-WIP-lift-declarations-relations.diff`)
applies unchanged on `bc17c1f8`: 58 gate lines in `check.rs` (40),
`iteration.rs` (5), `expressions.rs` (2), `jsdoc_annotations.rs` (2),
`destructuring_assignment.rs` (2), `flow.rs`, `comparison_overlap.rs`,
`enum_member_name.rs`, `heritage_conformance.rs`, `meaning_mismatch.rs`,
`parameter_self_reference.rs` and `satisfies.rs`. r6-parsegate §2 classified
every one as NO-NATIVE-GATE: the native counterpart reports with an ungated
`c.error`/`addDiagnostic`. Its §4 lists them with their lines at `7dba1e1`.

What the WIP lacked, now measured on `bc17c1f8`, unfiltered:

- diagnostics **+7**, 0 lost: r6-parsegate's six (`aliasErrors`,
  `functionsMissingReturnStatementsAndExpressions(target=es2015)`,
  `objectLiteralWithSemicolons5`,
  `labeledStatementDeclarationListInLoopNoCrash3(target=es2015)`,
  `parserErrorRecovery_Block3`, `parserMemberAccessorDeclaration8(target=es2015)`)
  plus `parserUnfinishedTypeNameBeforeKeyword1`;
- types byte-identical in verdicts (551,182 RIGHT both sides);
- slowcases clean on both dumps (0 missing, nothing over budget or 3×).

Comment cleanup the WIP left open: `Checker::file_has_parse_errors`' doc now
says it mirrors `hasParseDiagnostics` and is read only at native-gated sites;
`jsdoc_annotations.rs`' "declines outright" comment and `check.rs`' orphaned
TS2695 doc block (which claimed the parse-error gate excluded the TS2657
class; `check_comma_left` ports `isInDiag2657` itself) are corrected, the
latter moved onto `check_comma_left`.

Remaining `file_has_parse_errors` reads after this commit: 71 code lines in
14 files (plus the field and its initializer), each a NATIVE-GATE site of
r6-parsegate §2 (grammar helpers and explicit tests), or `unused.rs`'
node-level stand-in for `NodeFlagsThisNodeOrAnySubNodesHasError`. `calls.rs`
has none left.

## §3 `narrowTypeByInstanceof` over `getInstanceType` for non-class callees (r6-errorsplit3 diff I)

Native narrows `x instanceof C` by `mapType(rightType, getInstanceType)`
(`flow.go:836`); `getInstanceType` (`flow.go:964-976`) reads `prototype`, then
the union of the construct signatures' erased returns. The port took its
class-identity road for every anonymous callee and read construct signatures
only off named types (`signature_candidates_of_named_type`), so a type
literal `{ new (): D }` or an intersection callee declined. r6-errorsplit3's
diff I (`r6-errorsplit3-instanceof-general-road.diff`) keeps the class road
for class symbols only and reads signatures through `signatures_of_type_kind`
(unions, intersections, type literals). Applied unchanged.

Measured on `216195f6`, unfiltered: types **+15**, 0 lost
(`typeGuardsWithInstanceOfByConstructorSignature` 8,
`inKeywordAndIntersection` 4, `narrowByInstanceof` 2, `controlFlowInstanceof` 1);
diagnostics **+1**, 0 lost (`controlFlowInstanceof`). No cache or traversal
added: the same signature reads the named-type road already made.

The rest of the instanceof arm is still not native's shape: the
`reference_is_top_level_var` false-branch decline (§126) and the
class-identity road are port constructs layered over `getNarrowedType`. A
rewrite onto `narrowed_type_worker` alone is NARROW-INSTANCEOF-CONSTRUCT-SIGNATURES'
remaining work (§ below when measured).

## §4 A never-reduced intersection leaves the discriminant's union property (r6-errorsplit3 diff N)

`getPropertyOfType` reads `getReducedApparentType` (`checker.go`
`getReducedType`, `:21819`): a union constituent that is an intersection
whose discriminants conflict (`a & b` from distributing `(a | b | c) & (b | c)`)
reduces to `never` and drops out, so `createUnionOrIntersectionProperty`
never sees it. The port's `union_property_type_for_discriminant` declined on
it. Diff N (`r6-errorsplit3-never-discriminant-constituent.diff`) skips such a
constituent through the shared `intersection_has_never_discriminant`
(`isDiscriminantWithNeverType`). Applied unchanged.

Measured on `0633293e`, unfiltered: types **+12** (`discriminatedUnionTypes2`),
diagnostics unchanged, 0 lost. No cache or traversal: one existing predicate
per constituent of a discriminant read.

**Held: diff R (`isReadonlySymbol` on property signatures).** Measured on
`0633293e`: types +16 (`controlFlowAliasing`) against **2 lost**,
`mappedTypes6` 277/279 (`x5.b` where `x5: Readwrite<Bar>`, `Bar`'s `b` is
`readonly`), RIGHT `number` → WRONG `any`. Cause: a mapped type's member is
answered by its modifiers-type symbol (`b` of `Bar`), so widening
`is_readonly_symbol` to property signatures makes the `-readonly` member
read as readonly at members.rs' `isAssignmentToReadonlyEntity` arm
(`is_readonly_property_of_type`). Native's mapped symbol has no value
declaration and carries `CheckFlagsReadonly` from the mapped modifiers
(`checker.go:20930-20937`). Reopens when `is_readonly_property_of_type`
reads the mapped member image's `readonly` (members.rs, r7-shared) — the
same root as the routed `Mutable<A>|Mutable<B>` TS2540 (§5).

## §5 `getTypeOfDottedName`'s private-identifier arm (routed from r7-calls)

`getTypeOfDottedName` (`flow.go:2137`) looks a private name up as
`getPropertyOfType(t, GetSymbolNameForPrivateIdentifier(t.symbol, name))`;
the port declined every private name, so `this.#p1(v)` had no effects
signature (no narrowing, and r7-calls' TS2775 port reported a false TS2775).
This port keys private members by text (`crate::private_name_identity`), so
`private_property_of_dotted_type` takes the text lookup restricted to a
member declared by `t`'s own class symbol — what the mangled name finds.
`t` without a symbol answers nothing, as natively.

Measured on `e7ca1519`: types **+4** (`privateNamesAssertion` es2022/esnext,
2 each), diagnostics unchanged, 0 lost.

The other two TS2775 gaps r7-calls named:

- `assertionTypePredicates1` 150:9/192:9 (`for (let item of items)`) were
  already closed by §1's for-of arm on this branch; with r7-calls'
  `r7-calls-assertion-target.diff` applied on `e7ca1519`, `diagcase` shows the
  case's 16 diagnostics identical to native.
- `requireAssertsFromTypescript` (`const { art } = require('./ex')`) is a
  binder gap, not flow: native binds a binding element of a JS
  require-initialized declaration as an **Alias**
  (`bindVariableDeclarationOrBindingElement`, `binder.go:1168`, through
  `IsVariableDeclarationInitializedToRequire`, `ast/utilities.go:2825`, which
  climbs a binding element to its declaration). TSR's
  `is_require_alias_variable` (`tsr-binder/src/binder.rs`) admits only a
  `VariableDeclaration` with an identifier name, so `art` is a plain
  variable without an annotation and `getExplicitTypeOfSymbol` answers
  nothing. Needs the binder arm plus the checker's alias target for it
  (`getTargetOfAliasDeclaration`'s binding-element arm, `symbols.rs`
  `declaration_of_alias_symbol` lists it as unported). r7-shared's files.

The `Mutable<A>|Mutable<B>` TS2540 (r7-declared's
`r7-declared-readonly-union-mapped-member.diff`) is the same mapped-symbol
root as held diff R (§4).

## §6 A union's readonly property reads each constituent's mapped member (routed from r7-declared)

`createUnionOrIntersectionProperty` (`checker.go:21452`) sets the union
property's `CheckFlagsReadonly` when any constituent's property is readonly
by `isReadonlySymbol`. A mapped constituent's property is the transient
mapped symbol whose `CheckFlagsReadonly` comes from the mapping's modifiers
(`resolveMappedTypeMembers`, `checker.go:20930-20937`). The port's union arm
of `is_assignment_to_readonly_property` asked the modifiers type's symbol
instead, so `Mutable<A> | Mutable<B>` lost its `-readonly` (a false TS2540,
8 in jsTyping) and `Readonly<A> | A` lost its `+readonly` on a member `A`
declares writable (a missed TS2540). r7-declared's
`r7-declared-readonly-union-mapped-member.diff` adds
`constituent_property_is_readonly`, which reads `mapped_identity_optionality`
and the member image first, as the non-union tail already did. Applied
unchanged.

Measured on `533e29b6`: corpus neutral (types and diagnostics verdicts
unchanged, 0 lost). The probe in
`tests/r7_flow_readonly_union_mapped.rs` matches native exactly; the base
reported the false `m.flags` TS2540 and missed `r.kind`/`mr.kind`. No cache:
one image lookup per union constituent at an assignment target, the image
resolved on first read as the non-union tail already does.

## §7 `getFlowTypeOfDestructuring`'s synthetic reference (requested by r7-contextual)

`getBindingElementTypeFromParentType` (`checker.go:17743`, `:17771`) and the
destructuring-assignment checks (`:12612`, `:12678`) pass a destructured
name's indexed-access type through `getFlowTypeOfDestructuring`
(`checker.go:17849`). That walks a synthetic `parentAccess["name"]`
(`getSyntheticElementAccess`, `:17857`; `getParentElementAccess`, `:17882`;
`getDestructuringPropertyName`, `flow.go:1792`) from the parent access's flow
node, so `if (!state.cache) return; const { cache } = state; cache.x` reads
`cache` narrowed.

**Representation.** Native builds a fresh element-access node. This tree is
immutable (ADR-0012), so `FlowState` gains `synthetic: Option<SyntheticAccess>`:
the reference is its root (`FlowState::reference`, the real parent access,
whose flow node starts the walk) plus the accessed names, innermost first.
Every reader of `state.reference` was audited:

- `is_matching_reference` → `synthetic_reference_matches`: native's access
  arm (same `getAccessedPropertyName`, matching receiver), down to the
  root's own `isMatchingReference`;
- `contains_matching_reference`: the parts are the shorter synthetic
  prefixes, the root, then the root's own parts;
- the `in`/`hasOwnProperty` missing-type arms read the receiver and name
  through `reference_receiver_name`;
- the START arm treats it as the access it is (no creation-site edge);
- `isConstantReference` answers false (a synthetic access has no resolved
  symbol);
- loop-cache keys use the destructuring node's id, the fresh node's
  stand-in;
- the tail converts only the unreachable sentinel: the synthetic access's
  parent is the destructuring node, never `x!`.

**Checker port boundary.** No cache or side table; one walk per call as
native's `getFlowTypeOfReference`, sharing `flow_loop_cache` under the
destructuring node's identity. Nested patterns chain without a walk of their
own.

**Measured.** The entry point lands unused (`#[allow(dead_code)]`):
corpus-neutral by construction. Its callers are r7-contextual's.
`r7-flow-destructuring-call-site.diff` wires the binding-element site
(object arm and array-like positional arm): on `4a4b5b5b` that measured
types **+20** (`destructuringTypeGuardFlow` 8, `destructuringControlFlow` 7,
`dependentDestructuredVariables` 3, `narrowingDestructuring` 2) and
diagnostics **+2** (`destructuringTypeGuardFlow` EMPTY_WRONG→EMPTY_RIGHT,
`dependentDestructuredVariables`), 0 lost. A probe of the three shapes
(object, nested object, array under an object) matches native.

## §8 `autoType` identity for the implicit-any test (requested by r7-reports)

**Forcing constraint.** `checkIdentifier` (`checker.go:11182`) reports
TS7034 at the declaration and TS7005 at the reference when the flow type of
an auto-typed variable *is* `autoType` (`checker.go:976`): an identity test
against a distinct intrinsic that prints `any`. `getUnionType` collapses
`autoType ∪ string` to `anyType` (`includes&Any`), and an assignment of an
`any` value answers `anyType`, so `any` alone does not say "auto". The port
spelled auto as `intrinsics.any` on every road, so the test was not
expressible.

**Port.** `Intrinsics::auto` (`intrinsics.rs`, granted by the integrator):
`ANY` flags, prints `any`, distinct identity, created after `unique_literal`
so no earlier intrinsic's id moves. `identifier_flow_type_is_auto(reference,
symbol)` (flow.rs) walks the identifier's graph once more with `auto` as the
declared type and native's initial type for an automatic declaration:
`autoType` when `assumeInitialized` (its outer-variable disjunct, shared with
the START arm's declared exit through `outer_auto_reference_is_initialized`),
else `undefinedType`. It answers `answer == auto` (or the unreachable
sentinel, which `flow.go:111` turns into the declared type).

Two native rules the identity needs, ported for the auto walk only:

- `getTypeAtFlowBranchLabel`'s shortcut (`flow.go:1270`, `:1293`): a path
  answering the declared type when declared == initial ends the join. With
  auto as both, the join answers auto even beside an assigned `number`,
  which is what makes `controlFlowNoImplicitAny`'s `f10` report. **Refused on
  the query road, with its number:** applied to every walk it cost 8 lines in
  `typeGuardsWithInstanceOf` (0:28-35 `C | (Validator & Partial<OnChanges>)`
  became the declared type). The query road's initial type is the declared
  type even where native's `assumeInitialized` is false and its initial is
  `getOptionalType(declared)`, so `declared == initial` there is not native's
  test. It reopens when the query road carries native's initial type.
- `convertAutoToAny` at the UNREACHABLE arm.

**Boundary (the integrator's condition).** `intrinsics.auto` never leaves
the auto query: it is that walk's declared/initial type; the UNREACHABLE arm
converts it to `any`; loop-cache keys carry an auto bit (`1 << 62` in the
symbol word) so no loop result crosses between the auto walk and the query
road; the shortcut tests `auto` by identity; the answer is a `bool`. The
query road (`get_flow_type_of_reference*`) never sees it. Not covered: the
auto-array track (`autoArrayType`, TS7034 with `any[]`:
`controlFlowArrayErrors`, `evolvingArrayResolvedAssert`), and
`assumeInitialized`'s module-exports, spread-target and same-scoped-binding
disjuncts. Cost: one extra walk per auto-typed identifier read the reporter
asks about, under `noImplicitAny` only; no new cache.

**Measured.** The API lands unused (corpus-neutral: types and diagnostics
verdicts identical). With the reporter prototype
`r7-flow-auto-reporter-prototype.diff` (check walk's identifier arm, beside
TS2454; not shipped, r7-reports owns the reporter) on `71f8076c`: diagnostics
**+5** (`controlFlowNoImplicitAny`,
`implicitAnyDeclareVariablesWithoutTypeAndInit`, `narrowingPastLastAssignment`,
`tsxEmit1`, `tsxReactEmit1`), types byte-identical, 0 lost.

## §9 NARROW-PRIVATE-IDENTIFIER-IN-EXPRESSION: `#x in obj`

`narrowTypeByBinaryExpression`'s `in` arm (`flow.go:519`) sends a private
identifier left operand to `narrowTypeByPrivateIdentifierInInExpression`
(`flow.go:982`): a matching right operand narrows by `getNarrowedType(t,
target, assumeTrue, checkDerived=true)`, the target the declaring class's
declared type, or its static side (`getTypeOfSymbol(classSymbol)`) when the
member is static. The port's `in` arm had only the string-key road.
`getSymbolForPrivateIdentifierExpression` is the existing lexical lookup
(`lexical_private_declaring_class`, members.rs); the member's
`HasStaticModifier` is read off its first declaration in that class. An
undecidable `narrowed_type_worker` keeps `t`, as at the other callers.

Measured on `e65ec3bf`: types **+32** (`privateNameInInExpression` es2022 and
esnext 14 each, `privateNameInInExpressionTransform` ×3, `importHelpersES6`),
diagnostics unchanged, 0 lost. No cache or traversal: one class-member scan
per narrowing.

## §10 `getUnionOrEvolvingArrayType`'s declared-union arm (routed from r7-declared)

`getUnionOrEvolvingArrayType` (`flow.go:1314`) ends every branch and loop
junction: when the union it built has exactly the declared union's
constituents, it answers the declared type itself (`flow.go:1319`,
`slices.Equal(result.types, declaredType.types)`), so the alias and origin
survive for printing. The port answered the rebuilt union; its §58 arm only
recovered a *named* union through `named_union_by_members`.
`declared_if_same_union` runs at both junctions after `recombineUnknownType`,
as native's does.

Measured on `8f08f3a2`: corpus-neutral (types text and diagnostics verdicts
identical) and jsTyping identical. It is r7-declared's prerequisite: once
every union-bodied alias is admitted, `typeGuardsAsAssertions` 0:23/0:31
print `Optional<r>` through this arm instead of `None | Some<r>`.

## §11 `signature_shapes_of_type_kind`: the ephemeral view only while resolving (routed from r7-declared)

`signature_shapes_of_type_kind` serves inference and relater parameter
consumers (`inference.go:838`, `relater.go:4452`), which read signature
links without forcing returns. For an anonymous function type it returned
`parameter_only_signature_of_active_function(..)?` unconditionally, so any
function that view declines — a function merged with a namespace
(`function log` plus `namespace log`, flagged `MODULE`), or one whose
return is not resolving — answered `None` and the relation went undecided:
`typeof log extends AnyFunction ? 1 : 0` stayed unevaluated (native `1`),
which kept jsTyping's `MatchingKeys<typeof Debug, AnyFunction>` from
evaluating.

The view exists to stand in for a vector this port would otherwise
materialize *while the function's own return is resolving*. It is now taken
only then (some declaration of the symbol has an active signature key on
the resolution stack); otherwise the ordinary kind-specific lists answer,
as native's `getSignaturesOfType` does.

Measured on `ec55197d`: corpus-neutral (types text and diagnostics verdicts
identical, slowcases clean); jsTyping one false TS2322 fixed
(`debug.ts(189,13)`), no new false line, no lost true line.

## §12 Logical assignments of `[]` start the evolving-array track (routed from r7-calls)

`let result; result ||= []; result.push("a")` reported a false TS2345
against `never` (jsTyping `checker.ts` 51501; tsgo clean). Native decides
the evolving-array track per assignment flow node: `isEmptyArrayAssignment`
(`flow.go:283`) reads *any* binary parent whose right operand is `[]`, so a
logical assignment (`||=`, `&&=`, `??=`, definite per
`getAssignmentTargetKind`) answers `getEvolvingArrayType(never)` exactly as
`=` does (`flow.go:233`), and `x.push` then reads the operation target's
`autoArrayType`. The port decides the track before the walk
(`is_auto_array_declaration` → `symbol_has_empty_array_assignment`, §736),
and that scan admitted only `=`. It now admits the four definite assignment
operators; a compound `+=` never reaches the auto arm (`flow.go:229`).

Measured with `19cbf619` on `9db14f99`: corpus-neutral (types text and
diagnostics verdicts identical); jsTyping: the false TS2345 at
`checker.ts(51501,41)` is gone, no new false, no lost true.

Still open on this track (the auto-array identity, §8): `let r; if (c) r ??=
[]; else r = [1]; r` is native `any[]` with TS7034/TS7005 `any[]`. The port
answers `any[] | number[]`: its evolving-array join
(`union_or_evolving_array`) unions without the junction's subtype reduction,
which native passes to `getUnionOrEvolvingArrayType`.
