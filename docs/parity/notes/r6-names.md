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
| B. `for…of` with an empty declaration list | `checkForOfStatement` (`checker.go:4051`) reaches the expression only through a declaration | parserForOfStatement2, parserForOfStatement21, parserES5ForOfStatement2, parserES5ForOfStatement21 | §5, diff `r6-names-unchecked-regions` |
| C1. TypeScript-only annotations in JavaScript files | `getTypeFromTypeReference` reached by `checkSourceElement` in a `.js` file | fillInMissingTypeArgsOnJSConstructCalls, parserArrowFunctionExpression10, parserArrowFunctionExpression17 | §6, diff `r6-names-js-type-annotations` |
| C2. JSDoc type names | JSDoc arms of `resolveTypeReferenceName`; script-file typedefs in `globals` | jsdocResolveNameFailureInTypedef, typedefScope1, recursiveResolveDeclaredMembers | routed, §13 |
| D. parameter initialisers | `resolveName`'s `useOuterVariableScopeInParameter` (`binder/nameresolver.go:74`, `:346`) | functionLikeInParameterInitializer(es2015), parameterInitializersForwardReferencing(es2015) | §11, diff `r6-names-parameter-scope` |
| E1. non-primitive keyword spellings as values | `checkAndReportErrorForUsingTypeAsValue`'s six-name `isPrimitiveTypeName` (`checker.go:1637`) | parserSymbolIndexer5 (TS2552) | §7, diff `r6-names-primitive-spellings` |
| E2. `typeof null` | `parseTypeQuery`'s reserved-word entity name (`parser.go:3114`) | invalidTypeOfTarget | §8, diff `r6-names-typeof-null` |
| F1. decorators native never checks | `checkDecorators` (`checker.go:6022`) gated by `ast.NodeCanBeDecorated` (`ast/utilities.go:4254`) | classExpressionWithDecorator1 | §10, diff `r6-names-unchecked-regions` |
| F2. parse recovery | parser trees that differ from native | arrowFunctionsMissingTokens, YieldStarExpression2_es6, bigintArbirtraryIdentifier, importDeferTypeConflict2, parserSuperExpression2 | routed, §13 |
| G. local export specifiers | `getTargetOfExportSpecifier`'s `resolveEntityName` (`checker.go:14970`) and its `onFailedToResolveSymbol` tail | duplicateErrorNameNotFound (TS2552) | §9, diff `r6-names-export-specifier` |
| H. module augmentation | augmentation merge (`tsr-2zk.38`, main's) | moduleAugmentationInAmbientModule1, moduleAugmentationInAmbientModule5 (TS2552) | routed, §13 |

## §3 Hook diffs, in apply order

Each diff wires one root cause into main's files (`check.rs`; for #7 the
binder and `checker.rs`), removes the `allow(dead_code)` on the function it
wires, and adds its test under `crates/tsr-conformance/tests/`. Every diff
applies on the branch tip alone and in this order stacked (checked with
`git apply` in a clean worktree, and the stacked tree is byte-identical to
the one measured below).

| # | Diff | Converts | Losses | Rows (miss/extra) |
|---|---|---|---|---|
| 1 | `r6-names-value-slots.diff` | +6 cases | 0 | 3,382/1,089 → 3,363/1,089 |
| 2 | `r6-names-unchecked-regions.diff` | +5 cases | 0 | see below |
| 3 | `r6-names-js-type-annotations.diff` | +3 cases | 0 | 3,382/1,089 → 3,378/1,089 |
| 4 | `r6-names-primitive-spellings.diff` | +1 case | 0 | 3,382/1,089 → 3,381/1,089 |
| 5 | `r6-names-typeof-null.diff` | +1 case | 0 | 3,382/1,089 → 3,381/1,089 |
| 6 | `r6-names-export-specifier.diff` | +1 case | 0 | 3,382/1,089 → 3,380/1,089 |
| 7 | `r6-names-parameter-scope.diff` (superseded by `r6-names2-parameter-scope.diff`) | +5 cases, +38 type lines | 0 | 3,382/1,089 → 3,373/1,088 |

Diff 2 replaces two earlier diffs, `r6-names-empty-for-of` (measured alone:
+4 cases, extra rows −4) and `r6-names-decorator-targets` (+1 case, extra
rows −3). Each put an ancestor walk in front of every value identifier; §12
merged them into one walk asked only on the paths that report. Both earlier
files are deleted from this directory; their measurements stay here.

