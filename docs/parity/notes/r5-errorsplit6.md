# Lane notes: r5-errorsplit6 (tsr-2zk.1038, continuing r5-errorsplit5)

Single owner of the intrinsic/error contract, step 6. Step 5 is
[r5-errorsplit5](r5-errorsplit5.md). The decision record is
[ADR-0048](../../adr/0048-errortype-is-a-producer-identity-and-the-writer-keeps-its-rewrites.md)
and its decision log: switch a producer only where the probe confirms it line
by line, and narrow a rewrite only where that costs zero RIGHT lines. Pinned
upstream: `vendor/typescript-go` @ `5b1047d`.

## §1 Baseline and instrument

**Base.** Integration batch AO (r5-errorsplit5's held diffs A, `symbols.rs`,
and B, `members.rs`) had not reached the integration branch when this session
started. The base is therefore the integration branch at `fafecee` with both
diffs applied exactly as shipped (`git apply`), which is what batch AO lands.
Frozen there, unfiltered:

- types 548,909 RIGHT / 881 GAP / 6,501 WRONG (556,291 aligned lines);
- diagnostics 5,440 RIGHT / 5,595 EMPTY_RIGHT / 1,151 WRONG / 52 EMPTY_WRONG.

`ceiling` at the base:

- credited gap **2,577** (60 attributed);
- `native_error` lines 30,803 (30,697 matched);
- wholesale narrowing would cost **3,032** RIGHT lines (`HadErrorBaseline`
  2,355, `AtLocation` 586, `AccessOrQualifiedParent` 62, `StatementName` 29;
  `GlobalAugmentation` narrowed by diff A).

r5-errorsplit5 §9 measured 2,575 and 3,030 on its own base. The integration
branch moved two lines between the two bases.

**The probe** is r5-errorsplit4 §2.1's instrument, rebuilt here:

- the pinned tsgo toolchain from `scripts/offline-cargo/build-tsgo.sh`;
- the compiler test runner built with `go test -c -overlay`, replacing
  `type_symbol_baseline.go` (tag ` @@E` when the line's type is
  `GetErrorType()`, ` @@A` when it is `GetAnyType()`; write the `.types` text
  to `$TSR_ERRPROBE` instead of diffing, skip `.symbols`) and
  `compiler_runner.go` (only `verifyTypesAndSymbols` under the variable);
- run over the whole submodule corpus: 12,157 baselines in about two minutes.

The port side is a measurement-only copy of `ceiling` (never committed) that
adds the rewrite, the producer and the expected text to each `LINE` row. A
join on `(case, file, position)` checks that the probe's line text equals the
expected baseline text; a row whose text differs is reported as unaligned.

**Control.** Of the base's 30,697 matched `native_error` lines, 30,690 are
`errorType` natively. Of the other 7, 4 are unaligned (parse recovery, as in
r5-errorsplit4 §2.1), 2 are untagged and 1 is `anyType`. The instrument
agrees with every switch the earlier steps verified.

Each producer switch below was first made behind a temporary environment
switch (measurement only, never committed). The lines it moved were joined
with the probe.

## §2 Item 1: `element_access_lookup`'s failed-lookup arms

`getPropertyTypeForIndexType` (`checker.go:27001`) fails in several places.
Each failure returns `nil` after its diagnostic, and
`checkElementAccessExpression` turns the `nil` into `errorType`
(`checker.go:8176`). Its two `anyType` exits are the JS-literal arm (already
ported as SS185) and the `IsTypeAny(indexType)` tail. The tail is only
reachable for an index that is not a property-key type, so no `any` index
reaches it. The port spelled every failure as the gap, except two arms that
answered the `any` stand-in.

### §2.1 The arms, probed one at a time

Each arm was switched alone against the base:

| arm | port site | lines moved | native `errorType` | false claims |
|---|---|---:|---:|---:|
| `C`: const enum read through a non-literal (TS2476, `checker.go:8157`) | `element_access_lookup`, was `any` | 8 | 8 | 0 |
| `U`: union key, one constituent misses (`checker.go:26975`) | the union loop | 16 | 16 | 0 |
| `S`: generic receiver writing through a non-numeric index signature (TS2862/TS2536, `:27094`) | three sites | 2 | 2 | 0 |
| `F0`: key names no property, no index signature applies | no-name tail | 253 | 210 | 43 |
| `F1`: named key misses property and index signature | named tail | 267 | 217 | 49 + 1 `anyType` |
| `J0`: the JS-literal arm on the no-name road (`:27129`) | (absent) | 0 | — | — |

`J0` moves nothing in the corpus, so it is not added.

