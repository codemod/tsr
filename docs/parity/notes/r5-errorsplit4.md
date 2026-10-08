# Lane notes: r5-errorsplit4 (tsr-2zk.1038, continuing tsr-2zk.1009 / .960 / .944)

Single owner of the intrinsic/error contract, step 4. Step 3 is
[r5-errorsplit3](r5-errorsplit3.md), step 2 [r5-errorsplit2](r5-errorsplit2.md);
the decision record is
[ADR-0048](../../adr/0048-errortype-is-a-producer-identity-and-the-writer-keeps-its-rewrites.md)
and its decision log (narrow per producer, only where it costs zero RIGHT
lines). Pinned upstream: `vendor/typescript-go` @ `5b1047d`.

## §1 Baseline

The integration branch at dispatch (`02a7110`) did not carry r5-errorsplit3's
held final-`else` diff, so this lane applied it verbatim as its first commit
(`c4e2746`) and froze the baseline there. Types 543,914 RIGHT / 1,018 GAP /
7,601 WRONG (552,533 aligned lines); diagnostics 5,328 RIGHT / 5,581
EMPTY_RIGHT / 1,263 WRONG / 66 EMPTY_WRONG. `ceiling`: credited gap **4,554**
(177 attributed); `native_error` lines 24,385 (24,310 matched); wholesale
narrowing would cost 5,158 RIGHT lines (HadErrorBaseline 3,590, AtLocation
1,155, StatementName 312, AccessOrQualifiedParent 78, GlobalAugmentation 23).
The producer row "reference, the name does not resolve" holds 875 of them:
the population this step is for.

## §2 The §31 gate, measured against native line by line

### §2.1 The instrument: a per-line native identity probe

r5-errorsplit2 and r5-errorsplit3 verified each switch with a hand-written
fixture through `GetTypeAtLocation`. That answers "is this construct
`errorType`?" but not "is *every line this rule moves* `errorType`?", which is
what a per-producer switch needs. So this lane probed the corpus itself.

The pinned tsgo was built with `scripts/offline-cargo/build-tsgo.sh`. Its
compiler test runner was then compiled with `go test -c -overlay`, replacing
two files (nothing tracked is touched):

- `internal/testutil/tsbaseline/type_symbol_baseline.go`: when `TSR_ERRPROBE`
  names a directory, `writeTypeOrSymbol` appends ` @@E` to a line whose type
  is `GetErrorType()` and ` @@A` to one whose type is `GetAnyType()`.
  `checkBaselines` writes the `.types` text to that directory instead of
  diffing it, and skips `.symbols`.
- `internal/testrunner/compiler_runner.go`: under the same variable,
  `runSingleConfigTest` runs only `verifyTypesAndSymbols`.

Run with `-test.run '^TestSubmodule$/^(case|…)\.tsx?$'` over the 1,991 cases
in which `ceiling` reports a gap or `native_error` line. The output is
upstream's own `.types` walk with each line's identity, so it aligns with
`ceiling`'s per-line dump (`TSR_CEILING_DUMP`) by (case, file, position).

**Control.** Of the port's 24,310 matched `native_error` lines at the base,
24,307 are `errorType` natively. The other 3 are join artifacts in
parse-recovery cases, where a multi-line source text shifts the `>` count.
The instrument agrees with every identity the earlier steps verified by
fixture.

### §2.2 The gate's sub-populations

The gap arm of `checkIdentifier`'s unresolved exit (`expressions.rs`) held
five disjoint sub-populations, each classified by the first condition that
holds. A temporary env-gated switch (measurement only, never committed) moved
one sub-population at a time to `native_error`, and `ceiling` attributed the
lines that moved.

| arm | condition | lines moved (matched) | native `errorType` | native other | decision |
|---|---|---:|---:|---:|---|
| `a` | found in another meaning **as an ALIAS** | 34 (15) | 15 | **19** (values natively) | **stays the gap** |
| `A` | found in another meaning, not an alias | 93 (90) | 90 (3 unaligned by the join, §2.1) | 0 | switch |
| `R` | `arguments`, not found as a value | 42 (42) | 42 | 0 | switch outside a function container |
| `I` | file has ES import machinery | 41 (40) | 41 | 0 | switch |
| `J` | `.js` file, no CommonJS machinery | 304 (304) | 304 | 0 | **held** (§3) |