**All seven stacked** (base `b18aec06`): diagnostics 5,530 → **5,552 RIGHT**
(+22), EMPTY_RIGHT 5,596 unchanged, WRONG 1,063 → 1,041; types **549,853 →
549,891 RIGHT** (+38), WRONG 5,607 → 5,569; 0 losses on either dump; rows
3,382/1,089 → 3,346/1,081; `slowcases` clean on both; `cargo test
--workspace --release` 3,494 passed, 0 failed; clippy reports only the 12
pre-existing stable-toolchain findings, none in these files. Ir and CPU in
§12.

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
`Checker::names_in_unchecked_region`: a parent walk that finds a `for…of`
whose `expression` slot contains the node (§10 adds the decorator arm to the
same walk). As first shipped it ran right after the allow-list test for every
value identifier; §12 moved it onto the reporting paths of
`check_value_identifier` (the failure cascade and the three arms that report
before resolution), which still covers every code that reporter emits
(TS2304, TS2552, TS2583, TS2693, TS2301) as native's silence does.

Native probe: `for (var of missingOf) { }` → TS1123 only; `for (var x of
missingDeclared)` → TS2304; `for (var in missingIn)` → TS1123 and TS2304. The
port with the diff matches.

Measured alone (as `r6-names-empty-for-of`): +4 cases, 0 losses, extra rows
1,089 → 1,085 and no new missing row; types dump unchanged; `slowcases`
clean. The merged diff 2 reproduces it (§3).

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
difference). **Corrected by r6-names2:** TS1003 is a checker grammar error
(`checkModuleExportName`, `checker.go:5388`), not a parser difference; both
rows are ported in `r6-names2.md` §3.

The hook passes `check.rs`'s `cannot_find_name_message` in as a function
pointer, so the tail shares the table rather than copying it.
**Correction:** the first push of this cluster (`9f70f8d`) had the new file
call that function by path, which needs it `pub(crate)`; the diff carried
that change, so the branch itself did not compile without the diff. Fixed in
the next commit by passing the table in; behaviour is unchanged. Measured alone: +1 case
(`duplicateErrorNameNotFound`), 0 losses, missing rows −2
(`bigintArbirtraryIdentifier`'s `badExport.ts` row converts too; that case
stays WRONG on a parse-recovery row, §2 F), no new extra row, `slowcases`
clean.

## §10 Cluster F1: decorators on a node that cannot be decorated

