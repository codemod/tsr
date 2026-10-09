# Lane notes: r5-errorsplit5 (tsr-2zk.1038, continuing r5-errorsplit4)

Single owner of the intrinsic/error contract, step 5. Step 4 is
[r5-errorsplit4](r5-errorsplit4.md). The decision record is
[ADR-0048](../../adr/0048-errortype-is-a-producer-identity-and-the-writer-keeps-its-rewrites.md)
and its decision log: narrow per producer, and only where it costs zero RIGHT
lines. Pinned upstream: `vendor/typescript-go` @ `5b1047d`.

## §1 Baseline and instrument

**Base.** The integration branch at `22a5e1a` carries r5-errorsplit4's
commits and its plain-JS and iteration diffs (batches X/Y). Frozen there,
unfiltered:

- types 544,668 RIGHT / 965 GAP / 6,900 WRONG (552,533 aligned lines);
- diagnostics 5,374 RIGHT / 5,584 EMPTY_RIGHT / 1,217 WRONG / 63 EMPTY_WRONG.

`ceiling` at the base:

- credited gap **4,056** (120 attributed);
- `native_error` lines 26,106 (26,020 matched);
- wholesale narrowing would cost **4,504** RIGHT lines (HadErrorBaseline
  3,375, AtLocation 732, StatementName 312, AccessOrQualifiedParent 62,
  GlobalAugmentation 23).

These are exactly r5-errorsplit4 §5's last column, so batches X/Y landed
unchanged.

**The probe.** This is r5-errorsplit4 §2.1's instrument, rebuilt here:

- the pinned tsgo test runner compiled with `go test -c -overlay`, tagging
  `@@E` (`GetErrorType()`) and `@@A` (`GetAnyType()`) on each `.types`
  line;
- run over the **whole** submodule corpus (12,157 baselines), so that a switch
  that moves a line in a case outside the starting set is still checked.

The port side is a measurement-only dump (never committed): `ceiling` prints,
for every line with a rewrite or a top-level error identity, its
`(case, file, position)`, identity, rewrite, match state and `gap_reason`
producer. A join on position checks that the probe's own line text equals the
expected baseline line. Of the base's 33,073 such lines, all but 118 align.
The misses are parse-recovery and multi-line-text cases, as in r5-errorsplit4
§2.1's control.

Each producer switch below was first made behind a temporary environment
switch (measurement only, never committed), and the lines it moved were
joined with the probe. The table entries are those joins.

## §2 Declaration-name producers (item 1)

At the base, the two largest narrowing producer rows were:

- `declaration name, symbol has no type: ALIAS / no value declaration`: 542;
- `… FUNCTION_SCOPED_VARIABLE / VariableDeclaration`: 350.

Joined with the probe, the ALIAS row is 532 `errorType`, 9 `anyType` and 1
other, all matched. The FUNCTION_SCOPED_VARIABLE row is 360 `errorType`
(45 more are unmatched and natively typed).

**The FUNCTION_SCOPED_VARIABLE row is not a producer.** `gap_reason` labels a
declaration by its symbol, but each of those variables gaps because its
*initializer* gaps:

- JSX elements: `jsxUnclosedParserRecovery` 27, `jsxNamespacePrefixInName*`
  24 + 18;
- `+` over `null`/`undefined`: the `additionOperatorWith*` family, and
  `operatorAddNullUndefined` 12;
- element access, object spreads, `new`.

So the row is switched producer by producer at the expression (§3, §4). It
falls as they land.

## §3 Commit 1: `+` over a nullable operand answers `errorType`

`checkNonNullType`'s two `errorType` exits are:

- `unknown` under `strictNullChecks` (`checker.go:7411`);
- a remainder that is nullable or `never` (`:7429`).

The `+` arm then answers `errorType` for an `isErrorType` operand
(`:12436`).

The port spelled the first exit as the gap in two places:

- `nullable_operand.rs`'s `non_null_operand_type`;
- `members.rs`'s `check_non_null_type`, through which the second exit reaches
  it. That file is main's.

`binary.rs`'s `check_addition` answered the gap for any `is_error` operand.

**The switch.**

