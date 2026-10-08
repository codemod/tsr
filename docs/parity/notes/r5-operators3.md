# Lane notes: r5-operators3 (`tsr-2zk.968`, continuing `.941`)

Round-5 cloud lane continuing [r4-operators2.md](r4-operators2.md). Native
source is `vendor/typescript-go/internal/checker/` @ `5b1047d`. Baseline
frozen at integration head `ac56208` (origin/main): diagnostics 10621
right (5070 RIGHT + 5551 EMPTY_RIGHT) of 12238 cases, types 543119 RIGHT
lines. Every number below is against that baseline, from unfiltered
`diagverdictdump` / `verdictdump` runs.

**Outcome: no code commit from this lane this round.** The one diff in an
owned file (TS18046) still loses a case, and the cause is call inference,
not the operator report. The other diff is lossless but changes
`assignreport.rs`, which this lane does not own. Both are measured below so
the integrator can land them in order.

## 1. TS18046 / TS2571 for an `unknown` operand: held, blocked on inference

`docs/parity/notes/r4-operators2-unknown-operand-ts18046.diff`
(`nullable_operand.rs` `check_non_null_type_reporting`) reports what
`checkNonNullTypeWithReporter` (`checker.go:7413`) reports for an `unknown`
operand under `strictNullChecks`: TS18046 for an entity name shorter than
100 characters, TS2571 otherwise.

**Measured alone** against `ac56208`: diagnostics 10621 → 10620. No case
converts; `unknownType1` (5 of 9 missing lines) and
`useUnknownInCatchVariables01` (1 of 2) gain lines. Types are unchanged.
**One loss**: `nonInferrableTypePropagation2` EMPTY_RIGHT → EMPTY_WRONG
(TS18046 at `n > 0`, line 34 of the source).

**The loss is a correct report on a wrong type.** On the baseline, without
the diff, the type dump of that case already has `(n) => n > 0 :
(n: unknown) => boolean`, where native has `(n: number) => boolean` (lines
`0:28`, `0:30`, `0:31`, `0:33` WRONG). The note in
`operators.md` / r4-operators2 §5 said the dump answered `number` while the
diagnostics walk saw `unknown`; on this head both say `unknown`, so the
reporter just shows TSR's own type. Muting the report for this shape would
be the §3a heuristic. The fix belongs to the inference producer.

The smallest repro (native tsgo infers `A = number` in all three; TSR infers
`unknown` exactly when the callee of the middle call is overloaded):

```ts
interface Predicate<A> { (a: A): boolean }
declare const f2: {
    <A>(predicate: Predicate<A>): (as: ReadonlyArray<A>) => ReadonlyArray<A>
    <A>(predicate: Predicate<A>, x: number): (as: ReadonlyArray<A>) => ReadonlyArray<A>
};
declare const f1: <A>(predicate: Predicate<A>) => (as: ReadonlyArray<A>) => ReadonlyArray<A>;
declare function pipe<A, B>(a: A, ab: (a: A) => B): B;
declare function id<A>(predicate: Predicate<A>): Predicate<A>;
declare const es: number[];
const a1 = pipe(es, f1(id((n) => n > 0)))                                // TSR: number
const a2 = pipe(es, f2(id((n) => n > 0)))                                // TSR: unknown
const b1: (as: ReadonlyArray<number>) => unknown = f2(id((n) => n > 0)) // TSR: unknown
```

