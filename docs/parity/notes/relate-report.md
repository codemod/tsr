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
