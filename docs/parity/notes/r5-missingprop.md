# r5-missingprop: the sole-TS2741 and sole-TS2353 cases

Lane on epic `tsr-2zk`, vendor `5b1047d`. Target: the diagnostics cases whose
**only** wrong code is TS2741 ("Property 'x' is missing in type … but required
in type …") — 28 missing a report, 2 with an extra one — and then the 21 whose
only wrong code is TS2353 (excess property).

Frozen baseline: integration head `bd8ae26`. Diagnostics dump 5350 RIGHT,
1241 WRONG, 5581 EMPTY_RIGHT, 66 EMPTY_WRONG; types dump 544,047 RIGHT,
7,492 WRONG, 994 GAP.

Method: each case was run with the reporter instrumented at
`report_relation_failure` / `report_argument_failure` entry (the pair, the
three-valued relation, `assignability_pair_is_reportable`; not committed),
and every probe was compared with a native `tsgo` built from the pinned
submodule (`scripts/offline-cargo/build-tsgo.sh`). "Not reached" means no
report function was asked about the position at all.

## 1. Triage: sole TS2741 (30 cases)

| Cluster | Cases | Root cause | Where the fix lives |
|---|---|---|---|
| A. Heritage type arguments are never constraint-checked | `subtypingWithNumericIndexer2`/`3`/`4`, `subtypingWithStringIndexer2`/`3`/`4`, `genericTypeConstraints` (7) | `checkTypeReferenceNode` (`checker.go:2982`) runs for `ExpressionWithTypeArguments` too (class `extends`, `checker.go:4371`; interface heritage, `:5022`). `check_type_argument_constraints` returns unless the node is a `TypeReferenceNode`, so `extends A<Base>` is never checked. | `constraints.rs` (not owned) — §3a |
| B. Calls: the argument check never runs or is lost | `fixingTypeParametersRepeatedly2`, `typeParameterFixingWithContextSensitiveArguments2`/`3`, `overloadresolutionWithConstraintCheckingDeferred` (×3), `inferenceFromIncompleteSource`, `mappedTypeAsStringTemplate`, `narrowingGenericTypeFromInstanceof01`, `objectLiteralThisWidenedOnUse` (10) | Not reached, or reached only inside a callback body checked during overload resolution whose diagnostics are discarded (`overloadresolution…` 19,14 is asked `D → A`, NotRelated, and nothing survives). Inference fixing / inferred-type-argument constraint fallback (`getInferredType` replaces an inferred argument that fails its constraint with the constraint, so `new G(x)` relates `D → A`). | `calls.rs`, `inference.rs` (main's calls lane) |
| C. Type-only re-export aliases resolve to `error` | `chained`, `renamed`, `mergeSymbolReexportInterface` (3) | The target of `const d: D = {}` is `error` (`export type { A as B }` chains), so `assignability_pair_is_reportable` declines. | alias resolution (`symbols.rs`/`declared.rs`, not owned) |
| D. `using` without `strictNullChecks` | `usingDeclarations.14`, `usingDeclarationsWithIteratorObject` (2) | `check_using_declaration_initializer` declines when `Disposable \| null \| undefined` collapses to `Disposable`, saying the missing-property reporter is private to `assignreport.rs`. It is `pub(crate)` (`report_relation_failure` with a head). | §2.3 (owned prerequisite) + `using_declaration.rs` diff, §3b |
| E. JSX hyphenated attributes | `ignoredJsxAttributes`, `tsxUnionElementType3`, `tsxUnionElementType6` (3) | The element with a `data-*` attribute is declined by the JSX caller for want of the `isComparingJsxAttributes` relater flag (r5-report §2). | `jsx_component.rs` / `relater.rs` |
| F. Mapped relation | `assignmentCompatWithEnumIndexer` (`{}` vs `Record<E, any>` answers **Related**), `mappedTypeWithAsClauseAndLateBoundProperty` (**Unknown**) (2) | `Record<E, any>` over a numeric enum must have the required property `"0"`; the `as`-clause mapped type over `keyof number[]` is undecided. | `mapped.rs` (r5-mapped3), `relater.rs` (r5-relater5) |
| G. `yield*` assignability | `generatorTypeCheck20` (1) | `check_yield_expression_assignability` declined every `yield*`. | **`assignreport.rs` — fixed, §2.1** |
| H. Generator return annotation | `generatorTypeCheck7` (1) | `checkSignatureDeclaration`'s `checkGeneratorInstantiationAssignabilityToReturnType(returnType, flags, returnTypeNode)` (`checker.go:2763`) and TS2505 were not ported. Ported in §2.2; this case stays blocked because `WeirdIter extends IterableIterator<number>`'s iteration types are undecided (members inherited through type-argument bases, `tsr-2zk.1013`, `members.rs`). | §2.2, then `tsr-2zk.1013` |
| I. Destructuring-assignment rest | `nonIterableRestElement3` (1) | `[...c] = ["", 0]`: `checkArrayLiteralDestructuringElementAssignment`'s rest arm relates `(string \| number)[]` to `c`'s type; not reached. | destructuring assignment (not owned) |
| J. Expando members (extra TS2741) | `expandoFunctionExpressionsWithDynamicNames2` (×2), `expandoFunctionSymbolProperty` (3) | `foo[mySymbol] = true` / `bar[t] = true` after `const foo: Foo = () => {}` are expando assignments with late-bound element-access names; native binds them as members of the function, so the arrow's type has `[mySymbol]`. TSR's function type lacks them, so the relation is NotRelated. | binder / expando members (not owned) |

## 2. Fixed here

### 2.1 `yield*` (cluster G)

`checkYieldExpression` (`checker.go:10952`) relates
`getYieldedTypeOfYieldExpression` (`:11019`) to the annotation's yield type for
both forms: for `yield*` the yielded type is
`checkIteratedTypeOrElementType(IterationUseYieldStar, operandType, …)`.
`check_yield_expression_assignability` returned at the asterisk. It now asks
`yield_star_operand_types` (the existing iteration query also used by
return-type inference) for the yielded type and reports it at the operand
with the operand as the elaboration node, as native passes
`node.Expression()` for both. `anyType` comes back for a `never` or
non-iterable operand (whose TS2488 the iteration check already reports), so
nothing is double-reported. The plain operand's excess-property pre-check is
not run for `yield*`: the related type is the delegated element type, and
`elaborateError` on the literal reaches the members itself (native prints
TS2353 at `y` for `yield* [{x: 1, y: 2}]`; so does TSR).

Async generators stay declined for both forms (the yielded type is awaited
first), as before.

Probed against native tsgo on string, generator, non-iterable, `never`,
union-array, and union-annotation operands and an object-literal method:
identical output. The union-source chain child (`Property 'x' is missing in
type 'Baz'…` under `Type 'Baz | Foo' is not assignable…`) is not built by
this reporter at any site; only the head is pinned in the test.

Measured: `generatorTypeCheck20` WRONG → RIGHT; no diagnostics loss.
Both loss checks empty (types RIGHT 544,047 → 544,047). Perf, median child
CPU new/old: domain-model 1.011, generic-imports 1.007 at 41 samples (21
samples read 1.041/1.039 and were re-run per protocol; neither project has a
`yield*`). Coverage: checker_types 8232/9538, diagnostics 4518/5502.

### 2.2 The generator arm of `checkSignatureDeclaration` (cluster H)

`checkSignatureDeclaration` (`checker.go:2757`) checks every generator with a
body and a written return type: `void` is TS2505 at the annotation, anything
else goes through `checkGeneratorInstantiationAssignabilityToReturnType`
(`checker.go:29697`) with the annotation as error node — `Generator`
(`AsyncGenerator`) of the annotation's own yield, return (orElse yield) and
next (orElse `unknown`) iteration types must be assignable to it. TSR had the
predicate (`generator_instantiation_assignable_to_return_type`, used to filter
union annotations) but no check site.

`check_generator_return_annotation` (`assignreport.rs`) is the check site;
`check.rs` calls it from the function-like block next to
`check_async_function_return_type`, which is native's `else if` sibling. It
computes the same instantiation the predicate relates (the iteration queries
are `iteration.rs`'s, not duplicated) and hands it to
`report_relation_failure`, so the head swap and chain rules are the shared
ones. An undecided iteration query declines; JS files decline as the other
sites here do.

Probed against native on 18 annotations (void, `number`, `Iterator`,
`Iterable`, `Generator<…>`, async, class methods, function expressions,
overloads, `any`/`unknown`/`object`/`{}`): every line TSR prints matches.
TSR prints nothing where native reports for annotations whose iteration types
come from an interface extending a lib generic (`WeirdIter`, `BadGenerator`,
`AsyncIterator<number> & { x: 1 }`) — the `tsr-2zk.1013` decline above.

Measured against commit 1: `generatorTypeCheck6` (TS2322 at the annotation)
and `generatorTypeCheck9` (TS2505) WRONG → RIGHT; one right line gained in
`types.asyncGenerators.es2018.2` (67:42 TS2741). One wrong line is gained in an
already-WRONG case: `parser.asyncGenerators.objectLiteralMethods.es2018`'s
`async * x: 1;` gets TS2322 at `1`. Natively that member has **no body**
(`parseFunctionBlockOrSemicolon` takes the `;`, so `FunctionFlagsInvalid`
skips the check; tsgo reports no `'{' expected` for `{ * y(): 1; }` either),
while TSR's parser always builds a block for an object-literal method and
reports TS1005 `'{' expected`. The fix is the parser's (reported); the check
is right for the tree native builds.

Both loss checks empty (types RIGHT unchanged). Perf, median child CPU
new/old at 21 samples: domain-model 1.021, generic-imports 0.998.

### 2.3 `isRelatedToEx`'s nullable-union narrowing in the report path

Found while porting cluster D. `isRelatedToEx` (`relater.go:2646`), after
`getNormalizedType`, relates a definitely non-nullable source against a union
of `null` and/or `undefined` plus one other type to that type alone, and
`reportErrorResults` (`relater.go:4705`) reports against it (the original
target only when it has an alias). TSR applied this only inside
`report_weak_type_failure`. Everywhere else the reporter used the whole
union, which gave wrong heads:

| Source | Native | TSR before |
|---|---|---|
| `const x: Foo \| undefined = {}` | TS2741 `… in type 'Foo'` | TS2322 `… 'Foo \| undefined'` |
| `const w: Bar \| null = {}` | TS2739 `… from type 'Bar': b, c` | TS2322 |
| `f({})`, `p: Foo \| undefined` | TS2741 | TS2345 |
| `const n: Foo \| undefined = 1` | `Type 'number' … 'Foo'` | `Type '1' … 'Foo \| undefined'` |
| `type FU = Foo \| undefined; const q: FU = {}` | TS2322 `'FU'` + TS2741 child | TS2322, no child |

The literal case is two effects of one cause: with the narrowed target
`typeCouldHaveTopLevelSingletonTypes` is false, so `reportRelationError`
generalizes `1` to `number`.

`relation_error_targets` (`assignreport.rs`) returns the related target, the
target `chainArgsMatch` compares, and the head's printed target.
`report_relation_failure` and `report_argument_failure` use it for the
missing-property and head reports. `missing_property_chain` now takes the
compared target separately from the related one. A union object literal
whose target narrows takes the plain missing-property report again, unless
its excess check already failed. A target with no candidate behaves exactly
as before, including the `NoInfer` display.

Probed against native on the table above plus `Foo | Bar | undefined` (not
narrowed) and number/string arguments: identical output. No conformance
position or code changes on either dump. The fix is message text only (and
what diff §3b needs), and both loss checks are empty. Perf, median child CPU
new/old at 21 samples: domain-model 0.992, generic-imports 0.982.

## 3. Shipped as diffs (files not owned)

### 3a. Heritage type-argument constraints — [r5-missingprop-heritage-constraints.diff](r5-missingprop-heritage-constraints.diff)

`constraints.rs`, plus `pub(crate)` on `type_argument_arity.rs`'s
`is_class_extends_entry` / `class_extends_entry_names_a_class` and
`meaning_mismatch.rs`'s `resolve_entity_name_expression`, and a new test
file. `check_type_argument_constraints` takes an `ExpressionWithTypeArguments`
whose parent is a heritage clause:
- interface `extends` (`checker.go:5022`) and class `implements` (`:4371`) are
  `checkTypeReferenceNode`;
- a class `extends` entry (`:4326`) checks each of
  `getConstructorsForTypeArguments`' signatures, whose type parameters are the
  class's own when the base expression names a class, so it is gated on
  `class_extends_entry_names_a_class`. A base that is some other constructor
  value (`declare const Ctor: { new <T extends string>(): … }`, or `extends
  Array<T>`) declines.

Measured on top of `2f0aa85` (this lane's commit 2): **+7 cases**
(`genericTypeConstraints` and `subtypingWith{Numeric,String}Indexer{2,3,4}`).
No other diagnostic line moves. Both loss checks are empty. Perf, median child
CPU new/old at 41 samples: domain-model 1.012, generic-imports 0.977.

Probed against native on interface/class/implements/qualified-name/lib-base
heritage: identical output except two shapes. The construct-signature base
above is declined. `class V<T extends Foo = FooExtended>` is
`check_type_parameter_default_constraint`'s class-constraint decline,
already in place before this diff.

### 3b. `using` through the shared reporter — [r5-missingprop-using-reporter.diff](r5-missingprop-using-reporter.diff)

`using_declaration.rs`. `check_using_declaration_initializer` declined
whenever the disposable union collapsed to one interface
(`strictNullChecks` off), on the grounds that the missing-property reporter
was private to `assignreport.rs`. It is not private:
`report_relation_failure` takes a head message. The diff routes both modes
through it, which is native's `checkTypeAssignableTo(…, initializer,
headMessage)` with no elaboration. With §2.3 in place, the strict mode
matches native too. There, native reports TS2741, not TS2850, for `{}`, because
the target narrows to `Disposable`. Requires §2.3.

Measured on top of §2.3:
- `usingDeclarations.14` converts (WRONG → RIGHT).
- `usingDeclarationsWithIteratorObject` gains its expected 20:17 line and one
  wrong line, 14:17 (`using it3 = new MyIterator()`), so its verdict stays
  WRONG.
- No loss.

The wrong line is a relation false negative that the diff exposes but does
not cause. `class MyIterator extends Iterator<string>`, whose base comes from
the lib's abstract construct signature (`IteratorObject<…>`), relates
NotRelated to `Disposable`. TSR already reports the same false TS2741 for
`const d: Disposable = new MyIterator()`. Native relates it through the
inherited `[Symbol.dispose]`. Root: the base-from-construct-signature member
table (`members.rs`/`declared.rs`), the same family as `tsr-2zk.1013`.

The remaining text difference is type printing: TSR prints
`Iterator<string, undefined>` where native prints
`Iterator<string, undefined, any>` (a defaulted type argument the printer
drops).

## 4. Triage: sole TS2353 (21 cases)

| Cluster | Cases | Root cause | Where |
|---|---|---|---|
| K. `checkObjectLiteral`'s pattern arm | `checkDestructuringShorthandAssigment2`, `declarationEmitDestructuringObjectLiteralPattern`/`1`, `destructuredLateBoundNameHasCorrectTypes`, `missingAndExcessProperties` (5) | Not ported: a literal typed by a binding pattern's implied type, or by the left of a destructuring assignment, reports TS2353 per unnamed member (`checker.go:13250`). | **fixed, §4.1** |
| L. Conditional / mapped / reverse-mapped targets | `excessPropertyCheckIntersectionWithRecursiveType` (×3), `reverseMappedTypeLimitedConstraint` (×2), `typeSatisfaction_propNameConstraining` (`Partial<Record<Keys, unknown>>`) | The target never resolves to an enumerable object: conditional alias (`tsr-2zk.976`), reverse-mapped inference, and `Partial<Record<…>>` over a union of keys. | `relater.rs`, `mapped.rs`, `inference.rs` |
| M. Function-type constituent | `excessPropertyErrorForFunctionTypes` (`{…} \| (() => any)`) | `isKnownProperty` asked the function type literal's property table, which TSR could not certify, so the excess verdict declined. | **fixed, §4.2** |
| M2. Switch-case excess | `switchStatements` 35:20 | `case { id: 12, name: '' }` against `C`: `checkTypeComparableTo`'s `hasExcessProperties` reports TS2353. `check_switch_case_comparability` (`comparison_overlap.rs`) skips fresh literals because it has no access to the excess reporter. | diff, §4.3 |
| M3. Several discriminants | `excessPropertyCheckWithMultipleDiscriminants` 131:5 | `Attribute2 = string \| StringAttribute \| NumberAttribute` (intersections with a generic base): `findMatchingDiscriminantType` over intersection constituents declines. | `assignreport.rs` remainder |
| N. Generic-reference / alias tables | `excessPropertyCheckWithEmptyObject` 4:58 (`PropertyDescriptor & ThisType<any>`), `objectLiteralExcessProperties` 45:76 (`T extends IFoo`) | Already in r5-report2's remainder. | integrator's table diff / relater |
| O. Index-signature contextual target | `objectLitIndexerContextualType` 18:5 (`y = { s: … }` against a number index) | Not reached: `isKnownProperty` against a number-only index signature with a non-numeric name. | `assignreport.rs` (next item) |
| P. Symbol-keyed generic argument | `symbolProperty21` 10:5 (`[Symbol.toPrimitive]` against `I<T, U>` during inference) | Call path (argument against an inferred generic interface). | `calls.rs` |
| Q. JS / JSDoc | `checkJsdocTypeTagOnExportAssignment1`/`6`, `checkJsdocSatisfiesTag9`/`10` | `@type` on `export default` and `@satisfies` targets are not reported through. | r5-jsdoc3's files |
| R. Extra TS2353 | `namespaceImportTypeQuery2`/`3` | `typeof ns` for a namespace import of a module with type-only exports: TSR lists `A` as a property of the module object type, native does not, so `{ A, B }` reports `A`. | namespace object types (`symbols.rs`/`members.rs`) |

### 4.1 The `contextualTypeHasPattern` arm (cluster K)

`checkObjectLiteral` (`checker.go:13250`): when the contextual type was
recorded in `patternForType`, a property, shorthand or method member whose
binder name the contextual type lacks is TS2353 at the member name. The type
printed is the contextual type itself, and the member is printed by
`symbolToString`. Two exceptions:
- `ObjectLiteralPatternWithComputedProperties`: the pattern has a computed
  name that is not a property-name literal;
- a string index: a binding pattern with a rest element.

There are two pattern sources:
- **A binding pattern's implied type** (`getTypeFromObjectBindingPattern`,
  `checker.go:17938`, with `includePatternInType`). This is the contextual
  type of a declaration's own initializer only when the pattern has elements
  (`getContextualTypeForInitializerExpression`, `checker.go:29431`). A nested
  literal is typed by the implied property's own pattern type at any arity.
- **An assignment target's literal type**: `checkObjectLiteral` records it for
  `inDestructuringPattern` (`checker.go:13212`), so the right of
  `({ x } = { x: 0, y: 0 })` is checked against `{ x: number; }`, and the
  right of `({ } = …)` against `{}`. A spread on the left makes the left a
  spread type with no pattern.

`check_object_literal_binding_pattern_members` (`assignreport.rs`) finds the
pattern syntactically with the two finders `objects.rs` already uses for
optional-member copying:
- `contextual_binding_pattern` (§489);
- `contextual_assignment_pattern` (§897).

Both were made `pub(crate)`: a visibility change only, in a file this lane
does not own. The check runs once per literal from the check walk, beside the
other `checkObjectLiteral` diagnostics (`check_duplicate_object_literal_names`
and its siblings), because `checkObjectLiteral`'s result is cached natively.
A member's binder name follows the binder:
- written names give their text, and a numeric name is normalized;
- a computed string, template or numeric literal gives its text, and a signed
  numeric gives its sign and number;
- any other computed name is `__computed`, which no property matches.

A computed member prints as its written expression in brackets (`["x"]`,
`[k]`), read from the module host's source text. Without source text the
literal declines.

Declines: an array-pattern element's nested literal (`var [{ t1 }] = [{ t1: 1,
t2: 2 }]`; native reports `t2`), and whatever `contextual_binding_pattern`
declines (parameter defaults needing explicit pattern context).

Probed against native in both strict modes on 30 shapes: written, quoted,
numeric, method, template, computed-literal, computed-entity and signed
names; nested patterns; rest; parameter defaults with and without
annotations; and assignment patterns with defaults, renames, spreads, nesting
and computed const keys. Output is identical apart from the array-element
decline.

Measured against commit 3:
- `checkDestructuringShorthandAssigment2`,
  `declarationEmitDestructuringObjectLiteralPattern`/`1`,
  `destructuredLateBoundNameHasCorrectTypes` and `missingAndExcessProperties`
  go WRONG → RIGHT.
- `declarationsAndAssignments` gains 4 right lines (still WRONG on other
  codes).
- 20 lines are added, all right.

Both loss checks are empty and types RIGHT is unchanged. Perf, median child
CPU new/old at 41 samples: domain-model 1.012, generic-imports 1.004 (21
samples: 1.029 / 1.020). The hook costs one parent-kind lookup per object
literal on the path that finds no pattern.

### 4.2 Function type literals have no properties (cluster M)

`isKnownProperty` (`relater.go:719`) uses `getPropertyOfObjectType`, which
reads the resolved members only. A function or constructor type literal's
`__type` symbol holds just its signature, so `resolveAnonymousTypeMembers`
gives it no properties. The global `Function` members are
`getPropertyOfType`'s apparent-type fallback, which this lookup does not
take. `certified_property_names` (`assignreport.rs`) answered `None` for
every anonymous type, because an anonymous *function* or class type can carry
expando or static members. It now answers the empty list for an anonymous
type whose declarations are all `FunctionType`/`ConstructorType` nodes. That
shape is the only one where emptiness is certain. The answer also feeds
`isEmptyObjectType` for the excess check and the spelling-suggestion
candidates.

Measured against commit 4: `excessPropertyErrorForFunctionTypes` WRONG → RIGHT,
no other line moved. Both loss checks are empty and types RIGHT is unchanged.
Perf, median child CPU new/old at 21 samples: domain-model 0.997,
generic-imports 1.012.

### 4.3 Switch-case excess — [r5-missingprop-switch-case-excess.diff](r5-missingprop-switch-case-excess.diff)

This diff touches `comparison_overlap.rs` (not owned), plus a new
`pub(crate)` entry `report_fresh_literal_excess_property` in `assignreport.rs`
and a test. Both land together, because the entry has no other caller.

`checkSwitchStatement` (`checker.go:4188`) calls
`checkTypeComparableTo(caseType, expressionType, clause.Expression())` once
`isTypeEqualityComparableTo(expressionType, caseType)` fails. For a fresh
object literal case, `isRelatedTo` runs `hasExcessProperties` before the
structural comparison, so the report is TS2353 at the member.
`check_switch_case_comparability` skipped every fresh literal because it
could not reach the reporter. With the diff, a fresh literal whose switch type
is definitely not comparable to it goes through the shared excess verdict.
Any other fresh-literal outcome stays silent, as before.

Measured on top of `04a3aeb`: `switchStatements` WRONG → RIGHT, no other line
moves. Both loss checks are empty. Perf, median child CPU new/old at 21
samples: domain-model 0.988, generic-imports 1.025.

Probe: a discriminated-union switch type (`case { kind: "a", x: 1, z: 2 }`) is
still silent where native reports `z` against the whole union. TSR's
comparable relation for that pair is not `NotRelated`, so it declines rather
than reporting wrong.
