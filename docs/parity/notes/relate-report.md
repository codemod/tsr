# Lane `relate-report` (tsr-2zk.1) — relation error reporting and elaboration

Working notes for the parity box on epic `tsr-2zk`, lane issue `tsr-2zk.1`.
Case list: `docs/parity/lanes/relate-report.txt` (568 cases at `06f25e0`).
Upstream pinned at `vendor/typescript-go` @ `5b1047d`.

## 1. How the lane was bucketed

`crates/tsr-conformance/examples/relatelane.rs` lists every *missing* TS2322
line (or `CODE=<n>`) in the lane's cases with the
`report_assignability_failure` gate that decided it — `NEVER` (no
assignability report was asked at that position), `DECLINED` (the relation
did not answer `NotRelated`), `NOTREPORTABLE`, `OLUNION` — and the innermost
node kind at the position. `VERDICT=1` instead prints the lane's cases in
`diagverdictdump`'s format (a 7 s inner loop instead of the 3 min full dump).

Measured at `0d996e8` (before any change in this lane), 591 missing TS2322
lines:

| gate | lines |
|---|---|
| NEVER | 347 |
| DECLINED | 191 |
| NOTREPORTABLE | 32 |
| OLUNION | 16 |
| REPORTED (elsewhere) | 5 |

The NEVER bucket is dominated by elaboration positions: object-literal
property assignments (65), array-literal elements (~70), arrow-function
expression bodies (~48), JSX attributes (18). That confirms the brief's
hypothesis 1 *as a position* — but most of these are not missing
elaboration code. `elaborate_error` and its family exist in
`assignreport.rs`; the positions are NEVER because the **outer** check that
would hand the expression to `elaborate_error` is never made. The largest
families:

- **call arguments** whose parameter has no annotation, an overload set, or a
  generic signature (`destructuringParameterDeclaration1ES5`: 10 lines, every
  one an element of an array-literal argument to a function whose parameter
  type comes from an initializer). TS2345/TS2322-in-argument reporting lives
  in `call_arity.rs::check_argument_types`, which only handles one sole,
  non-generic, fully annotated signature. Not this lane's file; listed in the
  final report.
- **expression statements** (`x = y` assignments, 79 DECLINED + 47/32 NEVER),
  which split between relation completeness and property/element-access
  assignment targets that `check_assignment_operator` declines.

## 2. Ported: index-signature targets in `elaborateObjectLiteral`

`elaborateElement` (`relater.go:546`) reads the target member through
`getBestMatchIndexedAccessTypeOrUndefined` → `getIndexedAccessTypeOrUndefined`,
which falls back to the target's applicable index signature
(`getPropertyTypeForIndexType`) when the name is not a property. The port read
`get_type_of_property_of_type` only, so `var o: { [s: string]: number } = { p: "" }`
skipped the member, elaboration answered "nothing to say", and the outer
TS2322 was reported at `o` instead of at `p`. One MISSING plus one EXTRA per
case.

The fix asks `get_applicable_index_info` with the name's literal type
(`getLiteralTypeFromPropertyName`: a numeric-literal name is a number literal,
everything else a string literal), so numeric index signatures apply to
numeric names exactly as upstream. Computed names are still skipped (they were
before; `identifier_text` does not read them).

Converted: `compiler/contextualTypeAny`, `compiler/controlFlowForIndexSignatures`,
`compiler/objectLiteralIndexerErrors`.

Would be wrong if: a case reports TS2322 at an object-literal member against
an index signature that upstream reports at the outer node. None in the
corpus at this commit (zero-loss check empty).

## 3. Found, outside this lane's files

- **Qualified enum type references.** `let x: A.E = B.F.a` (both enums in
  namespaces) relates the source to a `Named { members: Some(<enum symbol>) }`
  OBJECT image for `A.E`, not to the enum's union of member literals, and the
  relater (correctly, for that image) answers `Related`. Unqualified
  `let x: E = F.a` reports TS2322 as upstream. The type-reference resolution
  for a qualified name naming an enum is `declared.rs` (type-refs box).
  Unlocks the enum families of `enumAssignmentCompat3` (12 lines) and
  `enumAssignmentCompat6` (6 lines).

## 4. Ported: the generic-mapped-target arm of `structuredTypeRelatedToWorker`

