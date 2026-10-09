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
| C. type positions in JavaScript files | `getTypeFromTypeReference` in a `.js` file; JSDoc type names | fillInMissingTypeArgsOnJSConstructCalls, parserArrowFunctionExpression10, parserArrowFunctionExpression17, jsdocResolveNameFailureInTypedef, typedefScope1, recursiveResolveDeclaredMembers | open |
| D. parameter initialisers | `resolveName`'s `useOuterVariableScopeInParameter` (`binder/nameresolver.go:74`, `:346`) | functionLikeInParameterInitializer(es2015), parameterInitializersForwardReferencing(es2015) | open |
| E. keyword-spelled identifiers | `onFailedToResolveSymbol`'s six-name `isPrimitiveTypeName`; `typeof null` | parserSymbolIndexer5 (TS2552), invalidTypeOfTarget | open |
| F. parse recovery | parser trees that differ from native | arrowFunctionsMissingTokens, YieldStarExpression2_es6, bigintArbirtraryIdentifier, importDeferTypeConflict2, parserSuperExpression2, classExpressionWithDecorator1 | open |
| G. spelling suggestions | `getSpellingSuggestionForName` at an export specifier | duplicateErrorNameNotFound (TS2552) | open |
| H. module augmentation | augmentation merge (`tsr-2zk.38`, main's) | moduleAugmentationInAmbientModule1, moduleAugmentationInAmbientModule5 (TS2552) | routed |

## §3 Hook diffs, in apply order

Each diff touches `check.rs` (main's), removes the `allow(dead_code)` on the
function it wires in `name_slots.rs`, and adds its test under
`crates/tsr-conformance/tests/`. Each was measured alone on the base.

| # | Diff | Converts | Losses | Rows (miss/extra) |
|---|---|---|---|---|
| 1 | `r6-names-value-slots.diff` | +6 cases | 0 | 3,382/1,089 → 3,363/1,089 |
| 2 | `r6-names-empty-for-of.diff` | +4 cases | 0 | 3,382/1,089 → 3,382/1,085 |

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
