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

## Concrete `keyof` keys are semantic

The two GAP-to-WRONG rows recorded above are removed by this unit. A
written `keyof X` over a concrete object operand used to read `keys_of` and
`literal_key_union`, which spell every key as a string literal. It now takes
the semantic `resolved_keyof_type` that mapped and type-parameter operands
already used. That function follows getLiteralTypeFromPropertyName
(checker.go:26773), so a numeric-literal property name becomes a number literal
key. A pinned tsgo control gives
`keyof { 0: string; 10: boolean; 2: number; a: number }` as `"a" | 0 | 2 | 10`,
and the port now prints the same.

The first draft took the semantic road for every operand, and its full run lost
two RIGHT rows and added nine GAP-to-WRONG rows:

- `keyof (T | U)` inside an alias argument became `keyof T & keyof U`, where the
  baseline prints the written form.
- An unresolved `React.ReactHTML`, which carries ANY in this port, became
  `string | number | symbol`.
- `t[k]` lost its `any` in unknownControlFlow.

The road is therefore restricted to OBJECT-flagged, resolved operands that
mention no type parameter. With that restriction, the full run against exact
starting main `cec7cef5` adds 37 assertions (29 GAP→RIGHT and 8 WRONG→RIGHT),
with no adverse transitions. Aligned verdicts move from 457,641 RIGHT, 2,538
GAP and 14,064 WRONG to 457,678 RIGHT, 2,509 GAP and 14,056 WRONG. The two
partialOfLargeAPIIsAbleToBeWorkedWith rows convert.

## Direct generic compounds distribute without erasing their spelling

Native `getIndexTypeEx` (`checker.go:26684`) applies the two set identities at
the semantic boundary: `keyof (T | U)` is the intersection `keyof T & keyof U`,
and `keyof (T & U)` is the union `keyof T | keyof U`. Each constituent remains
the deferred INDEX type created by `getIndexTypeForGenericType`; this is not a
printed-text rewrite. The port's `resolved_keyof_type` already implemented both
operations, but `get_type_from_type_node` admitted only a simple type-parameter
operand on its generic road. A direct compound therefore became `errorType` and
printed `any` at references.

The dispatch now recognizes only direct union/intersection syntax (through
transparent parentheses), resolves the whole compound, and delegates its keys
to `resolved_keyof_type`. That ordering preserves native `getReducedType` before
distribution: `any | X` and `never & X` absorb their compound, while
`unknown & X` reduces to `X`. Folding the written constituents independently
gets all three wrong. The declaration spelling is kept separately in the
existing written-node channel, so a parameter still prints
`key: keyof (T & U)` while a reference to that parameter prints
`keyof T | keyof U`. Equivalent named aliases stay on their previous road. This
boundary is deliberate: routing every generic operand through the semantic road
was previously measured at two RIGHT losses and nine GAP-to-WRONG transitions.

There is one native exception. `shouldDeferIndexType` keeps an instantiable
intersection containing an empty anonymous object deferred. Thus
`keyof (T & {})` remains one INDEX type rather than distributing to
`keyof T | never` and collapsing to `keyof T`. The port mints the same deferred
identity and records `T & {}` as its operand. Both conditions matter: concrete
`keyof ({ a: string } & {})` reduces to `"a"`, and a concretely substituted
mapped operand does the same. The implementation therefore checks the resolved
compound for an instantiable constituent rather than deferring on syntax alone.
A first draft omitted the exception entirely: it gained six lines in
`unknownControlFlow` but lost the already-correct `t[k] : any`; the faithful
branch removes that loss and converts ten lines in the case instead. A later
review caught the opposite overreach—testing only for written `{}` deferred
concrete intersections—before the commit was integrated.

Pinned tsgo declaration controls distinguish all four representations:

- direct `keyof (T | U)` retains the written parameter and returns
  `keyof T & keyof U`;
- direct `keyof (T & U)` retains the written parameter and returns
  `keyof T | keyof U`;
- `keyof (T & {})` remains written and deferred in both positions;
- concrete and concretely substituted mapped intersections with `{}` resolve
  to their actual keys;
- redundant nested parentheses preserve both union/intersection semantics and
  the normalized written parameter spelling;
- absorbing `any`/`never` and reducing `unknown` constituents are applied to
  the whole compound before any surviving union/intersection distributes;
- `keyof Wrapped<T, U>` retains the alias spelling. Native resolves its return,
  while this bounded port still declines that broader alias semantic road.

The full scorepair against exact starting main `8f8f4e1a` moves from 458,022
RIGHT, 2,445 GAP and 13,777 WRONG to 458,065 RIGHT, 2,441 GAP and 13,738 WRONG:
39 WRONG→RIGHT and 4 GAP→RIGHT, with zero adverse transitions. There are no new
or removed aligned rows and no changed still-WRONG payloads. Complete cases move
from 7,059/9,538 to 7,062/9,538.

