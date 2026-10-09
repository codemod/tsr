# Lane notes: r6-names (tsr-2zk.1133)

Round-6 box on epic `tsr-2zk`: diagnostics cases that are WRONG on exactly one
code, for TS2304 (25 cases) and TS2552 (4). Pinned upstream:
`vendor/typescript-go` @ `5b1047d`. Owned files: only files this box creates.
Name resolution (`symbols.rs`, the binder) and the TS2304 reporter (`check.rs`)
are main's, so every hook into them ships as a measured diff in this directory
(§3 gives the apply order).

## §1 Base and instruments

- **Base.** `claude/beautiful-shannon-ar5gh0` at `b18aec06` (main `17265fac`
  plus bookkeeping); batch BA had not landed. Unfiltered at the base:
  diagnostics 5,530 RIGHT / 5,596 EMPTY_RIGHT / 1,063 WRONG / 49 EMPTY_WRONG;
  types 549,853 RIGHT / 843 GAP / 5,607 WRONG.
- **Selection.** A case is "WRONG on exactly one code" when the multiset
  difference of its expected and actual `(file, line, column, code)` rows
  names one code. That gives TS2304 25 and TS2552 4, as dispatched.
- **Oracle.** Native tsgo from `scripts/offline-cargo/build-tsgo.sh`
  (`Version 7.1.0-dev`). Every test expectation in this lane is its output.
- **Setup.** PyPI is blocked; `assemble.py`'s three `tomlkit` calls ran on a
  stdlib-only stand-in kept outside the repository (r5-operators3 §4).
- **Row metric.** Besides verdicts, each gate sums missing and extra rows
  over every case (`miss/extra`), so a change that adds a wrong row to an
  already-WRONG case is visible. Base: 3,382 / 1,089.
- **Ir.** `valgrind --tool=callgrind`, `tsr -p tsconfig.json
  --singleThreaded --pretty false` on `benches/projects/{domain-model,
  generic-imports}`. Base 1,091,505,064 / 343,080,450; a second base run
  read 1,091,453,148 / 343,081,807, so noise is about ±50k.

## §2 Classification

TS2304 here comes from a walk over every identifier, gated by a parent-slot
allow-list (`check.rs`, `is_value_reference`, and the type-reference arm of
`check_type_reference_name`). Native reports it from `getResolvedSymbol` and
`resolveEntityName`, so a name is diagnosed exactly when some check reaches
it. Most of these cases are a slot the allow-list lacks or a slot native never
checks, not a resolution difference.

