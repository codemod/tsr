# Parity round 5 (tsr-2zk)

Dispatched 2026-10-08 after round 4 merged to `main` (PR #6, `ac56208f`), at
baseline `405b55ce`: plain checker_types 8,176/9,538, plain type lines 470,766
RIGHT, diagnostics 4,394/5,502, and configured rows as reported by ADR-0047.
Rules are round 4's (`docs/parity/round4.md`, `docs/parity/box-protocol.md`).
Each box gets one file set no other box touches. Lanes avoid the files main's
local boxes edited in the 36 hours before dispatch: `members.rs` (14 commits),
`contextual.rs` (10), `flow.rs` (9), `check.rs` (6, one-line hook calls only)
and `calls.rs` (5). Changes a lane needs in those files ship as measured diffs.

| Lane | Issue | Owns |
|---|---|---|
| r5-errorsplit2 | `tsr-2zk.960` (`.944`) | intrinsic/error contract, checkIdentifier unresolved arms in `expressions.rs`, writer rewrites in `types_producer.rs`; ADR-0048 |
| r5-report | `tsr-2zk.961` (`.956`, `.918`, `.1.2`) | report-head functions in `assignreport.rs`, the `jsx_component.rs` relation-report call site |
| r5-relater3 | `tsr-2zk.962` (`.927`, `.929`, `.917`) | relation arms in `relater.rs`, `variances.rs` |
| r5-iteration | `tsr-2zk.963` (`.957`) | `iteration.rs` |
| r5-triage2322 | `tsr-2zk.964` | analysis only: `docs/parity/notes/r5-triage2322.md` |
| r5-variants2 | `tsr-2zk.965` | `crates/tsr-conformance` harness |
| r5-typeparams2 | `tsr-2zk.966` (`.901`, `.911`, `.913`) | `declared.rs` type-parameter gathering functions |
| r5-perf4 | `tsr-2zk.967` (`.943`) | `perf_links.rs`, `resolution.rs`, the `get_property_of_type_ex` hook in `members.rs` |
| r5-operators3 | `tsr-2zk.968` (`.941`) | `nullable_operand.rs`, operator files |
| r5-declemit2 | `tsr-2zk.969` | the declaration-emit diagnostic files r4-declemit owned |

Sessions: the integrator's board, `board5.tsv` (one cloud session per lane).

### r5-operators3 finished; r5-intersections dispatched (`tsr-2zk.971`)

r5-operators3 (docs only, a5d6c35) found no operator-side remainder:
- **TS18046 unknown-operand diff:** held. Its one loss comes from an inference
  gap with overloaded callees (`tsr-2zk.970`).
- **in-operand diff:** measured lossless (+1 case). The integrator lands it in
  `assignreport.rs` while r5-report owns other functions there.
- **Other causes:** filed as `.971`-`.973`.

`r5-intersections` takes `.971` and owns `intersections.rs`. Main has not
touched that file in 36 hours.

### r5-triage2322 census → issues; r5-jsx3 dispatched (`tsr-2zk.982`)

r5-triage2322 bucketed 863 TS2322/TS2345 lines in 351 plain cases; 259 of the
lines are false positives (`docs/parity/notes/r5-triage2322.md`). Its buckets
are filed as `tsr-2zk.974`–`.984` and routed to the lane that owns each file:

- `r5-report` (`assignreport.rs`): excess properties (`.974`), literal
  elaboration (`.975`), the parse-error gate (`.981`).
- `r5-relater3` (`relater.rs`): conditional arms (`.976`), generic mapped arms
  (`.977`), discriminated targets (`.978`), Unknown on decidable pairs (`.983`).
- `r5-typeparams2` (`declared.rs`): qualified alias/enum references (`.979`).
- `r5-variants2` (harness): `@noCheck` (`.984`).
- Main's calls lane (`calls.rs`, active): chooseOverload applicability (`.980`,
  the largest at 154 lines). It is not dispatched.

The new box `r5-jsx3` takes `.982` (52 lines, `jsx_component.rs`).

### r5-variants2 finished; r5-modules dispatched (`tsr-2zk.994`)

r5-variants2 (64d9890, 9dbbf73) made two harness changes:
- test directives now go through `SetOptionsFromTestConfig`'s option table;
  27 vary-by options and `noCheck` had been silently dropped;
- varied `.trace.json` files are judged per configuration.

Its insensitivity probe found that the remaining configured gap is almost all
checker work, not harness work. Its checker causes are filed as
`tsr-2zk.985`–`.993`. The new box `r5-modules` takes printed module specifiers
(`.989`, 375 lines), `import.meta` (`.990`, 197 lines) and
`checkImportAttributes` (`.986`, 59 rows).

### r5-perf4 finished; r5-loader dispatched (`tsr-2zk.995`)

r5-perf4 delivered:
- **2c088ee**, a receiver signature-kind memo for getPropertyOfTypeEx's
  fallback: −3.97% Ir on p100.
- **Five measured diffs.** Four are landed by the integrator: shared relater
  names, a TypeData borrow, the property-walk buffer, and the mapped early
  return. With 2c088ee they total −12.7% p100 Ir, measured on the full stack.
- **One diff held:** the `resolve_name` memo, which rewrites 132 call sites
  across 20 files (`tsr-2zk.996`).

Its wall attribution on generic-imports matters more than any of these. That
project's check phase is 1 ms. Program construction is 98% of Ir, and TSR's
parallel loader is slower than its own single-threaded mode (82.9 vs 73.4 ms;
tsgo 71.6 ms). The new box `r5-loader` takes the loader. Lazy JSDoc (37% of
Ir) stays with `tsr-2zk.17.1`, which is claimed on main. Main's perf lane
(`tsr-2zk.17`) is active on checker memos, so no second checker-perf box is
dispatched.

### r5-declemit2 finished; r5-modfmt dispatched (`tsr-2zk.1002`)