- `non_null_operand_type` answers `native_error` for both exits. A `gap`
  from `check_non_null_type` is read as the exit it is (`:7411`/`:7429`),
  unless the operand was itself the gap.
- `check_addition` answers the gap only for a gap operand, and
  `native_error` for an operand that is upstream's `isErrorType`.

The edit stays in this lane's files. `check_non_null_type`'s other eleven
callers are unchanged.

**Probe join.** 256 lines move from the gap to `native_error`, and all 256 are
`errorType` natively. No line moves anywhere else.

**Measured** (unfiltered, both dumps, against `22a5e1a`):

| | base | commit 1 |
|---|---:|---:|
| types RIGHT / GAP / WRONG | 544,668 / 965 / 6,900 | unchanged, zero transitions |
| diagnostics | 5,374 / 5,584 / 1,217 / 63 | unchanged, zero transitions |
| type / diagnostics losses | — | **0 / 0** |
| credited gap | 4,056 | **3,802** (−254) |
| `native_error` lines (matched) | 26,106 (26,020) | 26,362 (26,276) |
| wholesale narrowing RIGHT→GAP | 4,504 | **4,313** (−191) |

Perf, median child CPU over 21 samples against the base binary:
domain-model 1.012, generic-imports 0.987.

Tests: `tests/errortype_producers.rs` pins `null + null` and `1 + undefined`
as `native_error`, with `"a" + null` (string) and `any + any` (`anyType`) as
controls.

## §4 Commit 2: a JSX element without `JSX.Element` answers `errorType`; a fragment `anyType`

`getJsxType` (`jsx.go:1295`) answers `errorType` when the `JSX` namespace or
its `Element` export is absent (`:1303`). `checkJsxElement` and
`checkJsxSelfClosingElement` return that as is (`jsx.go:72`, `:101`).
`checkJsxFragment` turns an `isErrorType` element type into `anyType`
(`jsx.go:123`).

`expressions.rs`'s `check_jsx_element` answered the gap for a missing hop.
§249 there had tried `any` and measured 36 to 69 RIGHT→WRONG lines, so it was
reverted. That attempt gave the *element* `anyType`, which is upstream's
answer only for a fragment. With ADR-0048's split, each form takes its own
native identity:

- an element or self-closing element: `native_error`;
- a fragment: `any` when the element type is upstream's `isErrorType`;
- the gap, when the declared `Element` type is itself the gap.

**Probe join** (each arm alone first):

| arm | lines moved | native `errorType` | native `anyType` | native other |
|---|---:|---:|---:|---:|
| element / self-closing → `native_error` | 601 | 599 | 0 | 2 |
| fragment → `any` | 27 | 0 | **27** | 0 |

The two "other" lines are false claims. They are
`jsxNamespaceGlobalReexport:2:2` and `jsxNamespaceImplicitImportJSXNamespace:2:2`
(`<div></div> : JSX.Element` natively). The port's `jsx_type_symbol` does not
follow a `JSX` namespace re-exported through a global or implicit import.
Both were WRONG before and stay WRONG. They are listed here, not counted as
converted. Five more moved lines in `conflictMarkerTrivia3` do not align with
the probe (parse recovery, §1).

**Measured** cumulatively with commit 1 (unfiltered, both dumps, against
`22a5e1a`):

| | base | commits 1+2 |
|---|---:|---:|
| types RIGHT / GAP / WRONG | 544,668 / 965 / 6,900 | **544,688 / 956 / 6,889** |
| diagnostics | 5,374 / 5,584 / 1,217 / 63 | unchanged, zero transitions |
| type / diagnostics losses | — | **0 / 0** |
| credited gap | 4,056 | **3,184** |
| `native_error` lines (matched) | 26,106 (26,020) | 26,968 (26,873) |
| wholesale narrowing RIGHT→GAP | 4,504 | **3,768** |

**Gains (+20: 11 WRONG→RIGHT, 9 GAP→RIGHT).** Every one is a computed type
around a JSX element whose `JSX` namespace is not in scope:

- `view : () => any[]` (a method returning an array of elements) in
  `jsxEmitWithAttributes`, `jsxFactoryAndReactNamespace`,
  `jsxFactoryIdentifier`, `jsxFactoryNotIdentifierOrQualifiedName{,2}` and
  `jsxFactoryQualifiedName`, plus each method's array literal;