Natively, each of the four switched arms is `getResolvedSymbol` answering
`unknownSymbol`, so `checkIdentifier` answers `errorType`
(`checker.go:11048`):

- **`A`.** `resolveName` with `Value` meaning passes by a type-only symbol, so
  the result is TS2693 or TS2708.
- **`R`.** Outside a function container, `arguments` reaches no
  `argumentsSymbol` (`nameresolver.go:228-235`) or falls in
  `checkIdentifier`'s property-initializer arm (`:11050-11053`).
- **`I`, `J`.** No declaration anywhere.

The gate's file-shape tests chose how the name *printed*. ADR-0039 and
ADR-0048 gave that choice to the writer, and the probe shows the shapes never
predicted identity.

`a` is the port's own miss. An import binding the port cannot resolve may
still be a value natively: `importAliasAnExternalModuleInsideAnInternalModule`
(`C : typeof C`) and `privacyGloImport` (`use_glo_M1_public : typeof …`) are
examples. So it stays the gap. `arguments` *inside* a function also stays the
gap when `IArguments` cannot be read: natively it is `argumentsSymbol`, never
`errorType`.

### §2.3 Commit 2: arms `A`, `R` (outside a container) and `I`

Measured unfiltered against §1's base, both dumps:

| | base | commit 2 |
|---|---:|---:|
| types RIGHT / GAP / WRONG | 543,914 / 1,018 / 7,601 | **543,921 / 1,016 / 7,596** |
| diagnostics RIGHT / EMPTY_RIGHT / WRONG / EMPTY_WRONG | 5,328 / 5,581 / 1,263 / 66 | unchanged, zero transitions |
| type losses / diagnostics losses | — | **0 / 0** |
| credited gap (`ceiling`) | 4,554 | **4,367** (−187) |
| `native_error` lines (matched) | 24,385 (24,310) | 24,561 (24,482) |
| wholesale narrowing RIGHT→GAP | 5,158 | 4,557 (−601) |
| producer row "reference, the name does not resolve" | 875 | 354 |

**Gains (+7 WRONG→RIGHT):** `arguments:0:13/16/23` (`typeof arguments` in a
signature now computes), `argumentsUsedInClassFieldInitializerOrStaticInitializationBlock:0:86/87`
and `defaultArgsInFunctionExpressions:0:63/64` (a parameter default reading an
unresolved name no longer gaps the signature).

**2 GAP→WRONG, a false claim:** `topLevelAwait.1(module=es2022|esnext,target=es2017):0:21`.
The port's parser does not reparse `await <number>(x)` as an await expression,
so `await` reaches `checkIdentifier` as a name that natively never exists.
Before, these lines answered the gap (honest). Now they claim `native_error`,
and the identity is wrong because the parse is wrong. This is the same shape
as r5-errorsplit3's `jsDeclarationsComputedNames` claim: listed here, not
counted as converted. The fix is the parser's top-level-await reparse.