| Cluster | Native operation | Cases | Status |
|---|---|---|---|
| A. value slots missing from the allow-list | `checkWithStatement` (`checker.go:4162`); JSX value tags, `resolveJsxOpeningLikeElement` (`jsx.go:562`), `checkJsxElementDeferred` (`jsx.go:84`) | parserStrictMode14, parserWithStatement1.d, jsxSpreadTag ×2, jsxAttributeWithoutExpressionReact, parseJsxExtends2 | §4, diff `r6-names-value-slots` |
| B. `for…of` with an empty declaration list | `checkForOfStatement` (`checker.go:4051`) reaches the expression only through a declaration | parserForOfStatement2, parserForOfStatement21, parserES5ForOfStatement2, parserES5ForOfStatement21 | §5, diff `r6-names-empty-for-of` |
| C1. TypeScript-only annotations in JavaScript files | `getTypeFromTypeReference` reached by `checkSourceElement` in a `.js` file | fillInMissingTypeArgsOnJSConstructCalls, parserArrowFunctionExpression10, parserArrowFunctionExpression17 | §6, diff `r6-names-js-type-annotations` |
| C2. JSDoc type names | JSDoc arms of `resolveTypeReferenceName`; script-file typedefs in `globals` | jsdocResolveNameFailureInTypedef, typedefScope1, recursiveResolveDeclaredMembers | open |
| D. parameter initialisers | `resolveName`'s `useOuterVariableScopeInParameter` (`binder/nameresolver.go:74`, `:346`) | functionLikeInParameterInitializer(es2015), parameterInitializersForwardReferencing(es2015) | open |
| E1. non-primitive keyword spellings as values | `checkAndReportErrorForUsingTypeAsValue`'s six-name `isPrimitiveTypeName` (`checker.go:1637`) | parserSymbolIndexer5 (TS2552) | §7, diff `r6-names-primitive-spellings` |
| E2. `typeof null` | `parseTypeQuery`'s reserved-word entity name (`parser.go:3114`) | invalidTypeOfTarget | §8, diff `r6-names-typeof-null` |
| F. parse recovery | parser trees that differ from native | arrowFunctionsMissingTokens, YieldStarExpression2_es6, bigintArbirtraryIdentifier, importDeferTypeConflict2, parserSuperExpression2, classExpressionWithDecorator1 | open |
| G. local export specifiers | `getTargetOfExportSpecifier`'s `resolveEntityName` (`checker.go:14970`) and its `onFailedToResolveSymbol` tail | duplicateErrorNameNotFound (TS2552) | §9, diff `r6-names-export-specifier` |
| H. module augmentation | augmentation merge (`tsr-2zk.38`, main's) | moduleAugmentationInAmbientModule1, moduleAugmentationInAmbientModule5 (TS2552) | routed |

## §3 Hook diffs, in apply order

Each diff touches `check.rs` (main's), removes the `allow(dead_code)` on the
function it wires in `name_slots.rs`, and adds its test under
`crates/tsr-conformance/tests/`. Each was measured alone on the base.

| # | Diff | Converts | Losses | Rows (miss/extra) |
|---|---|---|---|---|
| 1 | `r6-names-value-slots.diff` | +6 cases | 0 | 3,382/1,089 → 3,363/1,089 |
| 2 | `r6-names-empty-for-of.diff` | +4 cases | 0 | 3,382/1,089 → 3,382/1,085 |
| 3 | `r6-names-js-type-annotations.diff` | +3 cases | 0 | 3,382/1,089 → 3,378/1,089 |
| 4 | `r6-names-primitive-spellings.diff` | +1 case | 0 | 3,382/1,089 → 3,381/1,089 |
| 5 | `r6-names-typeof-null.diff` | +1 case | 0 | 3,382/1,089 → 3,381/1,089 |
| 6 | `r6-names-export-specifier.diff` | +1 case | 0 | 3,382/1,089 → 3,380/1,089 |

Diffs 1–3 applied together in this order: +13 cases, 0 losses on both
dumps, rows 3,382/1,089 → 3,359/1,085, types dump identical to the base
(549,853 RIGHT / 843 GAP / 5,607 WRONG). Each diff applies on the base alone
as well as stacked.

## §4 Cluster A: value slots the allow-list lacks

`is_value_reference` lists the parent slots whose identifier native
`checkExpression`s. Two native checks were missing:

- `checkWithStatement` checks the `with` expression. The existing
  `is_inside_with_statement` decline is correct for the *statement* only
  (`parseWithStatement` sets `NodeFlagsInWithStatement` on the statement,
  `parser.go:1386`), so `with (a)` never reported `a` because no arm admitted
  it, not because the decline fired.
- A JSX tag that is not intrinsic (`isJsxIntrinsicTagName`: lowercase first
  letter, a hyphen, or a namespaced name) is `checkExpression`ed at the
  opening tag (`resolveJsxOpeningLikeElement`) and at the closing tag
  (`checkJsxElementDeferred`). The port never reported TS2304 on a bare tag
  identifier. `<a.B/>` already worked through the property-access receiver.

`name_slots::value_reference_slot` is the extra arm; the hook makes it the
allow-list's fallback instead of `false`. It runs only for parents no other
arm matched, and only compares slots.

Native probe (`--jsx react --target es2015 --strict false`): `with
(missingWith)` → TS2304; `<Missing x={1} />` → TS2304 at the tag;
`<Open>text</Open>` → TS2552 *Did you mean 'open'?* at both tags; `<div />`
and `<my-element />` → nothing. The port with the diff matches all of it,
including the suggestion, because the reporter's suggestion arm was already
the native one.

Measured alone: +6 cases (the six above), 0 losses on both dumps, missing
rows 3,382 → 3,363 with no new extra row anywhere; types dump unchanged;
`slowcases` clean on both dumps; Ir +0.03% domain-model, −0.007%
generic-imports (within the ±50k base noise).

**Falsifier.** A JSX case whose expected baseline lacks TS2304 on an
unresolved value tag. None in the corpus: no extra row was added.

## §5 Cluster B: an empty `for…of` declaration list

`checkForOfStatement` (`checker.go:4051`) checks a declaration-list
initializer only through `checkVariableDeclarationList`. The right-hand side
is reached from each declaration: `getTypeForVariableLikeDeclaration`'s
for-of arm (`checker.go:16664`) calls `checkRightHandSideOfForOf`. With no
declaration (`for (var of X)`, TS1123), nothing checks `X` or anything inside
it. `checkForInStatement` (`checker.go:3988`) checks its expression before
looking at the initializer, so `for (var in X)` still reports `X`.

The port's walk visits every identifier, so the decline is
`Checker::in_unchecked_for_of_expression`: a parent walk to the nearest
`for…of` whose `expression` slot contains the node. The hook runs right after
the allow-list test in `check_value_identifier`, so it covers every code that
reporter emits (TS2304, TS2552, TS2583, TS2693…), as native's silence does.
The walk is O(depth) per value identifier, beside the existing
`is_inside_with_statement` walk; Ir with diffs 1 and 2 applied: +0.03%
domain-model, −0.007% generic-imports, inside the base's ±50k noise.

Native probe: `for (var of missingOf) { }` → TS1123 only; `for (var x of
missingDeclared)` → TS2304; `for (var in missingIn)` → TS1123 and TS2304. The
port with the diff matches.

Measured alone: +4 cases, 0 losses, extra rows 1,089 → 1,085 and no new
missing row; types dump unchanged (diffs 1 and 2 together); `slowcases`
clean.

**What it does not cover.** Other diagnostics inside such an expression
(a type error in a call, say) come from other checks that do not consult this
predicate. None appears in the corpus.

## §6 Cluster C1: TypeScript-only annotations in JavaScript files

`check_type_reference_name` declined every name in a JavaScript file, on the
reasoning (§779 of `checker-notes-diag2.md`) that `type a = b` in a `.js` file
is TS8008 "and upstream stops there". That is not what native does. TS8004,
TS8006, TS8008 and TS8010 come from `getJSSyntacticDiagnosticsForFile`, a
syntactic pass; the checker still reaches the annotation through
`checkSourceElement` and `getTypeFromTypeReference` resolves it as it would
in TypeScript. Native harness probe (`allowJs`, `checkJs`), one `.js` file:

```text
type A = Missing1;                              TS8008, TS2304 Missing1
function f(x: Missing2): Missing3 { … }         TS2304 + TS8010 on both
let v: Missing4 = 1;                            TS2304 + TS8010
class C<T extends Missing5> { p: Missing6; }    TS8004, TS2304 ×2, TS8010
interface I { a: Missing7 }                     TS8006, TS2304
var r = (): Missing12 => 1;                     TS2304 + TS8010
```

(The `tsgo` CLI cannot show this: it skips semantic diagnostics when a file
has syntactic ones. The probe is tsgo's own compiler-test runner, built with
`go test -c ./internal/testrunner` against the offline module file, run on a
case written to `testdata/tests/cases/compiler/` and never committed.)

The decline is narrowed to names inside a JSDoc comment
(`Checker::names_in_jsdoc`, `ast.IsInJSDoc`). Lifting it for JSDoc too was
measured first: +4 cases, **6 lost** (`importTypeResolutionJSDocEOF`,
`callbackTag2`, `checkJsdocTypeTag8`, `commonJSImportClassTypeReference`,
`commonJSImportExportedClassExpression`, `jsdocTypeDefAtStartOfFile`) and 13
new extra rows. Those are JSDoc resolution gaps (typedef placement, `import()`
types, CommonJS class references, the TS2583 lib arm in a JSDoc type) that
belong to the jsdoc lane; the JSDoc half of the decline stays until they are
ported (cluster C2).

Measured alone: +3 cases (`fillInMissingTypeArgsOnJSConstructCalls`,
`parserArrowFunctionExpression10`, `parserArrowFunctionExpression17`), 0
losses, missing rows −4, no new extra row. The diff also corrects the §779
comment in `check.rs` in place, noting the correction.

**Falsifier.** A `.js` baseline that has TS8010 on an annotation with an
unresolvable name and no TS2304 beside it. The probe above and the corpus
have none.

## §7 Cluster E1: `symbol`, `bigint` and `object` used as values

`check_value_identifier` routed ten keyword spellings away from the cascade
and, after §880/§948, let native's six (`any`, `string`, `number`,
`boolean`, `never`, `unknown`) through to TS2693. The other four stayed
declined on the reasoning that a rule reporting TS2693 for them was wrong.
It was, but silence is wrong too: native's `isPrimitiveTypeName`
(`checker.go:1637`) does not list them, so
`checkAndReportErrorForUsingTypeAsValue` (`checker.go:1662`) passes on and
the ordinary cascade runs: the spelling suggestion, then TS2304. `void` never
reaches here (it parses as a `VoidExpression`).

Native probe (`tsgo --target es2015`): `let a = symbol` → TS2552 *Did you
mean 'Symbol'?*; `bigint` → TS2304; `object` → TS2552 *'Object'*; `string`
→ TS2693; `class E extends symbol {}` → TS2552. The port with the diff
matches every row.

The diff deletes the four-name early return and corrects the §948 comment
in place. Measured alone: +1 case (`parserSymbolIndexer5`), 0 losses,
missing rows −1, no new extra row.

## §8 Cluster E2: `typeof null`

`check_value_identifier` declines a reference spelled `null` because this
parser makes an identifier of `class C extends null`, where native makes a
`NullKeyword` (a parser divergence worked around at the reader). A type
query is where both parsers make an identifier: `parseTypeQuery`
(`parser.go:3114`) parses the entity name with `allowReservedWords`, and
`checkIdentifier` then fails to resolve it. Native: `var x6: typeof null;`
→ TS2304 *Cannot find name 'null'* at the name; `typeof null.a` the same at
`null`; `class C extends null {}` → nothing.

`Checker::names_in_type_query_entity_name` keeps the decline off the
leftmost name of a `typeof` entity name. `this` stays declined everywhere:
`typeof this.x` is the `this`-type path natively, not a name lookup.

Measured alone: +1 case (`invalidTypeOfTarget`), 0 losses, missing rows −1,
no new extra row.

## §9 Cluster G: a local export specifier that resolves nowhere

`export { X }` with no module specifier resolves `X` through
`resolveEntityName(name, Value|Type|Namespace, ignoreErrors=false)`
(`getTargetOfExportSpecifier`, `checker.go:14970`), reached from
`checkAliasSymbol`. Failure runs `onFailedToResolveSymbol`. The port ran one
rung of it (`check_export_specifier_is_local` reports the exporting-primitive
TS2661) and stopped; nothing reported TS2304 or TS2552 at a specifier.
§841 of `checker-notes-diag2.md` had measured admitting the specifier to
`is_value_reference` at −7 cases: that path resolves at value meaning and
runs value-only rungs, which is the wrong meaning here.

`export_specifier_names.rs` runs the tail at the specifier's own meaning:
resolve from the parent scope (`resolve_name_excluding`, as
`check_export_specifier_is_local` does), stay silent for native's six
primitive names (TS2661 is already reported), then the missing-lib arm, the
spelling suggestion at `Value|Type|Namespace`, and
`getCannotFindNameDiagnosticForName`'s message. The rungs it skips cannot
fire at this meaning: the missing-prefix and extending-interface rungs need
a class or heritage position, and each mismatch rung looks the name up under
a meaning `Value|Type|Namespace` already covers.

**A non-ambient namespace is excluded.** Only a source file's or an ambient
module's exports skip a pure export-specifier alias
(`binder/nameresolver.go:121-133`); in `namespace N { export { inner } }`
the walk finds the specifier's own alias and native reports TS2303 (circular
alias), not a failed lookup. The first draft reported TS2304 there; the
native probe caught it. The port does not report that TS2303 either; that is
a pre-existing gap, not this diff's.

Native probe (`--module commonjs`): `export type { RoomInterface }` beside
`type RoomInterfae` → TS2552 *Did you mean 'RoomInterfae'?*; `export {
Missing }` → TS2304; `export { value as renamed }` beside `valuu` → TS2552
at `value`; `export { string }` → TS2661 only; `declare namespace D { export
{ amb } }` → TS2304; `declare module "mm" { export { amb2 } }` → TS2664 and
TS2304. The port with the diff matches every row except two pre-existing
ones (the namespace TS2303, and TS1003 for `export { "str" as s2 }`, a parser
difference).

The diff also makes `check.rs`'s `cannot_find_name_message` `pub(crate)` so
the tail shares the table rather than copying it. Measured alone: +1 case
(`duplicateErrorNameNotFound`), 0 losses, missing rows −2
(`bigintArbirtraryIdentifier`'s `badExport.ts` row converts too; that case
stays WRONG on a parse-recovery row, §2 F), no new extra row, `slowcases`
clean.