`relater.go:3593`. A source `S` against `{ [P in Q]: T }` / `{ [P in Q as R]: T }`
(a mapped type whose constraint or name type is still generic) had no arm, so
`U -> { [P in keyof U]: U[keyof U] }` fell to the source type parameter's
`unknown` constraint and answered a confident `NotRelated` — a false TS2322
(`compiler/mappedTypeParameterConstraint`). `Relater::generic_mapped_target_related_to`
mirrors the arm: the `{ [P in Q]: S[P] }` identity shortcut, then (for a
non-generic-mapped source) `Q`/`R` related to `keyof S` (`?` targets need a
non-empty key intersection), then the `Obj[P]` fast path or `S[P] -> T`.

Deviations, each conservative (they leave a pair undecided rather than
deciding it differently):

- `keyof S` is `resolved_keyof_type`, which includes index-signature keys where
  upstream asks `IndexFlagsNoIndexSignatures`; a source with index signatures
  (or an unresolved index table) skips the arm.
- An `Unknown` key relation or an indexed access the port cannot build answers
  `Unknown`; upstream's arm would fall through to the remaining arms with a
  definite answer. `V -> { [P in keyof W]: W[P] }` therefore stays undecided
  (no report) where upstream reports TS2322.
- `isGenericMappedType` is the existing over-approximating
  `is_generic_mapped_target`, used for both sides.

Would be wrong if: a pair decided `Related` by the arm is reported by upstream.
Zero-loss check at this commit is empty on both suites.

## 5. Ported: widened object-literal types have a property table for the relation reporters

`var a = { x: 1, y: 2 }; a = { x: 1 }` is TS2741 upstream (`reportUnmatchedProperty`)
and `a = { x: 1, z: 3 }` is TS2353 (`hasExcessProperties`). The port reported
TS2322 for both, because `member_completeness` certified no table for a type
whose declaration is an `ObjectLiteralExpression` (the widened, regular
literal type — the fresh one already reads its captured list), so
`missing_required_property` and `check_excess_properties` declined.

`Checker::relation_property_table` / `relation_members_are_complete` add the
literal's own written member list (no spreads, no computed names, not JS) and
are read by **this lane's reporters only**. Measured alternatives:

| where the object-literal table is admitted | diagnostics | losses |
|---|---|---|
| `declared_property_table` **and** `declared_members_are_complete` | +8 | 1 (`nonPrimitiveAndEmptyObject`) |
| `declared_property_table` only | +1 | 0 |
| lane reporters only (chosen) | +1 | 0 |

The loss in the first row is TS2339 (`crate::nonexistent_property`, not this
lane's file) on `fooProps.barProp` where `fooProps: (BarProps & object) | {}`:
upstream's `createUnionOrIntersectionProperty` gives an object-literal
constituent that lacks the property an `undefined` member instead of failing
the lookup. Once that union rule is ported, admitting object literals in
`declared_members_are_complete` is worth the other seven cases
(`checkingObjectWithThisInNamePositionNoCrash`, `importWithTrailingSlash`,
`lambdaParamTypes`, `requireOfJsonFileInJsFile`,
`requireOfJsonFileWithEmptyObjectWithErrors`, `thisInObjectLiterals`,
`typeSatisfaction_optionalMemberConformance`). The middle row equals the chosen
one, so the narrower change was kept.

Converted: `compiler/typeMatch2`.

## 6. Ported: TS2561 in the excess-property check

`hasExcessProperties` (`relater.go:2714`) reports TS2561 ("Did you mean to
write …?") when the excess name is an identifier with a spelling suggestion
among the target's properties (`getSuggestionForNonexistentProperty`), else
TS2353. The port *returned* on a suggestion — leaving the outer TS2322 to be
reported at the declaration instead. Now it reports TS2561 at the name; a
string-literal name never gets a suggestion, as upstream. The suggestion is
`crate::check::spelling_suggestion` over the certified table's names (only the
code and position are compared by the suite).

Converted: `compiler/spellingSuggestionLeadingUnderscores01`.
`objectLiteralExcessProperties` gains its TS2561 lines but still needs union
and intersection excess checks (`findMatchingDiscriminantType`,
`isKnownProperty` over intersections).

## 7. Ported: `object` against a target requiring a property

`structuredTypeRelatedTo` relates the non-primitive `object` through its
apparent type, the empty object type; `propertiesRelatedTo` then fails on a
required target property the empty object (Object's members included) cannot
supply. The port answered `Unknown` (the empty object has no member table
here). The arm in `is_related_to_with_excess` takes only that definite
negative, from `relation_property_table(target)`; every other `object` pair
keeps its existing path. A lib target such as `Date` has no certified table
and stays undecided.

Converted: `conformance/nonPrimitiveAssignError`.