## Generic alias operands and semantic empty-object deferral

The named-alias refusal above is superseded by tsr-6.42. The semantic
`resolved_keyof_type` query now projects generic aliases through the existing
alias-body evaluator before inspecting their kind. Declaration queries and
`instantiateTypeWorker` share this boundary: a function returning
`keyof NonNull<T>` for `type NonNull<T> = T & {}` returns `"a" | "b"` after
substitution with `{ a: number; b: string }`. Fixing only the written-node
dispatch made the generic signature print correctly but left this call at error.

The native rules are `getIndexTypeEx` (checker.go:26684),
`shouldDeferIndexType` (checker.go:26834), and `IsEmptyAnonymousObjectType`
(checker.go:26476). Empty-object deferral now tests the resolved intersection,
not written `{}` syntax. Named empty literals are recognized through their
literal origin; raw binder tables cannot prove emptiness because they omit
late-bound computed members. Nonempty objects, callables, and concrete
intersections must not acquire this deferral.

Pinned tsgo was built with Go 1.26.8 using `go build -o /tmp/tsr-tsgo ./cmd/tsgo`
in `vendor/typescript-go`. The regression sources were checked with
`/tmp/tsr-tsgo --strict --declaration --emitDeclarationOnly --outDir /tmp/tsr-keyof-oracle-out /tmp/tsr-keyof-oracle.ts`.
The new cases in `tests/union_key_element_access.rs` retain these outcomes:

- Alias of `T | U`, including reordered alias chains: `keyof T & keyof U`.
- Alias of `T & U`: `keyof T | keyof U`.
- Alias of `T | any` or `T & never`: `string | number | symbol`.
- Alias of `T & unknown`: `keyof T`.
- Alias `Wrapped<T> = T & {}`: `keyof Wrapped<T>`.
- `T & Empty`, with `type Empty = {}`: `keyof (T & Empty)`.
- Generic `Box<T>` with two ordinary properties: `keyof Box<T>`.
- Generic `Box<T>` with one property `value`: `"value"`.
- A generic alias with a deferred key remapping retains `keyof Alias<K>`;
  substituting `"_a" | "b"` into an underscore-key filter yields `"_a"`.

The first raw-row audit exposed expanded mapped bodies at seven still-wrong
alias-key positions. Retaining the original mapped alias as the deferred
operand, just as for an empty-object intersection, repairs those positions
and six downstream rows. This is operand metadata, not a printed-text rewrite;
the concrete-call regression checks that subsequent substitution still computes
the filtered key set.

The same native run corrected an older regression expectation:
`keyof P<O>` for a homomorphic optional mapping retains `keyof O`, not the
expanded string-literal union. An invalid `type Cycle<T> = T | Cycle<T>` probe
exposed unbounded key recursion; active index-query identities now stop that
cycle without a depth cap. Its native error-any key set and circularity
diagnostics remain unported, explicitly pinned as a gap rather than a native
expectation. Remaining computed-symbol and recursive mapped-key failures are
tracked in tsr-6.43.

Full-corpus measurement against exact starting main `cfcbcfab`, after rebasing
onto the independent compiler-host performance changes, yields **458,508 of
478,855 matching assertion lines (95.75%)**, up 36. The 474,251 aligned rows
contain 458,508 RIGHT, 2,433 GAP and 13,310 WRONG: 34 WRONG→RIGHT,
2 GAP→RIGHT, one GAP→WRONG, zero RIGHT losses, and no added or removed rows.
Seventeen already-WRONG payloads changed: fifteen in
`mappedTypeIndexedAccessConstraint`, one in `mappedTypeNotMistakenlyHomomorphic`,
and one in `mappedTypeConstraints2`. The exposed wrong row is
`compiler/computedTypesKeyofNoIndexSignatureType:0:14`: expected `"bar" | "foo"`,
received `never`. It remains in tsr-6.43 rather than being excluded.

`target/release/coverage checker_types diagnostics` retains 7,084/9,538 complete
checker cases and 2,800/5,488 diagnostic cases. Verification on the rebased
source: `cargo test --release --workspace --locked --no-fail-fast` passes all
227 result blocks; the targeted union-key suite passes 13 tests; strict release
workspace clippy and formatting pass. Anchor checks resolve all 3,400 anchors,
and section checks resolve all 16,642 citations. The existing issue-ID gate
still reports 197 missing historical records, and Dolt sync is blocked by the
unconfigured remote; publishing that history has not been approved. The 99%
target remains 15,559 matching lines away.

## Limits

- The expression road still does not apply isStringIndexSignatureOnlyType's
  key simplification. Distribution gives the same value for the forms
  measured.
- A write through a union key intersects the property read types. Native
  intersects the write types. The two differ only for divergent accessors.
- Numeric-literal keys stay string literals on the generic and unresolved
  `keyof` roads, and anywhere `keys_of` is still read directly.