- `{ wrong: any; }` in both `jsxSpreadTag` targets;
- fragments: `jsxFragmentAndFactoryUsedOnFragmentUse` ×2 and
  `inlineJsxAndJsxFragPragmaOverridesCompilerOptions` ×4.

Tests: `tests/errortype_producers.rs` pins `<div />` without a namespace as
`native_error` and `<></>` as `any`, with a declared `JSX.Element` as the
control.

## §5 Commit 3: `checkSuperExpression`'s identities, and `GetSuperContainer`'s static-block and decorator arms

This one function holds item 2's largest native-`anyType` population
(`superInObjectLiterals_ES6`, 18 lines, plus `classExtendingAny`) and a row of
native-`errorType` exits. All of them were spelled as the gap or as the `any`
stand-in:

- **object-literal container.** Upstream answers `anyType`: "for object
  literal assume that type of 'super' is 'any'" (`checker.go:7917`). The port
  kept the gap, under `checker-notes-rank.md` §6's ban on banking `any`. The
  probe shows the line *is* upstream's `anyType`, so the ban no longer
  applies: it guarded against a guessed `any`, and this one is measured.
- **illegal usage** (`checker.go:7912`, after its diagnostic). These
  cover a function container, a super call outside a constructor, a call
  through an arrow, a computed name, and the walk finding no member. Native
  answers `errorType` for all of them.
- **the base arms.** No `extends` (TS2335, `:7925`), a call on an `extends
  null` class (`:7929`), a class without base types (`:7939`), and a
  constructor-argument initializer (`:7944`) all answer `errorType`.

**The probe found the walk incomplete.** Switched alone, the "no member found"
exit claimed `errorType` for 13 lines that natively have types:

- `classFieldSuperAccessible`, `classFieldSuperAccessibleJs1`,
  `javascriptThisAssignmentInStaticBlock`. `GetSuperContainer`
  (`ast/utilities.go:1835`) returns a `ClassStaticBlockDeclaration` as a
  container, and `ast.IsStatic` holds for it. The port's walk had no such
  arm, so a static block's `super` walked to the class and found no member.
- `esDecorators-preservesThis`. The `KindDecorator` arm (`:1837`) skips the
  decorated class element, or the member that owns a decorated parameter,
  and keeps walking past the class. The port stopped at the decorated
  element's class.

Both arms are ported, the decorator one with the same skip the computed-name
arm uses. After that, the exit moves 4 lines, all `errorType`, and it switches
too.

**Probe join** (each arm alone, against commit 2):

| arm | lines moved | native `errorType` | native `anyType` | other |
|---|---:|---:|---:|---:|
| object-literal container → `any` | 27 | 0 | **27** | 0 |
| illegal usage and base arms → `native_error` | 131 | 131 | 0 | 0 |
| the stand-in `any` exits → `native_error` | 35 | 29 | 0 | 6 |
| "no member found", after the walk fix | 4 | 4 | 0 | 0 |

The 6 "other" lines are not false claims. They are a parse-recovery
misalignment in `derivedClassSuperCallsInNonConstructorMembers` (`a: super()`
in a type position, §1). The probe tags that case's `super` lines `@@E`.

**Measured** cumulatively with commits 1–2 (unfiltered, both dumps, against
`22a5e1a`):

| | commits 1+2 | commits 1–3 |
|---|---:|---:|
| types RIGHT / GAP / WRONG | 544,688 / 956 / 6,889 | **544,751 / 936 / 6,846** |
| diagnostics | unchanged | unchanged, zero transitions |
| type / diagnostics losses vs base | 0 / 0 | **0 / 0** |
| credited gap | 3,184 | **3,058** |
| wholesale narrowing RIGHT→GAP | 3,768 | **3,606** |

**Gains (+63: 43 WRONG→RIGHT, 20 GAP→RIGHT)**, all from the two walk arms:

- static blocks: `classStaticBlock5` ×18 (three targets),
  `javascriptThisAssignmentInStaticBlock` 15, `classFieldSuperAccessible` 6,
  `classFieldSuperAccessibleJs1` 6;
