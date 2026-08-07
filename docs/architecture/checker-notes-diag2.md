# The check traversal, and the first diagnostic that comes out of it

`bd tsr-o9tl`. Successor to `checker-notes-diag.md`, which measured ADR-0040's
falsifiers and built nothing. This page is where the traversal gets built.

Upstream is pinned at `5b1047d10`; every `checker.go:` line below was
`grep -n`-verified against that commit.

---

## 1. Why this is a workstream and not a row

`diagnostics` has read **80/5,488 (1.46%)** across every session, unmoved by a
`checker_types` gradient that went 36% → 73.65%.
[ADR-0040](../adr/0040-diagnostics-come-from-a-check-traversal-and-assignability-gets-a-reporting-twin.md)
diagnosed it correctly and structurally: this port built upstream's *query* road
(`getTypeOfNode`) and none of its *reporting* road (`checkSourceFile` →
`checkSourceElement`). A diagnostic is an eager side effect of a walk; no answer
on the query road can produce one.

That diagnosis has been quoted for two sessions as *"diagnostics is structurally
blocked"*, and the handoff repeats it. **It is a statement about why the number
is flat, not a sizing of the work**, and nobody had measured what the blocked
cases are blocked *on*. `STATUS.md`'s fourth rule cuts both ways: a population is
a ceiling, and so is a blocker.

### The measurement, `examples/diaggap.rs` at `7299a14`

New instrument. Over the suite's own 5,488 judged cases (its skips copied, not
re-derived), it runs today's comparison and splits the difference sets:

```
judged 5488  passing 80
blocked by an EXTRA diagnostic alone:   19
blocked by MISSING alone:             4906
blocked by both:                       483
```

and then the column that can be built against — **cases missing exactly one
distinct code and reporting nothing extra**, which is the only bucket where one
rule converts a case on its own:

| code | converts alone | ceiling (cases containing it) |
|---|---:|---:|
| TS2322 | 476 | 904 |
| TS2454 | 255 | 403 |
| TS2304 | 191 | 537 |
| TS2564 | 166 | 538 |
| TS2339 | 132 | 333 |
| TS2345 | 96 | 249 |
| TS6133 | 77 | 98 |
| **TS2307** | **50** | **75** |
| TS2741 | 36 | 129 |
| TS2353 | 34 | 79 |

**3,258 of the 5,488 judged cases are blocked on exactly one code**, spread over
469 codes. That is 59% of the suite sitting behind single rules, which is a very
different shape from "structurally blocked" — the structure was the traversal,
and once it exists the suite is a long tail of independently-sizeable rules
rather than one wall.

The ceiling column is printed second on purpose. Ranking by "cases containing the
code" would put TS2322 at 904 and reproduce the *population identified by the
shape of the answer* failure `docs/conventions.md` records — almost every case
containing TS2322 also wants a second code nobody can emit.

### Why TS2307 first, and not the four rows above it

Not because it is the biggest. Because it is the one whose **machinery is already
at 100%** on its own suites — `module_resolution` 95/95, `file_loader` 96/96 —
so the first commit through a brand-new emission path is the one where a wrong
answer is least likely to be the emitter's fault. TS2454 and TS2564 need
definite-assignment analysis, TS2304 needs a full identifier walk, and each is a
new *analysis* on top of a new *road*. Two unknowns at once is how a build
becomes unattributable.

The order the board should take afterwards is the table above, and the reason to
say so here is that this page's method — `diaggap.rs` re-run, single-code column,
counterfactual, bar — transfers to every row of it unchanged.

---

## 2. What was built

### The road: `Checker::check_source_file` (`crates/tsr-checker/src/check.rs`)

ADR-0040 decisions (1) and (2), the two its own falsifiers left standing:

1. diagnostics are produced **inside** `tsr-checker`, appended to a collection on
   the `Checker`, and *drained* by the consumer;
2. `check_source_file` is a **second entry point**, not a hook on the query path.

It walks statements, recurses into module bodies, and today visits only the
declarations carrying a module specifier. That is far short of upstream's
`checkSourceElement`, deliberately: **every node kind the walk learns to visit is
a new opportunity to report something upstream does not**, and under the suite's
exact-multiset rule an invented diagnostic fails a case exactly as a missing one
does *and* can break a case that passes. The walk grows one sized rule at a time.