r5-declemit2 landed two fixes, +3 cases: TS9026 for augmentation imports and
TS4094 for class expressions written as type literals. Its remaining clusters
are filed:
- `tsr-2zk.999`, module specifiers into `node_modules` (TS2883 ×12, plus the
  same cases' `.types` lines). It goes to `r5-modules`, which is the single
  owner of module-specifier generation.
- `tsr-2zk.1000`, a declaration-emit SymbolTracker with IsSymbolAccessible.
- `tsr-2zk.1001`, checker-side TS4xxx producers.

The new box `r5-modfmt` takes the module-format grammar (`.985`, 94 configured
rows) and the readers for newly applied options (`.993`).

### r5-report finished (+19 diagnostics cases); r5-report2 dispatched

r5-report landed three fixes:
- `170b30f`, the chainArgsMatch head gate (removes 5 extra TS2741);
- `2ccf35d`, the JSX-attributes arm with no outer head (`.918`, 6 tsx cases);
- `6ce0ff4`, hasExcessProperties before the structural relation for fresh
  literals (9 cases).

Its generic-reference property-table diff (`member_completeness.rs`/`declared`
tables) is landed by the integrator and re-measured in the batch gate.
`r5-report2` keeps the same `assignreport.rs` ownership for the census buckets
`.974`, `.975` and `.981`.

### r5-loader finished; r5-classfields dispatched (`tsr-2zk.1004`)

r5-loader `ec62ee2`:
- embedded lib texts are never dependency-pool jobs;
- parallel bind only runs when no single file dominates.

Results:
- generic-imports wall vs tsgo went from 1.165 to **0.978**, and its CPU fell
  20%.
- domain-model went from 0.929 to 0.865.
- Both dumps are byte-identical.

tsgo gets nothing from parallelism on generic-imports either, so the rest of
that ratio is per-file front-end speed: lazy JSDoc (`tsr-2zk.17.1`, main) and a
pipelined bind (`tsr-2zk.1003`, an ADR-0003-level change). The new box
`r5-classfields` takes the target- and useDefineForClassFields-gated class
checks (`.987`).

### r5-relater3 finished (+22 diagnostics cases); r5-relater4 dispatched (`tsr-2zk.1008`)

r5-relater3 landed:
- comparable optional-property arms;
- the alias-variance gate;
- isValidOverrideOf for protected targets;
- inherited signatures as structural requirements;
- namespace object types related over their exports.

On the real-world projects it cleared every SearchResult and TracingNode
false positive. It refused UNIQUE_ES_SYMBOL decidability at −3 cases, because
unique-symbol identity is minted per node (`tsr-2zk.1005`). It also filed
`.1006` and `.1007`. The new box `r5-relater4` keeps `relater.rs` for the
census buckets `.976`, `.977`, `.978` and `.983`.

### r5-errorsplit2 finished; decision on the gap rewrites; r5-errorsplit3 dispatched

r5-errorsplit2 delivered:
- `8d7a436`: checkIdentifier's deterministic errorType arms and requireSymbol
  answer `native_error`, each verified against a native probe;
- ADR-0048, which supersedes ADR-0038;
- three landable diffs (empty-name, P4 alias symbols, flow TS2563), landed by
  the integrator.

**Decision.** The writer's gap→`any` rewrites are not narrowed wholesale. That
would turn 5,063 RIGHT lines in 1,028 cases into GAP. They are narrowed per
producer instead: when a producer is switched to `native_error`, its lines stop
needing the rewrite. The gradient becomes honest step by step, and each step
costs zero RIGHT lines.

Why not narrow wholesale? The "no previously passing test regresses" rule is
written against verdicts, and some of those 5,063 lines may be right for the
right reason in places nobody has audited. Wholesale narrowing would discard
those along with the falsely credited ones. Per-producer narrowing keeps the
gate meaningful.

How we would know this is wrong: the credited-gap count (`ceiling.rs`) stops
falling while producers are being switched. That would mean the rewrites are
covering lines no producer switch reaches.

`r5-errorsplit3` (`tsr-2zk.1009`) is now the single owner of the contract. It
audits `Checker::is_error`'s 201 call sites and propagates `native_error`
through access, spread and destructuring.

### r5-jsx3 finished (+9 diagnostics cases); r5-constraints2 dispatched (`tsr-2zk.1011`)

r5-jsx3 `fc25cec` ported:
- namespaced JSX names;
- a narrower hyphenated-attribute rule;
- the freshness-gated excess check;
- generic spreads.

Its largest remaining blocker is outside JSX. Alias references such as
`React.DetailedHTMLProps<…>` enumerate as empty objects, which leaves about 29
lines (`tsr-2zk.1010`). TS2875 (`.988`) is back to open. The new box
`r5-constraints2` takes TS2344 (`checkTypeArgumentConstraints`,
`constraints.rs`) and TS2403 (`identity.rs`).

### r5-intersections finished (+122 type lines); r5-index4 dispatched (`tsr-2zk.1012`)

r5-intersections `cd9cc81` handles a constraint's own `{}` for a constrained
type variable. TSR mints one `{}` per written literal where native shares one
`emptyTypeLiteralType`, so the constraint's `{}` has to be recorded. Its four
measured diffs are landed by the integrator:
- getReducedType on printed types (+94);
- union-with-intersection-origin instantiation (+13);
- the base-constraint reduction with the getStringMappingType arm (+4);
- the shared empty type literal (+3).

Alias naming of single-constituent intersections, and alias references that
enumerate as empty (`tsr-2zk.1010`), go to `r5-typeparams2` (`declared.rs`).
The new box `r5-index4` takes the index-signature and index-access reports.

### r5-iteration finished (+5 diagnostics cases); r5-tables dispatched (`tsr-2zk.1014`)

r5-iteration ported:
- checkNonNullExpression on for-of operands;
- the error node through the slow iteration protocol (TS1320/TS2490/TS2767);
- the sent-type check (TS2763–2766);
- iteration over the reduced type.

It also found that the two "known failing" workspace tests have passed since
round 4.

Its blockers are member lookups: inherited members through type-argument
bases (`tsr-2zk.1013`, `members.rs`, main's), object-literal tables with
computed names, and the destructuring parameter road. The new box `r5-tables`
takes the last two (`member_completeness.rs`, `destructure.rs`).

### r5-modfmt finished (+59 diagnostics rows); r5-sigs dispatched (`tsr-2zk.1021`)

r5-modfmt ported:
- the module-format grammar checks gated on each file's emit format
  (`module_format.rs`, `.985`);
- readers for `allowUmdGlobalAccess`, `erasableSyntaxOnly` and
  `noFallthroughCasesInSwitch` (`.993`).

Its `moduleDetection: force` diff touches the same once-per-file module
indicator as r5-modules' `import.meta` work, so it goes to `r5-modules` and the
indicator keeps a single owner. Its remainder is filed as `.1018`–`.1020`. The
new box `r5-sigs` takes `.1016`, written-annotation reuse under type-parameter
renaming, which unblocks r5-typeparams2's +148 lines, and `.1015`,
async-generator `next` inference.

### r5-tables finished; r5-decls dispatched (`tsr-2zk.1023`)

r5-tables:
- object-literal tables now admit late-bound computed names (asyncIteratorExtraParameters);
- the destructuring parameter road binds against the unwidened initializer (restElementWithNullInitializer).

That is +2 diagnostics cases and +7 type lines. The new box `r5-decls` takes
the declaration checks: TS2391, TS2300, TS2507 and TS2502. Main's decls lane
has not touched these in 36 hours.

### r5-classfields finished (+30 rows, +3 with diffs); r5-declemit3 dispatched (`tsr-2zk.1024`)

r5-classfields ported:
- checkKindsOfPropertyMemberOverrides (TS2610/2611, moved into
  `heritage_conformance.rs`);
- TS2373's scope-change rule;
- TS2818 checkReflectCollision;
- TS2301's emit-standard gate;
- TS2699's computed arm;
- TS2372/2373 through binding elements.

The integrator lands its three diffs: the TS2729 target gate, the parameter
scope walk, and the useDefineForClassFields default. The new box
`r5-declemit3` takes the declaration-emit SymbolTracker/IsSymbolAccessible
(`.1000`) and the late-bound override-modifier reports (`.1001`).

### r5-sigs finished; the merged type-parameter stack lands; r5-heritage2 dispatched (`tsr-2zk.1026`)

r5-sigs `d70453b` reuses written annotations when type parameters are renamed
for printing (typeParameterToName, nodecopy.go:292). That unblocked
r5-typeparams2's held stack: rename-reuse, identity substitution, merged
parameters and alias call sites. The stack is +170 type lines and +3
diagnostics cases with zero losses, and the integrator lands it.

Async-generator `next` (`.1015`) is blocked on members inherited through
type-argument bases (`.1013`, `members.rs`). That file is inside main's
property lane (`tsr-2zk.4`), so it is filed but not dispatched. The new box
`r5-heritage2` takes the heritage conformance checks.

### r5-report2 finished (+18 diagnostics cases); r5-bind dispatched (`tsr-2zk.1028`)

r5-report2 landed:
- excess properties over computed names;
- elaborateArrayLiteral/ObjectLiteral for union targets, spreads and numeric names;
- removal of the whole-file parse-error gate from assignreport's 9 report sites.

The calls.rs half of the gate removal is +18 cases but loses 5 until two arity
rules are ported (`tsr-2zk.1027`, main's calls lane). Performance is the other
unmet target, so the new box `r5-bind` takes the front-end wall: a pipelined
bind or cheaper AST publication, and first-touch page faults.

### r5-constraints2 finished; r5-jsdoc2 dispatched (`tsr-2zk.1029`)

r5-constraints2 landed:
- getTypeParametersForTypeAndSymbol's declaration walk;
- typeof arguments decided in checkTypeArgumentConstraints;
- object-vs-enum identity (TS2403);
- checkTypeParameter's default-vs-constraint check;
- bare type-parameter arguments related through their constraint.

The integrator lands its circular-any diff. Its remaining TS2344 clusters are
the relater's generic pairs (r5-relater4), JSDoc type nodes never visited,
generic import types and calls.rs overloads (main). The new box `r5-jsdoc2`
takes the JSDoc type-hosting roots: about 50 blocked cases across
`.16.98`/`.105`/`.106`/`.107`/`.120`/`.147`/`.163`/`.38`, none of them
claimed, with the JSDoc files quiet on main.

### r5-modules finished; r5-tuples dispatched (`tsr-2zk.1030`)

r5-modules was the round's largest lane. It delivered `import.meta`,
`checkImportAttributes`, printed module specifiers including `node_modules`
packages, the TS2883 producer, and the once-per-file module indicator with
`moduleDetection` Force folded in. Its statement-only call-site diff (15
sites, zero verdict change) is held until the end of the round, because it
touches files other lanes own.

The new box `r5-tuples` takes the tuple roots. About 428 WRONG plain type
lines mention tuple shapes; `tuples.rs` and `spreads.rs` are quiet on main.

### r5-declemit3 finished (+8 rows); r5-mapped3 dispatched (`tsr-2zk.1033`)

r5-declemit3 landed:
- a whole port of `symbolaccessibility.go` (`symbol_accessibility.rs`);
- the SymbolTracker walk over inferred types (TS4023/4025/2527);
- override-modifier checks for late-bound members (TS4113/4114).

The new box `r5-mapped3` takes the mapped and keyof roots in `mapped.rs`:
about 70 blocked cases, and 225 WRONG type lines mention `keyof`.

### r5-relater4 finished; r5-relater5 dispatched (`tsr-2zk.1035`)

r5-relater4 landed:
- typeRelatedToDiscriminatedType (no OOM on the final code);
- the conditional relation arms, with getSimplifiedConditionalType.

That is +4 diagnostics cases and +17 type lines. Its conditional producers in
`declared.rs` are filed as `tsr-2zk.1034`. The new box `r5-relater5` takes the
generic mapped arms (`.977`), the decidable Unknowns (`.983`), and one shared
isDiscriminantProperty.

### r5-decls finished (+6 cases); r5-vardecl dispatched (`tsr-2zk.1036`)

r5-decls ported:
- overload adjacency by source trivia (TS2391);
- getBaseConstructorTypeOfClass without the parse-error gate (TS2507);
- late-bound duplicate keys (TS2300).

Its this-heritage diff is landed by the integrator; the late-bind-member diff
was verified only on a filtered run and stays held. r5-typeparams2 had been
idle for hours waiting on a yes/no for its follow-ons. It was told to take
`.1034`, `.1010`, the alias naming and `.979`. Lesson: follow-on messages to a
box say "take it" explicitly. The new box `r5-vardecl` takes
checkVariableLikeDeclaration (TS2502, the remaining TS2403).

### r5-errorsplit3 finished; r5-errorsplit4 dispatched (`tsr-2zk.1038`)

r5-errorsplit3:
- audited all 205 `is_error` call sites against the pinned Go code: 110 became
  `is_gap`, 37 `is_type_any`, and 43 stay `is_error`;
- propagated `errorType` through spread, destructuring rests and element access;
- with its final-else diff (landed by the integrator), took the credited gap
  from 14,715 lines to 4,243 with zero losses.

No rewrite narrowing is free yet: every rewrite still matches some RIGHT gap
lines. The new box `r5-errorsplit4` takes the next producer, the §31 gate's 838
lines, and the held IsTypeAny arms.

### r5-bind finished; r5-binperf dispatched (`tsr-2zk.1039`)

r5-bind `e1c66eb` skips block comments by bytes: lib.dom is 66% comment text,
so generic-imports Ir fell 6.45%. Wall vs tsgo, interleaved runs:
generic-imports 1.063 → 0.974, domain-model 0.785 → 0.734.

Refused, with numbers:
- **Pipelined bind:** it hides only 3–4 ms, because lib.dom parses last.
- **Huge-page arenas:** faults fell 17–24%, but zeroing dominates, and the wall
  change's sign flipped between runs.

What remains in the front end: JSDoc is about 40% of generic-imports Ir
(main's `.17.1`; a single-pass note is filed as `.1040`), and the binder is
16%. The new box `r5-binperf` takes the binder hot paths.

### Gate hole: an OOM case read as EMPTY_RIGHT (`tsr-2zk.1041`)

r5-relater4 found that `varianceProblingAndZeroOrderIndexSignatureRelationsAlign`
OOMs (exit 137) when run alone at `99b337b`. Both full dumps still recorded it
as EMPTY_RIGHT. The verdict compares diagnostics only, so an empty-baseline case
that is pathologically slow, or that survives only on host memory headroom, is
indistinguishable from a fast one. Its fix, `25a8f62`, merges in batch S. The
harness change is tracked as `tsr-2zk.1041`: it adds per-case wall time to the
dumps and makes the gate treat over-budget cases as losses.

### r5-typeparams2 retired; r5-declared dispatched

r5-typeparams2 finished its lane. It then declined the follow-ons as
other-session instructions outside the brief it was given, and sat idle. Its
four follow-ons (`tsr-2zk.1034`, `.1010`, alias naming, `.979`, all in
`declared.rs`) now go to a fresh box, r5-declared, whose initial brief
carries them. Lesson: put follow-on work in the next box's brief. A message to
a finished box does not reliably restart it.

### r5-tuples finished; r5-harness dispatched

r5-tuples landed two pieces. `1ad41b2` ports addOptionalityEx into
normalize_variadic_tuple for +13 type lines. A measured `declared.rs` patch adds
variadic addOptionality and shouldDeferIndexType's generic-tuple arm for +17.
Neither loses a case, and both are within Ir noise. About 437 tuple-shaped
WRONG lines remain, routed to the owners of signatures.rs (.16.331),
declared.rs (`tsr-2zk.1042`, `.1043`), inference.rs, mapped.rs, members.rs
and index_signatures.rs.

The freed slot went to r5-harness, for gate robustness: `tsr-2zk.1041`
(per-case wall time and a budget check), `.46` (intermittent SIGABRT), `.37`,
and then `.1017`.

### r5-heritage2 finished; r5-modexports dispatched

r5-heritage2 landed two commits. `23c10f0` narrows the merged-declaration
declines in checkClassLikeDeclaration (+3 cases). `b3b54d3` makes interface
bases go through getBaseTypes, which adds 7 TS2430 lines and changes no
verdict. Its weak-target diff (+1 case, a TS2559 report) lands in batch T.

The inherited-this diff is held as `tsr-2zk.1045`. It gains +3 cases but loses
3 type lines, until the relater can decide the distinct generic signature pair
`set<K extends keyof this>`. Private-name binder keys are filed as
`tsr-2zk.1044`.

The freed slot went to r5-modexports, for `tsr-2zk.991` and `.992`: string-literal
export names, and the JSON/ESM synthetic default, 438 type lines between them.
Its symbols.rs part ships as a measured diff, because symbols.rs is main's file.

### r5-jsdoc2 finished; r5-jsdoc3 dispatched (`tsr-2zk.1046`)

r5-jsdoc2 delivered `2b47619`, owned reparse queries that change no verdict on
their own, plus three diffs that land in batch T:
- withJSDoc on arrow and function expressions, in the parser;
- reparsed-parameter consumers;
- the accessor annotation fallback.

Together they add +72 type lines and +1 case, lose nothing, and keep CPU at
0.989 on domain-model and 1.008 on generic-imports.

It also found that diagnostics anchored inside JSDoc comments are silently
dropped: `source_file_of_for_diagnostics` dead-ends at the parentless JSDoc root.
The faithful fix loses 36 rows until the JSDoc scope hop lands. Both the fix and
the hop go to r5-jsdoc3.

### r5-binperf finished; r5-symtab dispatched (`tsr-2zk.1047`)

r5-binperf made three performance-only changes with byte-identical dumps:
- the loader's two whole-tree walks are gated on parser source flags, as
  upstream does;
- registered tokens take a light bind path, and name_nodes became a Vec;
- the diagnostics truncate in restore is skipped when there is nothing to drop.

On generic-imports this cuts Ir by 8.1% and moves interleaved wall vs tsgo
from 0.984 to 0.901. Domain-model is neutral within noise, at about 0.76–0.78,
because half its wall is the checker.

It refused symbol-table pre-sizing. reserve_rehash is 4.7M of the 7.6M insert
Ir, but pre-sizing would change iteration order at 97 checker sites. Go's map
order is randomized, so tsgo cannot depend on it. r5-symtab's ADR decides the
representation.

### r5-index4 finished; r5-missingprop dispatched (`tsr-2zk.1048`)

r5-index4 ported the typeof-globalThis TS7017 arm in `de9d3a7` (+3 cases).
Its wide-radix-literal diff in tsr-core and printing.rs lands in batch U: a
radix literal wider than u128 takes tryParseInt's big.Int → Float64 value,
for +4 cases and +78 type lines, measured with no losses.

It refused one change with a number: top-level primitive → apparent index
infos measured +2 / −12 type lines and −1 case, and is blocked on
inferToMultipleTypes ordering. The TS2536 port waits on the relater
answering Unknown for unprovable generic keys (`tsr-2zk.1049`). The TS7053
literal-key certification is filed as `.1050`.

The freed slot went to r5-missingprop, picked from the batch-T base clusters:
30 cases whose only wrong code is TS2741, and 21 whose only wrong code is
TS2353. Also from those clusters: the gate's "gains" figure counts only
WRONG→RIGHT, not EMPTY_WRONG→EMPTY_RIGHT. That explains why batch T showed +2
cases where the boxes measured about +5.

### Batch V merged from green box heads; r5-relater6 dispatched (`tsr-2zk.1051`)

With the queue idle, batch V merges the green heads of four boxes ahead of their
final reports: r5-declared, r5-relater5, r5-mapped3 and r5-vardecl. A box pushes
only commits that pass its own gate, and the batch gate re-proves them together.

r5-relater5 finished with `fe31884`: the generic mapped relation arms
(mappedTypeRelatedTo and the generic-mapped source/target arms) and decidable
IA/keyof endings. That is +3 cases, 33 diagnostic lines and +2 type lines,
with no losses and Ir down 0.09%. It declined three pieces, each because a
measured loss names an upstream piece that is missing:
- the mapped iteration-parameter constraint (`tsr-2zk.1053`);
- IAM;
- inferFromObjectTypes' mapped arm (`.1052`).

Its successor, r5-relater6, takes IAM, Unknown for unprovable generic keys
(`.1049`, which unblocks TS2536), the as-clause mapped types, IAW, B16, and a
cached isDiscriminantProperty. The uncached version measured +0.41% Ir.

### r5-vardecl finished; r5-instexpr dispatched

r5-vardecl made three changes:
- checkVariableLikeDeclaration's and checkAccessorDeclaration's getTypeOfSymbol
  call (TS2502 on unused self-references), with three declines that wait on
  deferred resolution;
- identity's generic-mapped arms;
- TS2371 in parameter patterns.

That is +8 cases with no losses. Running the structural identity arm on
inferred operands was refused: +3/−5 cases. Its remainder outside the lane is
filed as `tsr-2zk.1054` (flow.rs) and `.1055` (relater.rs).

The freed slot went to r5-instexpr. The obvious diagnostic clusters (TS7006,
TS2339/TS18046, TS2345/TS2769) are main's active lanes (`.11`, `.4`, `.9`), so
the lane was picked from the type-line clusters instead: instantiation
expressions (`tsr-2zk.1006`; instantiationExpressionErrors alone has 66 WRONG
lines) and unique-symbol identity (`.1005`; uniqueSymbolsErrors has 34).

### r5-mapped3 finished; r5-mapped4 dispatched (`tsr-2zk.1056`)

r5-mapped3 landed `f5ea71e9`: the any arms of resolveMappedTypeMembers,
template optionality, and the non-generic mapped print split. That is +30
type lines and +2 cases, with Ir +0.05%. Its enum-keys diff touches mapped.rs
plus flow.rs's tryGetNameFromEntityNameExpression and lands in batch W, for
+12 type lines and no losses. The mapped half alone loses 2 cases, so the two
halves land together.

The declared-route diff (net +56 type lines, +1 case) waits on two print fixes.
One is keyof-origin parenthesization in intersections.rs (13 lines). The other
is node reuse for a signature's type-parameter constraint (6 lines). r5-mapped4
takes both, and also `.1053`.

Box finding: varianceProblingAndZeroOrderIndexSignatureRelationsAlign peaks at
about 13.6 GB RSS even on the base. This is the memory-headroom risk behind
`tsr-2zk.1041`.

### r5-symtab finished (ADR-0049); r5-checkperf dispatched

r5-symtab replaced the FxHashMap SymbolTable with an insertion-ordered table:
a Vec of entries, plus a boxed open-addressing index above 8 names. It is the
same size and keeps the same method names, so no checker file changed. Ir fell
0.15% on generic-imports and 0.73% on domain-model. The diagnostics dump is
identical, and 13 type lines reorder toward tsgo, 3 of them WRONG→RIGHT.

Pre-sizing was measured at about ±0.05% and refused. The rehash cost it was
meant to remove came from hashbrown's doubling, which the Vec table does not do.

The correction recorded by r5-symtab: TSR has 59 checker iteration sites over
binder tables, not 97. Only 5 of them are order-observable. Porting
getNamedMembers' sort at those sites is filed as `tsr-2zk.1057`.

The freed slot went to r5-checkperf. Domain-model's wall is about half checker,
and this round's three perf lanes were all front end. It takes `.967`, `.998`,
`.997`, `.996` and `.935`, with byte-identical outputs.

### r5-modexports finished; r5-typetriage dispatched (`tsr-2zk.1058`)

r5-modexports shipped both items as measured diffs, because symbols.rs is
main's file. Both land in batch W:
- string-literal export names in getExternalModuleMember: +175 type lines,
  +1 case;
- getTypeWithSyntheticDefaultOnly, createDefaultPropertyWrapperForModule and the
  synthetic-default import type: +305 type lines.

Neither loses anything, and Ir stays within ±0.03% noise. The remainder is
filed as `.1059` (declared.rs alias road) and `.1060` (JSON file flags, and the
export= import() decline). The spread method-form print goes to r5-typetriage.

The freed slot went to r5-typetriage. The integrator's clusters are keyed on
diagnostic codes, which fits type-print failures badly. With ~7,400 type lines
still WRONG, the next lanes need a root-cause table ranked by the number of
cases each cause alone blocks.

### r5-declared finished; r5-declared2 dispatched (`tsr-2zk.1061`)

r5-declared landed two commits:
- `08095b7`, `.1034`: inline deferred conditionals carry CONDITIONAL, with the
  distributive constraint and forConstraint (+3 cases). The flags diff's 0:71
  loss was not in flow.rs: the inline mint had no constraint.
- `e9d15e8`, `.979`: qualified alias and enum references answer a twin of the
  declared type (+1 case).

Its isEnumTypeRelatedTo diff in relater.rs lands in batch W and is gated
unfiltered there. The intersection-alias diff, which covers `.1010` and alias
naming together, measured +60 lines and +3 cases filtered but loses 1 line and
1 case. It is held as `.1061` for r5-declared2, the next single owner of
declared.rs, which also takes `.1042`, `.1043` and `.1059`.

### Type-line triage landed (r5-typetriage)

r5-typetriage clustered the 7,379 WRONG and 988 GAP type lines (1,517 cases) by
root cause. Its cases.tsv, lines.tsv and classify.py regenerate the table at
any head.

Cases solely blocked, by owner:

| Owner | Cases solely blocked |
|---|---|
| Unclaimed | 175 |
| r5-declared | 155 |
| JS-file lines | 113 |
| Symbol-chain printer (main `.39`) | 93 |
| errorType GAP | 93 |
| Module exports | 71 |
| Unaligned lines | 52 |

The unclaimed causes are now filed or claimed:
- `.1062`: function-declaration type answers error (14 cases).
- `.1063`: object-literal `this` (11 cases, main's contextual.rs).
- `.1064`: flow undefined/optionality (26 cases, main's flow.rs, needs a split
  by construct).
- `.16.65` (T_1 rename, 21 cases) and `.16.233` (escaping) go to r5-typetriage,
  which owns printing.rs.

### r5-harness finished; gate v3 (`tsr-2zk.1041` closed)

Both dumps now run every case under case_guard. It appends `ms=`/`mib=`
columns and writes PANIC, OOM or TIMEOUT marker rows; the dump exits 3 or 134.
`examples/slowcases` compares a base and a new dump. On 99b337b the case that
used to read EMPTY_RIGHT now stops as OOM.

The integrator's gate moves to v3 (`bgate_core3.sh`) at batch Y:
- **A base-RIGHT key absent from the new dump is a loss.** The old `join` dropped
  such keys silently (r5-harness §5 item 8). Checked against the current base
  itself: 0 missing.
- **slowcases runs on both dumps** once the base carries guard columns. Batch Y
  is the first guarded freeze, so it skips the check there.
- **A dump that exits non-zero stops the gate**, through `set -e`/pipefail.

Already over budget, recorded as KNOWN_SLOW:
- both varianceProbling cases;
- relationComplexityError;
- performanceComparison…GenericSignatures.

Profiles became issues: `.1065` (relationCount/TS2859), `.1066` (instantiation
depth, which goes to r5-declared2), `.1067` (template literal matching), `.1068`
(variance probing) and `.1069` (timing pass for known-divergence cases).
The freed slot went to r5-funcdecl (`.1062`, `.1067`). `.37` closed as a duplicate. `.46` got 8 MiB workers everywhere; its root
cause was not reproduced in 27 full runs.

Integrator process note: in batch W the queue skipped `bmerge r5-declared`, so `e9d15e8`
(`.979` qualified twins) did not land and only its enum diff did. The cause: I inserted
lines at the index of the command that was currently running. The runner then re-ran
that batch's gate and stepped over the inserted line. `e9d15e8` lands through r5-declared2's branch.
The queue is append-only again, and inserts go strictly after the running line.

### r5-errorsplit4 finished; r5-errorsplit5 dispatched (`tsr-2zk.1070`)

r5-errorsplit4 built a per-line native identity probe: a go -overlay on the
pinned tsgo runner that tags each `.types` line @@E (errorType) or @@A
(anyType). The probe agreed on 24,307 of 24,310 lines already matched as
native_error. With the probe, `4bf9119` switches checkIdentifier's unresolved
exit to errorType wherever native does:
- +7 lines;
- credited gap 4,554 → 4,367;
- narrowing cost 5,158 → 4,557.

Its plain-JS diff adds native's resolveErrorCall arm in calls.rs (+7 lines,
304 lines leave the gap). It lands with the two inert iteration diffs. No writer
rewrite costs zero yet, so none is narrowed (ADR-0048 decision log).
r5-errorsplit5 takes the declaration-name producers: ALIAS with no type (544
lines) and FUNCTION_SCOPED_VARIABLE (350). The default-declared cache diff
goes to r5-declared2.

**Gate hole, found and fixed in batch X.** `bgate_core.sh` printed its loss
counts but never stopped on them. Every batch up to W showed `losses=0`, so
nothing slipped through. Batch X was the first with losses: 7 type lines in
parsingDeepParenthensizedExpression. Resolving the r5-errorsplit4 merge
conflict had kept the §31 gate's JS arm as the gap. The run was killed before
its accept step, and the plain-JS patch was landed first, without its
redundant calls.rs hunk. Both gate cores now end with
`STOP: losses` when either count is non-zero.

### r5-typetriage finished; r5-align dispatched (`tsr-2zk.1071`)

r5-typetriage's fixes:
- the spread method form (getSpreadSymbol's rule; it changes no verdict alone);
- U+2028/U+2029/U+0085 escaping in `quote()` (+13 lines, 3 cases).

The T_1 renaming cause turned out to be decided in inference.rs, not
printing.rs, so it is filed for main as `.1072` and `.16.65` is unclaimed
again. The enum-member ASCII-escape diff goes to r5-declared2.

The freed slot went to r5-align, the largest cause left in unowned files: 52
cases fail only on type lines that never align with the baseline (the types
producer's walker). It also takes `.1069`, the timing pass for known-divergence
cases.

### r5-instexpr finished; r5-shapes dispatched (`tsr-2zk.1073`)

r5-instexpr made two changes:
- instantiation expressions (getInstantiationExpressionType and
  checkExpressionWithTypeArguments, with a cache keyed (node, exprType)):
  +91 type lines and +1 case; instantiationExpressionErrors is now fully RIGHT;
- unique-symbol identity keyed on the declaration symbol: +14 lines.

Its node-reuse diff (+20) and the call-site identity diff land with it. All are
zero-loss, with Ir flat. The remainder is filed as `.1074`.

The freed slot went to r5-shapes, which takes the typetriage's unowned mixed
buckets: signature-differs (26 cases solely blocked), object-members-differ
(17), partial-any (14), function-expression error (8) and others. It
sub-clusters them by producer first, then fixes.

r5-typetriage also left a held diff for the T_1 rename,
`r5-typetriage-shadow-site-anchor.diff` (+27/−2 lines, +5 cases). It anchors the
shadow test at the print site and ports the binder's computed-name rule
(nameresolver.go:216-227). It is not lossless: rename_type_parameters_for_site
renames by instantiation and re-resolves deferred conditional constraints, where
native renames only at print time. It waits on a print-only rename in
inference.rs (`.1072`, main `.9`).

### r5-jsdoc3 finished; r5-jsdoc4 dispatched (`tsr-2zk.1075`)

r5-jsdoc3 root-caused all 36 of r5-jsdoc2's walk losses: the JSDoc scope hop,
reparsed-return parents, unstamped JSDoc roots, this_container and arity. It
shipped its owned queries plus seven diffs, landed in batch AB in order:
1. contextual JS assignments;
2. node-reuse @import;
3. require alias;
4. scope hop (rebuilt; r4's version cost +0.47% Ir);
5. hosted declaration types;
6. JSDoc diagnostics;
7. @template constraint.

Together: +110 type lines and +9 diagnostics cases, with no losses. Ir is
+0.10% on domain-model and flat on generic-imports. One placement was refused
on its number: in_js_file crossing the comment cost +0.22% Ir, so the loader
stamps JSDoc roots instead. r5-jsdoc4 takes the remainder.

### r5-funcdecl: first report

`4c9d6b66` (`.1062`) ports the signature-construction producers in
signatures.rs: cloneBindingName, getTupleElementLabelFromBindingElement /
getUniqAssociatedNamesFromTupleType, the contextual yield NEXT slot, and the
§929 type-query gate. +113 type lines (67 WRONG→RIGHT, 46 GAP→RIGHT), no
losses.

`7bb98dd8` (`.1067`) uses isTypeMatchedByTemplateLiteralType in union
reduction. templateLiteralTypes1 drops from about 35 s to 2 s, the dumps are
identical, and Ir is lower. Its members.rs fixture diff lands with it, so the
tests stay green.

The contextual.rs remainder is filed as `.1076` and the binding-element key as
`.1077`. `214c9830` then fixed `.16.70`: the GROUNDED gate now exempts
type parameters declared by an enclosing declaration, for +18 type lines and
+1 case. The freed slot went to r5-modules2 (printed import specifiers, 16
cases solely blocked; `.989`, `.999`, `.1060`).

### r5-declared2 finished; r5-declared3 dispatched

r5-declared2 first carried r5-declared's `e9d15e8`, which batch W's queue skip
had dropped. Then:
- `.1061`, the intersection alias: +3 cases, +53 type lines;
- `.1042`, tuple alias naming via getTupleElementFlags: +23 type lines;
- `.1059`, the renamed-import alias road: +10 configured cases, +20 type lines;
- `.1043`: held as an inference.rs diff (fillMissingTypeArguments' errorType
  pre-fill, +17 lines). It lands with the merge.

The total is +13 cases and +96 type lines, plus 17 from the diff, with no losses
and Ir down 0.01–0.06%.

r5-declared3 is now declared.rs' single owner. It takes the instantiation-depth
bound that hangs recursiveConditionalCrash3 (`.1066`), the leftovers (`.1078`),
and two small diffs other lanes left for declared.rs.

### r5-missingprop finished; r5-unionorder dispatched (`tsr-2zk.1079`)

r5-missingprop triaged the 30 cases blocked only by TS2741 and the 21 blocked
only by TS2353, then ported:
- checkYieldExpression's yield* arm;
- checkSignatureDeclaration's generator arm;
- isRelatedToEx's nullable-union narrowing in the report path;
- contextualTypeHasPattern TS2353;
- function-type known-property certification.

That is +9 cases. Its four diffs (heritage constraints, the using reporter,
switch-case excess, JSX hyphen intersection) land in batch AD for +11 more.
All zero-loss and within perf noise.

About half the remainder is in main's calls.rs and inference.rs (`.1080`), plus
one parser item (`.1081`). The freed slot went to r5-unionorder: union member
order (15 cases solely blocked), tested first for a single systemic cause.

**Batch AC reverted.** The r5-declared2 merge lost 1 case once stacked on
batch AA, aliasInstantiationExpressionGenericIntersectionNoCrash2: its `.1061`
decline stopped firing after instantiation expressions landed. The gate
stopped it, but the merge was already on the remote. The integrator's
autopush daemon had pushed the branch tip on a timer, ahead of the gate; it
also explains the earlier pushes of un-gated merges. The daemon is stopped,
so only an accepted gate pushes now. `58ead272` reverts the batch, leaving the
source tree equal to batch AB's. r5-declared3 re-lands r5-declared2's work with
the decline re-measured.

### r5-shapes finished; r5-ts2322 dispatched (`tsr-2zk.1082`)

r5-shapes split the typetriage's six mixed buckets by producer, then fixed
three things: binding elements (getPropertyTypeForIndexType's string-index
fallback, late-bound symbol keys, the per-constituent isArrayLikeType),
checkVoidExpression's undefinedWideningType, and missing member names. That is
+9 cases and +25 type lines.

Three diffs land in batch AE for +12 more cases:
- the late-bound overload implementation exclusion (symbols.rs);
- DeclarationNameToString's (Missing) (declared.rs);
- getReturnTypeFromBody's implicit-undefined arm (signatures.rs).

Identical-member collapse is filed for main as `.1083`. The freed slot went to
r5-ts2322: the largest diagnostic cluster without an owner, 136 cases whose
only wrong code is TS2322.

### r5-align finished; r5-smallcodes dispatched (`tsr-2zk.1084`)

r5-align fixed three harness causes of unaligned type lines, each faithful to
iterateBaseline:
- JSON units are walked;
- sections are matched to units by their removeTestPathPrefixes name;
- echoed `>` code lines are skipped.

This aligned 3,758 lines (3,542 RIGHT) and flipped 28 checker_types cases with
none lost. The 49 cases still unaligned are parser divergences, filed for main
as `.1086`. It also:
- added `divergentcost`, a guarded timing pass over the 518 known-divergence
  cases that are in neither dump. It found a new hang,
  noCircularitySelfReferentialGetter2 (`.1085`);
- made scorepair report RIGHT→UNALIGNED;
- fixed any_audit's control by recording the producer's own return arm.

The freed slot went to r5-smallcodes: about 49 cases across ten small
diagnostic codes that no lane owned.

### r5-relater6 finished; r5-relater7 dispatched (`tsr-2zk.1088`)

r5-relater6 ported:
- IAM, getSimplifiedIndexedAccessType's generic-mapped arm;
- `.1049`'s relater side: generic key vs keyof, alias-image bodies and the
  template fallthrough;
- IAW, the indexed-access write constraint. Ir fell 2.9% on domain-model,
  because pairs that used to run to Unknown now end early;
- B16;
- r5-relater5's isDiscriminantProperty, with a (union, name) cache: Ir +0.03%,
  against +0.41% without the cache.

Every commit is zero-loss. The TS2536 reporter is held until a binder fix for
imports inside `declare module "x"` blocks in scripts. The as-clause item is
blocked on mapped.rs pieces (`.1089`).

r5-relater7 is now relater.rs' single owner. It takes r5-ts2322's 36 relater
cases (`.1088`), `.1055`, relationCount/TS2859 (`.1065`), the variance-probe
cost (`.1068`), and the binder fix plus the TS2536 reporter.

### r5-checkperf finished; r5-checkperf2 dispatched (`tsr-2zk.1091`)

r5-checkperf made the round's first checker-CPU gains. `ab77d39` borrows
TypeData instead of cloning it and keeps literal twins: dm Ir −0.94%.

Its stack diff lands in batch AH, all byte-identical:
- the sharedFlows lookup becomes a per-node chain, replacing a linear scan that
  cost about 300M Ir on dml;
- a minimal resolve-identifier memo at four sites (`.996`);
- a once-per-checker global-alias list;
- no format! on enum keys;
- one indexed assignment walk per container;
- a settled (owner, name) property memo.

Ir on dm falls 4.35%, on dml 10.67%. Wall vs tsgo, dml 0.738 → 0.659.
jsTyping goes from 4.88 to 3.64, still slower than native.

`.935` is not duplicated work: with four checkers, the wall is checker 0's
share, and the lever is per-file speed. `.997`, the Signature representation,
is cross-cutting and becomes a single-owner item between rounds (`.1092`).
r5-checkperf2 takes allocator pressure and the jsTyping hot spots.

### r5-mapped4 finished; r5-mapped5 dispatched (`tsr-2zk.1093`)

r5-mapped4 cleared both blockers of the semantic mapped-node route:
- intersection constituents are parenthesised by printed node precedence
  (+3 type lines);
- type-parameter constraints reuse their written node (+35).

It then ported instantiateAnonymousType's per-instance mapped parameter clone
(`.1053`, +4). The clone is cached per (P, map); without the cache one case
took 1,854 ms. It also removed a quadratic key merge. That is +42 type lines,
with no losses and every commit's Ir within 0.085%.

The relater-iteration and keyof-parens diffs land in batch AH. The refreshed
route diff (+81 lines, +1 case, no verdict loss) is held by gate v3's
slowcases check: hugeDeclarationOutputGetsTruncatedWithError goes from 172 ms
to 2,060 ms, because non-generic mapped members resolve and print eagerly.
r5-mapped5 takes native's lazy members and print truncation (an ADR), then
`.1089` and reducible indexed access.

### r5-ts2322 finished; r5-printer2 dispatched (`tsr-2zk.1094`)

r5-ts2322 triaged the 121 plain cases whose only wrong code is TS2322, then:
- ported checkReferenceAssignment's relation for destructuring assignments
  (new destructuring_assignment.rs), +9 cases;
- fixed eight check-site target causes in assignreport.rs, +10 cases.

That is +19 cases with no losses. It refused checkMappedType's key relation on
its number: 15 cases lost, 0 gained, because the relater answers NotRelated on
generic keys. Its remainder is the relater's 36 cases (`.1088`, r5-relater7),
write types (`.1095`), getFlowTypeOfDestructuring (`.1096`, main's flow.rs),
and items in main's calls/inference.

The freed slot went to r5-printer2: optional-parameter `| undefined`
(`.16.60`), names as written (`.16.125`), and the small printer families.

### r5-modules2 finished; r5-smallcodes2 dispatched (`tsr-2zk.1097`)

r5-modules2 made three changes:
- GetModuleSpecifiers' file arm, with endings, preferences and processEnding;
- the import-type resolution-mode attribute: +21 type lines;
- the JSON JavaScriptFile stamp.

Its four diffs land in batch AJ for +78 type lines and +1 case:
- the symbol-chain specifier (checker.rs);
- the export= class specifier (printing.rs);
- the import-call specifier (calls.rs);
- the import-call type of an export= module (calls.rs plus module_exports.rs).

The remainder is blocked on main's symbol-chain qualification (`.39`), a
symlink cache, and ModuleHost exposure (`.1098`). The freed slot went to
r5-smallcodes2, the second set of small diagnostic clusters.

### r5-jsdoc4 finished; r5-js dispatched (`tsr-2zk.1099`)

r5-jsdoc4 extended the JSDoc walk to casts, @satisfies, @this, @callback,
@overload and export @type, added heritage TS2344 including @augments, and
ported TS2492. That is +9 cases with no losses. The heritage arm overlapped
r5-missingprop's landed diff; the integrator merged the two in batch AF.

Its three diffs land in batch AK: @augments binding, typedef block scope, and
the CommonJS require ambient arm. Together +1 case and +10 type lines.

It refused narrowing the JS decline in check_type_reference_name on its
number: +1 / −6. That waits on JS value-reference arms in declared.rs. The
freed slot went to r5-js: the 113 cases blocked only by JS-file causes, with a
rule not to duplicate main's `.5` epic.

### r5-unionorder finished; r5-nodereuse dispatched (`tsr-2zk.1101`)

r5-unionorder corrected the brief's premise. tsgo orders union members with
CompareTypes (utilities.go:415); the type id is only the last tiebreak, so no
id or init-order change was needed. It fixed the real sort and origin bugs:
- origin entries per addNamedUnions, printed with formatUnionTypes;
- enum named unions;
- NoInfer as a substitution type;
- compareTupleTypes.

That is +15 type lines with no losses. Its printing-parentheses and
object-mapper diffs land later, for +7.

37 of the 54 union-order lines are node reuse: native re-emits the written
node. node_reuse.rs therefore moves from r5-mapped5 to a new owner,
r5-nodereuse, which takes those lines together with r5-shapes' pseudochecker
group.

### r5-declared3: batch AC re-landed zero-loss

r5-declared3's branch reverts the revert (`58ead272` and `b52e2e5`), adding
r5-declared2's work back. It then fixes the batch-AC loss at its root.
r5-declared2's decline used "the alias body does not evaluate" as a proxy for
"a `typeof Class<T>` alias reference is a member-less mint". Once
instantiation expressions landed, the proxy never fired. The decline is now
narrowed to the real missing piece.

Measured unfiltered on the current integration head:
- +15 diagnostics cases and +118 type lines, with no losses;
- slowcases clean;
- Ir −3.6% on domain-model, from `.1066`'s guard and cache;
- recursiveConditionalCrash3 finishes in 81 s, down from more than 120 s.

Filed: `.1102`, the instantiation-expression cache ignoring alias frames, and
`.1103`, the per-statement instantiation_count reset and TS2589's currentNode.

### r5-smallcodes finished; r5-spans dispatched (`tsr-2zk.1104`)

r5-smallcodes converted 21 diagnostics rows, none lost:
- TS2652;
- the TS2306 side-effect-import gate;
- the TS2686 alias value-meaning test;
- TS18060 on `import.defer`;
- TS2688 through the loader's existing diagnostic channel;
- non-literal computed names in implied binding patterns.

Its TS2880 parser diff and TS2538 index-image diff land later, for +10. It
also found that two of its commit messages quoted Ir from a stale tsr binary:
`cargo build -p tsr-conformance --examples -p tsr` does not rebuild the bin. The
numbers are corrected in its notes §4.

The freed slot went to r5-spans: 20 cases that are wrong only on position, and
TS2589's currentNode reporting (`.1103`).

Most of the unowned pool is now spent. The largest remaining clusters (TS2345,
TS2769, TS2339, TS7006, TS2304, flow and contextual typing) sit in main's
active lanes (`.4`, `.6`, `.9`, `.11`) or in main's files.

### r5-errorsplit5 finished; r5-errorsplit6 dispatched (`tsr-2zk.1106`)

r5-errorsplit5 ran the native identity probe over the whole corpus (12,157
baselines) and switched four producers, each verified line by line:
- nullable `+`;
- JSX elements (errorType) vs fragments (anyType);
- checkSuperExpression, including GetSuperContainer's static-block and
  decorator arms;
- the indexed twin.

That is +84 type lines with no losses. Its symbols.rs diff (alias not-a-value,
the getTypeOfSymbol fallthrough) and members.rs diff (the property twin) land
in batch AO.

With them, the first writer rewrite reaches zero RIGHT cost and is narrowed:
GlobalAugmentation. The ADR-0048 narrowing cost falls from 4,504 to 3,030.
r5-errorsplit6 takes indexed.rs' failed-lookup arms, the false-claim roots and
spread propagation. Main's remainder is `.1107`.

### r5-smallcodes2 finished; r5-config dispatched (`tsr-2zk.1108`)

r5-smallcodes2 fixed:
- TS2540: the constructor exemption's Property gate, and globalThis
  read-only;
- TS1156: no report inside a `with` body, plus the modifier-chain gate;
- TS2300: lateBindMember conflicts;
- TS2695: isInDiag2657, through a new ModuleHost::parse_diagnostics.

That is +10 cases with no losses and Ir flat. Its error_span missing-node diff
and its parser as-ASI diff land next, for +5 cases and +10 type lines.

The freed slot went to r5-config: configuration-dependent failures (the
weakest suite) and the harness compile root (`/.src`, `.1087`).

Note for the user: main's lazy-JSDoc lane (`.17.1`) is claimed but shows one
commit in 96 hours, while JSDoc scanning is about 40% of generic-imports' Ir.
It is the largest open perf lever and is not taken over without the owner.

### r5-js finished; r5-jsdoc5 dispatched (`tsr-2zk.1110`)

r5-js triaged the 100 cases blocked only by JS-file causes; 53 of them are
JSDoc. Every producer sat in an owned or hub file, so it shipped measured
diffs only. Three land in a later batch:
- the JS setter-parameter gate (+4 lines);
- shorthand ambient modules in import() (+48 lines, 8 cases);
- the assignment-context nil arms (+9 lines).

Its super-in-static-block diff is not landed, because r5-errorsplit5's
`abf502e` already ports GetSuperContainer's static-block arm.

The remainder in main's files (late-bound expandos, require-destructuring
aliases, inference from `any`) is `.1111`. The JSDoc bucket goes to
r5-jsdoc5.

### r5-mapped5 finished; r5-mapped6 dispatched (`tsr-2zk.1112`)

r5-mapped5 recorded ADR-0050:
- mapped property types are instantiated on first read (getTypeOfMappedSymbol);
- mapped member prints truncate as the node builder does.

hugeDeclarationOutputGetsTruncatedWithError is now 295 ms instead of 2,060 ms,
with its native truncation point matched exactly. It also ported:
- getIndexTypeForMappedType over generic key domains;
- symbol-keyed computed properties;
- mapped info for concrete mapped alias instances;
- reducible indexed access (isGenericReducibleType and the uniqueLiteral
  intrinsic).

Six diffs land in batch AR.

**Refused on its number:** the semantic mapped-node route gains +86 type lines
and +1 case but costs domain-model Ir +0.48%, from evaluating 40
`KeysOfType<…>` bodies. The performance rule rejects it as it stands.
r5-mapped6 must find native's cache or deferral for that path and land the
route within noise.

### r5-checkperf2 finished; r5-checkperf3 dispatched (`tsr-2zk.1113`)

r5-checkperf2 made two byte-identical changes:
- the pending-signature-return walk goes by reference;
- union_index_infos reuses each primitive's apparent answer.

Ir fell 0.70% on dm and 0.81% on dml. Its relater.rs enum-payload diff cuts
jsTyping's check by 7.5% of Ir.

Wall vs tsgo on its container: dm 0.76, dml 0.74, gi 0.89, jsTyping 3.6. The
0.50 target is not met. On dm and dml no checker function is above 1.5% self
any more; about 16% is the allocator, mostly Signature copies (`.1092`, single
owner between rounds).

r5-checkperf3 takes the two largest jsTyping levers. Both are in main's files,
so they ship as small measured diffs: native's per-call resolvedSignature link,
and a union property certification memo.

### r5-printer2 finished; r5-printer3 dispatched (`tsr-2zk.1114`)

r5-printer2 ported:
- the optional parameter's symbol `| undefined` (`.16.60`);
- written-annotation reuse in type-literal properties;
- the merged object-literal name spelling;
- divergent accessor pairs printed as get/set.

That is +178 type lines and about +27 cases, with no losses. Its four diffs (two
member-form arms in checker.rs, an overloaded optional method in declared.rs,
names as written `.16.125`) land later, for +76 lines and +11 cases.

Defaulted type arguments dropped in printed references (28 lines) live in
declared.rs (`.1115`). r5-printer3 takes the synthetic optional parameters,
the enum-member producer, object-literal accessor identity and contextual
signature type parameters.

### r5-declared3 finished; r5-declared4 dispatched

r5-declared3's work (the `.1066` guard and cache, and the batch AC re-land)
landed in batch AM. It declined `.1115` as new work past its session rhythm.
r5-declared4 is now declared.rs' single owner, and also owns
instantiation_expressions.rs and unique_symbols.rs. It takes `.1115`, `.1102`
and the `.1078` leftovers, and lands declared.rs diffs other lanes send it.

### r5-spans finished (batch AU); r5-smallcodes3 dispatched

r5-spans landed four diffs: the scanner's octal-minus and unterminated-comment
spans, the parser's `this`-parameter modifier span, the missing-node raw-span
sites (stacked on r5-smallcodes2's `error_span` arm), and the currentNode
tracking TS2589 needs. `tsr-2zk.1104` is closed. Three diffs stay held, each
with its blocker:

- TS2589 report sites: 8 losses, because this port's instantiation depth
  reaches 100 where upstream's does not. They wait on lazy anonymous-type
  instantiation and the conditional tail guard (`tsr-2zk.1116`, which blocks
  `tsr-2zk.1103`). That work is in main's laziness lane, `tsr-2zk.11.5`.
- The `await using` list span waits on the parser's `AwaitUsing` flag
  (`tsr-2zk.2.3`).
- The delete exact-optional arm waits on `Partial`'s members carrying their
  own symbols.

r5-smallcodes3 takes the slot, with the single-code clusters outside main's
active lanes (`tsr-2zk.1117`).

### r5-nodereuse finished (batch AV); r5-nodereuse2 dispatched

r5-nodereuse ported one reuse decision per native node-builder slot:
- the type-parameter constraint (`typeParameterToDeclaration`);
- the return (`serializeReturnTypeForSignature` with the pseudochecker's
  return answers);
- the property (`serializeTypeForDeclaration` with `GetTypeOfDeclaration`'s
  Direct arms).

Its three call-site diffs land in printer and spread files. The box measured
them together: +207 type lines across 61 cases, 0 lost; dm Ir +0.33%, gi 0.00%.

The cost is a consequence of baking text when a type is created. Native does
that work only when it prints. Printing spread members on demand is filed as
`tsr-2zk.1120`, and the identity relation cache the predicate gate wants as
`tsr-2zk.1119`. `tsr-2zk.1101` is closed.

r5-nodereuse2 continues with structural pseudo types (`tsr-2zk.1118`).

### r5-relater7 finished (batch AW); r5-relater8 dispatched

r5-relater7 landed six relater arms for `.1088` and `.1055`'s fundule arm: +9
diagnostics cases. It also landed two native perf paths:
- `relationCount` and the overflow budget (`.1065`): relationComplexityError
  went from 67 s to 2.9 s;
- the target-symbol recursion identity for written class references (`.1068`):
  varianceProbling went from 41 s to 0.4 s.

dm Ir fell 1.7%. The TS2859 reporter diff lands with it. Three diffs stay held,
each blocked on a file outside the lane:
- primitive-index waits on contextual's literal through an index signature
  (`tsr-2zk.1121`);
- enum-object waits on inference from an enum object (`tsr-2zk.1122`);
- the binder's declare-module imports wait on bounded conditional-alias
  evaluation in declared.rs (`tsr-2zk.1123`). Unbounded, the dumps OOM at
  ramdaToolsNoInfinite2.

r5-relater8 takes the remaining relater arms (`tsr-2zk.1124`) and `.1065`'s
TS2321.

### r5-config finished (batch AY); r5-isolated dispatched

r5-config fixed two pieces of plumbing:
- the harness reads `message TS` baseline headers;
- the loader ports the JSX module indicator.

It also lands three measured diffs:
- checkAliasSymbol's isolatedModules/verbatimModuleSyntax arms: +4 diagnostics
  rows;
- the `/.src` compile root together with relative `import("./x")` naming
  (`.1087`): +8 type lines. The harness half alone loses 356 lines, so the two
  halves land together;
- `GetEmitModuleKind`, verdict-neutral.

Its triage found the checker's option reads faithful apart from module kind.
The 25 option-gated rules it found are routed: the isolatedModules export and
reference arms to `tsr-2zk.1125` (r5-isolated), the exactOptionalPropertyTypes
arms to `tsr-2zk.1126`. `.1108` and `.1087` are closed.

### Round-5 wrap-up: final box reports (batch AZ)

At the user's request (2026-10-09 06:35 UTC), every box was told to push its
green work and stop. Batch AZ lands the final commits and diffs. Every claim
that `claude-cloud-r*` boxes still held was released, so main's sessions see
the work as unclaimed.

Measured and held, each with its blocker:
- r5-nodereuse's property-slot diff, backed out after batch AW's first gate:
  circularAccessorAnnotations went RIGHT -> WRONG (`tsr-2zk.1129`). It
  stays held together with r5-nodereuse2's object-literal-slot diff.
- r5-mapped6's conditional-typed-print diff (+5/-4): waits on
  getObjectTypeInstantiation's referenced-parameter keying in declared.rs.
- r5-declared4's print-arity WIP (+37/-3): waits on signature return reuse
  of `Iterator<X>`.
- r5-smallcodes3's type-as-namespace diff (+3/-1, waits on JSDoc dotted
  `@callback` names) and missing-brace-body diff (needs NodeIsMissing(body)
  readers).
- r5-relater8's variance and conditional WIP: built on r5-relater7's tip,
  never gated.
- r5-errorsplit6's import-equals-alias and script-alias-merge diffs (main's
  symbols.rs and binder.rs).

Duplicates resolved: r5-js's cast-context, return-param-host and
full-signature-generic diffs are superseded by r5-jsdoc5's, whose lane owns
those files. r5-printer2's member-constraint-reuse diff is superseded by
r5-nodereuse's constraint printers.

r5-isolated (`tsr-2zk.1125`) did not start. Its native anchors:
- checkExportAssignment: checker.go:5583-5680;
- checkAliasSymbol's export-specifier arm: :6806-6822;
- TS2866: resolveName's success path, :1869-1885;
- TS2748: checkConstEnumAccess, :7589;
- markDecoratorAliasReferenced: :28686;
- GetResolutionDiagnostic: module/util.go:123;
- TS5097: checker.go:15238.

TSR/tsgo wall time, from r5-checkperf3's container with its stack applied:

| Project | Ratio |
|---|---|
| domain-model | 0.708 |
| domain-model (large) | 0.666 |
| generic-imports | 0.817 |
| jsTyping | 3.203 |

The 0.50 target is not met.

Correction after batch AZ's first gate: r5-nodereuse2's merge and its
predicate-at-site diff were backed out (84ef78b5). With them,
`shadowed_names::constraints_and_defaults_keep_outer_parameter_names` fails:
a written constraint naming the local alias `Outer` is reused where `Outer`
is out of scope. Native prints `T_1 extends T = T`. The +26 type lines they
measured are not in this round. `tsr-2zk.1118` holds the work, with that test
as its falsifier.

## Round 6 dispatch

The goal (99.9% parity, TSR/tsgo wall ≤0.50) is not met. Round 6 resumes
from main `17265fac`, with ten boxes taking round 5's handoff:

| Box | Items |
|---|---|
| r6-relater | `.1124`, `.1065` |
| r6-nodereuse | `.1118`, `.1129` |
| r6-printer | `.1114`, `.1127`, `.1128` |
| r6-declared | `.1115`, `.1123` |
| r6-mapped | `.16.71`, `.16.100`, `.16.8`, `.16.91` |
| r6-isolated | `.1125` |
| r6-errorsplit | `.1130` |
| r6-jsdoc | `.1110`, `.1100` |
| r6-checkperf | `.1131` |
| r6-smallcodes4 | `.1132` |

Main's own claims stay untouched: `.11.5`, `.16.2`, `.17`, `.17.1`, `.22`,
`.38`, `.39`, `.4.12`, `.7.8` and `.9.7`.

### r6-checkperf finished; r6-names dispatched

- **main-park** (calls.rs, perf_links.rs) lands: getResolvedSignature's
  resolving park, read only by the call link. jsTyping check Ir is −1.5%,
  with both dumps byte-identical. Parking `resolving_signature_calls` itself
  was refused: −2 diagnostics and −18 type lines, because contextual.rs's
  readers answer `any` on that park.
- **The JSDoc-deferral diff and ADR-0051 are held.** They port withJSDoc's
  TS-file deferral: dm Ir −10.8%, gi −34.4%; TSR/tsgo gi 0.903 → 0.662,
  dm 0.789 → 0.729; dumps byte-identical. This is `tsr-2zk.17.1`, which
  main's owner has claimed, so it waits on that owner.
- **Item 3 refused:** the composite-name decline is now 431 declines and 61 M
  Ir on jsTyping, so it is not a byte-identical lever.
- **Fat LTO, measured only:** dml 0.745 → 0.692, gi 0.686 → 0.628.
- **0.50 is not met.** dm's check phase has no checker function above 2.1%
  self, and the allocator is 16%: `tsr-2zk.1092`, a between-rounds single
  owner.

r6-names takes the slot with TS2304/TS2552 (`tsr-2zk.1133`).

### r6-nodereuse finished (batch BB)

The scope fix gives each entered type parameter and object-literal member
its own visitor root, as native's reuseNode/reuseName do. It re-lands r5-nodereuse2's
structural pseudo types with `shadowed_names` green. The accessor arm
(GetTypeOfAccessor, nodebuilderimpl.go:2233) explains the batch-AW refusal
of the property slot.

Batch BB lands:
- the three owned commits;
- r6-printer's global-augmentation visibility diff
  (IsExternalModuleAugmentation, the prerequisite for r5-declared4's print arity);
- the object-literal-slot, predicate-at-site and property-slot printing diffs.

Measured by the box: +28 type lines and 0 lost, with Ir flat.

The property slot's spreads.rs half (+19 types, 0 lost) is held. It costs dm
Ir +0.12%, because spread members bake their text at creation. It lands with
`tsr-2zk.1120`, on-demand spread member text.

### r6-smallcodes4 finished (batch BC); r6-typesroots and r6-smallcodes5 dispatched

r6-smallcodes4's six diffs land in batch BC, +25 diagnostics cases:
- onFailedToResolveSymbol's Namespace arm (TS2503/TS2833);
- processPragmasIntoFields (TS1453/TS1084);
- TS1009 on `import(x,)`;
- IsInTopLevelContext (TS1262);
- IsExternalOrCommonJSModule (TS2686);
- getTypeOnlyAliasDeclarationEx's Alias && !Value test (TS1362).

Held:
- unknown-operand: +5/−4, waits on inference;
- import-type-node: +38/−8, waits on alias names through symbol_chain;
- for-of destructuring: +0, waits on a tuple's `[Symbol.iterator]` in members.rs.

Dispatched: r6-typesroots took r6-nodereuse's slot (fourteen stale `.16.x`
clusters, re-measured), and r6-smallcodes5 takes this one (`tsr-2zk.1134`).

### r6-mapped finished (batch BD); r6-accessible dispatched

Batch BD lands r6-mapped's two commits:
- a generic mapped instance prints its mapped form (`.16.91`);
- a mapped node under alias frames goes through instantiateMappedType's
  sequence arms (`.16.100`).

It also lands two diffs:
- getResolvedApparentTypeOfMappedType for a non-alias instance, plus the
  contextual rest arm;
- the readonly member image.

Measured by the box: +22 types and +1 diagnostics case, 0 lost; dm Ir +0.032%.

Refused for cost: the conditional-typed print (+5 types) costs dm Ir +0.153%,
because both branches print at mint. It waits on lazy branch text
(`tsr-2zk.1135`).

Routed: the unevaluable conditional alias reference's `return error` arm
goes to r6-declared; its +78/−21 fallthrough was measured. Cross-file alias
accessibility at print (35 lines) and unique-symbol mapped keys (6) go to
r6-accessible (`tsr-2zk.1136`).

### r6-isolated finished (batch BE); r6-modules2 dispatched

Batch BE lands r6-isolated's ports in isolated_alias.rs and five hook diffs:
- checkExportAssignment's isolated/verbatim arms;
- TS2866;
- checkConstEnumAccess (TS2475/TS2748);
- markDecoratorAliasReferenced (TS1272);
- GetResolutionDiagnostic with the ResolvedUsingTsExtension arms
  (TS6263/TS7042/TS6142/TS2846/TS5097).

Measured by the box: +21 diagnostics rows, 0 lost, types identical; Ir within
noise after each straight port's extra cost was removed.

aliasSymbolLinks.referenced is recorded in the notes but not built: its only
consumer is the JS emitter. The remaining module diagnostics go to r6-modules2
(`tsr-2zk.1137`).

### r6-printer finished (batch BF); r6-lazytext dispatched

Batch BF lands r6-printer's commits:
- the erased-alias road asks IsSymbolAccessible;
- instantiateContextualType's return mapper for mutable locations
  (arrayLiteralInference);
- Scanner::rescan_template's stale value;
- ADR-0051, lone surrogates as a two-character sentinel. The one-character
  sentinel was refused at −2, because the corpus writes "\u{10FFFF}".

It also lands three diffs:
- serialize_type_name asks the full IsSymbolAccessible (+21);
- Parser::rescan_template refreshes token_value (+14, `.1127`);
- entity-name discriminants (+0, faithful).

Measured by the box: +81 types stacked, 0 lost.

**ADR number collision:** r6-checkperf's held JSDoc-deferral diff also
numbers its ADR 0051. It must be renumbered when it lands.

r5-declared4's print-arity WIP is unblocked, but its +0.22% dm Ir is with
r6-declared to remove. r6-lazytext takes the printer lane for print-time text
(`tsr-2zk.1138`, covering `.1120` and `.1135`), which gates two held diffs
(+24 lines).

### r6-relater finished (batch BG); r6-relater2 dispatched

Batch BG lands r6-relater's commits:
- object's apparent `{}` arm;
- native variance: Unmeasurable/Unreliable, markers as type parameters,
  structural fallback, getMappedTargetWithSymbol;
- the mapped_conditionals and out-of-reach lifts;
- conditional source arms with the bare-check-reference true type;
- TS2321 per-side stack overflow (`.1065`);
- the TS2322 reporter no longer reports a pair the relation relates.

Measured by the box: +9 diagnostics cases, +29 type lines, dm Ir −0.15%.

r5-relater8's variance WIP as written lost 10 type lines and 2 diagnostics
cases, because it had never run verdictdump. Four native steps fixed it.

Held:
- the write-constraint lift: +23/−2, waiting on mapped.rs's
  getMappedTypeNameTypeKind deciding Remapping;
- the union-walk decline lift: loses correlatedUnions 181/299.

r6-relater2 continues with `tsr-2zk.1139`.

### r6-errorsplit finished (batch BH); r6-errorsplit2 dispatched

Batch BH lands r6-errorsplit's two commits:
- getPropertyNameFromType's unique-symbol arm;
- the element-access receiver widened for a write or a call.

It also lands five diffs, measured lossless together:
- O, createUnionOrIntersectionProperty's object-literal undefined arm: +19;
- L, the binder's InternalSymbolNameAssignmentDeclaration table and its late
  binding: +87 types, +1 diagnostic;
- G, getApparentType's unconstrained-instantiable unknown: +32;
- S, import-equals alias: +6 types, +2 diagnostics;
- M, script alias merge: no transitions.

The credited gap goes from 2,164 to about 2,129.

S's value half was refused at +8/−2: it prints `typeof x` where native has
`typeof a` (tsr-4jk). r6-errorsplit2 takes ADR-0048 step 8 (`tsr-2zk.1140`).

### r6-smallcodes5 finished (batch BI); r6-jsx dispatched

Batch BI lands r6-smallcodes5's nine hook diffs, measured lossless as a stack
(+16 diagnostics, +9 types):
- relater.go:988's per-class private names;
- checkQualifiedName's receiver check;
- `implements` of a non-generic alias;
- import-type constraint checks;
- argument-less generic qualified references as errorType;
- @import alias excludes;
- removal of a parse-error gate that has no upstream counterpart;
- TS18042/TS18043's JS arm;
- JS module @typedef exports.

19 cases remain, routed in r6-smallcodes5.md §4. r6-jsx takes the 55 still-WRONG
JSX/TSX diagnostics cases (`tsr-2zk.1141`).

### r6-names finished (batch BJ); r6-names2 dispatched

Batch BJ lands r6-names' commits and six hook diffs:
- value slots for `with` and JSX tags;
- unchecked regions;
- TS-only annotations in JS;
- primitive spellings;
- `typeof null`;
- the export-specifier failure tail.

Measured by the box: +17 diagnostics cases.

The parameter-scope diff (resolveName's useOuterVariableScopeInParameter:
+5 diagnostics, +38 types) is held. Its BindResult options slot adds about
1.5M Ir on dm through layout, and the stack read dm +0.19–0.25%.
r6-names2 lands it without that cost (`tsr-2zk.1142`).

### r6-accessible finished (batch BK); r6-specifiers dispatched

Batch BK lands r6-accessible's ports of IsTypeSymbolAccessible/IsSymbolAccessible
as the type printer asks them, and getPropertyNameFromType's unique-symbol key.
It also lands three hook diffs:
- the alias arm;
- a mapped object printed at its site where it has no accessible alias;
- unique-symbol mapped keys.

Measured by the box: +12 types on the batch-BG tip, Ir within noise. The
serialize-type-name diff was superseded by r6-printer's identical change.

Remaining, routed:
- the chain printer for `import("./x").T` spellings (`.39`);
- opaque `Alias<Args>` mints with no structure to fall back to (`.16.2`,
  ADR-0045 rule 4);
- an inline mapped node in an intersection alias under strictNullChecks off
  (r6-declared).

r6-specifiers takes module specifiers into node_modules: `.999`, `.1098`,
`.16.59` and `.16.77`, about 36 blocked cases.

### r6-typesroots finished (batch BL); r6-typesroots2 dispatched

Batch BL lands r6-typesroots' five query files and seven hook diffs, measured
lossless as a stack (+78 type lines across 29 cases; Ir flat):
- arity-window errorType: +31;
- late-bindable index signatures of type literals: +7;
- typeof reuse for instantiation expressions: +4;
- dedupe of a signature's merged type parameters: +17;
- the rest parameter binding pattern's implied type: +6;
- late-bound destructuring through the apparent type: +2;
- unqualified type-meaning import types: +11.

Closed: `.16.28/.34/.35/.40/.41/.43/.47/.54`. Released with their causes:
`.16.14/.16/.21/.32/.36/.46`.

The origin-slice gate (+35/−2) is held. Its losses were right by accident: a
type literal recursing through a union alias answers Unknown, and fixing that
needs lazy type-literal members.

r6-typesroots2 takes `.16.65/.69/.73/.74/.7/.6/.76/.79`.

### r6-declared finished (batch BM); r6-declared2 dispatched

Batch BM lands r6-declared's commits:
- getObjectTypeInstantiation's referenced-parameter key
  (isTypeParameterPossiblyReferenced);
- an indexed type literal resolves only the selected member. That removes
  the 54 MB `__Reverse` text with no count bound: native needs only 48,750
  instantiations and reports no TS2589;
- the import-equals written name;
- memoized conditional extends instantiations (ramdaToolsNoInfinite2 31 s →
  2.1 s once its imports resolve);
- a namespace-rooted qualified enum reference answers the enum (+2 diagnostics);
- an intersection alias's deferred conditional constituent;
- r5-declared4's print arity at Ir within noise (+37 types). Its +0.22% cost
  came from resolving the Iterable targets per print, not once per checker.

It also lands the unresolved-alias written-name diff (+21 types).

Held:
- the binder declare-module imports diff: +3 diagnostics, but it still loses
  ramdaToolsNoInfinite2 448/449/485/490–492;
- the conditional-typed print: dm Ir +0.16%, waiting on `.1135`.

r6-declared2 takes `tsr-2zk.1143`.

### r6-names2 finished (batch BN); r6-triage dispatched

Batch BN lands resolveName's useOuterVariableScopeInParameter at dm Ir
+0.03%, down from +0.15%. The options now travel as a walk parameter, not a
BindResult field; a wrapper-only variant that lost optionalParamReferencingOtherParams2
was refused.

It also lands three more diffs:
- value_reference_slot inlined (−0.255M Ir);
- TS1003 through checkModuleExportName;
- TS2303 through native's namespace export-specifier alias walk.

Measured by the box: +5 diagnostics, +38 types, 0 lost.

r6-triage re-clusters the remaining failures on the round-6 tip
(`tsr-2zk.1144`), because the `.16.x` roots date from an older base.

### r6-lazytext finished (batch BO); r6-printer4 dispatched

ADR-0052: a print-time decision is a plan the site renderer reads. Batch BO
lands two print-time slots, both at Ir within noise:
- spread members (`.1120`), which lands the held property-slot spreads half:
  +19 types, against the +0.12% dm the held diff cost;
- deferred conditional typed parts (`.1135`): +4 types, against the +0.153%
  dm the held diff cost.

Lazy mapped member text (ADR-0050 alt 1) was measured and refused: dm
−0.03%. The members are read anyway, and avoiding the string build needs an
&mut printer chain across five owners.

ADR numbering: 0051 is the lone-surrogate ADR and 0052 this one.
r6-checkperf's held JSDoc-deferral ADR must take the next free number.

r6-printer4 takes the printer lane (`tsr-2zk.1145`).

### r6-modules2 finished (batch BP); r6-modules3 dispatched

Batch BP lands r6-modules2's commits and seven diffs:
- the import-equals value half (r5-errorsplit6's refused +8/−2, now lossless by
  narrowing the export= two-hop rename): +10 types;
- TS1280/TS1281;
- the const-enum TDZ under isolatedModules;
- `export =` of a dotted entity: +2 rows, +5 types;
- rewriteRelativeImportExtensions TS2876/TS2877: +6 rows;
- grammar's NodeCanBeDecorated made the single faithful copy;
- the diagnostic formatter's nested `{0}` placeholder.

Measured by the box: +11 diagnostics rows, +15 types, 0 lost, dm Ir +0.05%,
which is within the base's own 0.06% run spread.

Refused: typing unannotated initializers for TS2475 (+1/−3). It waits on lazy
signature resolution. r6-modules3 takes the remaining module rows
(`tsr-2zk.1146`).

### r6-jsx finished (batch BQ); r6-jsx2 dispatched

Batch BQ lands r6-jsx's ports:
- chooseOverload for JSX value tags;
- getJSXFragmentType (TS2879);
- resolveCall's JSX type-argument arms (TS2558/TS2344);
- TS2608;
- getCandidateForOverloadFailure's TS2786;
- the tag-name value-reference arm (TS2304);
- the attributes resolver reading the chosen overload.

Measured by the box: about 11 cases and 78 rows to RIGHT, +5 types, 0 lost,
perf within noise.

A TS6229 port was measured and not committed: declining at the factory text
lost 30 cases. r6-jsx2 takes the remainder (`tsr-2zk.1147`).

### r6-relater2 finished (batch BR); r6-relater3 dispatched

Batch BR lands r6-relater2's ports:
- the union-walk decline lift;
- an alias written as a tuple or array relating as that type;
- the intersection-bodied alias measurement;
- distributeIndexOverObjectType in the indexed-access simplifier;
- the inline conditional root diff (`conditional_root` reads
  `conditional_inference_nodes`).

Measured by the box: +3 diagnostics cases, and +2 with the root diff after
r6-declared's 55cbd3f (landed in BM). domain-model Ir −0.07%.

The write-constraint step stays held. Its blocker is instantiateContextualType
in main's files. The single-code rows traced outside relater.rs go to
r6-relater3 (`tsr-2zk.1148`).

### r6-specifiers finished (batch BS); r6-specifiers2 dispatched

Batch BS lands r6-specifiers' work:
- the port of GetEachFileNameOfModule, getAllModulePathsWorker and
  KnownSymlinks (`d243288`);
- the symlink-cache host diff;
- the pure-alias scope diff (IsNonLocalAlias, checker.go:16266);
- the composite-slots diff.

Measured by the box: +52 type lines and +1 diagnostics case, 0 lost.
domain-model Ir +0.087% from the pure-alias scope, where native's
trySymbolTable does the same work.

The symlink-chain-gate diff is held on `tsr-2zk.39`'s alternative containers.
Without them it scores +1/−2 on diagnostics, from TS2883 on
declarationEmitReexportedSymlinkReference{,2}.

moduleResolutionWithSymlinks is not a symlink case. r6-smallcodes5 §4's
routing of it was wrong: an export-specifier alias's declared type reads
`any`. The remainder is in `tsr-2zk.1149`. The root trackers .16.59 and
.16.77 are reopened unassigned.

### r6-errorsplit2 finished (batch BT); r6-errorsplit3 dispatched

Batch BT lands r6-errorsplit2's eight diffs, in this order (N, P, Q, W, M, C,
A, B):
- getTypeOfNode's fall-through (checker.go:32035) and its in-with-statement
  exit (:31932);
- the complete-receiver property-access miss (:11353-11369);
- a private name that no class declares;
- checkMetaProperty's type half;
- getContextualTypeForAssignmentExpression's `F[xxx] = expr` arm;
- lateBindMember's merged declarations;
- the binder's JS `this[k] = v` arm.

Measured by the box against `b9ede2e`, with zero losses on both dumps:
- 555 lines go to errorType, every one errorType natively;
- +11 types and +1 diagnostics case;
- the credited gap goes from 2,131 to 1,602.

Diff C shifts domain-model Ir by +0.045% from codegen layout; its path runs 0
times there (notes §11).

The box refused the reference-receiver property miss: +391 to +430 lines
against 40 to 46 false claims. Its causes are the narrowing and augmentation
ports. The remainder is in `tsr-2zk.1150`.

### r6-typesroots2 finished (batch BU); r6-typesroots3 dispatched

Batch BU lands r6-typesroots2's new files and three diffs:
- import-type value meaning (checker.go:24575);
- the shadowed type-parameter rename: enterNewScope's render scope, the
  binder's ComputedPropertyName arm, and native's collision condition;
- the mapped contextual property read through getTypeOfMappedSymbol. This one
  is a correctness prerequisite; it is byte-identical on both dumps.

Measured by the box against `e6eadf4`: +67 types and +2 diagnostics cases,
0 lost. Ir is ×1.0007 on domain-model and ×1.0001 on generic-imports.

The conditional-node consumers diff (+35 types) is held. It loses
complicatedIndexesOfIntersectionsAreInferencable until reverse-mapped
intersection inference lands (`inference.rs`, MAIN).

Closed: .16.7, .16.79, .16.73 and .16.65 (their roots). .16.6 and .16.76 are
reopened unassigned, waiting on `tsr-2zk.39`'s naming. r6-typesroots3 takes
.16.69, .16.74 and `tsr-2zk.1151`.

### main merged into round 6 (batch BV)

Batch BV merges the 23 commits main gained since `17265fac`:
- property reads through extends expressions and `this` receivers (.16.88, .4);
- contextual-type arms (.16.95, .16.288, .31);
- the shared-contract alias carriers (.16.56, .16.57, .47.5);
- parser recovery spans (.2);
- the box setup's detached bd bootstrap (.50).

The gate measures the merge against the batch-BU tip, like any box batch.

### Batch BV: main merge, plus the IsImportCall fix the gate required

BV's first gate lost 6 diagnostics cases, all variants of dynamicImportDefer.
Each one gained a TS7006 on the `ns =>` callback of
`import.defer("./a.js").then(...)`. Main's eebc8123 now reports TS7006 when
getContextualSignature is nil.

That exposed a pre-existing gap. The port typed only `import(…)` through
checkImportCallExpression; native's `ast.IsImportCall` also admits the
`import.defer(…)` MetaProperty callee. The port instead checked
`import.defer` as an errorType meta property, so the `then` callback had no
contextual signature.

calls.rs now admits both forms, as IsImportCall does. That is the
integrator's fix for a merge interaction: it ports the native predicate
and suppresses nothing.

### r6-triage finished (batch BW); r6-callreport dispatched

r6-triage dumped both suites unfiltered on `e6eadf4`:
- types: 6,082 non-RIGHT lines in 1,174 cases;
- diagnostics: 1,020 non-RIGHT cases.

It keyed every line to the native operation whose port converts it: 759
clusters, with 2 lines unclassified. Its §2 ranks the 198 clusters that
unlock three or more cases alone. The integrator filed its 102 proposed
entries as `tsr-2zk.1152` through `tsr-2zk.1253`, prioritised by cases
unlocked (P1 ≥15, P2 ≥5).

65 of the 102 clusters sit in MAIN-reserved files. Those are worked as
measured diffs that the integrator lands, as before.

Batch BW lands r6-triage's checkImportAttributes port (checker.go:5413/5361)
and its hookup diff: +5 diagnostics cases, 0 lost, Ir −0.008% on
domain-model.

r6-callreport takes the calls.rs reporting group, 106 cases:
- `tsr-2zk.1153`, `.1158`, `.1163`, `.1169`, `.1172`.

### r6-jsdoc finished (batch BX); r6-jsdoc2 dispatched

Batch BX lands r6-jsdoc's owned files (jsdoc_overloads.rs, jsdoc_checks.rs)
and its fifteen diffs in the box's §12 order. That order includes
r5-smallcodes3's type-as-namespace diff, which now measures +3/−0.

Measured by the box against `b18aec06`:
- types +84 lines and +14 cases; diagnostics +14 cases;
- 0 lost on both dumps;
- Ir within layout noise.

The diffs land on a base nine batches newer than the box's freeze, so the
gate's numbers are the authoritative ones for this batch.

r6-jsdoc2 takes the JSDoc remainder (`tsr-2zk.1254`), r6-triage's
JSDOC-TAG-SEMANTICS (`tsr-2zk.1178`) and JS-FILE-CHECK-DECLINE
(`tsr-2zk.1165`). The calls.rs/call_arity.rs JS gates in `.1165` go to
r6-callreport.