Of the 354 lines left in the producer row, the `a` arm and the held `J` arm
are named above. The rest are member-name and JSX-name positions that do not
reach this arm (`gap_reason`'s label is positional).

The removed file-shape tests also removed the only reasons the gate needed
`file_has_commonjs_machinery`. It stays only for the held arm (§3), and goes
when that lands.

## §3 Held: the plain-JS arm needs `resolveErrorCall`

Switching `J` alone measured **11 type losses**, all in two cases with no
`.errors.txt`:

- `parsingDeepParenthensizedExpression` ×6;
- `spellingUncheckedJS` ×5, e.g. `inmodule.toFixed() : error`.

There the writer's fast path prints upstream's `errorType` as `error`. The
lines are *calls* through an unresolved callee. Native `resolveCallExpression`
tests `isErrorType(apparentType)` before the untyped-call arm and answers
`resolveErrorCall`'s `unknownSignature`, which returns `errorType`
(`checker.go:8516`, `:9923`). The port's `check_call_expression_worker`
(`calls.rs`) has no such arm, so a `native_error` callee falls to the
untyped-call arm and answers `anyType`.

With the gap callee, the call answered the gap, which printed `error` by
accident. `calls.rs` is main's file, so the arm ships with the switch as
[`r5-errorsplit4-plain-js.diff`](r5-errorsplit4-plain-js.diff), measured in
§3.1.

### §3.1 The diff, measured

The diff carries the `J` switch, the `calls.rs` arm (`callee_type ==
native_error` answers it, before `is_untyped_call_target`), the removal of
`file_has_commonjs_machinery` and its cache, and the JS test flipped back to
`native_error`. It applies on commit 2. Measured unfiltered, both dumps:

| | base | commit 2 | commit 2 + diff |
|---|---:|---:|---:|
| types RIGHT / GAP / WRONG | 543,914 / 1,018 / 7,601 | 543,921 / 1,016 / 7,596 | **543,928 / 1,020 / 7,585** |
| diagnostics | — | unchanged | unchanged |
| type / diagnostics losses vs base | — | 0 / 0 | **0 / 0** |
| credited gap | 4,554 | 4,367 | **4,056** |
| `native_error` lines (matched) | 24,385 (24,310) | 24,561 (24,482) | 25,427 (25,341) |
| wholesale narrowing RIGHT→GAP | 5,158 | 4,557 | 4,504 |

Transitions from commit 2 to the diff:

- **+7 WRONG→RIGHT**, all in `parsingDeepParenthensizedExpression`. These are
  `e : error`-shaped lines through calls whose callee is the undeclared name.
- **4 WRONG→GAP:**
  - `topLevelAwait.1` ×2: §2.3's false claim. The misparsed `await<number>(x)`
    is now a call through `native_error`, and it prints `error`.
  - `dynamicImportsDeclaration:3:0/1`: a false claim on an existing
    divergence. The probe lists that case's `native_error` lines as natively
    neither identity (§6).

The `calls.rs` arm is the only edit outside this lane's files. It is one
identity test that mirrors `resolveCallExpression`'s order. Measured without
it, the switch costs the 11 losses above.

## §4 The held predicate sites, ported (diffs)

r5-errorsplit3 §2.3 held three sites because a predicate swap alone would send
upstream's `errorType` down a road upstream never takes. Each is ported here
as a diff against commit 2 (the files are main's or r5-typeparams2's), and
measured unfiltered on both dumps against commit 2.

| diff | file | sites | type / diag transitions | `ceiling` |
|---|---|---|---|---|
| [iteration-symbols](r5-errorsplit4-iteration-symbols.diff) + [iteration-array](r5-errorsplit4-iteration-array.diff) | `symbols.rs`, `array_literals.rs` | 4 + 2 | **none** | unchanged |
| [default-declared](r5-errorsplit4-default-declared.diff) | `declared.rs` | 2 + its test | **none** | unchanged |

**The iteration arms** (`semantic_iterable_yield_types_worker`,
`array_spread_element_type`):

- The decline becomes `is_gap`.
- The adjacent `== any` test becomes `is_type_any`, and it answers the
  `anyType` *intrinsic*, not the input. That is the part a swap would have
  missed. `getIterationTypesOfIterableSlow` (`checker.go:6463`),
  `getIterationTypesOfIteratorWorker` (`:6496`), `getIterationTypesOfMethod`
  (`:6555`) and `getIterationTypesOfIteratorResult` (`:6650`) all return
  `IterationTypes{anyType, anyType, anyType}` for an any-like type. The port
  returned the type itself, which was `any` only because only `any` reached it.
- The result arm reads `IsTypeAny` per union constituent, because a union
  holding an any-flagged type *is* that type natively.

Zero transitions is expected at this base: no `native_error` reaches a
`[Symbol.iterator]` member. The arms matter once member producers switch
(the P4 alias type, §32's twins in §6), and with them in place those switches
cannot route `errorType` into the protocol-failure arm.

**The default** (`get_resolved_type_parameter_default`): the guard becomes
`is_gap`, so a default that is upstream's identity is published like any
other. That covers `native_error` and the any-flagged unresolved reference
`Missing<T>`. **Recommendation: cache.** Native caches the resolved default
unconditionally (`getResolvedTypeParameterDefault`, `checker.go:22007`), and
measured, caching changes no line. It also stops the port minting a fresh
unresolved-reference identity on every read of the same default. The test
that pinned "not published" is renamed and now asserts one published
identity across reads (`…_and_is_published_once`). Only the gap and the
OBJECT-flagged deferred placeholders stay unpublished. The cache key is
unchanged (parameter, alias bindings, mapped-template depth), so no new
ownership question arises.

## §5 Narrowing, re-measured after each switch

Per the ADR-0048 decision log, after every producer switch each of the
writer's five gap→`any` rewrites is re-measured, and a rewrite is narrowed to
`native_error` only if that costs zero RIGHT lines. `ceiling`'s narrowing
table gives the cost of each rewrite, by rewrite, as RIGHT→GAP lines:

| rewrite | base (`c4e2746`) | commit 2 | commit 2 + plain-JS diff |
|---|---:|---:|---:|
| `HadErrorBaseline` (SS180) | 3,590 | 3,416 | 3,375 |
| `AtLocation` (the position guards) | 1,155 | 741 | 732 |
| `StatementName` | 312 | 312 | 312 |
| `AccessOrQualifiedParent` | 78 | 65 | 62 |
| `GlobalAugmentation` | 23 | 23 | 23 |
| **total** (cases) | 5,158 (1,053) | 4,557 (940) | 4,504 (925) |

**No rewrite reaches zero, so none is narrowed.** The smallest,
`GlobalAugmentation` (23), is all declaration names of `declare global`. Its
producer is the gap of a namespace symbol with no value declaration, which no
`checkIdentifier` arm reaches. `StatementName` (312) did not move either. Both
wait on declaration-name producers (`symbol has no type: ALIAS` 544,
`FUNCTION_SCOPED_VARIABLE` 350), not on unresolved references.

The ADR's falsifier is "the residual stops falling across rounds". It fell by
601 lines in this step (5,158 → 4,557), and by 654 with the held diff.

## §6 Other findings

- **The credited gap is mostly upstream's `errorType`, but not all of it.** At
  commit 2, 4,250 of the 4,367 credited gap lines are `errorType` natively.
  115 are upstream's `anyType`: producers that should answer `any`, not
  `native_error`, when they are ported. The largest are
  `superInObjectLiterals_ES6` (18), `fallbackToBindingPatternForTypeInference`
  (10), `circularAccessorAnnotations` (6), `inlineJsxAndJsxFragPragma` (6),
  and `extendFromAny`/`classExtendingAny` (4 each). The probe output names them
  line by line. Two are neither.
- **False `native_error` claims at commit 2:** 32 unmatched lines are neither
  `errorType` nor `anyType` natively, the same count as at the base. They sit
  in parse-recovery and instantiation-expression cases:
  - `expressionWithJSDocTypeArguments` 12;
  - `manyCompilerErrorsInTheTwoFiles` 4;
  - two each in `mappedTypeProperties`, `satisfiesEmit`,
    `expandoFunctionContextualTypesNoValue`, `emptyTypeArgumentList` and
    `arrowFunctionsMissingTokens`;
  - one each in six others.

  One *matched* line is natively `anyType`: `arrowFunctionsMissingTokens:0:7`.
  Each is a parser or binder divergence that hands `checkIdentifier` a name
  upstream types differently. §2.3's `topLevelAwait.1` lines (a varied case, outside `ceiling`) are this
  commit's only new claim of this kind.
- **§32's twins answer `any` for an unresolved type-reference receiver**
  (`members.rs` `check_property_access_expression`, `indexed.rs`). They do so
  behind the old §31 import-machinery test. Natively such a receiver is
  any-flagged with an alias, so `isErrorType` holds and the access answers
  `errorType` (`checker.go:11314-11320`, `:8153`). That is a producer switch
  in main's file and in `indexed.rs`, and it is reported, not made.