The collection stores `(source file, diagnostic)` pairs rather than bare
diagnostics. Upstream can store bare ones because its `Diagnostic` holds its
`*ast.SourceFile`; `tsr_diagnostics::Diagnostic` holds only a `Span`, and under
[ADR-0034](../adr/0034-a-program-needs-one-identity-space.md) one `NodeTable`
spans every file of a program, so a span alone cannot say which unit it is an
offset into.

### The rule: TS2307, and the four gates that are the design

Upstream's site is the **fallthrough** of `resolveExternalModule`
(`checker.go:15149`) — a 190-line function carrying fourteen distinct messages,
of which TS2307 is what is left when every other one declines. Porting the
condition means porting the declines, and each decline that is missing is a
*wrong code at a right position*.

The error node is the specifier literal, so the column is the **opening quote**:
`badExternalModuleReference.errors.txt` records `(1,21)` for
`import a1 = require("garbage")`, and 21 is the `"`.

| gate | what upstream does there instead |
|---|---|
| the declaration is not directly under a `SourceFile` or an **ambient** module block (`checkExternalImportOrExportDeclaration`, `checker.go:5332`) | TS1147 / TS1148 grammar error, and `return` **without resolving** |
| the import has no clause — a side-effect import (`checker.go:5321`) | TS2882, a different message from the same resolution |
| resolution named a file the program does not hold | TS7016 / TS6142 / TS2306 — *the same position, a different code* |
| `declare module "x"` names it, or any pattern ambient module exists | resolved; no diagnostic |
| a Node core module name, or `@types/…` | TS2580 / TS2591 / TS6137 substituted at `checker.go:15109` |

Two of those gates cost real machinery and are worth naming:

**`ModuleHost` grew a second method.** `resolved_module`'s doc said, correctly at
the time, that its `None` covers *both* "resolution found nothing" and "resolution
found a file the program does not hold", and that *"telling them apart is a
diagnostic distinction this port has no consumer for."* The traversal is that
consumer. `module_resolution_found` is upstream's
`GetResolvedModule(...).IsResolved()` without the
`GetSourceFileForResolvedModule` membership hop. ADR-0041's *"one method rather
than eighteen"* is a rule against porting the `Program` interface speculatively,
not a cap; this is the second question a real caller asks.

**`noUncheckedSideEffectImports` is plumbed as a flag**, read through upstream's
`IsTrueOrUnknown` so *unset means on*. It exists because
`compiler/ambientExportDefaultErrors` writes it explicitly `false`, and without it
the side-effect arm reports two diagnostics upstream suppresses.

---

## 3. The counterfactual — probe and build are the same function

`examples/diag2307.rs` does **not** re-implement the rule. It calls the shipped
`Checker::check_source_file` and merges its output into the diagnostic set the
`diagnostics` suite compares today. `STATUS.md` §7 records why this shape is
worth the trouble: the composite-print twin landed on a 1,500-line forecast to the
line because `sigprint::compose` and `signature_to_string_at` were one function.
Here the identity is stronger still — there is no second implementation to
diverge from.

What is therefore *not* true while the probe runs: the suite does not consult the
checker. So these are a forecast of the **wiring** commit, and the wiring commit
is the only thing that can move `diagnostics`.

### The gates, measured one at a time

Each row is a full corpus run of the same probe, with one gate added:

| build | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| no gates beyond ambient / pattern / core / `@types` | 45 | 0 | 100 | **60** |
| + the three gates in §2 (position, side-effect, resolved-elsewhere) | 50 | 0 | 101 | **23** |
| + `noUncheckedSideEffectImports` | **50** | **0** | **101** | **21** |

The 39 wrong lines the gates removed were, by case: 14 namespace-positioned
imports (`privacyImportParseErrors` and its sibling, wanting TS1147), 15
resolved-elsewhere (`untypedModuleImport_*` wanting TS7016,
`moduleResolutionWithExtensions_notSupported` wanting TS6142), 5 side-effect
imports wanting TS2882 — which the side-effect arm then converted rather than
merely silenced, which is why CONVERTS rose 45 → 50 while WRONG fell — and 2 the
option suppresses.

### The residual 21, each with an owner