`b1` takes `pipe` out of the picture. The context-sensitive parameter of an
argument (`id(...)`) to an **overloaded** callee does not get the type
inferred from the callee's contextual return type. Upstream's
`chooseOverload` runs `inferTypeArguments` for each candidate, including
the contextual-return inference (`checker.go:9390`), so the nested call's
contextual type (`Predicate<A>` under that candidate's context) carries
`A = number`. In TSR, `contextual_return_inferences` (`inference.rs`)
and the contextual type of an argument to a multi-candidate callee
(`contextual.rs` / `calls.rs`) are main's active files. Routed; the TS18046
diff lands, unchanged, once this case's types are RIGHT.

The other TS18046 cases (`catchClauseWithTypeAnnotation`,
`controlFlowAliasingCatchVariables`, `es2016IntlAPIs`,
`jsdocCatchClauseWithTypeAnnotation`, `privateNameAndAny`,
`reverseMappedPartiallyInferableTypes`, `useUnknownInCatchVariables01`
line 6, `unknownType1` lines 51/52/63/64) all sit at a **property-access
receiver** (`e.toUpperCase()`, `x.foo`, `x[10]`). That is
`checkPropertyAccessExpressionOrQualifiedName`'s `checkNonNullExpression`,
which the property-access lane owns. These cases never depended on this
diff. TS2571 in `bindingPatternCannotBeOnlyInferenceSource` (destructuring
of `unknown`) and `typeArgumentInferenceWithClassExpression2` (receiver)
are not operator-side either.

**Falsifier.** If `nonInferrableTypePropagation2`'s types become RIGHT and
the diff still reports TS18046 there, the report itself is wrong.

## 2. `in` operands through `checkNonNullType`: measured, lossless

`docs/parity/notes/r4-operators2-in-operand.diff` (unchanged,
`assignreport.rs` `check_in_expression`, owner r5-report). It makes both
operands go through `check_non_null_type_reporting`, as `checkInExpression`
does (`checker.go:13095`, `:13098`).

**Measured alone** against `ac56208`:

- diagnostics 10621 → 10622 (+1 case, `inOperatorWithInvalidOperands`
  WRONG → RIGHT);
- `widenedTypes`' two missing TS18050 now match (the case stays WRONG on an
  unrelated extra TS2322 at line 12);
- `privateNameInInExpression` (both targets) gains its TS18047 (`#field in
  u`, `u: object | null`);
- types unchanged (543119);
- **zero losses** in both checks.

Perf, median child CPU over 21 samples, new/base: domain-model 1.006,
generic-imports 0.994, `diagnostics_match: true`.

**Measured with §1 together**: diagnostics 10621 (+1
`inOperatorWithInvalidOperands`, −1 `nonInferrableTypePropagation2`, the
§1 loss). The pair also supplies `privateNameInInExpression`'s TS2571
(`#field in v`, `v: unknown`) and `inKeywordTypeguard(strict=true)`'s
TS18046 (`"a" in x`). Neither case converts, because each has unrelated
missing lines (TS18016/TS2339/TS1451/TS2406; TS2339).

## 3. Remaining operator-code mismatches in this lane's files: none operator-side

Each case was checked against the native baseline.

| Code | Case | Cause | Owner |
|---|---|---|---|
| TS2362/2363 extra ×10 | `mappedTypeProperties` | Parser. Native parses `{ [P in K]: void; model: ... }` as a mapped type and reports TS7061 (9 missing). TSR's tree differs: extra TS1005/TS1128/TS2304/TS2693, and the `'hour' \| 'day'` member type ends up as a `\|` binary expression | parser |
| TS2367 missing | `unknownControlFlow` line 341 (`fx4`) | `value: T & ({} \| null)` with `T extends {} \| null`. Native reduces the declared intersection to `T` (type dump `value : T`); TSR keeps `T & ({} \| null)` (type lines `0:419`–`0:424` WRONG), so the comparison overlaps. The comparison arm is right for its input; the `fx2` twin at line 323 already matches | intersection construction (`getIntersectionType` constraint reduction) |
| TS18047 missing | `privateNameInInExpression` | §2 diff | r5-report |
| TS2532 missing | `narrowingOfQualifiedNames` | narrowing of qualified names | flow |
| TS2532 missing | `destructuringAssignabilityCheck`, `strictNullEmptyDestructuring` | binding-pattern `checkNonNullType` | destructure |
| TS2532 missing | `optionalChainWithInstantiationExpression1` | `a?.b<c>.d`: receiver of a property access after an instantiation expression (TS1477 also involved) | property access |
| TS18048 missing | `discriminateWithOptionalProperty4`, `mappedTypeIndexedAccessConstraint`, `typeofThis` | property-access receivers | property access |
| TS18050 missing | `omittedExpressionForOfLoop` | `checkRightHandSideOfForOf` | iteration |
| TS18050 missing | `functionsMissingReturnStatementsAndExpressions` | `throw undefined.` receiver of a property access | property access |

## 4. Environment note

The offline bootstrap's hash-pinned `tomlkit` could not be installed: PyPI
answers 403 through the proxy, and loading it from a git checkout was
refused. `assemble.py` only needs `parse`, `inline_table` and `dumps`. A
throwaway stdlib-only stand-in for those three, kept in the session
scratchpad and not committed, produced a working vendored tree, and the
release build succeeded. If later boxes hit the same block, the faithful
fix is to make the offline Python install part of box setup again, not to
commit a shim.
