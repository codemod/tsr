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
  `ResolvedReturnType` frame the comment on `report_circularity_error` records as missing);
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

## §2 TS2403: `unknown` is trusted; an enum literal is not its enum union

Two narrowings of `check_subsequent_declaration_identity`'s declines, both
toward `isTypeIdenticalTo`'s answer.

**`unknown` as an operand.** `identity_side_is_trusted` (`check.rs`) trusted
a top-level `any` or `unknown` only where the annotation wrote that keyword,
because in this port `any` is often "no better answer" (the comment on
`is_decidable_primitive` in `check.rs` records trusting `any` at −34 cases). `unknown` was grouped with it by analogy, not
by measurement. This port's could-not-compute answer is the error type
(ADR-0048) and, historically, `any`; `unknown` comes from inference defaults
and annotations, as upstream's does. `contextualSignatureInstantiation`
prints `bar("one", 1, g) : unknown` RIGHT in its `.types` and missed the three
TS2403 lines (`string | number` against `unknown`) only because of the trust
rule. Trusting `unknown` converts the case.

**Falsifier.** A new extra TS2403 naming `unknown` would mean some producer
answers `unknown` for "not computed" (`inference.rs` has `unwrap_or(unknown)`
fallbacks on unresolved indexed accesses); that producer is the fix, and the
rule here would narrow to the producers known good.

**Enum literal against enum union.** `identity.rs` declines every flags
difference between two enum-like types, because one enum has two
representations here. One pair is safe: an enum literal against a union of
enum literals with at least two distinct values. Upstream's flags differ
(`NumberLiteral|EnumLiteral` against `Union|EnumLiteral`), so
`isTypeRelatedTo` answers false, and no representation of a two-valued union
is one value. This is the property comparison inside `typeof E` against
`{ readonly a: E; … }` (`typeOfEnumAndVarRedeclarations`).

That case still does not convert: its primary declaration is unannotated
(`var x = E`), so the pair takes the assignability road (`decls.md` §2), and
the relater answers `Unknown` both ways for `typeof E` against the object
literal type (`relater.rs`, not owned). Running the structural arm for every
pair converts it, and `parserCastVersusArrowFunction1` and
`FunctionAndModuleWithSameNameAndCommonRoot` too, but measured **5 cases
lost**: `arrayLiteralWidened` and `typeRelationships` (decls.md §2's known
producers) gain an extra TS2403, and `forStatementsMultipleInvalidDecl`,
`invalidMultipleVariableDeclarations` and `strictTupleLength` lose one,
because the structural arm answers `Unknown` where mutual assignability
decided. Not shipped; the trust rule stands.

## §3 TS2403: a generic mapped type is identical only to a generic mapped type

`structuredTypeRelatedToWorker` under identity (`relater.go:3805`, `:3817`):
a generic mapped target relates only to a generic mapped source through
`mappedTypeRelatedTo`, and a generic mapped source relates to nothing else.
Both arms run after the alias-variance probe (`:3389`), which only settles a
pair whose arguments are identical. `identity.rs` declined every mapped
operand. It now:

- tries the same-target reference arm first (a success settles it, as the
  alias probe does);
- classifies each side with `mapped_shape_for_identity`, which reads
  `isGenericMappedType` from the alias body's mapped metadata (the
  constraint is a generic index type);
- answers `NotRelated` when exactly one side is generic and the other is not
  mapped or is a resolved mapped type;
- keeps `Unknown` for two generic sides (`mappedTypeRelatedTo` is not ported
  here), for two resolved sides (members not complete enough), and for an
  `as` clause over a non-generic constraint (`isGenericMappedType`
  instantiates the name type; not ported).

Converts `noExcessiveStackDepthError`: `FindConditions<any>` (constraint
`string | number | symbol`) against `FindConditions<Entity>` (constraint
`keyof Entity`).

### Measured (§2 and §3 together, on top of §1)

Both dumps unfiltered against the frozen baseline, both loss checks empty:
diagnostics plain 8,761 → 8,763 / 9,816 (`contextualSignatureInstantiation`,
`noExcessiveStackDepthError`), configured unchanged; type lines 543,912
RIGHT, unchanged. The enum-literal arm alone converts no case (its only
witness is blocked on the relater, above). Perf, median child CPU over 21
samples: domain-model 1.024, generic-imports 0.961; diagnostics match.
