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
