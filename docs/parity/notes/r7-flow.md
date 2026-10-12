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

## §13 TS2339 on a class whose base is generic, circular or unresolved (routed from r7-grammar)

`(new N).blah` with `class N extends N0<string>`, and `(new M).blah` with
the circular `class M<T> extends M<string>`, were silent: the miss
certificate (`apparent_type_lacks_at`) trusted a class table only through
`declared_members_are_complete` (member_completeness.rs), which declines
any instantiated or circular base. Native `resolveObjectTypeMembers`
(`checker.go:19143`) layers the base types' properties under the declared
ones, and member names do not depend on type arguments (§41's audit), so
`class_chain_lacks` walks the class's declarations and its `extends` chain
by name:

- a circular base resolves no base types (`resolveBaseTypes`,
  `checker.go:19189`, reported TS2506), so a symbol met again adds nothing;
- an `extends X` that resolves to nothing is the `errorType` base
  constructor (TS2304) and adds no base type;
- any other base the port cannot follow (alias, type parameter, mixin call)
  declines, as do computed member names.

When no index signature is declared anywhere on the chain, the resolved type
has no index infos; otherwise the existing `no_index_signature_admits`
asks. No cache: run once per reported miss, bounded by the chain's symbols.

Measured on `4a5c81bf`: diagnostics **+5** (`recursiveBaseCheck3/4/6`,
`genericRecursiveImplicitConstructorErrors3`,
`destructuringParameterProperties4`), types text identical, 0 lost; jsTyping
unchanged. `complicatedGenericRecursiveBaseClassReference` still misses its
TS2554 (call arity, not this rule).

## §14 TS18014 reads the receiver's type (routed from r7-grammar)

`checkPrivateIdentifierPropertyAccess` (`checker.go:11494`) looks the
private name up among `getPropertiesOfType(leftType)`: the receiver's
*type*. `check_private_name_shadowing` found the receiver's class from its
written annotation (`annotated_class_of`), so `new C().#bar` inside a nested
class declaring its own `#bar` was silent (`privateNameNestedMethodAccess`).
It now takes the class of the receiver type (a class reference's declared
class, or `typeof C`'s class for the static side), checks the name on that
side, and prints `TypeToString(leftType)` as native does. The written-name
helper is deleted. TS1111 (the same tail) is plain-JS-only
(`IsPlainJSFile`, `checker.go:11304`) and not ported here.

Measured on `9acf69d7` + `c6df7541`: diagnostics **+1**
(`privateNameNestedMethodAccess`), types text identical, 0 lost, jsTyping
unchanged.

## §15 `isPropertyInitializedInConstructor`'s synthetic `this.<name>` (requested by r7-grammar)

`isPropertyInitializedInConstructor` (`checker.go:4977`) builds a fresh
`this.<name>` (or `this[<expr>]`) parented to the constructor, gives it the
constructor's `ReturnFlowNode`, and asks `getFlowTypeOfReferenceEx(reference,
propType, getOptionalType(propType))`; TS2564 reports when the answer contains
`undefined`. `flow_type_of_this_property_at(constructor, name, declared,
initial)` is that query on §7's synthetic access: `SyntheticAccess` gains a
`this_root`, whose root matches any runtime `this` target
(`isMatchingReference`'s `KindThisKeyword` arm, `flow.go:1617`) and has no
parts of its own; `FlowState::reference` is the constructor (container
lookups). No return flow node answers `declared`, as native's nil flow node
does. A computed name the caller cannot spell as a literal has no
counterpart.

Checker port boundary: no cache; one walk per property, as native.

Measured on `52536a40`: the API alone is corpus-neutral (types text and
diagnostics verdicts identical). With the measurement-only wiring
`r7-flow-ts2564-prototype.diff` (check.rs `check_property_initialization`,
r7-grammar's to port), diagnostics **+1** (`detachedCommentAtStartOfConstructor2`),
0 lost, jsTyping unchanged.

## §16 TS2565: `this.x` used before being assigned in the constructor

`getFlowTypeOfAccessExpression`'s `assumeUninitialized` arm
(`checker.go:11411-11431`): under `strictNullChecks` and
`strictPropertyInitialization`, a read of `this.x` (or `this["x"]`) whose
property is a non-static, non-abstract `PropertyDeclaration` without an
initializer or `!` (`isPropertyWithoutInitializer`, `:4956`), inside a
constructor of the declaring class, walks its flow from
`getOptionalType(propType)`; when `undefined` survives and `propType` does
not contain it, native reports TS2565 at the name (the index of an element
access). `check_property_used_before_assigned` (flow.rs) is that arm, run
from the check walk's access arms through `check_nonexistent_property` (the
same native function). The query road already answers the declared type
there, which is what native returns after reporting. `contains_undefined_type`
(check.rs) became `pub(crate)` for it. Not ported: the expando disjunct
(`:11420`, a property declared by `this.x = …`).

Measured on `42e0cbea`: diagnostics verdicts and types text unchanged (the
TS2565 lines of `strictPropertyInitialization` 113-115 now match; the case
stays WRONG on two TS2564 lines, r7-grammar's port), exact oracle 0 gained
0 lost, jsTyping unchanged, Ir gi 1.0002 dm 1.0007.

Checker port boundary: no cache; one flow walk per qualifying constructor
read, the walk native runs on every such read anyway.

## §17 `Extract<T, U>`'s base constraint reads the true branch's substitution (routed: `declared.rs`)

`controlFlowGenericTypes` 0:38-0:41 (`f2<T>(x: Extract<T, string |
undefined> | null)`, `if (x) { x.length; return x }`): native prints `x :
string` in both constraint positions, the port `NonNullable<Extract<…>>` with
`length : any`.

**Native.** `getNarrowableTypeForReference` (`checker.go:31491`) substitutes
base constraints when some constituent `isGenericTypeWithUnionConstraint`.
The conditional's base constraint is `getConstraintFromConditionalType`
(`:17255`): no distributive constraint (the check `T` is unconstrained), so
`getDefaultConstraintOfConditionalType` (`:17263`), whose true constraint is
`getInferredTrueTypeFromConditionalType`. A reference to the check type in
the true branch is a substitution type (`getConditionalFlowTypeOfType`,
`:24952`; `getImpliedConstraint`, `:24989`), and its base constraint is
`getSubstitutionIntersection`'s `T & (string | undefined)` =
`(T & string) | (T & undefined)`, base constraint `string | undefined`: a
union, so the reference is substituted and truthiness narrows it to `string`.

**Port.** `capture_conditional_alias_branches` (`declared.rs`) published the
bare `T` as the true branch, whose base constraint is none. The fix publishes
`extends & T` when the true branch is the naked check parameter, the check is
bound to a type parameter, and the extends type is not `any`/`unknown`
(instantiateType's Substitution arm keeps the substitution exactly then: a
restrictive type variable is assignable only to a top type). The table is read
only by `next_base_constraint` and `const_type_variable_inner`, which both see
through the intersection; the relater reads only `(c, c)` distributive pairs,
which this does not write. Not covered: a substituted reference below the true
branch's top level (`T | null`, `[T]`), which keeps the bare check, as before.

Not my file: held as `r7-flow-conditional-substitution-constraint.diff` (with
its test, `tests/r7_flow_conditional_substitution.rs`), routed to the
integrator. Measured on `127660ee` unfiltered: types **+4**
(`controlFlowGenericTypes` 0:38-0:41), diagnostics unchanged, 0 lost; jsTyping
122 → 122, new_false 0, lost_true 0; Ir gi 0.9999, dm 1.0001. No cache: one
intersection per captured alias reference, built where the branch is captured.

## §18 The auto-array track: `autoArrayType`'s identity and the junction's subtype reduction

`let r; if (c) r ??= []; else r = [1]; r` is native `any[]` with TS7034/TS7005
`any[]`; `let x = []; let y = x` reports both with `any[]`
(`controlFlowArrayErrors`, `evolvingArrayResolvedAssert`). The port answered
`any[] | number[]`, `y : any`, and reported nothing for the array track (§8
left it uncovered).

**Native.** `getTypeAtFlowBranchLabel` hands the antecedent types to
`getUnionOrEvolvingArrayType` with `UnionReductionSubtype` when one is not a
subset of the initial type (`flow.go:1280`, `:1309`); its mixed arm
finalizes each evolving array and unions with that reduction (`flow.go:1318`),
so `autoArrayType | number[]` is `autoArrayType`. An evolving array with no
elements finalizes to `autoArrayType` (`createFinalArrayType`, `flow.go:1582`),
which is `createArrayType(autoType)`, a distinct identity printing `any[]`.
`checkIdentifier` reports when the flow type is `autoType` or
`autoArrayType` (`checker.go:11182`), with `TypeToString(flowType)`.

**Port, query road.**

- `union_or_evolving_array` takes the junction's `subtype_reduction` and
  runs `union_with_subtype_reduction` over the finalized list (the literal
  union where it declines, as before).
- `finalize_evolving_array` finalized an element-less evolving array to the
  declared type. For `let x = []` that is `Array<any>` already; for `let r;`
  it was `any`, so `r = []; let y = r` read `y : any`. It now finalizes to
  `Array<any>` — **except** where the initial type is the declared one (an
  initialized outer reference, or non-strict), because there the START arm
  also answers the declared `any` and this port spells the evolving array as
  the declared type (§736), so the answer cannot say which it is. Those keep
  `any`, the previous answer. Would be wrong if a corpus line wants `any[]`
  for an evolving `let r;` read through a closure or in a non-strict file; the
  fix is a distinct evolving spelling on the query road, which every narrowing
  arm would then have to finalize first, as native's `getTypeAtFlowCondition`
  does.

**Port, auto query.** `identifier_flow_auto_kind` (flow.rs) generalizes §8's
`identifier_flow_type_is_auto` (now its scalar half, unchanged for the
reporter) to native's two identities, answering `AutoFlowType::{Auto,
AutoArray}` with the printed `text()`:

- declared type `autoType` for `let r;`, `Array<auto>` (native's
  `autoArrayType` is exactly `createArrayType(autoType)`) for `let x = []`;
  the assignment arm is the automatic one for both (`flow.go:232`);
- the evolving array is spelled `Array<auto>` through the new
  `FlowState::evolving`, so `autoType` (unassigned) and the evolving array
  are apart in this walk; every evolving test reads `evolving_type()`, which
  is `declared_type` on the query road (no change there);
- `assumeInitialized` for a `let x = []` outer reference holds outright (an
  initializer means never `isNeverInitialized`), so its initial type is the
  declared type;
- `FlowState::auto_query` replaces the `declared == auto` tests (loop-key bit,
  the junction shortcut, the UNREACHABLE arm's `convertAutoToAny`, which now
  converts `autoArrayType` to `any[]` too).

Boundary, as §8: `auto` and `Array<auto>` never leave the walk; the answer is
the enum. `Array<auto>` is an ordinary interned instantiation keyed by the
`auto` argument, which only this walk passes. Accepted: where the walk's
`let x = []` declared type and evolving spelling coincide, the junction
shortcut (`flow.go:1270`) can take an evolving path for a declared one; both
finalize to `autoArrayType` when no element was pushed, which is the only
answer the test reads.

Behaviour change without the reporter: `identifier_flow_type_is_auto` now
also answers for an array-track `let r;` whose flow type is still `autoType`
(an outer read before any assignment, native TS7034 `any`); it used to
decline every array-track symbol.

Reporter (`implicit_any.rs`, r7-reports): `r7-flow-auto-array-reporter.diff`
asks `identifier_flow_auto_kind` and prints `kind.text()` in both messages.

Measured on `127660ee` unfiltered. This commit: types **+2**
(`controlFlowArrayErrors` 0:18-0:19, `let y = x : any[]`), diagnostics
unchanged, 0 lost; jsTyping 122 → 122, new_false 0, lost_true 0; Ir gi 0.9999,
dm 1.0003. With the reporter diff: diagnostics **+3**
(`controlFlowArrayErrors`, `evolvingArrayResolvedAssert`,
`narrowSwitchOptionalChainContainmentEvolvingArrayNoCrash1`), types the same
+2, 0 lost. Checker port convention: no cache or side table; `Array<auto>` is
the existing instantiation table's entry; the walk is the §8 walk, now also
run for array-track symbols the reporter asks about.

## §19 `reportNonexistentProperty`'s union-constituent chain

`reportNonexistentProperty` (`checker.go:11545-11552`) starts every message
for a property access on a union receiver (not a private name, no primitive
flags: `boolean` and an enum's declared union are primitive) with the first
constituent that has neither the property (`getPropertyOfType`, over the
reduced apparent type) nor an applicable index signature, as "Property 'p'
does not exist on type '<constituent>'", and chains each of the four
messages (TS2576, the lib suggestion, TS2551, TS2339) onto it. The port
reported the head alone: `x.nope()` on `"foo" | "bar"` printed one line where
native prints two (`unionPropertyExistence`, `isArray`, the `typeGuard*`
cases).

`union_constituent_miss` reads each constituent with the certificate the
union's own miss used (`union_property_lacks`: the primitive apparent type,
then `apparent_type_lacks`); a constituent it cannot certify either way
declines the chain, and the message is reported without it, as before.
`chained_miss` is `NewDiagnosticChainForNode` (the child is at the same name,
so the head keeps the span). No cache: one pass over the union per reported
miss.

Measured on `a9e63316` unfiltered: types and legacy diagnostics verdicts
unchanged (the legacy suites compare codes and positions); exact oracle 9659
→ 9659, 0 gained, 0 lost (the chained cases stay non-exact on other rows, see
below); jsTyping 122 → 122, new_false 0, lost_true 0; Ir gi 1.0001, dm
1.0000. Test: `tsr-checker/tests/r7_flow_union_miss_chain.rs`.

The chained row itself is not exact yet, for every chain in the port: a chain
child (and every related-information entry) is published without a file, so
its oracle row is file-less with position `-`, where native's
`NewDiagnosticChainForNode`/`NewDiagnosticForNode` give it the node's file and
span. On the `909a027b` base report, 83 non-exact cases differ from native
only in those columns of `C`/`R` rows. That is diagnostics infrastructure
(`tsr-diagnostics`' chain constructor or the checker's `report`), routed to
the integrator.

## §20 NONEXISTENT-PROPERTY-CERTIFICATION-GATE: `union_receiver_flow_is_uncertified`, re-measured

Dropping the gate (both call sites answering "certified") on `909a027b`
(main `b563c6b9` + the §18 commit) + §19:
diagnostics **+13** (`inKeywordTypeguard(strict=false)`, `isArray`,
`narrowByClauseExpressionInSwitchTrue6`, `typeGuardConstructorClassAndNumber`,
`unionPropertyExistence`, `discriminatedUnionTypes2`, `typeGuardOfFormInstanceOf`,
`typeGuardRedundancy`, `typeGuardsWithInstanceOf`,
`typeGuardsWithInstanceOfByConstructorSignature`,
`typeGuardsWithInstanceOfBySymbolHasInstance`, `unionTypeMembers`,
`unionTypeReadonly`) against **4 lost**, each EMPTY_RIGHT → EMPTY_WRONG with a
false TS2339 on a receiver the port narrows less than native:

| case | false TS2339 | the narrowing gap | owner |
|---|---|---|---|
| `controlFlowWithIncompleteTypes` | `x.slice()` on `string \| boolean` (native `string`) | `getTypeAtFlowLoopLabel`'s incomplete in-loop types (`silentNeverType` from an incomplete never narrowing, `flow.go`) | flow.rs |
| `typeGuardNarrowByUntypedField`, `typeGuardNarrowByMutableUntypedField` | `.length` on `ArrayLike<any> \| Iterable<any>` | the predicate `{ readonly [K in P]: unknown }` instantiated at `'length'` is a memberless mapped mint | mapped.rs |
| `discriminatedUnionTypes3` | `.property` on `SomeReturnType` | `areTypesComparable(undefined, \`${string} is wrong!\`)` is Unknown, so `narrowTypeByDiscriminant` declines | relater.rs |

The gate stays until those land; the previous measurement (+12/−5) had one
more narrowing loss, since closed.

## §21 `newFlowType`'s `silentNeverType`

`getTypeAtFlowCondition` narrows a loop's provisional (incomplete) type, and
`newFlowType` (`flow.go:28`) turns an incomplete `never` into
`silentNeverType`, "the *only* place a silent never type is ever generated"
(`flow.go:359-365`). A property access on it is any-like (`isAnyLike`,
`checker.go:11266`, `:11314-11321`: it answers itself, no lookup, no report),
a call resolves to `silentNeverSignature` (`resolveCallExpression`,
`checker.go:8512`), and `getAssignmentReducedType` keeps an assigned `never`,
so the path drops out of the loop's join. In `controlFlowWithIncompleteTypes`'
`foo2` (`while (cond) { if (typeof x === "number") x = "abc"; else x =
x.slice(); }`) the else branch's `x` is `never` on the provisional pass; the
port typed `x.slice()` as `error`, the assignment as the declared union, and
the loop as `string | number | boolean` (native `string | number`).

This commit adds `new_flow_type` at the condition arm (the stale "not ported:
nothing here produces an incomplete type yet" comment is gone: the loop
label's on-stack answer has been incomplete since §12.8). The port's branch
label does not carry `incomplete` (native's `seenIncomplete`), so the
condition arm is the only producer here, as natively it is the only place a
narrowing makes one. Alone it changes nothing measurable: the two consumers
are `members.rs` (r7-shared) and `calls.rs` (r7-calls), routed as
`r7-flow-silent-never-callers.diff` with its test
(`tests/r7_flow_silent_never.rs`). With the diff, on `a9e63316`: types **+13**
(`controlFlowWithIncompleteTypes` 7, `typeGuardsAsAssertions` 6), diagnostics
unchanged, 0 lost; exact oracle 9659 → 9660 (`controlFlowWithIncompleteTypes`),
0 lost; jsTyping unchanged; Ir gi 1.0001, dm 1.0003. It also closes one of
§20's four losses (`controlFlowWithIncompleteTypes`).

## §22 TS2339 for a destructured name of an `unknown` parent

`getBindingElementTypeFromParentType` (`checker.go:17707`) returns early only
for `isTypeAny(parentType)`; an `unknown` parent goes on to
`getIndexedAccessTypeEx`, whose `getPropertyTypeForIndexType` finds no
property and no index info on its reduced apparent type (`unknown` under
`strictNullChecks`, `emptyObjectType` otherwise, `getApparentType`) and
reports "Property 'x' does not exist on type 'unknown'" (`'{}'` non-strict)
at the name. `destructured_property_is_absent` declined every `unknown`
parent; it now certifies the miss for it, printing that apparent type, after
the same source-flow certification as any other parent
(`destructureCatchClause`'s `catch ({ x }: unknown)`, 8 lines over its four
variants).

Decline kept, narrowed: an unannotated parameter's `unknown`
(`index_access_reports.rs`). The port's contextual parameter type is
`unknown` in `inferredRestTypeFixedOnce` (`wrap(({ cancelable } = {}) =>
{})` under `Args extends unknown[]`), where native's parameter is `{
cancelable: any; }`; without the decline that case got a false TS2339
(measured, EMPTY_RIGHT → EMPTY_WRONG). Only a written or catch-clause
`unknown` is certified; the contextual road waits on the implied pattern
type (BINDING-PATTERN-IMPLIED-TYPE, r7-contextual).

`destructureCatchClause` stays non-exact on TS2488 for the array patterns
(`checkIteratedTypeOrElementType` on `unknown`, `iteration.rs`, not this
lane's).

## §23 `instanceof` over a union callee maps `getInstanceType` per constituent

`narrowTypeByInstanceof` (`flow.go:836`) narrows by `mapType(rightType,
c.getInstanceType)`: a union callee `AA | BB` (`AA` a callable with
`prototype: A`, `BB` with `new(): B`) narrows by `A | B`. The port's general
road (§3) read `prototype` and the construct signatures off the union as a
whole: the union has no `prototype` (one constituent lacks it) and no
construct signatures (one constituent has none), so it declined and kept
`A | B | C` (`narrowByInstanceof` 27:x). `instance_type_of_callee` is
`getInstanceType` for one non-class callee, with the Function-derivation
gate before it (a union passes `isTypeDerivedFrom(rightType,
globalFunctionType)` only when every constituent does), and the arm maps it
over the union's constituents. A callee constituent that declines declines
the whole narrowing, as before. No cache: the same reads per constituent.

Measured (§23 alone) on `379e4ed5` unfiltered: types **+1**
(`narrowByInstanceof` 27:x), diagnostics unchanged, 0 lost; exact oracle 9659
→ 9660 (`narrowByInstanceof`), 0 lost; jsTyping 122 → 122, new_false 0,
lost_true 0; Ir gi 1.0000, dm 1.0000.

## §24 The branch label's named-union lookup (§58): measured, held

`checker-notes-narrow.md` §58 made a branch join whose union re-formed a
*named* union's exact members answer the named type
(`named_union_by_members`), at both the literal and the subtype-reduced join.
Native has no such step: `getUnionOrEvolvingArrayType` (`flow.go:1314`)
returns the declared type when the join has the declared union's
constituents (§10's `declared_if_same_union`) and otherwise a fresh union;
tsgo keys unions by alias too (`getUnionTypeFromSortedList`), so a plain
`TestA | TestB` is not the alias `AllTests`. In `discriminantPropertyCheck`
(`let test; test = mapOfTests[id]; if (test.type === 'testA') …; switch
(test.type)`) the declared type is `autoType`, and native prints the join
`TestA | TestB` where the port printed `AllTests`.

Removing the lookup on `379e4ed5` + §23: types **+1** (that line, nothing
else moves), diagnostics unchanged, exact +1, 0 lost, but jsTyping
**new_false 1**: `builder.ts(1304,21)` TS2322 on `emitSignatures =
append(emitSignatures, [fileId, cond ? emptyArray as [] : emitSignature])`.
The narrowed `emitSignature` now reaches inference as `string | [signature:
string]` rather than the alias `EmitSignature`, as natively, and this port's
`append<T>` inference then picks a widened candidate native does not. That is
an inference/relation gap (r7-contextual / r7-reports), not a flow one. The
lookup stays until it is fixed; the removal is two hunks in
`flow.rs`'s branch label.

## §25 `isParameterOrMutableLocalVariable` as written, and `markNodeAssignments`' export-specifier arm

`checkIdentifier` extends a closure's flow container to the declaring
function when the variable is a parameter or mutable local and the read is
past its last assignment (`isPastLastAssignment`, `flow.go:2668`); a mutable
local is a `let` neither exported nor at the top of a **global** script
(`isMutableLocalVariableDeclaration`, `utilities.go:1053`). The port's
position reader refused every file-level `let`, an approximation kept because
moving it "moves `checker_types`" (`checker-notes-diag2.md` §42), and did not
port `markNodeAssignmentsWorker`'s export-specifier arm (`flow.go:2726`: a
value re-export `export { x }` marks `x` assigned at `MaxInt32`, so a read is
never past it). `narrowingPastLastAssignmentInModule`'s module-level `x4`/`x5`
read in an arrow stayed `string | number` (native `string`).

Both are ported: `is_parameter_or_mutable_local_variable` is now the written
predicate (the separate `_faithful` copy is gone), and `mark_exported_local`
is the export-specifier arm (not type-only, no module specifier, an
identifier name resolved as a value). The arm is what keeps `export { x2 }`
and `export { x3 as foo }` wide once module-level `let`s qualify.

## §26 `isUnitLikeType` and `extractUnitType` in the switch default filter

`narrowTypeBySwitchOnDiscriminant`'s default clause (`flow.go:1139`) keeps a
constituent unless it is unit-like (`isUnitLikeType`, `checker.go:25414`: read
through its base constraint, a unit type or an intersection with a unit
member) and its unit (`extractUnitType`, `:25424`) is comparable to some case.
The port asked the constituent's own `UNIT` flag, so `T & 1 | T & 2` kept
both in `default:` (`intersectionNarrowing` `f4`, native `never`).
`is_unit_like_type`/`extract_unit_type` port both; `narrowTypeByEquality`'s
false-branch filter, which had the unit-like test inline, now shares it.

Measured (§25 + §26) on `edf2a433` unfiltered: types **+7**
(`narrowingPastLastAssignmentInModule` 2, `nestedBlockScopedBindings7/8` 2
each, `intersectionNarrowing` 1), diagnostics unchanged, 0 lost; jsTyping 122
→ 122, new_false 0, lost_true 0; exact oracle 9660 → 9664 (those four cases), 0
lost; Ir gi 0.9999, dm 0.9999. No cache added; the export arm runs inside
the existing once-per-root marking walk.

## §27 The key-property map (`getKeyPropertyName`) in discriminant and predicate narrowing

`computeKeyPropertyNameAndMap` (`relater.go:1141`) indexes a union of at
least ten object/instantiable constituents by its first unit-typed property
when the keys are literal and mostly unique. `narrowTypeByDiscriminantProperty`
(`flow.go:702-719`) answers a `===`/`!==` on that property by lookup: the
matching constituent itself on the equal branch, the union without it on the
other when its key is a unit type. `getNarrowedTypeWorker` (`flow.go:885-899`)
compares each candidate part with the constituent its discriminant selects.
The port had neither ("an optimisation with one behavioural edge", the old
doc said). The edge is real: the lookup answers the constituent alone where
the comparability filter keeps every constituent without the property,
including a nullable one. `narrowingUnionWithBang`'s ten-member optional
`thing` narrowed by `working.thing!.name !== "Correct"` is native `{ name:
'Correct'; id: string; }` in the else branch, the port's `… | undefined`
(its nine-member twin has no key property and keeps `undefined` natively too).

`key_property_map` (flow.rs) computes the name and map per narrowing. Native
caches both on the union type; here only a union of ten or more object
constituents pays the walk (one property-name list and one property type per
constituent), so no cache was added. Property order is
`get_property_names_of_type`'s, the declaration order native's
`getPropertiesOfType` yields for the type literals these unions are made of.
The value's type for the lookup goes through the equality arm's existing
memo and re-entry guard, now shared as `narrow_value_type`.

Measured on `0a4150b0` unfiltered: types **+2** (`narrowingUnionWithBang`
0:49/0:51), diagnostics unchanged, 0 lost; exact oracle 9664 → 9665, 0 lost;
jsTyping 122 → 122, new_false 0,
lost_true 0; Ir gi 1.0000, dm 1.0000.

## §28 The function miss certificate reads function expressions and arrows too

`function_declaration_lacks` certifies a miss on a function's type:
`resolveAnonymousTypeMembers` gives it the symbol's exports, then
`getPropertyOfTypeEx` falls to the global `Function` family and `Object`
(`checker.go:18899`). It accepted only a `function` declaration's symbol, so
`const { date } = (inspectedElement: any) => 0` (`destructuringControlFlowNoCrash`)
was silent where native reports TS2339 "Property 'date' does not exist on
type '(inspectedElement: any) => number'". A function expression's or arrow's
type is the same anonymous function type over its own symbol, so the
certificate now admits those declarations too (still not in JS). An expando
assignment (`const f = () => {}; f.x = 1`) binds to the initializer's symbol
(`initializer_symbol`, as native's expando container), whose non-empty exports
keep declining.

Measured on `ef61f7d9` unfiltered: diagnostics **+1**
(`destructuringControlFlowNoCrash`), types unchanged, 0 lost; jsTyping
unchanged.

## §29 `typeof globalThis` leaves out ambient modules for an indexed access type

`resolveAnonymousTypeMembers`' globalThis arm (`checker.go:20674-20680`)
keeps a global only when it is not block-scoped and not a value module whose
every declaration is an ambient module. The indexed-access report
(`index_access_reports.rs`) tested the block-scoped half only, so
`(typeof globalThis)["\"ambientModule\""]` found `declare module
"ambientModule"`'s symbol and was silent (`globalThisAmbientModules`, two
TS2339). It now applies both halves (an ambient module is a
`ModuleDeclaration` with a string-literal name).

Measured (§28 + §29) on `ef61f7d9` unfiltered: diagnostics **+2**
(`destructuringControlFlowNoCrash`, `globalThisAmbientModules`), types
unchanged, 0 lost; exact oracle 9665 → 9666 (`destructuringControlFlowNoCrash`;
`globalThisAmbientModules` keeps a `globalThis.namespaceModule.typ` printing
row), 0 lost; jsTyping 122 → 122, new_false 0, lost_true 0; Ir gi 0.9999, dm
1.0000.

## §30 The call-receiver half of the TS2339 certificate, re-measured

`receiver_type_is_the_declared_one` certifies a call-expression receiver only
when its type is `never`; every other call result is trusted to no one, since
it rests on this port's call resolution and return inference. Certifying all
call results on `e5de971c`: diagnostics **+4** (`jsdocInTypeScript`,
`templateLiteralsInTypes`, `propertyAccessOnTypeParameterWithConstraints4/5`)
against **1 lost** (`superPropertyAccessNoError(target=es2015)`, EMPTY_RIGHT →
EMPTY_WRONG): `instance.returnThis().fn()` reported a false TS2339 because the
derived `returnThis() { return super.returnThis(); }` infers `SomeBaseClass`
where native infers the derived polymorphic `this` (the `super.m()` call's
this-argument instantiation of a `this` return, `calls.rs`). The gate stays
until that lands.

## §31 `isNullOrUndefined` against the synthesised `undefined` (routed: `symbols.rs`)

`let x = undefined; if (cond) x = 1; …; x` is native `string | number |
undefined` under `noImplicitAny` (`controlFlowNoImplicitAny` f2/f6): the
declaration is automatic (`isNullOrUndefined(initializer)`,
`checker.go:16702`) and the flow walk starts from it. The port's
`is_null_or_undefined_expression` (`symbols.rs`) accepted the identifier only
when `undefined` resolved to **nothing**, a test written before the binder
synthesised the global `undefined` (`declare_synthesised_globals`); it now
always resolves, so the test never held and the flow walk did not treat the
declaration as automatic (`x : any`). Native compares with `c.undefinedSymbol`.
`r7-flow-undefined-initializer.diff` compares with `binder.undefined_symbol()`
(still accepting an unresolved name, and still refusing a shadowing local).
Measured on `e5de971c` unfiltered: types **+6** (`controlFlowNoImplicitAny`
4, `assignEveryTypeToAny` 2), diagnostics unchanged, 0 lost; exact oracle 9666 →
9668 (both cases), 0 lost; jsTyping 122 →
122, new_false 0, lost_true 0; Ir gi 1.0000, dm 1.0000.

## §32 An element access whose index has a literal type reports per literal

`getIndexedAccessTypeOrUndefined` (`checker.go:26961`) asks
`getPropertyTypeForIndexType` once per literal of the index type (a union
distributes; `boolean` does not), so `{ a: 1, b: "", c: false }[id]` with `id:
'a' | 'b' | 'z'` reports the `'z'` miss and `{}[10]` reports `'10'` under
`noImplicitAny` (`indexedAccessWithFreshObjectLiteral`, `noImplicitAnyIndexing`).
The port read a string-literal argument only. `literal_index_names` takes
the argument's type when it is a string or number literal or a union of
them, and each name goes through the same certificate and report as a
written string literal (`check_nonexistent_name`, the tail of
`check_nonexistent_property`, split out for it). Any other index keeps the
computed-key road. No cache: one index-type read per element access.

Measured on `e5de971c` unfiltered: diagnostics **+3**
(`indexedAccessWithFreshObjectLiteral`, `noImplicitAnyIndexing`,
`objectSpreadIndexSignature`), types unchanged, 0 lost; exact oracle 9666 →
9668, 0 lost; jsTyping 122 → 122, new_false 0, lost_true 0; Ir gi 1.0001, dm
1.0000.

## §33 TS2339 in an ambient context

`check_nonexistent_property` returned at once in an ambient context (a
`declare` declaration, a `.d.ts`). Native has no such exit: the expressions an
ambient context holds — a heritage clause's `M.C` (`declare class D2 extends
M.C {}`), a computed property name's `Symbol.isRegExp` (`declare class C {
[Symbol.isRegExp]: string }`) — are checked by
`checkPropertyAccessExpressionOrQualifiedName` like any other, and the miss
is reported (`genericTypeReferenceWithoutTypeArgument.d`/`3`,
`parserES5SymbolProperty4(target=es2015)`). The exit is gone; the certificates
below it are unchanged.

Measured on `b5b993e1` unfiltered: diagnostics **+3** (those three cases),
types unchanged, 0 lost; exact oracle 9668 → 9671 (those three), 0 lost; jsTyping
122 → 122, new_false 0, lost_true 0; Ir gi 1.0000, dm 1.0000.

## §34 TS2576's static member text

`reportNonexistentProperty` names the static member as `typeName+"."+propName`
with `typeName := TypeToString(containingType)` (`checker.go:11553-11556`):
`List<T>.Foo`, `C<number, string>.foo`. The port printed the class's symbol
name (`List.Foo`). And `getPropertyTypeForIndexType`'s element-access arm
(`checker.go:27148-27150`, under `noImplicitAny`) reports TS2576 at the access
with `typeName+"["+GetTextOfNode(argument)+"]"` (`A["\""]`, `A['y']`); the
port declined it as needing source text, which the checker reads through
`module_host.source_text` (as `assignreport.rs` does for computed names). Both
now match (`staticOffOfInstance2`, `staticPropertyNotInClassType`,
`classStaticPropertyAccess(target=es2015)`). The element-access arm asks
`typeHasStaticProperty`, as native; the other direction (an instance member
read off the static side) keeps its decline.

Measured on `e2e5c9d1` unfiltered: diagnostics **+1**
(`classStaticPropertyAccess(target=es2015)`; the other two were WRONG on
message text only, which the legacy suite does not read), types unchanged,
0 lost; exact oracle 9671 → 9674 (all three), 0 lost; jsTyping 122 → 122,
new_false 0, lost_true 0; Ir gi 0.9998, dm 1.0000.

## §35 TS18046 for an `unknown` property or element access receiver

`checkPropertyAccessExpression` and `checkElementAccess` pass the receiver
through `checkNonNullExpression`, whose `unknown` arm (`checker.go:7413`,
under `strictNullChecks`) reports TS18046 "'e' is of type 'unknown'" at an
entity-name receiver shorter than 100 characters (TS2571 otherwise) and
answers `errorType`; a chain link passes `getOptionalExpressionType` of the
receiver, which leaves `unknown` alone. The port had the callee half
(`calls.rs`' `check_non_null_callee`) only, so `catch (e) { e.toUpperCase() }`
under `useUnknownInCatchVariables` was silent. `report_unknown_receiver`
(nonexistent_property.rs) is the receiver half; after it reports, the access
asks nothing else, as the error type would.

Certified receivers only: an identifier whose `unknown` is its declaration's
(a catch-clause variable, or a declaration with a written type). Uncertified,
the arm measured diagnostics +4 against **3 lost** (`badInferenceLowerPriorityThanGoodInference`,
`inferFromGenericFunctionReturnTypes2`, `typeArgumentInferenceWithClassExpression3`):
each receiver was `unknown` only in this port, an inferred type argument
(`const result = canYouInferThis(() => ({ … }))`) or a contextually typed
parameter (`s => s.length`) native types concretely. Those wait on inference
(r7-contextual, r7-calls).

Measured (certified) on `57d327e6` unfiltered: diagnostics **+4**
(`catchClauseWithTypeAnnotation`, `controlFlowAliasingCatchVariables(useunknownincatchvariables=true)`,
`es2016IntlAPIs`, `privateNameAndAny`), types unchanged, 0 lost; exact oracle
9674 → 9676, 0 lost; jsTyping 122 → 122, new_false 0, lost_true 0; Ir gi
1.0000, dm 1.0005 (the receiver's cached type read on every access; the
optional-chain type only for an `unknown` receiver). `useUnknownInCatchVariables01` keeps its
`e++` TS18046 (the arithmetic operand's `checkNonNullType`, not this arm).

## §36 One reporter for a non-literal key's TS7053/TS7015, with native's chain

Two boxes ported `getPropertyTypeForIndexType`'s no-index-signature arm
(`checker.go:27129-27184`) for a non-literal key on the same day:
`report_implicit_any_element_access` (`index_access_reports.rs`, every valid
non-literal key kind, no message chain) and `check_computed_index_implicit_any`
(`nonexistent_property.rs`, plain `string`/`number` keys, chained). Where both
certified the miss the access was reported twice, once without the chain
(`noImplicitAnyForIn`'s `x[i][j]` and `k1[k]`). Native reports once:
`Element implicitly has an 'any' type because expression of type '{full key}'
can't be used to index type '{object}'`, chained to "No index signature with a
parameter of type '{failing key}' was found on type '{object}'" for a plain
`string`/`number` failing key and to nothing for the other kinds this arm
admits.

`report_implicit_any_element_access` is the one reporter now, with that chain
(the failing key is the first constituent no index info applies to);
`check_computed_index_implicit_any` and its two certificate helpers are
removed. The legacy suites saw no position or code move (the removed
reporter certified nothing the other did not).

Also here, without a measurable gain: `class_static_side_lacks` reads a TS
class expression's static side as a declaration's (`typeOfThisInStaticMembers12/13`'s
`static [this.c]` now reports; their instance-side `[this.c]` stays declined,
the class's own computed names leaving its member table uncertified).

Measured (§36 with the class-expression change) on `08c242f7` unfiltered:
types and diagnostics verdicts unchanged, 0 lost; exact oracle 9676 → 9676,
0 lost (`noImplicitAnyForIn` now differs only in the chain row's file, §19);
jsTyping 122 → 122, new_false 0, lost_true 0; Ir gi 0.9998, dm 0.9990 (one
reporter instead of two).

## §37 TS2454's block-scoped fast path requires a statement list

`uninitialized_variable_reads_declared` (check.rs, the TS2454 condition of
`checkIdentifier`'s flow arm) skips the walk for a read of an initialised
block-scoped declaration after it "in its own block" (`flow.md` §13, a
measured ~5% on `domain-model`). It tested only that the statement's parent
was not a `case`/`default` clause, so a declaration that is an `if`'s bare
body (`if (1 > 0) const e = 3; console.log(e);`, TS1156) was taken as assigned
on every path. Native walks: the else path never runs the declaration, and
reports TS2454 (`disallowedBlockScopedInPresenceOfParseErrors1`, four
reads). The fast path now requires the statement's parent to be a statement
list (`Block`, `ModuleBlock`, `SourceFile`), where statements run in order;
everything else walks.

Measured on `97cc68fc` unfiltered: diagnostics **+1**
(`disallowedBlockScopedInPresenceOfParseErrors1`), types unchanged, 0 lost;
exact oracle 9676 → 9677, 0 lost; jsTyping 122 → 122, new_false 0, lost_true 0;
Ir gi 1.0001, dm 1.0001.

## §38 `GetErrorRangeForNode` for an unnamed declaration and an arrow function (routed: `check.rs`)

`GetErrorRangeForNode` (`scanner.go:2588`) reports a name-reporting kind with
no name (`errorNode == nil`, `:2644`: an anonymous function expression, a
class expression, a default-exported function or class) at the construct's
first token, and an arrow function through `getErrorRangeForArrowFunction`
(`:2542`: a block body spanning lines reports through the end of its first
line). The port's `error_span` fell to the whole node's span for both, so
TS2872 on `function () { … } || undefined` covered the function where native
covers `function` (`generatedContextualTyping`, four rows).
`r7-flow-error-span-unnamed.diff` adds both arms to `error_span` (a shared
helper, not this lane's). Measured on `6efc0ae6` unfiltered: legacy verdicts
unchanged (start positions do not move); exact oracle 9677 → 9682
(`assignLambdaToNominalSubtypeOfFunction`, `commaOperatorLeftSideUnused`,
`exportDefaultInterfaceAndTwoFunctions`, `generatedContextualTyping`,
`parserUnaryExpression2`), 0 lost; jsTyping 122 → 122, new_false 0,
lost_true 0; Ir gi 1.0000, dm 1.0002. Approximation: the arrow's "first line" is the line of the body's
`{` (native: of the body's full start, the end of `=>`); they differ only when
`{` starts a new line.

## §39 `getAssignedType`'s destructuring arms

`getTypeAtFlowAssignment` narrows a union-typed reference assigned inside a
destructuring target by `getAssignedType(node)` (`flow.go:2288`), which
recurses through the target: an array literal element takes
`getTypeOfDestructuredArrayElement` of its literal's assigned type (a
tuple-like source's element, else the iterated element), a spread the array
of the iterated element, a property assignment `getTypeOfDestructuredProperty`
(the property, else an applicable index signature's value), a shorthand that
with its default, and a destructuring default (`[x = d]`, `{ p: x = d }`)
`getTypeWithDefault(getAssignedType(binary), d)` (`getAssignedTypeOfBinaryExpression`,
`:2314`). The port answered `None` (the declared type) for every
destructuring arm and read a default's `x = d` as the plain assignment `x =
d`: `[k = "", v = false] of map` narrowed `v: boolean` to `false`
(`for-of45`), and `[x] = [true]` left `x` at its declared union
(`assignmentTypeNarrowing`, `controlFlowAssignmentPatternOrder`).

`assigned_type` (flow.rs) is `getAssignedType` with those arms
(`type_of_destructured_array_element`, `type_of_destructured_property`,
`type_with_default`, `is_destructuring_assignment_target`); an arm this port
cannot decide (an unresolvable element or property) still answers `None`, the
declared type. No cache: the recursion is bounded by the target's nesting and
runs at an assignment flow node, as native's.

Measured on `6efc0ae6` unfiltered: types **+19** (`controlFlowAssignmentPatternOrder`
12, `assignmentTypeNarrowing` 6, `for-of45` 1), diagnostics **+1**
(`controlFlowAssignmentPatternOrder`), 0 lost; jsTyping 122 → **117**
unique lines, new_false 0, lost_true 0 (five false TS2322/TS2365 lines in
`program.ts`, `semver.ts` and `esDecorators.ts` gone); exact oracle 9677 → 9679, 0
lost; Ir gi 1.0000, dm
1.0000.

## §40 `GetRootDeclaration` in `isParameterOrMutableLocalVariable`

`GetRootDeclaration` climbs a binding element to its pattern's owner (`for
IsBindingElement(node) { node = node.Parent.Parent }`), so `let { oldProgram
} = options` is a mutable local whose closure reads past its last assignment
narrow (`isPastLastAssignment`). The port's loop climbed through parents of
the listed kinds and stopped on the pattern for a `VariableDeclaration`
owner (only a `Parameter` owner was in its list), so every destructured `let`
was "not a mutable local". It now climbs as native does.

Measured on `5aed9899` unfiltered: types and diagnostics verdicts unchanged,
0 lost; exact oracle 9679 → 9679, 0 lost; jsTyping 117 → **114** unique
lines, new_false 0, lost_true 0 (`program.ts`' three false TS18048 on the
destructured `oldProgram` read in callbacks gone); Ir gi 0.9999, dm 1.0000.

## §41 A union receiver's readonly property in `isConstantReference`

`isConstantReference` (`flow.go:1817`) admits a property access when its
receiver is constant and `isReadonlySymbol` of the resolved property holds;
for a union receiver that property is the synthetic union property, readonly
when **any** constituent's property is (`createUnionOrIntersectionProperty`,
`checker.go:21508`). This port's `is_readonly_property_of_type` assembles the
synthetic property for intersections only (`composite_property_of_type`), so a
union receiver's property read as writable, and the aliased-condition
narrowing (`const node = declaration.attributes; if (node)`) declined for
`declaration: ImportDeclaration | ExportDeclaration | JSDocImportTag`.
`union_property_is_readonly` (flow.rs) answers for a union whose every
constituent has the property, through each constituent's
`constituent_property_is_readonly`; otherwise the shared function answers as
before. The shared function's union gap is `members.rs`' (r7-shared).

Measured on `4c94490e` unfiltered: types and diagnostics verdicts unchanged,
0 lost; jsTyping 114 → **111**, new_false 0, lost_true 0 (`checker.ts`
38800, 45144 and 48616 gone); exact oracle 9679 → 9679, 0 lost; Ir gi
0.9999, dm 1.0001.

## §42 A compound-like assignment reads the declared type at its literal's base

`getTypeAtFlowAssignment` (`flow.go:243-246`) takes the declared type at its
literal's base when the assignment is compound-like (`x = x + 1`, a binary
right operand reading the target, `isInCompoundLikeAssignment`) before the
union reduction or the declared-type answer: `let foo = empty` (`empty: ""`)
is `string` after `foo = foo + "bar"`. The port answered the declared `""`
(`literalWideningWithCompoundLikeAssignments`, eleven lines). The assignment
arm now applies the same base read, through the existing
`is_in_compound_like_assignment` (the identifier road's twin, expressions.rs).

Measured on `f65819c4` unfiltered: types **+11**, diagnostics unchanged, 0
lost; exact oracle 9679 → 9680, 0 lost; jsTyping 111 → 111, new_false 0, lost_true
0; Ir gi 1.0000, dm 1.0001.

## §43 The effects signature of an optional call drops the callee's nullable half

`getEffectsSignature` (`flow.go`) types a call's callee by
`checkNonNullExpression`, or for an optional chain
`checkNonNullType(getOptionalExpressionType(…))`: either way the callee's
`null`/`undefined` constituents are gone before its call signatures are read.
The port read `check_expression(callee)` as it stood (its comment assumed a
chain's callee already answered the non-optional type, which holds for
`o?.f` but not for `f?.(x)` with `f: undefined | ((x: any) => x is number)`),
so the union with `undefined` had no call signatures and `if (f?.(x))` did not
narrow `x` (`controlFlowOptionalChain`: `x` four times, `o6` twice). The
effects road now takes `getOptionalExpressionType` for an optional call and
`GetNonNullableType` of a nullable callee (under `strictNullChecks`).

Measured on `ef353bb9` unfiltered: types **+6** (`controlFlowOptionalChain`),
diagnostics unchanged, 0 lost; exact oracle 9680 → 9681, 0 lost; jsTyping 111
→ 111, new_false 0, lost_true 0; Ir gi 1.0001, dm 1.0000.

## §44 Finding (routed): a union with a generic alias reference keeps it unflattened

`narrowingTypeofDiscriminant`'s `booleanBad(wrapped: WrappedStringOr<boolean> |
null)` does not narrow `wrapped` by `typeof wrapped?.value !== 'string'`: the
discriminant test (`isDiscriminantProperty` over the declared type,
`getDiscriminantPropertyAccess`) sees the union's constituents as `[null,
WrappedStringOr<boolean>]`, the alias reference standing as one constituent,
so `value` has one member type and is not a discriminant. Native unions are
always flattened (`getUnionType` adds a union's constituents; the alias and
origin only shape printing), so the constituents are `null`, `{ value?:
string }`, `{ value?: boolean }`. The same declaration without `| null`, or a
non-generic alias, narrows here. The union constructor is `unions.rs` /
`declared.rs`' (r7-declared); not changed in this lane.

## §45 The class-chain miss certificate declines a value base

§13's `class_chain_lacks` walks a class's `extends` chain by symbol, reading
each base symbol's declarations. A class's base type is its base
constructor's instance type (`getBaseConstructorTypeOfClass`): for a class
symbol that is its declared type, but for a value (`declare const Base02:
Base02Constructor` merged with `interface Base02`, whose construct signature
is `new <T = A>(a: T): Base02 & T`) it is the signature's return, `Base02 & A`
here. The walk read the merged interface and certified `a` absent from
`Derived03` (`genericDefaults`, a false TS2339). A class's base must now be a
class symbol (a class declaration or expression among its declarations);
anything else declines. An interface's `extends` keeps reading the type
meaning.

Measured on `ead5eb83` unfiltered: types and diagnostics verdicts unchanged
(`genericDefaults` stays WRONG on a TS2415 of the heritage check), 0 lost;
exact oracle 9681 → 9681, 0 lost; jsTyping 111 → 111, new_false 0, lost_true 0;
Ir gi 0.9999, dm 1.0001.

## §46 `elaborateNeverIntersection` on TS2339

`reportNonexistentProperty` chains the TS2339 for a never-reduced
intersection receiver to `elaborateNeverIntersection` (`checker.go:21868`):
"The intersection 'A & B' was reduced to 'never' because property 'kind' has
conflicting types in some constituents." (TS18031), or the private-conflict
form (TS18032), naming the first discriminant property with a `never` type,
else the first conflicting private one, and printing the intersection without
reduction. The port reported the head alone (the chain was "message text this
diagnostic model does not carry", before chains). `never_reduced_property`
(flow.rs, the scan of `intersection_has_never_discriminant` refactored to
also say which property and which kind) supplies it, on the property-access
road only, as native (`getPropertyTypeForIndexType` does not elaborate).
The flattened head now matches native (`intersectionReduction`,
`intersectionReductionStrict`); the chain row is file-less until §19's
infrastructure lands.

Measured on `c1a10e20` unfiltered: types and diagnostics verdicts unchanged,
0 lost; exact oracle 9681 → 9681, 0 lost; jsTyping 111 → 111, new_false 0,
lost_true 0; Ir gi 0.9999, dm
0.9999.

§31 update: `nullish.rs`' `getSyntacticNullishnessSemantics` identifier arm
made the same stale test (an `undefined` that "resolves to nothing"), so
`(opt ? undefined : null) ?? x` missed TS2871 (`predicateSemantics`, two
rows); `truthiness.rs` had already been corrected. The routed diff now covers
both files. Re-measured on `954817a5` unfiltered: types **+6**, diagnostics
unchanged, 0 lost; exact oracle 9681 → 9683, 0 lost; jsTyping 111 → 111,
new_false 0, lost_true 0; Ir gi 0.9999, dm 1.0000. `predicateSemantics`
keeps a TS2872 span difference: `GetErrorRangeForNode`'s `SatisfiesExpression`
arm (the `satisfies` keyword) is missing from the shared `error_span`, which
§38's routed diff could take too.

§38 update: the routed `error_span` diff now also has `GetErrorRangeForNode`'s
`SatisfiesExpression` arm (`scanner.go:2626`: the token after the expression,
the `satisfies` keyword). Re-measured on `bae795fe` unfiltered: legacy verdicts
unchanged; exact oracle 9681 → 9686 (the same five; `predicateSemantics`
gains with §31's diff as well), 0 lost; jsTyping 111 → 111, new_false 0,
lost_true 0; Ir gi 1.0001, dm 1.0002.

## §47 TS2542 through a `readonly` mapped type with a generic key

`checkIndexedAccessIndexType` (`checker.go:8219`) has a readonly arm the port
lacked: an element access that is an assignment target, whose type stayed a
deferred `Object[Index]` with every index constituent assignable to the
object's keys, and whose object is a mapped type with a `readonly` (or
`+readonly`) modifier, reports TS2542 "Index signature in type
'Readonly<T>' only permits reading." at the access, printing the mapped type
(`y[k] = x[k]` with `y: Readonly<T>`, `k: keyof T`; `mappedTypeRelationships`
×4). `check_readonly_mapped_index_write` (readonly_target.rs) reads the
access's own type and this port's deferred mint
(`deferred_indexed_access_types`) for the object and index, which is native's
`t.flags&IndexedAccess` test: the mint exists only when
`getIndexedAccessTypeOrUndefined` deferred. It is entered only when the
receiver is itself a readonly mapped type, so the common `array[i] = v` does
not read the access type.

Declined rather than reproduced: a remapping (`as`) mapped object, whose keys
are `getIndexTypeForMappedType`'s, and the `hasNumberIndexInfo` disjunct (an
index it alone admits declines; it never reports). Both directions miss
rather than over-report.

## §48 TS2536/TS4105: the final arms of `checkIndexedAccessIndexType`

TS2536 ("Type '{0}' cannot be used to index type '{1}'.") was not reported
anywhere. Native reports it, at the access node, for a deferred access whose
index has a constituent not assignable to the object's `getIndexType`, unless
the index names a property of the distributed apparent object that is
non-public, which is TS4105 ("Private or protected member '{0}' cannot be
accessed on a type parameter.") instead, for a generic object
(`getConstituentProperty`, `:8257`). Both arms are now
`check_deferred_access_index_type` (index_access_reports.rs), called from the
indexed access type node check (native `checkIndexedAccessType`) with this
port's deferred mint of the node, and from the element access check for a
generic index (native `checkElementAccessExpression`). The element access
road cannot use the mint: this port's `deferred_indexed_access` (indexed.rs)
mints only after the key is admitted, so the receiver (less a written
`null`/`undefined`, `checkNonNullExpression`) and index are read directly.

The relation is the judgment call. A port relation that answers
`NotRelated` where native answers `True` reports a TS2536 native never
does, so the arm uses `relate_ternary` and declines on `Unknown`. On the
jsTyping leg the plain `is_type_assignable_to` produced two false TS2536
(`parser.ts(504,59)`, `visitorPublic.ts(619,55)`: `TNode["kind"]` with a
mapped type parameter constrained to the 300-member `ForEachChildNodes`);
with the ternary both are `Unknown`, and two corpus true positives go with
them (`mappedTypeErrors2` (15,38), `intersectionsOfLargeUnions` (21,15)'s
second report).

The type node arm is declined where native reaches the node through a
substitution or not at all:
- the true branch of a conditional type, and the template of a mapped type
  without an `as` clause: `getConditionalFlowTypeOfType` substitutes the
  implied constraint there (and `number | `${number}`` for a mapped type's
  parameter over an array-constrained homomorphic variable, which made
  `mappedTypeInferenceFromApparentType` (6,59) a false report). This port has
  no substitution types on that road; native's report in a true branch
  (`P["ctx"] extends object ? P["ctx"] : object` reports both) is a missed
  positive here;
- an import type's type arguments: `checkImportType` (`:3324`) never
  `checkSourceElements` them, unlike `checkTypeReferenceNode` (`:2994`); the
  port's walk visits them (`declarationEmitIsolatedDeclarationErrorNotEmittedForNonEmittedFile`
  was a RIGHT→WRONG loss before this decline).

Also declined: a remapping mapped object, an apparent `number` index info,
and an object or index this port could not compute (`is_gap`,
`unresolved_types`; `keyof U` for a constrained `U` is such a gap today, so
`mappedTypeRelationships`' `x[k]` with `k: keyof U` stays missing).
Known remaining false reports, in a case already EMPTY_WRONG:
`ramdaToolsNoInfinite2` (45,83) and (60,42), where this port resolves an ambient module's
`import { Boolean } from "Boolean/Boolean"` to the global `Boolean`
interface, so the index's constraint is wrong before this rule reads it
(routed to the integration thread).

No table or cache: the relation's own caches. The type node arm runs one
relation per deferred indexed access type node, lib files included.

Measured (§47 and §48 together) on `de5cbc86` (main `cdb668dc` + the three
§46/§31/§38 commits, rebased): types unchanged; diagnostics **+1** case
(`indexedAccessPrivateMemberOfGenericConstraint`, its three TS4105), 22 more
right diagnostics across nine still-wrong cases (`mappedTypeRelationships`'
four TS2542 and two TS2536, `mappedTypeErrors2` ×4, `keyofAndIndexedAccessErrors`
×2, `constraintWithIndexedAccess`, `intersectionsOfLargeUnions(2)`,
`unknownControlFlow`), 0 lost; exact oracle 10635 → 10636, 0 lost; jsTyping
gap 28 → 28, new_false 0, lost_true 0; Ir gi 1.0000, dm 1.0013.

## §49 NONEXISTENT-PROPERTY-CERTIFICATION-GATE: an exact certificate for a narrowed union receiver

§20's gate refused every union receiver a narrowing could reach, because a
narrowing arm that cannot decide returns its input, and from the result a
declined narrowing is indistinguishable from none. The certificate is now
exact about that: the walk counts (`FlowState::declines`, and the deferred
`FlowState::unchanged` list) every step that may have answered wider than
native, and a union receiver is certified only when no walk of it met one
(`flow_type_is_certified`, reading the `flow_uncertain_references` set the
walk roads fill).

Counted:
- a condition narrowing (`narrow_type`, so every nested operand too), a
  switch clause, or an assertion/predicate call that left its input
  unchanged at a node that mentions the reference's root (an identifier with
  the root's text, or `this`/`super` for a `this` root; text only, no name
  resolution). This over-counts native's legitimate no-op narrowings, which
  only keeps those receivers uncertified as before;
- a CALL flow node with no effects signature whose call mentions the root:
  this port's `None` is also its answer for an uncomputed one;
- an assignment whose assigned type this port cannot reach, on the auto and
  the declared-union roads (the declared type where native reduces);
- a loop or branch join whose subtype reduction the relation could not decide
  (`union_with_subtype_reduction` → `None`);
- a `flow_loop_cache` hit whose converging walk counted one (the cache value
  carries the bit; a replay skips the walk);
- a condition that made `silentNeverType` (§21): its consumers are still the
  routed `r7-flow-silent-never-callers.diff`, and without them
  `controlFlowWithIncompleteTypes`' loop converges to `string | boolean`
  where native has `string` (the one loss the certificate alone measured,
  +3/−1). Drop this count when the diff lands.

A step that narrows partially (keeps an undecidable constituent and drops
others) would not be counted by the unchanged rule. The constituent-wise
filters read for this (`narrow_type_by_equality`'s comparable filter, the
discriminant and `typeof` arms) decline whole on an undecidable pair
(Kleene), so the unchanged rule sees them; an arm found to keep a partial
answer needs its own count.

The walk roads that record (`flow_uncertain_references`):
`get_flow_type_of_reference_ex`, `narrow_destructured_parent`,
`get_flow_type_of_property` and `narrow_reference_by_condition`, keyed by the
reference. A reference no road recorded reads as certified, so
`union_receiver_flow_is_uncertified` still requires a narrowable kind and
keeps `flow_cannot_narrow_identifier` as the walk-free proof. Grant: the two
`checker.rs` lines (the set, and the cache value's `bool`), requested of the
integration thread.

Cost. Scanning each unchanged narrowing's condition for the root as it
happened measured domain-model Ir **1.024** (the scan ran for every
unrelated condition of every walk). The unchanged nodes are now only
recorded, and the scan runs when a walk publishes an answer that could be
asked about (the answer or the walk's declared type is a union), and for a
loop label's cache bit: Ir gi 0.9999, dm 1.0018, with identical legacy
verdicts.

Rejected: instrumenting each arm's internal decline points. There are dozens
(`grep -n declin flow.rs`), an arm missed is a false TS2339, and the
result-unchanged test covers every arm by construction, including ones added
later. It would win if the over-count (native no-op narrowings) turned out to
hide many reports; the falsifier is a corpus case where native reports a
TS2339 on a union receiver after a no-op narrowing.

Measured (§49 with §50) on `fbb108f7`: types unchanged; diagnostics **+3**
cases (`isArray`, `unionPropertyExistence`, `unionTypeReadonly`) and 33 more
right TS2339 across eight still-wrong cases (`controlFlowAliasing` ×8,
`typeGuardsWithInstanceOfByConstructorSignature` ×4,
`typeGuardsWithInstanceOfBySymbolHasInstance` ×4, `inKeywordTypeguard` ×2 per
variant, `typeGuardRedundancy` ×2, `unionTypeWithIndexSignature`), 0 lost;
exact oracle 10812 → 10812, 0 lost; jsTyping gap 28 → 28, new_false 0,
lost_true 0 (before §50, two false TS2339); Ir gi 0.9999, dm 1.0018.

§20 update: the blanket gate is replaced by this certificate; §20's three
remaining losses are certified-uncertain here (`typeGuardNarrowByUntypedField`
×2 and `discriminatedUnionTypes3` decline in the narrowing, and
`controlFlowWithIncompleteTypes` makes `silentNeverType`).

## §50 An array mutation evolves only the array it mutates

`getTypeAtFlowArrayMutation` (`flow.go:1393`) acts only when the mutated
expression (`x` of `x.push(…)`/`x.unshift(…)`, or of `x[i] = …`) is the
reference, through `getReferenceCandidate`, and adds an element assignment's
right-hand side only when the index's context-free type is number-like. The
port's arm added every `ARRAY_MUTATION` node's arguments to whichever
evolving array was being walked, so two auto arrays in one function shared
their elements: `const a = []; const b = []; a.push(1); b.push("s")` typed
both `(string | number)[]`. In jsTyping's `checker.ts`
`runWithoutResolvedSignatureCaching` this made `cachedResolvedSignatures`
yield `NodeLinks | SymbolLinks`; §49's certificate exposed it as two false
TS2339 (`checker.ts(2004,27)`, `(2007,29)`), which is how it was found.
`array_mutation_target` reads the mutated expression; the depth step (§14)
now follows native's recursion, which happens only on a matching mutation.
The index check reads `check_expression` (this port has no context-free
expression type); a contextually typed index expression is the one place
the two could differ.

## §51 `'{0}' is declared here.` on TS2729 and TS2551; chain children carry their file

`checkPropertyNotUsedBeforeDeclaration` (`checker.go:11727`) and
`reportNonexistentProperty`'s suggestion arm (`:11574`) add the related
`'{0}' is declared here.` at the property's (the suggested symbol's) value
declaration. The port reported both heads without it; 52 oracle cases differed
only by such related rows (67 TS2729, 16 TS2551 among them). The suggested
symbol is the property the suggestion names on the type its candidates came
from (`get_property_of_type`), so a union's synthetic property supplies
whatever value declaration this port gives it.

A chain child built with `NewDiagnosticForNode(accessExpression, …)` has a
file and a location; the port's children were file-less (`Diagnostic::with_args`),
so the oracle's C rows differed (16 cases differed only there). The TS7053
children (`nonexistent_property.rs`' two element-access arms,
`report_implicit_any_element_access`) are now built with
`related_diagnostic_at`, which sets the file image. Children native builds
with `chainDiagnosticMessages` (the union constituent miss,
`elaborateNeverIntersection`) stay file-less, as there.

## §52 TS7052, and one noImplicitAny report per union key

`getSuggestionForNonexistentIndexSignature` (`checker.go:27231`) was declined
wherever a `get`/`set` member existed. It is now
`suggestion_for_nonexistent_index_signature` (index_access_reports.rs), shared
by the literal-key and non-literal-key element-access arms: the member's
single call signature (one call signature, no construct signature, index
info or property), at least one required argument, and the key assignable to
its first parameter; the suggestion prefixed by
`tryGetPropertyAccessOrIdentifierToString` of the receiver. It declines (no
report at all) for a member type whose signatures this port does not hold (an
interface or class instance) and for a first parameter that still mentions a
type parameter: `MyMap<string, string>`'s `get(key: K)` read through
`get_type_of_symbol` was uninstantiated, and asking `"a"` against `K` made a
TS7053 where native reports TS7052 (`noImplicitAnyStringIndexerOnObject`
(102,1), now missing rather than wrong).

`getIndexedAccessTypeOrUndefined` (`:26985`) asks a union key's constituents in
order and sets `AccessFlagsSuppressNoImplicitAnyError` once one has missed,
so `o[k]` with `k: "a" | "b" | "c"` reports one TS7053 (for `"b"`); the port
reported per literal. The noImplicitAny arm now returns when an earlier
literal of the same key has no property that no index signature certainly
admits. The object-literal TS2339 arm and the const-enum final arm are not
suppressed, as native. The TS7053 child names an enum literal constituent
`[E.p]` (`:27167`).

## §53 A `null`/`undefined` destructuring parent; computed names that cannot bind

`const { a } = null` reports TS2339 "Property 'a' does not exist on type
'null'." in either mode: `getBindingElementTypeFromParentType` passes the
parent on and `getApparentType` maps neither `null` nor `undefined`, so
`getPropertyTypeForIndexType` finds no property and no index info.
`destructured_property_is_absent` now answers the parent itself for those two
intrinsics (`usingDeclarations.6`/`.7` and their `await` forms).

`computed_name_is_bound` (the class/interface member-table certificate)
refused every computed member name but a well-known `Symbol.x`. A computed
name whose expression is not an entity name expression (`[this.c]`, `[f()]`)
can never be late-bound (`isLateBindableName`) and adds no member, so it no
longer refuses the table; a computed name that is a string or numeric literal,
or an entity name, still does. `typeOfThisInStaticMembers12`/`13` (three
targets each) lacked the instance arm's TS2339 for this reason alone.

Measured (§51–§53 together) on `06cb8ae9`: types unchanged; diagnostics **+8**
cases (`usingDeclarations.6`, `awaitUsingDeclarations.6`,
`typeOfThisInStaticMembers12`/`13` ×3 targets) and 21 more right TS7052/TS7053
in `noImplicitAnyStringIndexerOnObject`, 0 lost; exact oracle 10816 → **10868**
(+52), 0 lost; jsTyping gap 25 → 25, new_false 0, lost_true 0; Ir gi 0.9999,
dm 0.9998.

Finding (routed): a widened object literal's arrow member prints its return as
`error` (`const c = { get: (key: string) => 'hello' }` prints
`{ get: (key: string) => error; }`, native `string`), so the new TS7052 for
such receivers carries that text; and a non-strict `[]` is `never[]` here
where native has `undefined[]` (`downlevelLetConst16`'s `for (let {a: x} of
[])`).
