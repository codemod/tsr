# r5-vardecl — checkVariableLikeDeclaration fidelity (tsr-2zk.1036)

Round-5 lane `tsr-2zk.1036`. Native reference: `vendor/typescript-go` @
`5b1047d`, `internal/checker/checker.go` unless noted. Owned: the
variable-like declaration checks in `assignreport.rs`, `identity.rs`, this
file. Populations come from the frozen integration baseline (`02a7110`)
`diagverdictdump`, WRONG/EMPTY_WRONG rows, expected-vs-actual
`(file, line, column, code)` multisets.

## §1 TS2502: the declaration resolves its own symbol's type

`checkVariableLikeDeclaration` calls `getTypeOfSymbol(symbol)`
(`checker.go:5893`) for every declaration that gets past the binding-pattern
and `require` exits, whether or not anything references it. The circularity
report lives inside that resolution (`reportCircularityError`), so
`export var r12: typeof r12;` reports TS2502 with no use anywhere.
`checkAccessorDeclaration` does the same through `getTypeOfAccessors`
(`checker.go:2974`).

This port resolves a symbol's type only when something asks, so an unused
self-reference never reached `report_circularity_error` (`symbols.rs`).
`resolve_variable_like_symbol_type` and `resolve_accessor_symbol_type`
(`assignreport.rs`) make the call; `check.rs` calls them from the
`VariableDeclaration`, `PropertyDeclaration` and `PropertySignatureDeclaration`
arms and from the accessor grammar arm. Like upstream, the call is made for
ambient declarations too (`declare global { const foo: typeof foo }`,
`crashDeclareGlobalTypeofExport`).

### The declines, and what they wait on

The first unrestricted build measured +7 diagnostics cases and **7 lost**.
Every loss was a cycle this port forms and upstream does not, because
upstream defers work this port does eagerly. Forcing from the declaration
changes which symbol is resolved first, and so where an eager cycle closes:

| lost case | shape | eager piece |
|---|---|---|
| `cyclicGenericTypeInstantiation{,Inference}` | `var z = foo<typeof y>()` beside `var y: { y2: typeof z }` | return type computed with the function's symbol type; type-literal members minted with the literal |
| `declarationsWithRecursiveInternalTypesProduceUniqueTypeParams` | extra TS7024 | return types |
| `functionWithDefaultParameterWithNoStatements16` | `function foo(a = bar())`, `function bar(a = foo())`: extra TS7022 | parameter types computed with the function's symbol type |
| `unionTypeWithRecursiveSubtypeReduction3` | `declare var a27: { prop: number } \| { prop: T27 }; type T27 = typeof a27` | type-literal members |
| `recursiveTypesWithTypeof` | `var hy2: { x: Array<typeof hy2> }`, `var i: (x: typeof i) => typeof x` | type-literal members; signature parameters |
| `thisInObjectLiterals`, `checkingObjectWithThisInNamePositionNoCrash` | lost TS2339 on `this.x` in an unannotated object literal | resolution order of an object literal's `this` type |

So the call declines:

- **an unannotated declaration** — its type is its initializer's, and the
  initializer reaches eager return and parameter types (the
  `ResolvedReturnType` frame `symbols.rs` §217 records as missing);
- **an annotation that writes a type literal, mapped type, function type or
  constructor type** anywhere in it — upstream resolves those members
  lazily (`resolveStructuredTypeMembers`); this port mints them with the
  annotation;
- **a parameter** — upstream's parameter type never resolves the owner's
  signature; this port builds a function's type with its parameters.

What would remove each decline: deferred type-literal and signature members,
and a `ResolvedReturnType` frame. When those land, deleting the declines and
re-measuring is the test; the unrestricted build's gains
(`accessorInferredReturnTypeErrorInReturnStatement`, `es2020IntlAPIs`,
`privateNameCircularReference`, all through unannotated declarations) are
what the declines currently cost.

**Falsifier.** A new extra TS2502/TS7022 in a case whose cycle runs through
a type alias or interface reference (not a literal) would mean a further
eager producer; the decline does not cover it.

### Not converted here

- `circularOptionalityRemoval` (`function fn1(x: number | undefined = x > 0 ?
  x : 0)`): upstream's cycle is `parameterInitializerContainsUndefined`'s
  `TypeSystemPropertyNameInitializerIsUndefined` frame
  (`checker.go:11219`), reached from the narrowing of a reference to `x`
  inside its own initializer. This port's counterpart
  (`flow.rs`, `parameter_initializer_contains_undefined`) seeds a
  provisional `true` and never reports. The fix is a resolution frame and a
  `reportCircularityError` call there, in `flow.rs` (main's).
- `implicitAnyFromCircularInference`: TS2502 on `var a: typeof a;` now
  reports; the case still differs on TS7023/TS7024 for functions
  (return-type frame).
- `circularIndexedAccessErrors`, `recursiveMappedTypes`: their TS2502 lines
  now report; other codes keep the cases WRONG.

### Measured

Against the frozen baseline (`02a7110`), both dumps unfiltered, both loss
checks empty:

- diagnostics: plain 8,757 → 8,761 / 9,816, configured 2,152 → 2,152 / 2,422
  (RIGHT + EMPTY_RIGHT); converted `typeofAnExportedType`,
  `typeofANonExportedType`, `crashDeclareGlobalTypeofExport`,
  `circularAccessorAnnotations`;
- type lines: 543,912 RIGHT of 552,533, unchanged;
- perf, median child CPU over 21 samples, new/old: domain-model 1.020,
  generic-imports 1.002; diagnostics match the baseline binary.