### §2.2 The tails: the miss is upstream's only where the port's lookup is complete

`F0` and `F1` are the same `nil`. Their false claims are lines where the
port's lookup missed a member that upstream finds. To attribute them, a
temporary thread-local side channel (measurement only) recorded a receiver
category per miss and surfaced it in the dump. Running all arms together:

Counts are the access lines themselves. Lines downstream of an access (a
variable it initializes, an enclosing call) carry no category and follow
their access.

| receiver | `errorType` | false claims | the port's missing piece |
|---|---:|---:|---|
| generic index or receiver | 15 | 23 + 1 `anyType` | Upstream defers `T[K]` before any lookup (`getIndexedAccessTypeOrUndefined`). The port defers only a key it can admit (`mappedTypes4`, `isomorphicMappedTypeInference`, `correlatedUnions`, unconstrained `x['toString']`, and `classExtendingAny`'s `this['wot']`, the `anyType` line) |
| `unique symbol` index | 5 | 8 | `getPropertyNameFromType`'s third arm, this module's documented gap (`uniqueSymbols`, `declarationEmitLateBoundAssignments`' `foo[_private]`) |
| mapped receiver | 0 | 2 | Enum-keyed mapped members (`numericEnumMappedType`) |
| a function's type | 7 | 24 | Late-bound assignment members. `foo[strMem] = …` binds into `InternalSymbolNameAssignmentDeclaration` (`binder.go:1002`), and `getResolvedMembersOrExportsOfSymbol` reads it. The port's binder has no such table (`declarationEmitLateBoundAssignments{,2,JS}`, `expandoFunctionExpressionsWithDynamicNames`) |
| union with an object-literal constituent | 1 | 4 | `createUnionOrIntersectionProperty`'s object-literal `undefined` arm. `(options \|\| {}).a` misses on the property-access road too (`members.rs`, main's) |
| named reference to an unexpanded alias | 0 | 2 | `constr<{}, …>` is not expanded (`intersectionWithIndexSignatures`) |
| `typeof globalThis` | 7 | 1 | The element-access road had no globalThis members arm. **Ported** (§2.4) |
| everything else: interfaces, classes, literals, primitives, enums, plain unions, intersections | 197 | 0 | — |

So a miss answers `native_error` only when
`element_access_receiver_is_complete` holds: no generic index or receiver, no
`unique symbol` index, no mapped receiver, no union with an object-literal
constituent, no function's type, and no named reference without a usable
members table. Otherwise it stays the gap. Every exclusion keeps lines in the
status quo, and each one names the unported upstream piece. Lifting an
exclusion is the falsifier: port the piece, re-probe, and the exclusion should
then move only `errorType` lines.

A function receiver is excluded whole. Natively `(() => {})[0]` is
`errorType`, but a function declared by name may carry late-bound members the
port never binds, and the receiver type cannot tell the two apart. Telling
them apart would mean re-deriving the binder's assignment-declaration table
in the checker, which is a side pass (box protocol §3a). The cost is 7 true
lines that stay the gap.

### §2.3 Readonly writes: `isAssignmentToReadonlyEntity`

`check_element_access_expression` answers three readonly-write shapes before
the lookup. All three used the `any` stand-in. The element-access road's exit
is `getPropertyTypeForIndexType`'s readonly arm: TS2540, then `nil`, so
`errorType` (`checker.go:27036`).

| arm | lines moved | native `errorType` | other |
|---|---:|---:|---:|
| `R2`: a readonly property | 7 | 7 | 0 |
| `R3`: a namespace import's member (a readonly entity) | 12 | 12 | 0 |
| `R1`: a readonly tuple, any index | 5 | 2 | 3 |

`R1`'s three are `v[2]`, `v[0 + 1]` and `v[0 + 2]` on a `readonly [number,
number]` (`readonlyArraysAndTuples`). A non-literal or out-of-range index is
not a tuple property. It takes the index-signature road, where
`errorIfWritingToReadonlyIndex` reports and the element type is still
answered. So `R1` switches only when the index names a tuple property
(`createTupleTargetType`, `checker.go:24775`: `length`, or an element before
the first variable one). The other half keeps its stand-in; it was already
WRONG and is not this item's.

### §2.4 `globalThis['x']`

The element-access road reached `typeof globalThis` through
`get_type_of_property_of_type`, which has no globalThis arm. So
`globalThis['x']` for a `var x` missed, and with the switch it claimed
`errorType` for a line that natively is `number`. `getPropertyOfType` over the
globalThis anonymous type reads the globals that are not block-scoped
(`resolveAnonymousTypeMembers`). That is the table `crate::members`'
property-access arm already reads. `global_this_property_type` ports it on
the element-access road: +1 WRONG→RIGHT, and the miss for `globalThis['y']`
(a `const`) is `errorType` as probed.

### §2.5 A definite write keeps the miss

With the union arm returning `native_error`, `cannotIndexGenericWritingError`
lost two RIGHT lines on the first full run. `check_element_access_expression`'s
definite-write branch re-derived a write type (`T[string] & T[symbol]`) over
the failed access. Natively a definite target takes the access type as is
(`getFlowTypeOfAccessExpression`), so a `native_error` access now returns
before that branch. Both lines are RIGHT again.

### §2.6 A cycle in the base graph

`tests/types.rs`' `a_cycle_in_the_base_graph_terminates` caught one more
false claim that the corpus does not hold. `interface A extends B { [k:
string]: number }` with `interface B extends A {}`: upstream reports the
cycle, empties the bases, and still reads `A`'s own signature, so `a["k"]` is
`number`. The port's index-info walk answers `None` for the cycle, so the
lookup missed and the switch claimed `errorType`. `None` from
`get_index_infos_of_type` means the port could not decide the receiver's
signatures, so it joins the exclusions. It moves no corpus line (the moved
set is identical with and without it).

### §2.7 Commit 1, measured

The full moved set, joined with the probe: **396 lines move to
`native_error`, and all 396 are `errorType` natively**. Of those, 367 were the
gap and 29 were the `any` stand-in (`C` 8, `R2` 7, `R3` 12, `R1` 2). No line
moves anywhere else, apart from the two gains below that leave the error
identities altogether.

Measured unfiltered against §1's base, both dumps:

| | base | commit 1 |
|---|---:|---:|
| types RIGHT / GAP / WRONG | 548,909 / 881 / 6,501 | **548,914 / 881 / 6,496** |
| diagnostics | 5,440 / 5,595 / 1,151 / 52 | unchanged, zero transitions |
| type / diagnostics losses | — | **0 / 0** |
| credited gap | 2,577 | **2,210** (−367) |
| `native_error` lines (matched) | 30,803 (30,697) | 31,199 (31,093) |
| wholesale narrowing RIGHT→GAP | 3,032 | **2,720** (−312) |

**Gains (+5 WRONG→RIGHT):**

- `globalThisBlockscopedProperties:0:15`, `globalThis['x'] : number` (§2.4);
- `objectLitArrayDeclNoNew:0:10`: `Gar[]` is now `errorType`, so the
  object literal around it computes `{ tokens: any; endState: IState; }`;
- `privateNameComputedPropertyName3` ×3 targets:
  `new Foo("NAME").getValue(100) : error`. The case has no `.errors.txt`, so
  the writer prints upstream's `errorType` as `error`, and the port now
  answers that identity.

slowcases is clean on both dumps. Perf, median child CPU over 21 samples
against the base binary: domain-model 0.951, generic-imports 0.894
(diagnostics match). An earlier build of the same arms read 1.013 / 0.999.
The added work is one predicate on a failed lookup, which is rare.

**Tests.**

- `tests/element_access_miss.rs` (new) pins: a missing named member, a union
  key with a miss, a const-enum non-literal read, readonly field and
  namespace-member writes, and the readonly tuple split, all as
  `native_error`; a present member as its type; and the two gap exclusions
  (a function's type, a union with an object literal).
- `tests/types.rs`: four assertions pinned the miss as the gap (`error`) and
  now pin upstream's `errorType` (printed `any` by the checker). The cycle
  test keeps the gap (§2.6).
- `tsr-conformance/tests/enum_literals.rs`: the static-members case reports
  TS2475/TS2476 natively, so its writer runs with `hadErrorBaseline`. The
  helper now sets that flag for this case. `Constant[0]` and `Constant[key]`
  are `errorType` and print `any` under it, as the native outcome the test
  records.

## §3 Narrowing, re-measured

ADR-0048's decision log narrows a rewrite only at zero RIGHT cost. The cost
of each rewrite, as RIGHT→GAP lines:

| rewrite | base | commit 1 |
|---|---:|---:|
| `HadErrorBaseline` | 2,355 | 2,053 |
| `AtLocation` | 586 | 576 |
| `AccessOrQualifiedParent` | 62 | 62 |
| `StatementName` | 29 | 29 |
| **total** | 3,032 | 2,720 |
| credited gap | 2,577 | 2,210 |

No rewrite reaches zero, so none is narrowed.

## §4 Item 2a: the JSX namespace through an implicit import or a global re-export

r5-errorsplit5 §4 listed two false claims, `<div></div>` in
`jsxNamespaceGlobalReexport` and `jsxNamespaceImplicitImportJSXNamespace`.
Natively both are `JSX.Element` (printed `JSX.Element` and
`import("preact").JSXInternal.Element`); the port answered `errorType`
because `jsx_type_symbol` found no `Element`. Both runtimes reach `JSX`
through an import-equals of an *imported* type-only namespace:

```ts
import { JSXInternal } from '..';
export import JSX = JSXInternal;                       // implicit import
declare global { export import JSX = JSXInternal; }    // global re-export
```

Two roots:

1. **`resolve_alias`'s import-equals identifier arm (`symbols.rs`, main's).**
   `getSymbolOfPartOfRightHandSideOfImportEquals` resolves `|b|` in
   `import a = b` with `Namespace` meaning (`checker.go:14486`), and
   `getSymbol` accepts an alias whose *target* carries the meaning
   (`getSymbolFlags`). The port accepted a found alias only when its value
   type was a module clone. `JSXInternal` has no value, so `JSX` resolved to
   nothing. Shipped as a diff (§4.1).
2. **`jsx_type_symbol`'s global fallback (`jsx_intrinsic.rs`, unowned).**
   Native's fallback is `c.resolveSymbol(getGlobalSymbol(JSX, Namespace))`
   (`jsx.go:1334`); the port read `exports` off the global `JSX` symbol
   itself, which for `declare global { export import JSX = … }` is the
   alias. Ported in commit 2: an alias global is resolved before its exports
   are read.

Commit 2 alone moves no line (measured: zero transitions on both dumps
against commit 1). The alias it now resolves is the one root 1 leaves
unresolved, so its effect appears only with the diff.

### §4.1 Diff S (`symbols.rs`, main's): an alias whose target is a type-only namespace

[`r5-errorsplit6-import-equals-alias.diff`](r5-errorsplit6-import-equals-alias.diff)
applies on commit 2. After the module-clone arm, the identifier arm returns
the found alias when `getSymbolFlags` reaches a namespace that has **no value
meaning**.

The value half is held back. With it (any namespace target), the first
measurement gained 8 lines (`chainedImportAlias` ×7,
`aliasInaccessibleModule2`) but lost 2 RIGHT lines in
`es6ImportNamedImportInIndirectExportAssignment`: `import x = a` over an
imported value namespace resolved, and `x` then printed `typeof x` where
upstream records `typeof a`. That is the alias-naming hazard
`resolve_alias`'s other declines record (`bd tsr-4jk`). Moving the arm after
the clone arm did not change it. The type-only half has no value to name and
is what the JSX roots need.

**Measured** on commit 2 (unfiltered, both dumps, against §1's base):

- zero losses;
- types **548,920 / 879 / 6,492** (+6, below);
- diagnostics 5,440 / **5,596 / 1,151 / 51**:
  `jsxNamespaceImplicitImportJSXNamespace` EMPTY_WRONG→EMPTY_RIGHT;
- credited gap 2,210 → **2,208**. The 2 lines that move to `native_error` are
  both `errorType` natively;
- `StatementName` 29 → **27**.

The six type lines are `Comp`, `() => <div></div>` and `<div></div>` in each
case: 2 WRONG→RIGHT and 1 GAP→RIGHT per case. The two former false claims
(`<div></div>` claiming `errorType`) now compute `JSX.Element` and
`import("preact").JSXInternal.Element`.

The diff carries its own test, `tests/import_equals_type_only_namespace.rs`.
`import X = Y` with `import Y = N` and `N` type-only resolves `X.I`. It fails
without the diff, and the one-hop control passes either way.

## §5 Item 2b and 2c: JS `require` receivers, and the script alias merge

### §5.1 JS `require` receivers (r5-js's area)

On this base, r5-errorsplit5 §7's JS receivers reduce to two cases:

- `commonJSImportExportedClassExpression`: `const { K } = require("./mod1");
  /** @param {K} k */`, then `k.values()`;
- `varRequireFromTypescript`: `var ex = require('./ex'); /** @param
  {ex.Greatest} greatest */`, then `greatest.day`.

`varRequireFromJavascript` is RIGHT. The root is JSDoc type-reference
resolution through a CommonJS `require` binding, destructured or
namespace-like. It falls to the unresolved mint, and diff B's property twin
then claims `errorType`. That is JSDoc/CommonJS code, r5-js's. It was sent
to r5-js (`session_01GnG6zfXfbbkM9Lx93oc1ZS`) with both cases; nothing here
changes it.

### §5.2 Diff M (`binder.rs`, main's): a script alias merges into an earlier non-alias global

`duplicateVarsAcrossFileBoundaries` declares, in two script files:

```ts
// _4.ts                    // _5.ts
namespace P { }             namespace Q { }
import p = P;               import q = Q;
var q;                      var p;
```

The probe tags `p` (4:0) and `Q` (5:1) `errorType`. `P` (4:1) and `q` (5:0) are
untagged. They print `any` because they are the variable's type. With diff A,
the port claimed `errorType` on all four.

Upstream merges globals in `mergeSymbol` (`checker.go:14146`). A
non-transient target is first resolved (`resolveSymbol`), the excludes test
is repeated against the resolution, and the source merges into a *clone* of
the resolved target (`:14153-14159`):

- `_5`'s alias `q` into `_4`'s `var q`: the target is no alias, so it
  resolves to itself. The merge succeeds and `q` is `Variable | Alias`.
  `getTypeOfSymbol` tests `Variable` before `Alias`, so `q` is the variable's
  type.
- `_5`'s `var p` into `_4`'s alias `p`: the target resolves to `P`, and the
  `var` merges into a clone of `P`. `cloneSymbol` records `P` as merged into
  the clone, so the reference `P` now reads the variable. The alias `p`
  itself stays unmerged, an alias of a non-value: `errorType`.

The port's binder declines every merge in which either side is an alias,
because it cannot resolve aliases (`bd tsr-y4u.12`). The first shape needs
no resolution: the target is its own. The diff
[`r5-errorsplit6-script-alias-merge.diff`](r5-errorsplit6-script-alias-merge.diff)
lets a non-alias target merge an alias source. An alias target still
declines, and its error arm stays with the checker. The second shape needs
the checker-side resolve-and-clone, and stays a false claim (`P`, 4:1).

**Measured** on commit 2 (unfiltered, both dumps): zero transitions. Exactly
one line moves, `duplicateVarsAcrossFileBoundaries:5:0` (`q`), from
`native_error` to the variable's type, as the probe has it. Credited gap and
narrowing are unchanged, because the line was a matched `native_error`, not
the gap. The diff carries a test,
`tsr-conformance/tests/script_alias_merge.rs`, which fails without it.

## §6 Item 3: object spreads (`spreads.rs`)

`checkObjectLiteral` (`checker.go:13283-13336`) has three identity arms:

- a spread source that is not a valid spread type: TS2698 and `spread =
  errorType` (`:13304`). Later spreads `continue` while the spread is
  `isErrorType` (`:13299`), and the literal answers `errorType` (`:13336`);
- `getSpreadType`'s cross-product refusal, `checkCrossProductUnion`:
  TS2590 and `errorType` (`:13407`);
- an any-flagged source, including upstream's `errorType`: `getSpreadType`
  answers `anyType` (`:13388`). The port already did this, since
  `native_error` carries `ANY`.

The port answered the gap for the first two. Commit 3 answers `native_error`
for both. A failed spread stops the fold, so it is not turned into `any` by
the next `getSpreadType`. A gap source (`intrinsics.error`) still answers the
gap.

**Measured** on commit 2 (unfiltered, both dumps): zero transitions. **46
lines** move from the gap to `native_error`, and all 46 are `errorType`
natively (no false claims). Credited gap 2,210 → **2,164**.
`HadErrorBaseline` 2,053 → **2,013**.

Tests: `tests/errortype_spreads.rs` pins `{ ...1 }`, and `{ ...1, ...a, y: 2 }`
as `native_error`, a valid spread as its type, and `{ ...missing }` as `any`.

## §7 Narrowing, re-measured after each switch

| rewrite | base | commit 1 | commit 2 | + diff S | commit 3 |
|---|---:|---:|---:|---:|---:|
| `HadErrorBaseline` | 2,355 | 2,053 | 2,053 | 2,053 | 2,013 |
| `AtLocation` | 586 | 576 | 576 | 576 | 576 |
| `AccessOrQualifiedParent` | 62 | 62 | 62 | 62 | 62 |
| `StatementName` | 29 | 29 | 29 | 27 | 29 |
| **total** | 3,032 | 2,720 | 2,720 | 2,718 | 2,680 |
| credited gap | 2,577 | 2,210 | 2,210 | 2,208 | 2,164 |

Commit 3 and diff S were each measured on commit 2; together they read
2,678 / 2,162. Diff M changes no row. No rewrite reaches zero, so none is
narrowed. The falsifier ("the residual stops falling") has not fired: the
residual fell 3,032 → 2,680 in this step (2,678 with diff S).
