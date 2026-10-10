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