A decorator's expression is checked only by `checkDecorators`
(`checker.go:6022`), reached from the accessor, function-or-method,
class-like and variable-like checks, and it returns at once unless
`ast.NodeCanBeDecorated` holds. A decorator anywhere else is TS1206 from
`checkGrammarModifiers` and its names are never resolved. Under
`experimentalDecorators` a class expression cannot be decorated; under
standard decorators a parameter cannot. The port's allow-list admitted every
decorator expression (`is_value_reference`'s `Decorator` arm) and reported
TS2304 beside TS1206.

`Checker::names_in_unchecked_region`'s decorator arm walks to any enclosing
decorator (a decorator can be `@a.b()`, so the slot test alone cannot see it)
and asks a faithful `NodeCanBeDecorated` of the decorated node
(`names_decorators_are_checked`). `grammar.rs` has a
private `node_can_be_decorated` serving TS1206, but it deliberately answers
`true` for a legacy private name (its caller reports TS1206 for that arm
itself) and leaves the this-parameter test to its caller, so it is not the
predicate this question needs; the copy here follows `utilities.go:4254`
line by line.

Native probe, same file under both modes:

```text
                                     experimentalDecorators   standard
var v = @missingA class C {…}        TS1206                   TS2304 missingA
class D { @missingB m() {} }         TS2304                   TS2304
function f(@missingC x: number) {}   TS1206                   TS1206
class E { m(@missingD x: number) {}} TS2304                   TS1206
class F { @missingE.member() p = 1 } TS2304                   TS2304
```

The port with the diff matches all ten rows. Measured alone (as
`r6-names-decorator-targets`): +1 case (`classExpressionWithDecorator1`), 0
losses, extra rows −3, no new missing row. Now part of diff 2.

## §11 Cluster D: parameter initialisers do not see the body

`resolveNameHelper` (`binder/nameresolver.go:74`) passes over a variable
found in a function's `locals` when the walk arrives from a parameter and
the variable's declaration lies in the body, unless the parameter list
*requires a scope change* (`requiresScopeChange`, `:371`): a static field
of a class expression without standard class fields, `??` or `?.` below
ES2020, an object rest binding below ES2017. The walk then continues
outward. `function bar(func = () => foo) { let foo }` reports TS2304 on
`foo`; `function f1(p = outer) { var outer }` resolves the outer `outer`.

The port's binder walk lacked the arm, so a parameter initializer resolved
the body's variable: no TS2304 where native has one, and TS2454 *used before
assigned* against the wrong declaration. The binder's suggestion walk
(`suggestion.rs`) and the checker's TS2373 side check (`class_fields.rs`)
already ported the same arm; the resolver itself never had it.

**Where it lives.** The faithful home is the resolver, which is main's
(`tsr-binder/src/lib.rs`). The arm's logic is the new
`tsr-binder/src/scope_change.rs`; the diff wires it into the `locals` arm of
`resolve_name_excluding_with_export_alias`, beside
`local_type_hidden_outside_body`, so every caller of the walk gets native's
answer (111 direct `resolve_name` calls in the checker, plus the wrappers).
`requiresScopeChange` reads two options the binder does not hold. Native's
`NameResolver` carries `CompilerOptions`; here the checker publishes them
once into a `OnceLock` on `BindResult` (`set_scope_change_options`, called
where `Checker` sets `language_version`). Unset, the arm declines, so a
caller with no options keeps the earlier answer. Rejected: a callback
parameter on the walk (the alias-meaning callback already borrows the
checker mutably, so a second closure over it does not borrow-check) and
threading options through `bind`/`bind_into` (every program builder and
test would change for one arm).

**Convention record.** Pinned operation `useOuterVariableScopeInParameter`
/ `requiresScopeChange`. Native caches the per-function answer in links
(`declarationRequiresScopeChange`); this port computes it on demand. The
expensive part (a walk over the parameters' subtrees) runs only after a
variable declared in the body was found from a parameter, which on
`domain-model` is never; 315 calls reach the out-of-line function, all
returning early. No cache, side table or publication state beyond the
options slot, which is written once with the program's options before any
lookup.

**Native probe** (`--strict`), one file:

```text
export function bar(func = () => foo) { let foo = "in"; }        TS2304 foo
let outer = "";
export function f1(p = outer) { var outer: number = 2; ... }     (nothing)
export function nullish(a = c ?? 1) { var c ...; }               es2015: TS2373 c; es2022: TS2304 c
export function plain(a = d) { var d = 1; ... }                  TS2304 d
```

The base port reported TS2454 on `outer` and `d` and nothing on `foo`; with
the diff it matches native at both targets.

**Measured alone:** +5 diagnostics cases
(`functionLikeInParameterInitializer`, `parameterInitializersForwardReferencing`,
`parameterInitializersForwardReferencing1`, `parameterInitializersForwardReferencing1_es6`,
`optionalParamReferencingOtherParams2`), 0 losses; **+38 RIGHT type lines**
(549,853 → 549,891), 0 lost; `slowcases` clean on both dumps.

**Ir: +0.1% on domain-model, above the base's noise.** Base reads
1,091,410,570–1,091,505,064 over five runs (±50k). With the diff:
1,092.5–1,093.1 M (+0.10–0.15%); generic-imports unchanged (343.06–343.09 M).
Callgrind attributes it to `lookup_scoped` (+1.48 M) and `merged_symbol`
(+0.33 M) at **identical call counts** (229,359 and 138,712 calls in both
builds), i.e. codegen, not work. The same build with the `OnceLock` field
present and the arm removed reads 1,092.2–1,092.7 M, so the field's layout
change carries most of it. Two reductions were applied first: the
arm's `IsParameterDeclaration(lastLocation)` test is inline at the call site
and the rest is `#[cold]` (that took the first draft from +0.32% to +0.10%),
and a redundant `merged_symbol` call was dropped (`lookup_scoped` already
answers the merged symbol). The integrator decides whether +0.1% Ir for
+5 cases and +38 type lines is acceptable (§12 has the stacked numbers); the falsifier for "layout, not
work" is a build that moves the options out of `BindResult` and still reads
+0.1%.

**Superseded (r6-names2):** this section's diff is replaced by
`r6-names2-parameter-scope.diff`, which keeps the options off `BindResult`
(the falsifier above held: moving them out removed the `lookup_scoped` /
`merged_symbol` shift). Measurements in `r6-names2.md` §1.

## §12 Cost of the stack

Ir (`valgrind --tool=callgrind`, §1's command), domain-model; base
1,091,410,570–1,091,505,064 over five runs:

| State | Ir | vs base |
|---|---|---|
| diffs 1+2 as first built (value slots, for-of walk) | 1,091,880,439 | +0.03% |
| first stack of all, each walk in front of every value identifier | 1,095,370,570–1,095,403,967 | +0.36% |
| for-of walk tests the kind column before building a typed node | 1,094,592,326–1,094,653,800 | +0.29% |
| one merged walk, still in front of every identifier | 1,094,265,555–1,094,896,990 | +0.26% |
| **merged walk asked only on reporting paths (shipped)** | **1,093,539,345–1,094,125,133** | **+0.19–0.25%** |

generic-imports stays at 343.06–343.11 M against 343.08 M (flat). The
patched binaries show ±0.3 M run-to-run spread where the base shows ±50k;
the cause was not found, so each state above is two runs, low to high.

Attribution of the shipped residual (callgrind, per function, against the
base profile): `lookup_scoped` +1.48 M and `merged_symbol` +0.33 M against
`resolve_name` −0.37 M, at identical call counts — diff 7's `BindResult`
layout, §11; `check_node_worker` +0.66 M, not attributed to any new call (no
new function appears under it beyond the export-specifier tail, which returns
at its first test for an import specifier); `value_reference_slot` +0.25 M
(the allow-list fallback, one match per identifier no other arm takes);
`check_value_identifier` +0.11 M (the region walk, now off the hot path).

CPU (`whole_project_perf.py`, 21 samples, median user+sys, against the base
binary): domain-model 1.028, generic-imports 1.028, diagnostics identical.
Inside the protocol's 1.03 bound, but at its edge; with the Ir numbers above
it is a real cost of roughly 0.2%, not noise. Diffs 1–6 without 7 read
1,093,124,214 (+0.15%) before the walk was moved off the hot path.

The integrator decides whether +0.2% Ir buys +22 cases and +38 type lines.
Two further reductions were not tried: keeping the scope-change options out
of `BindResult` (§11's falsifier), and moving `value_reference_slot` inline
into the allow-list match (a `check.rs` edit, main's).

## §13 Remaining, with causes and routing

**C2. JSDoc type names (3 cases).** The JSDoc half of the type-reference
decline stays (§6). Lifting it converts `typedefScope1` and fixes the
missing row of `jsdocResolveNameFailureInTypedef`, but costs 6 cases whose
JSDoc resolution differs from native: a `@typedef` used before its
declaration in the same comment run (`jsdocResolveNameFailureInTypedef`'s
own `@param {Ty}`, `jsdocTypeDefAtStartOfFile`), `import()` types in JSDoc
(`importTypeResolutionJSDocEOF`), `@callback` names (`callbackTag2`),
CommonJS class references (`commonJSImportClassTypeReference`,
`commonJSImportExportedClassExpression`) and the TS2583 lib arm in a JSDoc
type (`checkJsdocTypeTag8`). Route to r6-jsdoc: once those resolve, delete
the decline. `recursiveResolveDeclaredMembers` is different: a `@typedef` in a
*script* `.js` file is a global natively, so `types.ts` resolves `E` through
it; this binder does not merge script-file JSDoc typedefs into `globals`
(binder, main's).

**F2. Parser trees that differ from native (5 cases), route to the parser
(main's).**
- `YieldStarExpression2_es6`: `parse_assignment_expression_worker` parses
  every `yield` keyword as a `YieldExpression`. Native's `isYieldExpression`
  (`parser.go:4150`) does so only in a yield context or when the next token
  is an identifier, keyword or literal on the same line; otherwise `yield`
  is an identifier (TS2304 *Cannot find name 'yield'*, and TS1212 in strict
  code).
- `parserSuperExpression2`: native's `parseSuperExpression` parses
  `super<T>`'s type arguments, reports TS2754 and **drops** them when a call
  follows, so `T` is never in the tree. The port keeps them and resolves `T`.
- `arrowFunctionsMissingTokens`: `var c = (x) { };` is an arrow function
  with a missing `=>` natively (TS1005 *'=>' expected*, `x` a parameter);
  the port parses a parenthesised expression (TS1005 *',' expected*) and
  resolves `x`. Same code, different message, which is why the verdict sees
  only the TS2304.
- `bigintArbirtraryIdentifier`: `import { 0n as foo } from "./foo"` recovers
  differently; native never reaches `foo` (TS1003, TS1141, TS1128, TS1434 and
  TS2304 on `from`). The case's `badExport.ts` row is already fixed by diff 6.
- `importDeferTypeConflict2`: `import defer type * as ns1 from "./a"`; the
  port reports TS2304 on `as`, which native's recovery never resolves.

**H. Module augmentation (2 cases), route to `tsr-2zk.38`.**
`moduleAugmentationInAmbientModule1` and `…5`: a name imported inside
`declare module "Map"` (or `"array"`) is used inside an augmentation nested
in that module (`module "Observable" { interface … { foo(): Cls } }`,
`global { interface Array<T> { getA(): A } }`). Native resolves it through
the enclosing ambient module's locals; the port reports TS2552. Not taken,
per the brief.

**Pre-existing gaps seen while probing, not in these cases:** TS2303
(circular alias) for `namespace N { export { inner } }`; TS1003 for `export
{ "str" as s2 }` without a module specifier; TS7006 under the conformance
harness's default `strict` differs from tsgo's test runner for a JS arrow
parameter (§6's probe).