These are **not** this rule's defects. In every one, the rule's condition is
correct and `tsr-module` disagrees with upstream about whether the specifier
resolves:

| lines | family | owner |
|---:|---|---|
| 9 | symlink / `realpath` resolution (`moduleResolutionWithSymlinks*`, `symbolLinkDeclarationEmitModuleNames`, `declarationEmitReexportedSymlinkReference3`) | `tsr-vfs` models no symlinks |
| 6 | `node16` / `nodenext` modes and `package.json` fields (`resolutionModeCache`, `nodeNextImportModeImplicitIndexResolution`, `resolutionCandidateFromPackageJsonField2`, `moduleResolutionWithoutExtension1`) | `tsr-module`'s mode handling |
| 2 | `isolatedModulesExportDeclarationType`, `reservedWords2` | a parse-recovery divergence; `reservedWords2` is `import while = require("dfdf")` |
| 1 | `decoratorMetadataTypeOnlyImport` | **the harness, not the compiler**: the unit is declared `// @filename: ./a.ts`, the baseline writes `a.ts`, and `diagnostics_suite` compares the two spellings literally. `binder_suite::same_unit` exists for exactly this and is not used here |
| 3 | unclassified | — |

The last row is written down rather than folded into the others. It is one case
and it is a *harness* conversion — fixing it would move `diagnostics` without the
compiler improving, so it must never be banked inside a compiler build's number.
`bd tsr-o9tl` carries it.

---

## 4. The bar, registered before the wiring commit

The wiring commit makes `diagnostics_suite::run` append
`Checker::check_source_file`'s output to the parser and binder diagnostics it
already collects. Nothing else changes.

| leg | registered | why this number |
|---|---|---|
| 1 | `diagnostics` passes ≥ **125** | 80 + 50 converts, minus a 5-case discount for the suite building a *program* per case where the probe's `today()` half does not — the two paths must produce identical parser/binder sets and any drift lands here |
| 2 | `checker_types` **unchanged**, exactly | the traversal is a second entry point and no query-path call site invokes it. A single line of movement means `check_source_file` is being reached from the gradient's producer, which it must not be |
| 3 | cases regressed == **0** | the rule only *adds* diagnostics, and every one of the 80 passing cases is already an exact multiset. LOST measured 0 in the counterfactual and a non-zero reading here is a wiring defect, not a rule defect |
| 4 | own new wrong ≤ **25** | the counterfactual's 21, plus margin. This is the mechanism's *own* column: a TS2307 or TS2882 emitted where the baseline records neither |
| 5 | every other suite unchanged | `ModuleHost` grew a method and `Checker` grew two fields; none of it is read off the query path |

**Falsifier 1.** If leg 1 lands materially *above* 130, the extra did not come
from this rule — the suite's program-based path is producing parser or binder
diagnostics the per-unit path does not, and the gain belongs to a harness change
rather than to the checker. Diagnose before banking.

**Falsifier 2.** If leg 4's 21 grows, a gate is being reached in a shape the
counterfactual did not exercise. The most likely one is the position gate: the
probe walks the case's own units only, and a `declare module` block nested two
deep is the shape `external_import_is_positioned_for_resolution` is thinnest on.

**Falsifier 3, and the one to take seriously.** The rule reports on a *negative*
— "resolution found nothing" — which is the consumer kind `docs/conventions.md`
warns is unsafe when a subsystem is incomplete. `module_resolution` reads 95/95
over **95 cases**; the corpus's real resolution surface is two orders larger, and
the 21 residual lines are the first measurement of that gap from outside its own
suite. If the residual is much larger than 21 on the wired run, the honest
conclusion is that TS2307 is bounded by module resolution's completeness and not
by this rule.

---

## 5. Scored — every leg passed, and the forecast was exact

Wired at the commit below; full `coverage` run, snapshots regenerated.

