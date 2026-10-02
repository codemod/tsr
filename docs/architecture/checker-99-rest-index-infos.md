# Object rest index infos and unchecked binding reads

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 2671c87c: 456,480/478,855 matching assertions (95.33%).
Unit: index signatures (tsr-6.25).

## Forcing cases

noUncheckedIndexedAccessDestructuring had 38 non-matching rows. Two mechanisms
caused them:

- `const { t1 } = strMap`, where `strMap` is `{ [s: string]: string }`, printed
  `string`. Native prints `string | undefined`.
- `const { ...t2 } = strMap` and `const { x, ...q } = numMapPoint` printed
  errorType. Every member read from that rest object followed it to `any`.

## Native rules

getBindingElementTypeFromParentType (checker.go:17707) reads an object binding
element through getIndexedAccessTypeEx with AccessFlagsExpressionPosition.
getIndexedAccessTypeOrUndefined (checker.go:26947) turns that flag into
AccessFlagsIncludeUndefined under noUncheckedIndexedAccess. Then
getPropertyTypeForIndexType (checker.go:27117) adds missingType to an index-signature
result. The enum carve-out still applies: E[E.A] does not get undefined. A
declared property never gets undefined this way. The port already had this
rule for element access in `include_unchecked_undefined`, so the binding element's
literal-name and computed-name index fallbacks now call the same helper.

getRestType (checker.go:17792) runs these steps:

1. Filter out nullable constituents.
2. Map `never` to the empty object.
3. Distribute over unions.
4. Copy the spreadable properties as `getSpreadSymbol(prop, false)`.
5. Build `newAnonymousType(..., getIndexInfosOfType(source))`.

Before getRestType runs, the binding element rejects an unknown source or a
source for which isValidSpreadType is false (checker.go:17723). That rejection
reports TS2700 and answers errorType, which prints as `any`.

The old port used a printed-member enumerator (`spread_members_of`). It had no
index infos and no intersection support. Its result was a member-less printed
type, so property reads on it failed. The rest now uses the same semantic
`spread_properties` that the object-spread fold uses. It keeps those property
records and the source's index infos, then builds the type through the spread
fold's anonymous-type constructor, without the object-literal and freshness
flags.

Intersections had no property enumeration on this road. That made both
`{ ...a & b }` and rest from an intersection fail. getPropertiesOfUnionOrIntersectionType
(checker.go:18861) lists names in constituent order. createUnionOrIntersectionProperty
(checker.go:21452) has two cases:

- A name with one holder resolves to that holder's own symbol.
- A shared name is a synthetic `Property`. Its type is the intersection of the
  holders' types, and it is optional only when every holder is optional.

A private or protected holder marks the synthetic property as non-spreadable.
`intersection_spread_properties` implements exactly these cases. It does not add
general intersection support to `get_property_names_of_type`, because that
function has many more consumers and its effect on them has not been measured.

## Exposed prerequisite: nested assignment targets

The first full run had three GAP-to-WRONG rows in objectRestAssignment. A nested
pattern `{ a: [{ ...nested2 }, ...y], b: { z, ...c } } = overEmit` now types `c`.
That exposed the array literal `[{ ...nested2 }, ...y]`, which printed as an array.
checkArrayLiteral sets `inDestructuringPattern := ast.IsAssignmentTarget(node)`
(checker.go:8026), so nested targets mint tuples too. The port only recognized
the direct left operand of `=`. It now uses `assignment_target_kind`.

That broader gate lost two RIGHT rows in restElementWithAssignmentPattern1/3.
The cause was `[a, b = 0]`, which minted `[string, 0]` because the tuple branch
read elements with check_expression. Native reads every element through
checkExpressionForMutableLocation, which gives `[string, number]`. With that read
in place, the losses became gains.

## Measurement

All runs are full-corpus `scorepair` runs against 2671c87c:

| step | right | adverse |
|---|---:|---|
| unchecked binding reads and rest index infos | +113 | 2 R→W (restInvalidArgumentType r14/r15), 3 G→W |
| plus the TS2700 validity gate | +115 | 3 G→W (objectRestAssignment) |
| plus the nested assignment-target tuple gate | +136 | 2 R→W (restElementWithAssignmentPattern1/3) |
| plus mutable-location tuple elements | **+151** | **none** |

The final transition set is 130 WRONG→RIGHT and 21 GAP→RIGHT. It has no RIGHT
losses, no GAP→WRONG and no changed WRONG rows. The landing re-measure on the
rebased commit is recorded in STATUS §7.

A pinned tsgo build (`go build ./cmd/tsgo` at the submodule commit, declaration
emit) confirms three focused results:

- `const { a, ...rest } = v!` over a three-way union with `undefined` emits
  `{ b: string; } | {}`.
- `const { ...r14 } = u` with `u: undefined` reports TS2700 and emits `any`.
- `{ ...b }` with `b: { x: number } & { y?: string }` emits `{ x: number; y?: string; }`.

## Limits

- **Generic constituents in a union rest.** A generic constituent declines.
  Native would build an `Omit` for it, and only a bare type-parameter source
  takes that branch in this port.
- **Synthetic intersection properties.** These properties keep the first
  holder's symbol as their origin, which approximates the ordering that native
  gets from the synthetic symbol's first declaration. Readonly and
  late-bound check flags of synthetic intersection symbols are not modeled.
- **Rest-only array targets.** `[...[a, b = 0]] = ["", 1]` keeps the ordinary
  array road. A rest-only target still prints `(string | number)[]` where native
  prints `[string, number]`. §431 excluded that form because of the
  `[...obj?.a] = x` controls.
- **ObjectRestType.** No ObjectRestType flag is recorded, so
  isObjectTypeWithInferableIndex does not yet treat rest objects as having an
  inferable index (relater.go:4624).