- decorators: `esDecorators-preservesThis` 12, `decoratorOnClassMethod12` 4;
- `errorSuperPropertyAccess(target=es2015)` 2.

Tests: `tests/errortype_producers.rs` covers four cases:

- object-literal `super` is `any`;
- a function container is `native_error`;
- a class without a base is `native_error`;
- a static block's `super` is `typeof B`.

## §6 Diff A (`symbols.rs`, main's): the ALIAS row, `getTypeOfSymbol`'s fallthrough, circular accessors, and the `GlobalAugmentation` narrowing

[`r5-errorsplit5-alias-symbols.diff`](r5-errorsplit5-alias-symbols.diff)
applies on commit 4 (`git apply`). It carries three producer arms in
`symbols.rs`, one writer narrowing in `types_producer.rs`, and their tests.

**N — `get_type_of_alias`'s final arm** (the ALIAS declaration-name row, 542
lines at the base). `getTypeOfAlias` answers `errorType` when the resolved
target is not a value (`checker.go:18614`). The port's arm answered the gap
for two different things, so it is split:

- **The chain resolves to its end, and that end is not a value: `native_error`.**
  Switched as "any `Some` target" first, it moved 473 lines: 459
  `errorType`, 14 natively `anyType` or typed. The 14 were all chains the
  port *breaks*: an alias whose own resolution answered `None`, as in
  shorthand ambient re-exports, `import mod = globalThis`, cross-file
  merges. `get_symbol_flags` then reads "no VALUE". Natively such a chain
  ends at `unknownSymbol`, which `getSymbolFlags` reports as all flags
  (`checker.go:16379`), so it is never the not-a-value arm. Restricting the
  arm to chains whose `resolve_alias_fully` reaches a non-alias symbol
  removes all 14. It also returns 37 `errorType` lines to the gap, a
  conservative cost.
- **The port resolved nothing (`None`), or broke the chain: the gap.**
  Switched alone, the `None` arm moved 168 `errorType` lines and 69 natively
  `anyType` or typed ones, mixed in every declaration form (import specifier, default,
  namespace import, `import =`, export specifier). It is the port's
  resolution miss beside upstream's `unknownSymbol`, so it stays the gap,
  for the same reason r5-errorsplit4 held its `a` arm.

**S — `get_type_of_symbol`'s final fallthrough** (`checker.go:16521`). This
is a symbol with no value meaning at all, such as the name of
`declare global` or a type-only namespace. 159 of 161 moved lines are
`errorType`. The other two are `duplicateVarsAcrossFileBoundaries` (a
`var`/`import =` merge across script files, so the port's merge differs) and
one already-WRONG line in `moduleAugmentationDuringSyntheticDefaultCheck`.

**CA — a circular accessor** (`getTypeOfAccessors`' failed pop,
`checker.go:18557`). Upstream answers `anyType`. The port kept the gap because
its eager object-literal members close cycles native never forms
(`noCircularitySelfReferentialGetter3/4`). Measured, that fear does not
materialize:

- every moved line is `anyType` natively (11) or already WRONG (2, in
  `circularAccessorAnnotations`);
- zero losses;
- +2 GAP→RIGHT: `recursiveGetterAccess:0:1` and
  `recursiveProperties(target=es2015):0:1`.

This is item 2's `circularAccessorAnnotations` (6 credited lines).

**GN — narrowing `GlobalAugmentation`.** With S in place, every `declare
global` name is `native_error`, and the rewrite's cost table reads **zero**.
Per the decision log, the rewrite narrows: the writer's guard applies to
`native_error` only, and a gap name prints `error`. Measured with the arms,
narrowing changes no line. Without S it would cost 23 RIGHT lines, so it
ships inside this diff.

**Measured** on commit 4 (unfiltered, both dumps, against `22a5e1a`):

| | commit 4 | commit 4 + diff A |
|---|---:|---:|
| types RIGHT / GAP / WRONG | 544,752 / 936 / 6,845 | **544,754 / 934 / 6,845** |
| diagnostics | unchanged | unchanged, zero transitions |
| type / diagnostics losses vs base | 0 / 0 | **0 / 0** |
| credited gap | 3,058 | **2,575** |
| `native_error` lines (matched) | 27,109 (27,008) | 27,587 (27,485) |
| wholesale narrowing RIGHT→GAP | 3,606 | **3,030** |
| `StatementName` | 312 | **29** |
| `GlobalAugmentation` | 23 | **0, narrowed** |

Perf, measured with diff B applied too (median child CPU): 21 samples read
domain-model 1.063 and generic-imports 0.992. Re-run at 41 samples,
domain-model read 1.008.

**Tests in the diff.**

- `errortype_producers.rs` adds two tests: an alias to a type is
  `native_error`, and a type-only namespace is `native_error`.
- Three existing tests pinned the gap where native is `errorType`, and now
  pin `native_error`:
  - `export_specifiers.rs`'s type-only specifier test, whose own falsifier
    note anticipated this. Both routes now answer `errorType`.
  - `symbol_chain.rs`'s type-only qualified alias, and constructing it.
  - `symbol_chain.rs`'s `export =` interface namespace import. Its
    self-import cycle stays the gap.

The workspace tests pass with both diffs applied. Clippy and fmt are clean in
the touched code.

## §7 Item 3: §32's twins, an unresolved type-reference receiver

Upstream's unresolved type reference (`getTypeFromTypeAliasReference` for an
unresolved symbol) is any-flagged *with an alias*. So `isErrorType` holds for
it (`checker.go:26641`), and the two accesses through it answer:

- **property access**: `errorType` (`checker.go:11314-11320`: `isAnyLike`,
  then `isErrorType(apparentType)`);
- **element access**: the receiver itself (`checker.go:8153`:
  `if c.isErrorType(objectType) { return objectType }`), printed by its alias
  name.

The port's twins (`members.rs` `check_property_access_expression`,
`indexed.rs` `element_access_lookup`) both answered `anyType`, behind the §31
gate (`!file_has_import_machinery`). In an import-machinery file they fell
through to the gap.

**The gate stays.** With the property twin switched to `native_error`
everywhere (gate removed), the probe join is:

| population | lines moved | native `errorType` | other |
|---|---:|---:|---:|
| files without import machinery (the twin's `any`) | 3,205 | 3,196 | 9 |
| import-machinery files (the gap fall-through) | 43 | 3 | **40** |

In import-machinery files the unresolved mint is mostly the port's own
resolution miss: `moduleAugmentation*`, `jsDeclarationEmitDoesNotRenameImport`.
That is the §31 gate's reason, so the gate stays and only the non-import arm
switches. The 9 "other" lines are JS `require` receivers
(`varRequireFromJavascript`, `varRequireFromTypescript`,
`commonJSImportExportedClassExpression`) that the port mints as unresolved
references. They were WRONG before and stay WRONG; they are listed here, not
counted.

The OBJECT-flagged deferred mints in `unresolved_types` are the port's gap
(`Checker::is_gap`), not upstream's type, and keep the twins' `any` stand-in.

### §7.1 Commit 4: `indexed.rs`

`element_access_lookup` answers the receiver for an any-flagged unresolved
reference. Exactly one printed line in the corpus changes:
`recursiveTypeRelations:0:47`, `obj[exportedClassName] : ClassNameObject`,
WRONG→RIGHT. Native prints the alias name there, which is the identity
`checker.go:8153` returns.

**Measured** cumulatively with commits 1–3 (unfiltered, both dumps, against
`22a5e1a`): zero losses; types **544,752 / 936 / 6,845** (+1 WRONG→RIGHT);
diagnostics unchanged. Credited gap and narrowing are unchanged (3,058 and
3,606), because the old `any` was not the gap.

Tests:

- `an_element_access_through_an_unresolved_reference_is_the_reference`
  (`x[0]` with `x: Missing` is `Missing`);
- `element_access_any.rs`'s `a_receiver_this_port_could_not_type_is_still_a_gap`
  pinned the old `any`, and now pins `Unresolved`.

Perf, median child CPU against the base binary: 21 samples read
generic-imports 1.099 and domain-model 0.952. Re-run at 41 samples per the
protocol, they read domain-model 1.025 and generic-imports 1.005. The change
is one arm behind a set lookup that the old code already made.

### §7.2 Diff B (`members.rs`, main's): the property twin answers `errorType`

[`r5-errorsplit5-members-twin.diff`](r5-errorsplit5-members-twin.diff)
applies on commit 4, independently of diff A. Outside import-machinery files,
`check_property_access_expression`'s §32 twin answers `native_error` for an
any-flagged unresolved receiver. It used to answer `anyType`. The test is in a
new file (`tests/property_access_unresolved_receiver.rs`), so the two diffs do
not touch the same file.

**Measured** on commit 4 (unfiltered, both dumps, against `22a5e1a`):

- zero losses;
- types 544,752 / 944 / 6,837;
- diagnostics unchanged;
- `native_error` lines 27,109 → **30,314**: 3,196 moved lines, all
  `errorType` natively, 3,000 of them in `parserRealSource*`.

The 9 false claims are the JS `require` receivers in §7. 8 of them move
WRONG→GAP: they now print `error` in cases with no `.errors.txt`, where
upstream prints a real type. They were never RIGHT.

Credited gap and narrowing are unchanged (3,058 and 3,606). The old `any` was
`anyType`, not the gap, so this diff fixes identity and is not narrowing work.

## §8 Held, remaining, and found

- **Held: the ALIAS `None` arm** (§6), and the property twin in
  import-machinery files (§7). Each is mixed natively.
- **The FUNCTION_SCOPED_VARIABLE row** fell 350 → 192 with commits 1–2. What
  remains comes from other initializer producers:
  - element access, 215 lines across all rows. `element_access_lookup`'s
    failed-lookup `error` is upstream's mix of `errorType` and `anyType` by
    arm (`getPropertyTypeForIndexType`, `noImplicitAny`, the JS-literal arm),
    so it needs per-arm probing;
  - `new` and call expressions;
  - object spreads (`spreads.rs`, r5-typetriage's).
- **Item 2's remainder.** 64 credited lines are natively `anyType` (115 at
  the base). The largest groups:
  - binding elements of contextually typed or empty-array patterns:
    `fallbackToBindingPatternForTypeInference` 10,
    `destructuringArrayBindingPatternAndAssignment2` 5,
    `declarationsAndAssignments` 4, `iterableArrayPattern21` 2. Their
    producers are in contextual typing and inference (main's);
  - `extendFromAny` 4, `underscoreTest1` 4, `truthinessCallExpressionCoercion2` 3;
  - `recursiveExportAssignmentAndFindAliasedType7` 3, an untyped module
    alias that is the §6 `None` arm.
- **False claims made by these switches.** All of them are listed above.
  None was RIGHT except `duplicateVarsAcrossFileBoundaries`' two lines in
  diff A. Each one traces to a port divergence elsewhere: alias resolution,
  JSX namespace lookup, script-file merging, or JS `require` minting.

## §9 Narrowing, re-measured after each switch

ADR-0048's decision log narrows a rewrite only at zero RIGHT cost. The cost
of each rewrite, as RIGHT→GAP lines:

| rewrite | base (`22a5e1a`) | commit 1 | commit 2 | commit 3 | commit 4 | + diff A |
|---|---:|---:|---:|---:|---:|---:|
| `HadErrorBaseline` | 3,375 | 3,184 | 2,639 | 2,513 | 2,513 | 2,353 |
| `AtLocation` | 732 | 732 | 732 | 696 | 696 | 586 |
| `StatementName` | 312 | 312 | 312 | 312 | 312 | 29 |
| `AccessOrQualifiedParent` | 62 | 62 | 62 | 62 | 62 | 62 |
| `GlobalAugmentation` | 23 | 23 | 23 | 23 | 23 | **0 → narrowed** |
| **total** | 4,504 | 4,313 | 3,768 | 3,606 | 3,606 | 3,030 |
| credited gap | 4,056 | 3,802 | 3,184 | 3,058 | 3,058 | 2,575 |

In the commits, no rewrite reaches zero. Diff A takes `GlobalAugmentation` to
zero and narrows it. `StatementName`'s 29 are the ALIAS `None` arm and
natively-`anyType` untyped-module aliases (§6, §8). Diff B leaves the table
unchanged (§7.2).