```
diagnostics   80/5,488 (1.46%)  ->  130/5,488 (2.37%)     +50
checker_types 2,841/9,538, gradient 73.65%  ->  unchanged, byte-identical
every other snapshot                        ->  unchanged, byte-identical
```

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 125 | **130** | pass |
| 2 | `checker_types` unchanged exactly | **2,841 / 73.65%**, snapshot byte-identical | pass |
| 3 | cases regressed == 0 | **0** — 130 = 80 + 50 exactly, and the counterfactual's LOST was 0 | pass |
| 4 | own new wrong ≤ 25 | **21 lines across 12 cases** (`diaggap.rs`'s false-positive table now carries a `TS2307 12 cases` row) | pass |
| 5 | every other suite unchanged | only `diagnostics.snap` differs in the whole snapshot directory | pass |

**The forecast was exact: 50 forecast conversions, 50 delivered**, and it was
exact for a reason worth keeping rather than by luck — the probe called the
shipped traversal, so there was no second implementation to diverge from. The
one thing that could still have gone wrong was the *merge* (the suite's
parser/binder half is per-unit while the probe's was too, but the suite now also
builds a program), and leg 1 landing on 80 + 50 rather than above it is what
rules that out. **Falsifier 1 did not fire.**

`STATUS.md`'s §4.1 conversion band — 15% to 57% of a sized population — does not
apply to this build and should not be quoted against it. That band is for
`.types` *lines* sized off a `depend.rs` row, where a mechanism can reach wider
than the row that sized it. Here the unit is a **case**, the sizing was a
counterfactual over the identical predicate, and 100% is the expected reading
rather than a surprise.

### One correction to `STATUS.md` §1, found by this run

The table carried `printer_round_trip 11,681/11,737`; the snapshot at `7299a14`
and at this commit both read **11,682/11,738**. Nothing in this build touches
the printer and `printer_round_trip.snap` is byte-identical across it — the
published figure was one case stale. Corrected in place.

### What the next session should take, and why the order is this one

`diaggap.rs` re-runs in ~4 minutes and its single-code column is the board:

| code | converts alone | what it needs |
|---|---:|---|
| TS2322 | 475 | assignability *plus* the reporting positions — ADR-0040's falsifier 3 measured 28 distinct anchors and the modal one is `BinaryExpression`, not `VariableDeclaration` |
| TS2454 | 255 | definite-assignment analysis over the flow graph this port already builds |
| TS2304 | 192 | a full identifier walk. The resolution is ported; the traversal is not |
| TS2564 | 165 | `strictPropertyInitialization` — a class-member walk plus the constructor's flow |
| TS2339 | 132 | property lookup, ported; again the walk |
| TS6133 | 77 | `noUnusedLocals` — reference counting, no types at all |

**TS6133 and TS2304 are the two that need no new analysis**, only walk plus
something already built, which is the same argument that put TS2307 first here.
TS6133 is the cheaper of the two and TS2304 the larger. Neither has been
counterfactualled; do that before costing either, because both are
*negative-acting* rules and the failure mode is the one falsifier 3 names.

---

## 6. The second rule: TS2564, and a bound that costs a whole disjunct

`Property '{0}' has no initializer and is not definitely assigned in the
constructor.` `diaggap.rs` sizes it at **165 cases blocked on it alone**, second
only to TS2454 among the rules that need no assignability.

`checkPropertyInitialization` (`checker.go:4933`), called from
`checkClassLikeDeclaration`'s last line (`checker.go:4390`). The error node is
the **member's name**, so the column is the property name.

### The bound, and why it is a refusal rather than an approximation

Upstream's condition is `constructor == nil || !isPropertyInitializedInConstructor(...)`
(`checker.go:4947`). The second disjunct **synthesises** a `this.x` property
access, hangs it off the constructor's `ReturnFlowNode`, and asks
`getFlowTypeOfReference` whether `undefined` survives — the sibling for static
blocks is visible in full at `checker.go:4960`.

This port cannot do that. The tree is arena-allocated and immutable after
parsing ([ADR-0012](../adr/0012-ast-is-sync.md)), and a flow query needs a
*registered* node with a parent and a flow node of its own. So **a class with a
constructor body is declined outright** — silence, never a wrong answer, because
upstream reports there only when the constructor fails to assign and this port
cannot tell those apart. What that refusal costs is measured in the residual
below, not assumed.

### The ambient bit, and a parser flag that is declared and never written

The first measurement read **86 wrong lines** and the cause was one fact:
`tsr_ast::NodeFlags::AMBIENT` exists and **nothing sets it** —
`grep -rn AMBIENT crates/tsr-parser/src` is empty. Upstream's parser sets it as a
context flag on every node of a declaration file and inside every `declare`d
declaration, and `checkPropertyInitialization`'s first line reads it.

Reading the unset flag reported TS2564 on **every property of every
`declare class` in the corpus** — `castTest`, `genericFunctionInference1`,
`signatureCombiningRestParameters3`, `noImplicitAnyParametersInAmbientClass` (12
lines by itself) and 20 more. Repaired by carrying the bit explicitly: the caller
supplies the file-level half (is this a `.d.ts`) and the walk carries the
`declare`-modifier half down through class and module declarations. That is a
faithful reproduction of the *effect* and an unfaithful one of the *mechanism*;
`bd tsr-o9tl` carries the parser fix, after which the parameter disappears and
every one of upstream's ~40 readers gets the flag for free.

**The number is the finding here.** A flag that is declared, documented and never
written reads exactly like a flag that works, and the only thing that
distinguished them was a measurement. `docs/conventions.md`'s *"a prerequisite in
your own doc comment is checked the way a handover's is"* — this is the same
failure with the prerequisite inside the same repository.

### Counterfactual, cumulative with §3's rule

`examples/diag2307.rs` with `RULE_CODES` extended to `{2307, 2882, 2564}`:

| build | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| TS2307 / TS2882 only (§3, shipped at `bc8045b`) | 50 | 0 | 101 | 21 |
| + TS2564, ambient bit unset | 182 | 0 | 1,305 | **107** |
| + the ambient bit carried | **183** | **0** | **1,305** | **27** |

So TS2564's own contribution is **+133 conversions for 6 wrong lines**, a 22:1
trade — the best ratio any build in this project has registered.

The 6, each diagnosed rather than counted: `typeParameterUsedAsTypeParameterConstraint4`
and `ClassAndModuleThatMergeWithModuleMemberThatUsesClassTypeParameter` are a
type-parameter *scope* divergence (this port resolves a `W` that is out of scope,
so the property gets a type where upstream gets `errorType`, which upstream's
`AnyOrUnknown` test then excludes); `indexSignatureWithAccessibilityModifier` and
`classExtendsEveryObjectType`/`2` are parse-recovery divergences producing a
`PropertyDeclaration` upstream does not have; `decoratorMetadataNoLibIsolatedModulesTypes`
is `@noLib` with a decorated member. **None is the rule's condition being wrong.**

### The bar, registered before the coverage run that scores it

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **255** (130 + 133, discounted 8 for the merge) |
| 2 | `checker_types` **unchanged, byte-identical snapshot** |
| 3 | cases regressed == **0** |
| 4 | own new wrong ≤ **10** (the counterfactual's 6, plus margin) |
| 5 | every other snapshot unchanged |

**Falsifier 1.** Above ~265 and the gain is not this rule's; diagnose before
banking, exactly as §5's falsifier 1 required and did not fire.

**Falsifier 2.** If leg 4 exceeds 10, the ambient bit is leaking through a
context the walk does not carry. The likeliest is a class the walk never reaches
at all — it visits statements and module bodies only, so a class inside a
function body or a block is invisible, and that shows up as a *missed*
conversion rather than a wrong line. A wrong line instead would mean a `declare`
context reached by a route the two carried halves do not cover.

**Falsifier 3, and the one that decides whether this item is finished.** The
no-constructor bound is a refusal of one of upstream's two disjuncts. If, after
this build, `diaggap.rs` still shows a large TS2564 single-code population, the
remainder is the constructor half and it is blocked on synthesising a flow
reference — which is an ADR-sized question about whether this port grows a
synthetic-node facility, not a follow-up patch. Read the post-build TS2564 row
and write the number down either way.

### TS2564 scored — every leg passed, and falsifier 3 answered the bound

```
diagnostics   130/5,488 (2.37%)  ->  263/5,488 (4.79%)     +133
checker_types 2,841 / 73.65%     ->  unchanged, byte-identical
every other snapshot             ->  byte-identical
```

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 255 | **263** — 130 + 133 exactly | pass |
| 2 | `checker_types` unchanged | byte-identical snapshot | pass |
| 3 | cases regressed == 0 | **0** | pass |
| 4 | own new wrong ≤ 10 | **6 cases** in `diaggap.rs`'s false-positive table | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

**Forecast exact for the second time**: 133 forecast, 133 delivered. Falsifier 1
did not fire.

**Falsifier 3 answered, and the answer is that the bound is nearly free.** The
TS2564 single-code row went **165 → 32**. The no-constructor refusal — declining
upstream's whole second disjunct, the one needing a synthesised flow reference —
costs **32 cases**, not most of 165. So the synthetic-node question this page
raised as *"ADR-sized"* does not need answering to finish this rule; it is worth
32 cases and belongs behind TS2454 and TS2304 on the board.

### The cascade this build made visible, and how not to misread it

Two rows *grew* across the build:

```
TS2322   475 -> 518      TS2454   255 -> 300      TS6133   77 -> 81
```

Nothing regressed. Those are cases that were blocked on **two** codes — one of
them TS2564 — and are now blocked on one. It is the same arithmetic that makes
the single-code column a forecast rather than a ceiling, running in the helpful
direction, and it means **the board gets more valuable as rules land** rather
than being consumed by them. Re-run `diaggap.rs` after every rule; a row's
number is only true of the compiler that produced it.

Board at `263` passing:

| code | converts alone | needs |
|---|---:|---|
| TS2322 | 518 | assignability + reporting positions |
| TS2454 | 300 | definite-assignment over the existing flow graph |
| TS2304 | 197 | a full identifier walk |
| TS2339 | 137 | property lookup + the walk |
| TS2345 | 100 | assignability at argument positions |
| TS6133 | 81 | reference counting, no types |
| TS2741 / TS2353 | 38 / 37 | assignability |
| TS2564 | 32 | the constructor disjunct — synthesised flow reference |

---

## 7. The third rule: TS2304, and four refusals that were each worth more than the rule

`Cannot find name '{0}'.` Sized by `diaggap.rs` at **197 cases blocked on it
alone** at 263 passing.

`getResolvedSymbol` (`checker.go:13890`) resolves every identifier expression
with `SymbolFlagsValue|SymbolFlagsExportValue`; failure lands in
`onFailedToResolveSymbol` (`checker.go:1564`) whose **last line** is this
diagnostic. Everything above that line is a decline, and there are ten of them.

### The walk had to become general first

The statement-and-module walk §2 shipped cannot reach an identifier. It is
replaced by a recursive walk over every registered child
([`tsr_ast::for_each_child_id`]) with a depth bound, rather than by porting
`checkSourceElement`'s 120-arm switch. The consequence is stated at the function
and is the load-bearing one: **a rule now sees nodes upstream's corresponding
`checkXxx` would never be handed**, so every rule carries its own position test.
For identifiers that test is an *allow*-list over the parent's **slot**, not its
kind — a `PropertyAccessExpression` resolves its `expression` and never its
`name`, and conflating those reports `Cannot find name 'length'` on every
`a.length` in the corpus.

Restructuring the walk alone moved the §6 counterfactual 183 → 185 converts with
`WRONG` unchanged: two more classes reachable, nothing new reported.

### Four measurements, each a refusal

| build | CONVERTS | LOST | WRONG |
|---|---:|---:|---:|
| TS2304 as first written | 299 | **30** | **933** |
| + skip *missing* identifiers (`!ast.NodeIsMissing`, `checker.go:13894`) and `arguments` | 303 | 1 | 541 |
| + `getSpellingSuggestion` ported exactly, + `globalThis` | 305 | 1 | 428 |
| + refuse every identifier in a file with **parse errors** | 298 | **0** | 122 |
| + refuse inside a `with` block (TS2410, `checker.go:29344`) | **298** | **0** | **113** |

Cumulative with §3 and §6, so TS2304's own share is **+113 conversions for 86
wrong lines**, and the wrong column is 55 cases.

**The spelling suggestion is the whole rule.** `onFailedToResolveSymbol` tries
`getSuggestedSymbolForNonexistentSymbol` immediately before falling through, so
every name with a near neighbour in scope is a **TS2552**, not a TS2304. A
hand-rolled within-one-edit test looked adequate and
`conformance/parserS7.6_A4.2_T1` alone produced 20 wrong lines from it: `$ERROR`
against `Error` is one deletion plus five *case* differences, and upstream's
distance charges 0.1 for a case difference and 2 for anything else
(`core.go:650`-`:653`). Plain edit distance says 6; upstream's says 1.4 against a
threshold of 3.3. `core.GetSpellingSuggestion` (`core.go:559`) is ported exactly,
including the `max(2, 0.34·len)` length filter and the "candidates under 3
characters only when they differ by case" rule, with two departures stated at the
function — upstream's byte-vs-rune length comparison is a Go slip and is not
reproduced, and the tie-break is dropped because this caller asks only whether a
suggestion exists.

**The parse-error refusal is the largest single lever and it is a new kind.**
Upstream's recovery *is* the recovery the baselines were produced from, so a node
it builds in a broken file is still the node the diagnostic is about. Here the
two parsers disagree about what tree a broken file has, and reporting an
unresolvable name on a node one parser invented is reporting about a program the
other never saw. Refusing costs **7 conversions** and removes **306 wrong
lines** — `jsxUnclosedParserRecovery` 21, `arrowFunctionsMissingTokens` 15,
`parserUnterminatedGeneric2` 8, and a long tail of `parserSkippedTokens`,
`parserErrorRecovery*` and conflict-marker cases. It is stated as a field on the
`Checker` rather than hidden in the rule because **every future diagnostic rule
wants it**.

### The residual 86, and the reachability it costs

Diffuse — the largest family is 4 lines. By owner: class and namespace scoping in
`BindResult::resolve_name` (`staticsInConstructorBodies`,
`initializerReferencingConstructorParameters`,
`constructorParametersInVariableDeclarations`, `exportNestedNamespaces2`,
`computedPropertyNamesWithStaticProperty` — ~25); the
`getSuggestedLibForNonExistentName` family, TS2583, unported
(`doYouNeedToChangeYourTargetLibraryES2016Plus`,
`modularizeLibrary_ErrorFromUsingES6FeaturesWithOnlyES5Lib` — ~8); TS2552 misses
where upstream's scope differs from ours (~6); the rest singletons.

**The cost is stated in the currency that matters.** A case carrying a spurious
diagnostic can never pass, however many rules land later, so the number to watch
is not the wrong *lines* but the single-code reachable *set*:

```
                    passing   single-code reachable   sum
before TS2304          263            3,285          3,548
after  TS2304          378            3,226          3,604
```

So the rule converts 115 now and removes 59 cases from single-rule reach, for a
net **+56** on the set of cases any one further rule could finish. That is a
thinner margin than §3's or §6's and it is the honest way to score a rule that
reports on a negative. Had the four refusals not been taken it would have been
**−85** — the first build in this file that would have made the board *worse*.

### The bar, and an honesty note about what it is

`diaggap.rs` calls `diagnostics_suite::reported_for`, which is the suite. So the
suite's passing count was **known before this bar was written**; the conversion
leg below is a *confirmation*, not a forecast, and is recorded as such rather
than dressed up. The legs that were genuinely open when it was registered are
2–5.

| leg | registered | measured | |
|---|---|---:|---|
| 1 (confirmation) | `diagnostics` passes == 378 | **378** | pass |
| 2 | `checker_types` unchanged, byte-identical | **2,841 / 73.65%**, snapshot identical | pass |
| 3 | cases regressed == 0 | **0** — 378 = 263 + 115 | pass |
| 4 | own new wrong ≤ 60 cases | **55** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

**Falsifier.** If `checker_types` moves at all, the general walk is being reached
from the gradient's producer — it is a `pub fn` on the same `Checker` the
producer builds, and nothing but discipline stops a future call site invoking it.

---

## 8. The fourth rule: TS2454, and a 227-line concentration that was one keyword

`Variable '{0}' is used before being assigned.` `diaggap.rs` sized it at **300
cases blocked on it alone** at 378 passing — the largest rule on the board that
needs no assignability.

`checkIdentifier` (`checker.go:11191`): the flow type carries `undefined` where
the declared type does not, and `assumeInitialized` is false.

### The bound is `assumeInitialized`, and it is chosen so the unported half cannot matter

`assumeInitialized` (`checker.go:11150`) is a nine-way disjunction and **every
disjunct that is false is a diagnostic**. Two of them need machinery this port
does not have: `isSymbolAssignedDefinitely` needs `markNodeAssignments`
(`flow.go:2655`), `isPastLastAssignment` needs recorded assignment positions.

Rather than approximate those, the rule **requires the shape in which they cannot
be consulted**:

- the symbol's declaration is a plain `VariableDeclaration` **with a type
  annotation** — which excludes `isParameter`, `isAlias`,
  `isSameScopedBindingElement` and the auto-typed path by construction;
- the reference's control-flow container **is** the declaration's, so
  `isOuterVariable` is false and `isNeverInitialized` — the sole consumer of
  `isSymbolAssignedDefinitely` — is never reached;
- the reference is not a definite assignment target, which `checker.go:11109`
  returns early for.

The rest of the disjunction is syntactic and *is* ported: `!` on the declaration,
an ambient declaration, `typeof x`, an ambient-or-type-node position, an
`ExportSpecifier` parent, a `NonNullExpression` parent.

`get_flow_type_of_reference` grew upstream's `initialType` parameter to serve it
(`get_flow_type_of_reference_ex`). That is the concrete form of ADR-0040's claim
that the two roads are different entry points: the query road wants the declared
type at the top of the graph, and this rule is *defined* by running the same
graph with `T | undefined` there instead.

### The first measurement, and what 227 lines in one case turned out to be

| build | CONVERTS | LOST | WRONG |
|---|---:|---:|---:|
| as first written | 530 | 0 | **4,894** |
| + exclude `const` and `declare` declarations | **531** | **0** | **206** |

Cumulative, so TS2454's own share is **+233 conversions for 93 wrong lines**.

**4,781 wrong lines fell to 93 on one predicate, and 227 of them were a single
case.** `compiler/genericDefaults` opens with `declare const a: A;` fourteen
times over. A `const` with no initialiser occurs only in an ambient context or
after a grammar error, and upstream's `assumeInitialized` short-circuits on
`declaration.Flags&NodeFlagsAmbient` (`checker.go:11158`) — the same unset parser
flag §6 met, arriving through a different door and costing 50× more.

`docs/conventions.md`'s *"concentration is a case-gate concern; for the line
gradient it is leverage"* has a diagnostics-side corollary worth writing down:
**a wrong column dominated by one case is almost always one predicate, and
reading the top case before tightening anything is the cheapest move available.**
Both of this session's largest residuals — 86 wrong for TS2564, 4,894 for
TS2454 — were the ambient flag, and both were found by looking at the top row
rather than at the total.

### The residual 93, and the cost accounting

Diffuse, top family 12 lines: `typeGuardOfForm*` and
`typeGuardConstructorPrimitiveTypes` (narrowing divergences — this port's flow
leaves `undefined` alive where upstream's guard removes it),
`shorthandPropertyAssignmentsInDestructuring_ES6` and the iterable-pattern cases
(destructuring). Every one is a flow or destructuring gap with an existing owner;
none is the rule's condition.

```
                    passing   single-code reachable   sum
before TS2454          378            3,226          3,604
after  TS2454          611            3,021          3,632
```

**+28 net**, and the number that matters is what it would have been without the
`const`/`declare` predicate: **610 + 2,784 = 3,394, or −210**. The same shape as
§7 and a starker margin — a rule that reports on a negative is worth roughly
nothing until its declines are right, and is worth *less than nothing* before
that.

Only **29 cases** carry a spurious TS2454, against 55 for TS2304.

### The bar

As in §7, `diaggap.rs` calls the suite, so leg 1 is a confirmation. Legs 2–5
were open.

| leg | registered | measured | |
|---|---|---:|---|
| 1 (confirmation) | `diagnostics` passes == 611 | **611/5,488 = 11.13%** | pass |
| 2 | `checker_types` unchanged, byte-identical | **2,841 / 73.65%**, snapshot identical | pass |
| 3 | cases regressed == 0 | **0** — 611 = 378 + 233 | pass |
| 4 | own new wrong ≤ 35 cases | **29** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

The falsifier did not fire: `checker_types` is byte-identical across the
`get_flow_type_of_reference` refactor, so delegating to the `_ex` form kept the
auto-typed default.

**Falsifier, and this one is real.** `get_flow_type_of_reference` was refactored
to delegate to the `_ex` form. If `checker_types` moves by a single line, the
refactor was not behaviour-neutral — the `initial_type` default now runs through
a `match` where it ran through an `if`, and the auto-typed arm is the one that
could have been dropped.
