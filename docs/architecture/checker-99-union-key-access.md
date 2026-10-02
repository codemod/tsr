# Union-key element access and identifier-key references

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 6c230ca2: full-run `scorepair` baseline (456,915 aligned RIGHT).
Unit: indexed access (tsr-6.30, tsr-6.25).

## Forcing case

`o[k]` with `k: "a" | "b"` answered errorType. A union of literal keys names no
single property, so the expression road fell through the index signatures to the
miss. The same gap blocked controlFlowComputedPropertyNames (`obj[key]` with
`key: keyof Thing`) and indexedAccessWithFreshObjectLiteral.

## Native rules

getIndexedAccessTypeOrUndefined (checker.go:26975) handles a non-boolean union key
after the generic deferral. It reads each constituent through
getPropertyTypeForIndexType. A single miss fails the whole access. The reads
combine as a literal-reduced union, or as an intersection when the position is a
write. The element-access road now splits at that point. `element_access_lookup`
keeps the receiver/index preparation. `element_access_for_index_type` is the
per-key getPropertyTypeForIndexType half. Enum members are regularized before
the union, as in `resolved_indexed_access_type`.

getPropertyTypeForIndexType's object-literal arm (checker.go:27134) applies after
a key finds no property and no index signature:

- Under noImplicitAny, a string or number literal key reads `undefined`.
- A `string` or `number` key reads the union of every property type plus `undefined`.

checkElementAccessExpression (checker.go:8148) first widens the receiver of an
assignment target or a called method (isMethodAccessForCall, checker.go:11466).
A widened literal has lost ObjectFlagsObjectLiteral, so the arm does not apply.
The first build omitted that widening and lost six RIGHT rows in
noImplicitAnyStringIndexerOnObject: `({...})['hello'] = …` printed `undefined`
where native prints `any`. Passing the widened state removed all six losses.

A union key exposed flow narrowing that did not reach `obj[key]`.
isMatchingReference (flow.go:1629) matches two element accesses with identifier
arguments when both arguments resolve to one symbol, and that symbol is either a
constant variable or an unassigned parameter/mutable local. The port accepted
only `const`. It now uses `is_constant_variable` together with the existing
assignment-marking walk.

tryGetElementAccessExpressionName (flow.go:1743) has a related rule. A non-literal
argument names a property only when its entity name resolves to a constant
variable or an enum member. The port keyed on the argument's type, so a parameter
narrowed to `"a"` made `obj[key]` the same reference as `obj.a`. A pinned tsgo
control (`key = "a"; if (obj[key] !== undefined) return obj[key]`) returns
`string | undefined`, and the port now agrees. That gate changes no corpus row.

## Measurement

Full-corpus `scorepair` against 6c230ca2:

| step | right | adverse |
|---|---:|---|
| union-key distribution only | +31 | 4 G→W |
| plus identifier-key reference matching | +56 | 2 G→W |
| plus object-literal arm, no widening | +58 | 6 R→W, 2 G→W |
| plus receiver widening and the name gate | **+64** | **2 G→W** |

The final result is 50 W→R and 14 G→R, with zero RIGHT losses. Both GAP-to-WRONG
rows are in partialOfLargeAPIIsAbleToBeWorkedWith: `obj[k] = …` with
`k: keyof MyAPI` prints the write intersection in the order of the key union.
The port's `keys_of` and `literal_key_union` make every key a string literal, so
`0 | 1 | 2 | 10` becomes `"0" | "1" | "10" | "2"`. Native
getLiteralTypeFromPropertyName gives a numeric-literal property name a number
literal type (checker.go:26773). The intersection therefore lists 10 before 2.
Fixing this means carrying the declaration's name kind through `keys_of`. That
work is tracked as a follow-up and is not part of this unit.

Pinned tsgo declaration controls check these results:

- `o[k]` gives `string | number`.
- `{ a: 1, b: "", c: true }[s]` gives `string | number | boolean | undefined`.
- `{ a: 1, b: "" }[k as "a" | "z"]` gives `number | undefined`.
- The narrowed and assigned key functions give `string | number` and `string | undefined`.

## Limits

- The expression road still does not apply isStringIndexSignatureOnlyType's
  key simplification. Distribution gives the same value for the forms
  measured.
- A write through a union key intersects the property read types. Native
  intersects the write types. The two differ only for divergent accessors.
- Numeric-literal keys of `keyof` are string literals, as described above.
