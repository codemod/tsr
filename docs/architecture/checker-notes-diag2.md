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

---

## 9. TS2339 — built, measured, REFUSED at 2 conversions for 254 wrong lines

`Property '{0}' does not exist on type '{1}'.` `diaggap.rs` sized it at **133
cases blocked on it alone** at 611 passing, third on the board.

Built to the tightest bound available and reverted the same measurement. The
build is at `reportNonexistentProperty` (`checker.go:11530`), error node the
property name, with this port's own incompleteness handled by firing **only**
where the receiver type carries `members: Some(_)` — an object type whose
members this checker actually resolved — and declining for every union,
intersection, type parameter, primitive and intrinsic.

```
CONVERTS  533 (+2 over §8)     LOST 0     WRONG 460 (+254)
```

**Two conversions for 254 wrong lines**, or 0.008 gained per wrong. Every
refusal in `STATUS.md` §5 is between 0.47 and 1.03; this is two orders of
magnitude below the worst of them.

### The diagnosis, which is the part worth keeping

The bound does not do what it was chosen to do. `Named { members: Some(_) }`
says *a* table was built, not that it is **complete**: members reached through
heritage, mapped types, conditional types and mixins are resolved lazily and by
different arms, so a type can carry a table and still be missing the property the
source names. The wrong column says exactly that —
`longObjectInstantiationChain1`/`3` 13 lines each (instantiation depth),
`mixinAccessModifiers` 9, `genericDefaults` 9, `discriminatedUnionTypes2` 8,
`conditionalTypes1` 8, `mappedTypes6`, `recursiveIntersectionTypes`,
`classExtendingClassLikeType`.

> **An absent property and an unbuilt members table are the same `None`, and no
> predicate over the *type* separates them.** That is the same structural shape
> as `checker-notes-selectable.md`'s refusal — decidability is a property of the
> pair, not of either side — arriving in a second subsystem. A flag on the type
> cannot say whether the table it points at is finished.

And the 2 conversions say the other half: of the 133 cases, the ones this bound
can reach are almost none, because the interesting TS2339 sites in the corpus are
exactly the generic and mapped receivers the bound excludes.

**What would make this win**, stated so the refusal is revisitable: a members
table that knows whether it is complete — i.e. `resolveStructuredTypeMembers`
ported with an explicit resolved/unresolved state per type, rather than an
`Option` that conflates "no members" with "not yet". That is a members-subsystem
question and it is worth more than this diagnostic; `bd tsr-o9tl` records it, and
the 133 cases are its size on the `diagnostics` side.

The four declines above the message in `reportNonexistentProperty` — TS2576
static member, TS2550 newer `lib`, TS2551 spelling, TS2812 DOM — were **not** the
problem and are not what refused this. They would each have cost a handful of
lines against 254.

---

## 10. Two syntactic rules: TS2369 and TS2695, +40 for **zero** wrong lines

After §9's refusal the board's next four rows all need assignability. The two
below do not need types at all, and that turns out to be the whole story.

| rule | site | sized by `diaggap.rs` at 611 |
|---|---|---:|
| **TS2369** `A parameter property is only allowed in a constructor implementation.` | `checkParameter` (`checker.go:2670`) | 16 cases |
| **TS2695** `Left side of comma operator is unused and has no side effects.` | `checkBinaryLikeExpression`'s comma arm (`checker.go:12533`) | 25 cases |

TS2369 is a modifier test (`ModifierFlagsParameterPropertyModifier` is
`AccessibilityModifier | Readonly | Override`, `ast/modifierflags.go:45`) plus
"is the owner a constructor **with a body**". TS2695 needs `isSideEffectFree`
(`checker.go:13011`, a kind switch ported one for one) and `isIndirectCall`
(`checker.go:13039` — `(0, x.f)()` and `(0, eval)()`, the idiom for calling
without passing `this`), plus the `allowUnreachableCode` option.

```
CONVERTS 531 -> 571   (+40)      LOST 0      WRONG 206 -> 206   (+0)
```

**Zero new wrong lines**, which is the first build in this file where the
residual did not need a single tightening pass. That is not luck and it is the
point worth extracting:

> **A rule that reports on a *syntactic* fact has no incompleteness to leak.**
> Every residual in §3, §6, §7, §8 and §9 came from the same place — this port
> answering `None` where upstream answers a symbol, a member or a resolution —
> and none of it can reach a rule whose whole condition is "which modifiers does
> this node carry" or "what kind is this operand". The four rules that needed
> bounds were the four that ask a *semantic* question of an incomplete
> subsystem.

That reorders the rest of the board. `diaggap.rs` at 651 carries ~150 more cases
in small `1xxx` and syntactic `2xxx` rows — TS1212 22, TS1036 19, TS2391 17,
TS1029 12, TS1107 12 and a long tail — and on this evidence each is worth its
row at roughly zero risk, where TS2322's 543 is worth a fraction of its row and
needs a members subsystem first.

Upstream's TS2695 additionally suppresses the diagnostic where a
`JSX_expressions_must_have_one_parent_element` parse error covers the position
(`checker.go:12537`). That whole class is already excluded by §7's parse-error
gate — the first time that gate has paid for something other than tree shape.

### The bar

| leg | registered | measured | |
|---|---|---:|---|
| 1 (confirmation) | `diagnostics` passes == 651 | **651/5,488 = 11.86%** | pass |
| 2 | `checker_types` unchanged, byte-identical | **2,841 / 73.65%**, identical | pass |
| 3 | cases regressed == 0 | **0** — 651 = 611 + 40 | pass |
| 4 | own new wrong == **0** | **0** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

Leg 4 is registered as an exact zero rather than a ceiling. These rules read
modifiers and token kinds; a single wrong line means a *kind* was misread, which
is a defect rather than a trade, and rounding it into a budget would hide it.

---

## 11. `checkGrammarBreakOrContinueStatement` — five codes from one walk, +25 for zero wrong

§10's finding said to take the syntactic rows next. This is the first of them and
it is a whole upstream function rather than a single code:
`checkGrammarBreakOrContinueStatement` (`grammarchecks.go:1480`) is one upward
walk producing **five** diagnostics — TS1107 (crosses a function boundary),
TS1104/TS1105 (no enclosing iteration or switch), TS1115/TS1116 (no such label).
`diaggap.rs` sized TS1107 alone at 12 cases; the family delivered 25.

```
CONVERTS 571 -> 596   (+25)      LOST 0      WRONG 206 -> 206   (+0)
```

The first measurement read **2** wrong, and both were the same thing:
`parserBreakStatement1.d` and `parserContinueStatement1.d`, one-line `.d.ts`
files containing nothing but `break;`. Upstream's call site
(`checker.go:4081`) is

```go
if !c.checkGrammarStatementInAmbientContext(node) {
    c.checkGrammarBreakOrContinueStatement(node)
}
```

and in an ambient context that guard reports **TS1036 `Statements are not
allowed in ambient contexts`** and short-circuits. TS1036 is not ported — it is
19 cases of its own on the board — so the short-circuit is reproduced as a
refusal, and the ambient bit §6 had to introduce for `NodeFlags::AMBIENT` paid
for itself a third time.

**Three of this file's six residual diagnoses have now been the ambient flag**
(§6's 86 wrong lines, §8's 4,781, this one's 2). A flag that is declared and set
by nothing is not a dormant feature; it is a landmine with one instance per
reader, and the reader count is upstream's ~40.

### The bar

| leg | registered | measured | |
|---|---|---:|---|
| 1 (confirmation) | `diagnostics` passes == 676 | **676/5,488 = 12.32%** | pass |
| 2 | `checker_types` unchanged, byte-identical | **2,841 / 73.65%**, identical | pass |
| 3 | cases regressed == 0 | **0** — 676 = 651 + 25 | pass |
| 4 | own new wrong == **0** | **0** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

---

## 12. `checkGrammarStatementInAmbientContext` — TS1036 and TS1183, +24 for zero wrong

The guard §11 had to reproduce as a refusal, ported instead
(`grammarchecks.go:2047`). It is the short-circuit thirteen of upstream's
`checkXxxStatement` functions are written around —
`if !c.checkGrammarStatementInAmbientContext(node) { … }` — so it returns whether
it reported, and the break/continue rule now runs behind it rather than
declining.

```
CONVERTS 596 -> 620   (+24)      LOST 0      WRONG 206 -> 206   (+0)
```

### The state is the rule, and its key is not what it looks like

Upstream keeps one bit, `hasReportedStatementInAmbientContext`. In the TS1183
branch it hangs on the **node**; in the TS1036 branch it hangs on the node's
**parent**. Same field, two keys — and that is load-bearing, not incidental: a
method body in an ambient class reports TS1183 *for the block*, which flags the
block, and every statement inside then finds its parent already flagged and stays
silent.

Keying the two branches separately produced this rule's single wrong line —
`conformance/initializersInDeclarations`, where upstream records TS1183 at the
body (6,16) and **nothing** at the `return` inside it, and this port added a
TS1036 at (7,3). One `insert` moved the residual to zero.

Under the suite's exact-multiset comparison the "report once per block" bit is
not an optimisation: reporting three times for `declare module "m" { a; b; c; }`
fails the case exactly as reporting none does.

### A convention this workstream had already adopted, found in upstream

`grammarErrorOnFirstToken` and `grammarErrorOnNode` (`grammarchecks.go:19`,
`:38`) both open with `if !c.hasParseDiagnostics(sourceFile)`. **Upstream
suppresses every grammar diagnostic in a file that failed to parse**, which is
exactly the refusal §7 introduced for TS2304 and justified on this port's
recovery differing from upstream's. The refusal is therefore *faithful* for the
grammar family rather than a deviation from it — a rare case of a bound invented
here turning out to be upstream's own rule, and worth recording because the
reasoning that produced it was completely different.

### The bar

| leg | registered | measured | |
|---|---|---:|---|
| 1 (confirmation) | `diagnostics` passes == 700 | **700/5,488 = 12.76%** | pass |
| 2 | `checker_types` unchanged, byte-identical | **2,841 / 73.65%**, identical | pass |
| 3 | cases regressed == 0 | **0** — 700 = 676 + 24 | pass |
| 4 | own new wrong == **0** | **0** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

(Superseded by §14's closing figure.)

---

## 13. TS7026 — built, measured, REFUSED at 12 conversions for 47 wrong lines

`JSX element implicitly has type 'any' because no interface
'JSX.IntrinsicElements' exists.` 27 cases on the board, `getIntrinsicTagSymbol`
(`jsx.go:1252`): an intrinsic tag (`IsIntrinsicJsxName`, a lowercase first letter
or a `-`, `scanner/utilities.go:98`) whose `JSX.IntrinsicElements` does not
resolve, under `noImplicitAny`.

It was expected to be safe for a reason §9 makes precise — it reports on the
absence of a **namespace**, which `binder_symbols` answers at 98.03%, not on the
absence of a member of a type this port may have failed to build. That reasoning
was right about the *kind* of question and wrong about *this port's* answer to
it.

```
CONVERTS 620 -> 632   (+12)      LOST 0      WRONG 206 -> 253   (+47)
```

**0.26 conversions per wrong line**, below every refusal in `STATUS.md` §5.

### One cause, already on record

`conformance/checkJsxChildrenCanBeTupleType`, `compiler/jsxElementType`,
`conformance/inlineJsxFactoryDeclarationsLocalTypes` and the rest all reference
`/.lib/react16.d.ts`, which declares its `JSX` namespace inside a
`declare global { … }` block. **Global augmentation is unported in this
binder** — `checker-notes-jsx.md` recorded it as the JSX row's first blocker in
the fifth session, and the `/.lib` mount that made `react.d.ts`'s *plain* global
`namespace JSX` visible did not make an augmented one visible.

So the rule's condition is `JSX.IntrinsicElements` does not resolve, and in 47
lines the honest reading of that is **"this binder cannot see it"** rather than
"it is not there" — the third distinct subsystem in this file where those two are
the same answer (module resolution in §3, members in §9, global augmentation
here).

**What would make it win**, and it is a prerequisite already named elsewhere:
`declare global` augmentation merged in the binder. `checker-notes-jsx.md`
carries the `.types` side of that item; this is its `diagnostics` side, worth 27
cases.

A bound was considered and declined: refusing whenever the program contains a
global augmentation would be sound, but the checker cannot enumerate the
program's files and the harness testing for the *text* `declare global` is not a
predicate, it is a grep. Recorded so the next attempt does not spend the cycle.

---

## 14. `checkFunctionOrConstructorSymbol` — the implementation-expected arms

TS2391/2390/2392/2393 — "an overload set has no implementation". 17 cases for
TS2391 alone on the board; the family delivered 17 net.

Upstream's worker (`checker.go:3469`) is 240 lines doing five unrelated jobs:
implementation presence, modifier agreement across overloads, question-token
agreement, class/function merging, and an implementation-versus-overload
*relation* check. **Only the first is ported.** The last needs the relation and
the middle three are their own rows.

```
CONVERTS 620 -> 637   (+17)      LOST 0      WRONG 206 -> 217   (+11)
```

### The 480-line measurement that was one expression

The first build read **686 wrong**, and every extra line came from one
transliteration:

```go
previousDeclaration.End() != node.Pos()      // upstream
self.nodes.span(earlier).end != self.nodes.span(declaration).start   // here
```

Upstream's `Pos()` is the **full start** — the end of the preceding token,
trivia included — so two declarations on consecutive lines satisfy
`prev.End() == next.Pos()`. `tsr_core::Span` records the token start *after*
trivia, so the same expression is false for **every** pair of declarations
separated by a newline, and the port reported TS2391 on 480 lines of perfectly
ordinary overload sets.

> **A faithful transliteration of an expression is not a faithful port of its
> meaning when the two ASTs disagree about what a position *is*.** The fix is not
> a tolerance; it is asking the question upstream's expression was asking —
> *is this the next sibling* — which the child walk answers exactly.

This is `docs/conventions.md`'s "a ported predicate can be sound upstream and
unsound here" with a new cause: not an incomplete subsystem, but a different
position model. `Pos()`/`End()` appear ~2,000 times in `checker.go`; every one is
this trap.

### Three declines, each read out of upstream's branch structure

| decline | why | lines removed |
|---|---|---:|
| the next sibling is the same kind, and either names match or it has a body | `checker.go:3567`'s subsequent-node scan reports TS2387/2388 or TS2389 there, or returns — it **never** reaches TS2391 | ~14 |
| any declaration is class-like | `hasNonAmbientClass` (`checker.go:3660`) has its own arm, TS2813/2814 | 18 |
| the declarations do not share one parent | upstream's overloads are siblings; `class Point { static Origin(){} }` beside `namespace Point { export function Origin(){} }` is **two** symbols upstream, reported by the binder as TS2300 and never reaching this function | 18 |

Plus the two bounds registered up front: single-file symbols only (a declaration
in another file has no reachable ambient bit here, since this port's is per-file
caller state rather than a node flag), and once per symbol —
`links.functionOrConstructorChecked`, without which a three-overload function
reports three times.

### The residual 11

Ten lines are two cases and one cause: `parserConstructorDeclaration12`
(`constructor<>() { }` eight times) reports TS2393 where upstream reports TS2392,
because this parser produces a `MethodDeclaration` named `constructor` for a
constructor carrying type parameters where upstream produces a
`ConstructorDeclaration`. A parser divergence on a deliberately malformed input;
filed rather than worked around. The other two are `jsFileCompilation*` overload
syntax in JS files.

### The bar

| leg | registered | measured | |
|---|---|---:|---|
| 1 (confirmation) | `diagnostics` passes == 717 | **717/5,488 = 13.06%** | pass |
| 2 | `checker_types` unchanged, byte-identical | **2,848 / 73.70%**, identical | pass |
| 3 | cases regressed == 0 | **0** — 717 = 700 + 17 | pass |
| 4 | own new wrong ≤ 15 | **11** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

**`diagnostics` closes the session at 717/5,488 = 13.06%, from 80/5,488 =
1.46% — 8.96×.** Nine builds, two refusals, `checker_types` byte-identical
throughout.

---

## 15. `checkUnusedIdentifiers` — noUnusedLocals / noUnusedParameters

The tenth session's first build, and the item the ninth session's handoff named
as *"the safest large item left"* — **because its blast radius is confined to
cases that SET the option**. 302 of the corpus's case files write
`@noUnusedLocals` or `@noUnusedParameters`; every other case is untouched by
construction, so the wrong-line risk is bounded by that set rather than by the
corpus.

### The board row

`diaggap.rs` at `6b363e3`: **TS6133 80** cases blocked on it alone, **TS6196 32**,
and the ceiling column reads TS6133 98 cases containing it. Neither figure had
been counterfactualled before this build (the handoff says so explicitly).

### What is ported

`checkUnusedIdentifiers` (`checker.go:7046`) and the four workers it dispatches
to: `checkUnusedLocalsAndParameters` (`:7140`), `checkUnusedClassMembers`
(`:7115`), `checkUnusedTypeParameters` (`:7290`), `checkUnusedInferTypeParameter`
(`:7283`). Seven codes: TS6133, TS6138, TS6192, TS6196, TS6198, TS6199, TS6205.

### The prerequisite the handoff named, and the shape it actually took

Upstream's `isReferenced` reads `symbolReferenceLinks[symbol].referenceKinds`,
written from exactly one place — the `SymbolReferenced` callback the checker
hands its `binder.NameResolver` (`checker.go:1499`). Every `resolveName` during a
type check marks. **This port has no type check to hang that on**, so reference
marking is its own pass over the file's identifiers.

The pass is deliberately an **over**-approximation, and the asymmetry is the
whole design: a symbol marked that upstream would not mark costs a *missing*
diagnostic; a symbol left unmarked costs a *wrong* one, which fails its own case
and can break a passing case as well. So the pass marks every identifier that is
not provably a declaration name or a member-access name, resolving it under all
three meanings and recording each that hits.

Private class members cannot be marked that way — `this.x` resolves through a
type, and this port's check traversal computes none. They are marked **by name**:
any member-name text occurring anywhere in the file counts as a reference. Same
asymmetry, same direction.

### The bar, registered before the code

| leg | registered |
|---|---|
| 1 | `LOST == 0` — no case that passes today may fail |
| 2 | `checker_types` byte-identical (this touches no type) |
| 3 | own new WRONG ≤ 40 lines |
| 4 | CONVERTS ≥ 40 |
| 5 | every other snapshot unchanged |

Falsifiers named in advance: (a) if the corpus's unused cases turn out to need
the *suggestion* channel rather than the error channel, the whole row is
unreachable and the build is reverted; (b) if reference marking by
over-approximation marks so much that CONVERTS lands under 40, the row is
refused with that number and the marking pass is not "tightened" — tightening it
is what produces wrong diagnostics.

### Scored — every leg passed, and the largest wrong family was one guard

```
CONVERTS 717 -> 832  (+115)     LOST 0      RIGHT 295     WRONG 11
```

| leg | registered | measured | |
|---|---|---:|---|
| 1 | `LOST == 0` | **0** | pass |
| 2 | `checker_types` byte-identical | **2,963 / 31.07% / 74.19%**, identical | pass |
| 3 | own new WRONG ≤ 40 | **11** | pass |
| 4 | CONVERTS ≥ 40 | **115** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

**`diagnostics` 717/5,488 = 13.06% → 832/5,488 = 15.16%.**

#### The 108-line guard: `checkSourceFile` registers a *module*, not a file

The first measurement read **158 wrong, 3 lost, 21 converts** — a failed bar on
three legs at once. One line explains 108 of the 158 and all three losses:

```go
if ast.IsExternalOrCommonJSModule(sourceFile) {
    c.checkExternalModuleExports(sourceFile.AsNode())
    c.registerForUnusedIdentifiersCheck(sourceFile.AsNode())   // checker.go:2210
}
```

The register sits **inside** the module test. A script's top level is the global
scope; its `class`, `function` and `namespace` declarations are not locals and
can never be unused. Without the guard this port reported every top-level
declaration of every script in the unused corpus — `unusedClassesinNamespace1`,
`unusedFunctionsinNamespaces1`–`6`, `unusedInterfaceinNamespace1`–`3` and the
rest, all of which then *converted* once the guard was in.

> A register site inside a conditional is a **precondition of the rule**, not
> plumbing. Reading `registerForUnusedIdentifiersCheck(sourceFile)` as "the file
> participates" rather than "an external module participates" is the same class
> of error as §14's `Pos()`/`End()` transliteration: the code was copied and the
> question it was answering was not.

The port asks the question through `bindSourceFileAsExternalModule`
(`binder.go:2591`), which gives a module file a symbol on its `SourceFile` node
and gives a script none — so `binder.symbol_of(file).is_some()` **is**
`IsExternalOrCommonJSModule`, and no second module-indicator scan was written.

#### `isUse` — the second measurement, +14 for −2 wrong

`getResolvedSymbol` (`checker.go:13896`) resolves with
`isUse: !ast.IsWriteOnlyAccess(node)`, and `resolveNameHelper`
(`nameresolver.go:314`) marks **only when `isUse`**. So `y = 1` is not a
reference to `y`. `unusedLocalsInMethod3` is the whole argument in three lines:

```ts
var x, y;
y = 1;
```

upstream reports **TS6199 `All variables are unused`** — `y` included. Without
the gate this port marked `y`, fell out of the all-unused grouping, and reported
TS6133 on `x` alone: a wrong code at a wrong position from one missing predicate.
`accessKind` (`ast.go:1426`) is ported arm for arm rather than approximated,
because its `PropertyAssignment` arm *reverses* the outer kind — `({ x: y } =
obj)` reads `x` and writes `y` — and no approximation gets that from the shape.

**This is the one place the module marks less than the naive reading**, and it is
therefore the one place it can produce a wrong diagnostic. It is here because
upstream says so, and it was measured before it was kept: +14 converts, −2 wrong.

#### The residual 11, and why it is not tightened

Nine of the eleven are **two** cases, `conformance/inlineJsxAndJsxFragPragma` and
`compiler/jsxFragmentFactoryNoUnusedLocals`, and one cause: a `/** @jsx h */`
pragma makes `h` the JSX factory, which is a reference upstream resolves through
`checkJsxOpeningLikeElement`. This port records no pragmas, so the import reads
unused. **Filed rather than approximated** — the fix is a parser-side pragma
table, not a heuristic over comment text, and two cases do not buy a heuristic.
The other two are `typeGuardNarrowsIndexedAccessOfKnownProperty9`, a private
member reached through a shape the by-name marking does not see.

---

## 16. TS2322 — the board's top row, opened at 15% of it

ADR-0040 decision (3), and the row every handoff since the eighth session has
named as the largest single thing on the `diagnostics` board: **544 cases blocked
on TS2322 and nothing else.** This build takes 29 of them, and the interesting
product is not the 29 — it is the **five measured declines** that got the wrong
column from 988 lines to 28 without losing a single case.

### The comparison does not include the message

`diagnostics` compares `(file, line, column, code)`. TS2322's two type arguments
— the whole of `Type 'X' is not assignable to type 'Y'` — are **not compared**.
That detaches this rule from type printing entirely, which is why a row that
looks like it needs the whole checker needs only the *predicate* and the
*position*.

### The five declines, each with the number that bought it

| # | build | CONVERTS | LOST | RIGHT | WRONG |
|---|---|---:|---:|---:|---:|
| 0 | assignment arm, gated on `error`/`any`/`unknown` only | 100 | **29** | 947 | **988** |
| 1 | + primitives-only gate | 21 | 5 | 191 | 172 |
| 2 | + declared type, `const`, multi-declaration, access targets | 19 | 1 | 168 | 40 |
| 3 | + auto-typed and enum declines | 19 | **0** | 149 | 20 |
| 4 | + the variable-declaration anchor | 32 | 0 | 208 | 44 |
| 5 | + the unnarrowed-reference decline | **29** | **0** | 186 | **28** |

**Build 0 is the number this section exists for.** Gated on nothing but the
error type, this port's relation disagreed with upstream *almost exactly half the
time* — 947 right against 988 wrong. That is `checker_types`' 26% non-gradient
arriving as diagnostics, and it is the direct measurement of a thing the project
had only ever asserted: **an incomplete relation is not a relation that reports
less, it is one that reports wrongly.** `arr_i1 = arr_c1` where `C1 implements
I1` is assignable upstream and not here, and every structural row behaves the
same way.

1. **Primitives only.** The gate admits a type whose assignability is settled by
   its flags — primitive, literal, or a union of those — and nothing that needs a
   members table, a signature list or an instantiation. That is the part of the
   relation this port has finished, and confining the rule to it is what turned
   988 wrong lines into 172.
2. **The declared type, not the flow type.** `checkIdentifier`
   (`checker.go:11109`) returns early with the declared type at a definite
   assignment target. Reading the flow type instead was 16 wrong lines in
   `controlFlowNoImplicitAny` alone: `let x;` narrows to `undefined` before its
   first assignment, so `x = 1` read as *number not assignable to undefined*.
3. **`const`, accessors, multi-declaration.** Assigning to a `const` is TS2588 —
   a *different code at the same position*, which fails the case either way. A
   `set` accessor whose parameter type differs from its getter's return type
   makes the write type the setter's (`divergentAccessorsTypes2`), so property
   and element access targets are declined whole. Two declarations of one name
   merge their types and this port's merge is not upstream's
   (`duplicateLocalVariable1`).
4. **Auto-typed declarations and enums.** `let x;` *and* `let x = undefined;`
   both get upstream's auto type, which `convertAutoToAny` makes `any`; this port
   answers `undefined`. That is the same divergence
   `checker-notes-narrow.md` §9.1 measured from the `.types` side and **refused
   there** — so it is declined here rather than worked around, and the refusal
   keeps one owner. Enums were 11 wrong lines and the build's only remaining
   loss: `isTypeRelatedTo`'s enum arms and `numberAssignableToEnum`'s numeric
   widening are unported.
5. **The unnarrowed reference.** Narrowing only ever applies to a *reference*, so
   a union still standing at a reference in an assignment position is the exact
   shape in which an unported narrowing mechanism shows up. `controlFlowAliasing`
   (13 lines, aliased conditional expressions) and `inferTypePredicates` (5,
   inferred type predicates) are the whole family. Restricting the decline to
   references rather than to every union source is worth **7 conversions** —
   `var x: number = f()` returning a union is a real error and no narrowing was
   ever going to touch it.

### The bar

| leg | registered | measured | |
|---|---|---:|---|
| 1 | `LOST == 0` | **0** | pass |
| 2 | `checker_types` byte-identical | **2,963 / 31.07% / 74.19%** | pass |
| 3 | own new WRONG ≤ 40 | **28** | pass |
| 4 | CONVERTS ≥ 20 | **29** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

**`diagnostics` 832/5,488 = 15.16% → 861/5,488 = 15.69%.**

### What is left on the row, priced

- **9 of the 28 residual wrong lines are `@ts-expect-error` / `@ts-ignore`
  suppression**, which is a *program-level* filter
  (`compiler/program.go:1386`, `getDiagnosticsWithPrecedingDirectives`) and not
  this rule's business at all. It is the next build, because it pays for every
  rule at once rather than for this one.
- **TS2741 / TS2739 / TS2740.** When the relation fails because the source is
  *missing properties* of the target, upstream reports one of those instead
  (`assignmentCompat1`). TS2741 alone is 38 cases on the board. That is the
  natural next slice, and it is gated on the same members table the primitives
  gate is declining today.
- **The other anchors.** `checkReturnStatement` reports at the *return statement*
  (`arrayAssignmentTest1.ts(6,16)` is the `return`, not the expression), and
  parameter defaults, property declarations and object-literal members each have
  one. Every anchor is worth its own measurement against this gate.

---

## 17. `@ts-ignore` / `@ts-expect-error` — the one build that pays for every rule

`getDiagnosticsWithPrecedingDirectives` (`internal/compiler/program.go:1386`) is
**not a checker rule**. It runs after the binder and the checker have both had
their say, filters diagnostics of every code from every producer, and is
therefore the only place in this port where one build pays for all of them at
once. §16 found it as 9 of its 28 residual wrong lines and correctly refused to
handle it locally.

```
diagnostics 861 -> 863   (+2)
```

**+2 is the honest number and it understates the build**, which is why the row is
here rather than folded into §16: the filter removes a *class* of wrong line from
every rule that exists and every rule that follows. Nine of §16's wrong lines
were this; nothing else in the suite reports on those two cases yet, so only two
cases finished.

### Two halves, and the second is not optional

1. A diagnostic whose line is preceded — across blank and comment lines **only** —
   by a directive line is dropped, and the directive is marked used.
2. Every `@ts-expect-error` still unused afterwards becomes **TS2578**
   `Unused '@ts-expect-error' directive.` at the directive's own position
   (`program.go:1377`).

Porting only the first half trades one wrong diagnostic for one missing one on
every case that writes a directive it does not need, and
`conformance/ts-expect-error` is exactly such a case. The backward scan's stop
condition — *"stop when you reach a line that is neither blank nor a comment"* —
is equally load-bearing: without it one directive at the top of a file silences
the whole file, which is a test in `crate::comment_directives`.

### Why it walks tokens rather than the text

Upstream's *scanner* records directives as it scans (`scanner.go:1003`), so a
`// @ts-ignore` inside a string literal is never a directive. This port's scanner
does not record them, and the obvious substitute — searching the text for the
marker — finds exactly those. The directives are recovered instead by walking the
token stream and asking `leading_comment_ranges` for the trivia in front of each
token: the same comments the scanner saw, reached from outside it. The first test
in the module is a string literal containing `// @ts-ignore`.

### Where it lives, and why not in the checker

`crates/tsr-conformance/src/comment_directives.rs`, called from
`diagnostics_suite::reported_for` — this port's program layer for the suite. It
needs the file *text*, applies to parser, binder and checker diagnostics
together, and belongs to whatever assembles a program's diagnostics. Putting it
in `tsr-checker` would give the checker a filter over diagnostics it did not
produce.

### 16.1 The third anchor — `checkReturnStatement`, +6 for −1 wrong

```
CONVERTS 29 -> 35   LOST 0   RIGHT 186 -> 215   WRONG 28 -> 27
```

The error node is the **return statement**, which `arrayAssignmentTest1.ts(6,16)`
pins: `IM1():void[] {return null;}` reports at column 16, the `r` of `return`,
not at the `null`. Only a **written** annotation is used — an inferred return
type is computed from the very returns being checked, so a mismatch against it is
not something upstream can report.

Two declines went in with the anchor rather than after a measurement, because
both are structural rather than statistical: an **async** function's annotation
is a `Promise<T>` and the returned value is compared against the unwrapped `T`
(`checker.go:12420`), and a **generator**'s is an `Iterator<…>`. Comparing against
the wrapper would be a wrong diagnostic on every correct `async` function in the
corpus, which is the largest single wrong family this rule could have produced.

`diagnostics` 863/5,488 → **868/5,488 = 15.82%**.

---

## 18. `noImplicitAny` for parameters — TS7006 and TS7019, +7 for **zero** wrong

```
CONVERTS 868 -> 875  (+7)     LOST 0      RIGHT 128     WRONG 0
```

The third option-gated rule, and the same safety profile as §15: nothing fires
unless the case writes `@noImplicitAny` or `@strict`, so a wrong answer can only
reach cases that opted in.

### A widening rule upstream, a syntactic one here

`reportImplicitAny` (`checker.go:18275`) is reached from
`reportErrorsFromWidening` (`checker.go:20452`) when a declaration's widened type
still carries `ObjectFlagsContainsWideningType`. This port has no widening
marker, so the question is asked of the syntax instead — **and it can only be
asked where the answer cannot depend on a contextual type**, because contextual
typing is the one thing that supplies a parameter's type without an annotation
and this port's version of it is the refused `ArrowFunction` row.

Admitted: a `function` declaration, a class method (not an object-literal one),
a constructor, and a function expression or arrow in exactly two positions — the
initialiser of a variable with **no** annotation, and a bare expression
statement. Everything else — an argument, an annotated declaration, a property
assignment, a `return`, a JSX attribute, an `as` — has a contextual type this
port computes only partially, and partial contextual typing is a *wrong* TS7006
rather than a missing one.

Widening from declarations-only to those two expression positions was worth
**+3 conversions and +21 right lines for zero new wrong**, which is the evidence
that the position list is the right axis to grow along.

### The wrong column was 28 lines and one word: JSDoc

The first measurement read **28 wrong, and every one was a `.js` file**.
`typedefOnStatements.js(71,…)` alone was 15 of them, on one line. Upstream reads
`@param {string} x` out of JSDoc and gives the parameter a type; this port parses
no JSDoc types, so a checked JS file reports an implicit any on every *annotated*
parameter it has.

`reportImplicitAny` already declines a `.js` file without `checkJs`
(`checker.go:18276`); this declines **every** `.js` file, which is wider than
upstream and is a refusal rather than a bound — it comes back when JSDoc types
are parsed. **The whole wrong column went to zero on one condition**, which is
the cleanest signal in this file that a residual had a single owner.

### 16.2 Two more anchors and two corrections — +5 right, −8 wrong, +0 cases

```
CONVERTS 35 (unchanged)   LOST 0   RIGHT 214 -> 219   WRONG 27 -> 24
```

**A build with a zero in the case column, kept anyway**, and the reason is worth
stating: `diagnostics` counts *cases*, and a case fails on the first difference
in either direction. A wrong line removed from a case that still fails for six
other reasons moves no number today and moves one the moment those six land.
Recording only the case delta would file this as worthless.

- **Property declarations and parameter defaults.** The same shape as the
  variable arm — a written annotation and an initialiser, reported at the
  declaration. An **optional** parameter is declined: its default is compared
  against the type with `undefined` stripped (`checker.go:9993`) and this port
  does not strip it.
- **`.js` files, for the three declaration anchors only.** A JS declaration's
  type comes from JSDoc, which this port does not parse — `typeFromJSInitializer4`
  was 5 wrong lines and every one was that. Declining `.js` for the *assignment*
  arm as well removes 5 more wrong lines and **costs 4 conversions**
  (`checkJsFiles1`–`4`), so the decline is drawn at the declaration anchors and
  the assignment arm keeps JS. That asymmetry is measured, not principled, and it
  disappears when JSDoc types are parsed.

#### The comment-directive correction: `lastLineStart`, not the `/*`

§17 shipped with three wrong lines left in `conformance/ts-expect-error`, and the
cause was one argument. `processCommentDirective` (`scanner.go:674`) is called
with **`lastLineStart`** for a block comment — the start of the comment's *last*
line — not with the `/*`:

```text
/*
 @ts-expect-error */
var x: number = 'nope';
```

records the directive on line 2, which is what lets the backward scan reach it
from line 3. Reading the `/*` as the position puts it on line 1, and the scan
stops at line 2 because a line beginning `@ts-expect-error` is neither blank nor
a comment. The same argument fixes the *reported* position:
`ts-expect-error.ts(11,1)` puts TS2578 at column 1 for a one-line block comment
whose `@` sits at column 4.

Second correction in the same place: `isCommentOrBlankLine` (`program.go:1445`)
recognises `//` and **not** `/*`. §17's version accepted both, so its backward
scan ran past lines upstream stops at. Both are now tests.

---

## 19. TS2314 / TS2707 — type-argument arity, +12 for **one** wrong line

```
CONVERTS 875 -> 887  (+12)     LOST 0      RIGHT 151     WRONG 1
```

The best ratio in this file after the grammar rules of §10–§12, and for the same
reason: **it reports on a syntactic fact.**

Upstream asks the *declared type* for its local type parameters
(`getTypeFromClassOrInterfaceReference`, `checker.go:23169`). But everything the
decision needs is on the **declaration**: a class or interface writes its type
parameters syntactically, and `getMinTypeArgumentCount` is *"the index of the
first parameter carrying a default"* — also syntactic. So the rule resolves the
name, reads the declaration's list, and counts. No type is computed, and the
§16 problem — an incomplete relation reporting wrongly — cannot arise.

This is §14's ordering rule holding for a *third* kind of rule. It is not that
grammar rules are cheap; it is that **a rule whose predicate is a syntactic fact
has no incompleteness to leak**, and a type-shaped question can still turn out to
be one.

### The declines, all four structural

| decline | why |
|---|---|
| a `.js` file | upstream substitutes `Expected_0_type_arguments_provide_these_with_an_extends_tag` for a missing JSDoc `@augments` (`checker.go:23181`), and with `noImplicitAny` off reports nothing at all |
| a type **alias** or a type parameter | their arity errors are TS2315 / TS2558 from a different function |
| a symbol whose declarations are not all class-or-interface, or whose lists disagree | upstream reads the parameters off the *merged* symbol, and this port's merge is not upstream's |
| an `ExpressionWithTypeArguments` outside a heritage clause | that syntax is also an **instantiation expression** (`f<number>`), a value position with nothing to do with this rule |

The single residual wrong line is `genericTypeReferenceWithoutTypeArgument2`.

---

## 20. TS2554 / TS2555 — call arity, +6 for **zero** wrong

```
CONVERTS 887 -> 893  (+6)     LOST 0      RIGHT 42      WRONG 0
```

§19's trick again: `getMinArgumentCount` and `getParameterCount` are asked of a
`Signature` upstream, but both are properties of the **parameter list** — the
minimum is the index of the first parameter that is optional, defaulted or rest,
and the maximum is the list's length. So the rule reads the declaration.

What cannot be read syntactically is *which* signature a call resolves to, so it
is confined to a callee naming a symbol with exactly one function declaration
**with a body**. An overload set is `checker-notes-callres.md`'s row.

### The 24-line wrong column was three facts, and the first is the whole rule

The first measurement read **2 converts against 24 wrong**. Sixteen of the 24
were one line of upstream:

```go
case len(args) < minCount:
    // too short: put the error span on the call expression, not any of the args
    NewDiagnosticForNode(errorNode, ...)          // checker.go:9770
default:
    pos := args[maxCount].Pos()                   // checker.go:9804
```

**The error node is not the same for the two directions.** Too few arguments
reports on the callee; too many reports on the *first excess argument*.
`functionCall6.ts` records both in one file — `(5,1)` on the `foo` of `foo()`,
and `(4,12)` on the `'bar'` of `foo('foo', 'bar')`. A rule that reports both at
the callee gets the too-few half right, which is exactly enough to look correct
while being wrong on the other half.

The other two, each with its case:

- **A `void` parameter may be omitted.** `getMinArgumentCountEx`
  (`relater.go:1737`) walks back from the minimum and drops every trailing
  parameter whose type contains `void`. `conformance/callWithMissingVoid` is 4
  lines of nothing else. Asked here of the *annotation* rather than of the type,
  which covers `void` and `T | void` and declines an alias or an instantiated
  type parameter — and a decline **raises** the minimum, so the shapes it does
  not cover are named rather than assumed.
- **A written type-argument list is declined.** `f < A, B > 7` is a call here and
  a comparison chain upstream (`conformance/grammarAmbiguities`), and a generic
  call's resolution is not the one this rule models. Declining it converted that
  case rather than merely silencing it.

After the three: **zero wrong lines over 42 right ones.**

---

## 21. TS2339 — the §9 refusal RETIRED, on the condition §9 named

```
CONVERTS 893 -> 904  (+11)     LOST 0      RIGHT 62      WRONG 11
```

§9 built this rule, measured **2 conversions against 254 wrong lines**, and
refused it. The refusal was right and it carried its own retirement condition:

> An absent property and an unbuilt members table are the same `None`, and no
> predicate over the *type* separates them. […] What would make this win: a
> members table that knows whether it is **complete**.

**11 conversions for 11 wrong lines** — 1.00 gained per wrong, against §9's
0.008. Two orders of magnitude, and the difference is one idea.

### The idea: ask the walk, not the type

`Named { members: Some(_) }` is a flag, and §9 is correct that no flag can
answer this. [`crate::member_completeness`] answers it by **re-walking the graph
`get_property_of_type` walks** and returning `false` the moment the walk reaches
anything this port resolves lazily, partially, or not at all. Completeness is a
property of the traversal that produced the answer, not of the type the answer
came from — which is precisely what §9 discovered and did not act on.

The conditions are not a heuristic. They are §9's own wrong column read back:

| answers `false` | §9's residual case |
|---|---|
| an instantiated reference, or an owner declaring type parameters | `longObjectInstantiationChain1`/`3` (13 each), `genericDefaults` (9) |
| a base `base_symbols_of` cannot follow | `mixinAccessModifiers` (9), `classExtendingClassLikeType` |
| a declaration that is not a class or interface | `conditionalTypes1` (8), `mappedTypes6` |
| an index signature on the declaration | every receiver where all names are legal |
| a member with a computed name | late binding, unported |
| a revisited symbol | `recursiveIntersectionTypes` |

`base_symbols_of`'s existing contract — *"any base that cannot be followed makes
the whole lookup a miss"* — is the load-bearing half, and it was already written
and already documented. **The subsystem §9 said this needed turned out to be
half-built, in a function whose doc comment had said so for two sessions.**

### The second gate: the type can be complete and still be the wrong type

Completeness is about the *table*. It says nothing about whether the receiver's
type is the one upstream computed, and the first measurement's **7 losses** were
all that second question:

- **`Object` and `Function`.** `addInheritedMembers` layers the global `Object`'s
  members under every object type, and `Function`'s under anything with a call
  signature. `i.toString()` is legal on an interface that declares no
  `toString`. Seven losses — `objectMembersOnTypes`,
  `classAppearsToHaveMembersOfObject`, `objectTypePropertyAccess`,
  `fluentClasses`, three `objectTypeWith*Signature*` — and **losses, not merely
  wrong lines**, because those cases pass today.
- **Library receivers.** TS2550 and TS2812 replace TS2339 for a lib type, and
  neither is modellable without a lib-version table. `Checker::set_checked_files`
  (new) gives the checker the program's *own* file set, so "declared in a
  library" is answerable at all — the libs are in the program and are never
  walked. Eight lines across three cases.
- **Narrowed and inferred receivers.** A call receiver (`fluentClasses`'
  polymorphic `this`), a dotted name (`narrowingOfDottedNames`), and an
  identifier whose flow type differs from its declared type
  (`controlFlowInstanceof`, `typePredicateInLoop`) are all declined. `this` is
  **not** — `thisBinding` and `statics` are conversions.

### The residual 11, priced

Five are the *opposite* narrowing failure and cannot be gated by comparing
declared against flow type: upstream narrows `target` to a subclass and finds
the member, this port does not narrow and reports. The gate catches "we narrowed
and upstream did not"; there is no cheap predicate for "upstream narrowed and we
did not" short of the narrowing itself. Three are `missingDomElements`, whose
receiver is a locally declared `interface Element` that upstream recognises as a
DOM name. Three are class-side and protected-member shapes.

---

## 22. TS2741 — built on §21's machinery, measured, REFUSED at 1 conversion for 8 wrong

```
CONVERTS +1     LOST 0     RIGHT 41     WRONG 8     STILL SHORT 15
```

**0.125 conversions per wrong line, against a project refusal band of
0.47–1.03.** Refused, and the switch is kept as a named constant
(`REPORT_MISSING_REQUIRED_PROPERTY`) rather than deleted, because everything it
switches is correct and four named families stand between it and a positive
score.

### Why it was worth trying, and what the number actually says

TS2741 is 40 cases on the board and it is the row that *should* have fallen out
of §21: `reportUnmatchedProperty` (`relater.go:4345`) asks whether a required
property is **absent**, and that is a member-table question, not a relation one.
No relation runs, so §16's *"an incomplete relation reports wrongly"* does not
apply — and indeed **41 right lines against 8 wrong** is a far better *line*
ratio than §16's first build managed.

The **case** ratio is what refuses it. `STILL SHORT 15` is the explanation: of
the row's 40 cases, fifteen gain a correct TS2741 and still fail, because a case
that assigns incompatible object types usually does so several times and in
several ways. TS2741 alone finishes one of them.

> **A row's population is a ceiling for the code, not for the rule.** `diaggap.rs`
> counts cases blocked on TS2741 *and nothing else*; a case can be blocked on
> TS2741 alone and still need three of them, of which this arm emits one.
> This is the fourth time this file has had to separate the two, and the first
> time the gap was this wide.

### The residual 8, each with an owner

| lines | case | owner |
|---:|---|---|
| 2 | `assignmentCompatWithObjectMembersStringNumericNames` | `{ 1: x }` and `{ "1": x }` are one member upstream and two here — `tsr-binder`'s module header names it: *"a numeric name must be built rather than sliced"* |
| 2 | `inheritance1` | an inherited member reached through a base this walk follows and a modifier it does not read |
| 2 | `flowControlTypeGuardThenSwitch` | narrowing, the same family §21's third gate declines |
| 1 | `classImplementsClass4` | `implements` conformance, a different diagnostic |
| 1 | `privateNamesUnique-4` | private-identifier members, whose table key upstream mangles (`GetSymbolNameForPrivateIdentifier`) |

### The trap this build paid for, and it is the third of its kind

`SymbolFlags::OPTIONAL` is declared in `tsr-binder` and **set by nothing**. The
first measurement reported TS2741 for every *optional* property of every target —
`assignmentCompatWithObjectMembersOptionality2` was three lines on one case —
because `entry.flags.intersects(OPTIONAL)` is `false` for `x?: T`.

That is `NodeFlags::AMBIENT` (eighth session, three residuals) and
`NodeFlags::JAVASCRIPT_FILE` (ninth session, one build's worth) arriving a third
time, in a third crate, on a third flag. **A declared flag with no writer is a
landmine with one instance per reader**, and this project has now stepped on
three. Optionality is read off the declaration's `?` here; the flag itself needs
the binder.

---

## 23. TS2353 — excess properties, +6 for **zero** wrong

```
CONVERTS 904 -> 910  (+6)     LOST 0      RIGHT 9      WRONG 0
```

`hasExcessProperties` (`relater.go`), reached when a **fresh** object-literal
type is checked against a target. Error node the offending property name:
`arrayCast.ts(3,23)` is the `foo` of `{ foo: "s" }`.

### Freshness is a syntactic question at these anchors

Upstream carries freshness on the type. This port asks the syntax — *is the
expression written as an object literal right here* — and the two agree at every
anchor this module has, because none of them is a place a literal's type can
arrive already widened. That is why a rule that looks like it needs the relation
needs only [`crate::member_completeness`].

The check uses the **index-signature-fatal** predicate, not the property
enumeration TS2741 uses: an index signature on the target makes every name
known, so it must decline. §22's note about why those are two entry points and
not one flag is exactly this pair of callers.

### One decline, and it was the only loss

`class C {}` with `c = { foo: '' }` reads **TS2322** upstream, not TS2353
(`conformance/classWithEmptyBody`): the excess check only speaks when the rest of
the relation would have succeeded, and nothing about that literal is assignable
to an empty class. This port runs no relation here, so *"the target has no
properties at all"* stands in for that condition. With it: 0 wrong, 0 lost.

Reporting only the **first** excess property is upstream's own behaviour —
`hasExcessProperties` reports and returns — and under the exact-multiset rule
reporting all of them would fail the case as surely as reporting none.

### What the type-literal arm bought

`declared_property_table` originally admitted only classes and interfaces, and
answered `None` for `{ id: number }` — the single most common target shape in the
corpus's assignability cases. A type literal is an interface's member list
without the interface: no type parameters to instantiate, no `extends` to follow,
so the same member test settles it. Adding the arm also took §21's TS2339 from
11 conversions to **12** with no new wrong lines.

---

## 24. TS2403 — built, measured, REFUSED at 6 losses, and the reason is a missing relation

```
CONVERTS +12     LOST 6      RIGHT 25     WRONG 22
```

**Refused on the loss column alone**, which no bar in this file has ever
permitted, and the diagnosis is worth more than the row.

The predicate is `isTypeIdenticalTo` (`checker.go:5929`), and the build assumed
that **`TypeId` equality models it** — types are interned here, so two
declarations that mean the same type should be the same id. That assumption is
false, and the six losses are all one shape:

```ts
var o: {} = c;
…
var o: {} = d;      // conformance/classWithEmptyBody, instantiatedModule,
                    // typeAliases, the two TwoInternalModules* cases
```

Two `{}` type literals written in two places mint **two anonymous types**. They
are structurally identical and are not the same `TypeId`, so the rule reported
that a variable redeclared with the same annotation has a different type — on
six cases that pass today.

> **Interning gives identity for the types it interns, and `isTypeIdenticalTo` is
> a *relation*.** The two coincide for primitives, literals and named references
> and part company at the first structural type. A port that has interning and no
> identity relation has half of what the predicate needs, and the half it has is
> the half that never fires.

Tightening to *"every declaration carries a written annotation"* took the wrong
column from 102 lines to 22 and the losses only from 7 to 6 — the losses are the
annotated case. There is no tightening that reaches this, because the failure is
in the comparison rather than in the population.

**What would make this win**: `isTypeIdenticalTo` — the relation in identity
mode. `crate::relater` has `is_type_related_to` with assignability and subtype
relations and no identity one. That is a relater build, not a diagnostics one,
and it is worth more than this row: `checkTypeIdentical` sites also gate
TS2717 (subsequent property declarations), TS2320 (conflicting inherited types)
and the overload-identity checks — **TS2403 29 cases, TS2320 14, TS2717 and
TS2394 13 each** on the current board, ~70 cases behind one relation mode.

The code is removed rather than switched off (contrast §22's
`REPORT_MISSING_REQUIRED_PROPERTY`): §22's machinery is correct and waiting on
data, and this one is waiting on a function that does not exist.

---

## 25. TS2322 again — the gate was reading the wrong function, and §16's 988 was that

```
CONVERTS 35 -> 42   LOST 0   RIGHT 219 -> 294   WRONG 24 -> 50
diagnostics 910 -> 917
```

**§16's primitives-only gate is deleted**, and the reason is a one-line
correction that this file should have made in §16 and did not.

`crate::relater` is **three-valued**. `Ternary::NotRelated` means *"the relation
does not hold, and this port is entitled to say so"*; `Ternary::Unknown` means
*"this port cannot decide the pair"*. `relate_ternary`'s own doc comment names
the caller the distinction exists for:

> The intended caller is one that acts on a **negative** […] Such a caller must
> refuse an `Unknown` pair rather than treat it as a rejection.

**TS2322 is that caller, and §16 called `is_type_assignable_to` — the binary
projection, which collapses `Unknown` into `false`.** So §16 build 0's *947 right
against 988 wrong* was not "the relation disagrees with upstream half the time".
It was **every undecidable pair being reported as an error**, and the primitives
gate was a bound drawn around a defect rather than around the relation.

> This is the third time this project has drawn a bound around the wrong thing
> and measured it honestly: `checker-notes-selectable.md`'s `SELECTABLE` flag set
> ("the blocker was the gate, not the relation"), §9's `members: Some(_)`, and
> now this. In all three the numbers were right and the *premise under them* was
> not. The tell is the same each time: **a bound that has to model a subsystem's
> incompleteness is a bound in the wrong place**, because the subsystem already
> knows.

### What the gate is now

Three declines, and none of them is about the relation being incomplete:

- **`any`, `unknown`, the error type, and enums.** None can *fail* a relation
  except by an unported arm. `unknown` alone was 25 wrong lines in
  `conformance/unknownType2`, the largest family after the switch.
- **An object literal against a union target**, asked of the *syntax* because the
  literal's type is synthesised. That is
  `getMatchingUnionConstituentForObjectLiteral` and `findMatchingDiscriminantType`,
  and upstream reports TS2353 / TS2561 / TS2739 there — 31 lines across six cases.
- **The unnarrowed reference**, unchanged from §16's fifth decline.

Everything §16 declined for being structural — classes, interfaces, type
literals, arrays, signatures — is now admitted, and the relater says `Unknown`
for the parts of itself it has not finished. **The correct bound was already
written, inside the function the rule was calling the wrong version of.**

### The residual 50

Diffuse: nothing above 5 lines per case. `assignmentCompatWithDiscriminatedUnion`
5, `typeFromJSInitializer4` 4 (JSDoc — declining `.js` here costs 4 conversions
for 5 wrong lines and is therefore not drawn), `controlFlowNoImplicitAny` 4,
`widenedTypes` 3, `tryCatchFinallyControlFlow` 3, and a tail of 2s.

---

## 26. TS2345 — argument assignability, +11 for 8 wrong

```
CONVERTS 917 -> 928  (+11)     LOST 0      RIGHT 20      WRONG 8
```

The board's third row (114 cases), opened with **no new machinery at all**: the
signature comes from §20's `sole_signature_arity` gate — one callee, one
non-generic function declaration with a body — and the verdict comes from §25's
`relate_ternary`. Error node the **argument**:
`arrayAssignmentTest3.ts(12,16)` is the `null` of `new a(null, 7, …)`.

Two declines the arity rule did not need:

- **A generic callee, whole.** Its parameter types are written in terms of type
  parameters that inference substitutes, and inference is
  `checker-notes-infer2.md`'s refused row. Comparing an argument against an
  uninstantiated `T` is a confident wrong answer, not a gap.
- **Everything at or after a rest parameter.** A rest parameter's annotation is
  the *array*, so position `i` stops naming parameter `i`.

That two rules this far apart — an arity check and a type check — share one gate
and one verdict function is the shape §25's correction bought: once the relation
answers *"cannot decide"* honestly, a consumer needs no bound of its own beyond
the ones about its **inputs**.

---

## 27. TS2415 / TS2420 / TS2430 — heritage conformance, +19 for 21 wrong

```
CONVERTS 928 -> 947  (+19)     LOST 0      RIGHT 54      WRONG 21
```

`checkClassDeclaration`'s `implements` loop and `checkInterfaceDeclaration`'s
`extends` loop, both of which run `checkTypeAssignableTo(typeWithThis,
baseWithThis, node.Name())`. Error node the **name**:
`declareClassInterfaceImplementation.ts(5,15)` is the `Buffer`.

**No new machinery.** The declared type of a class or interface has existed since
`crate::declared`; the verdict is §25's `relate_ternary`. What was missing until
§25 is the reason these two rows had never been attempted: a conformance check is
the purest consumer that acts on a negative — a class satisfying its interface is
the *normal* case, so under the binary relation **every undecidable pair would
have been an error on correct code**.

One decline was worth 4 of the first measurement's 20 wrong lines: a **merged**
declaration — two `interface I` bodies, or an interface merged with a class —
assembles its member table from several declarations, and upstream's merge is not
this port's for private and inherited members
(`mergedInterfacesWithInheritedPrivates3`,
`implementingAnInterfaceExtendingClassWithPrivates2`,
`interfacePropertiesWithSameName3`, `interfaceDeclaration3`).

The decline applies to the **base** as well as the source, and for the same
reason: a target whose members come from several declarations is a table this
port assembles differently from upstream.

**Three (declaration, keyword) pairs, three codes, one function.** A class's
`extends` is TS2415, its `implements` TS2420, an interface's `extends` TS2430;
`checkClassDeclaration` and `checkInterfaceDeclaration` run the same
`checkTypeAssignableTo(typeWithThis, baseWithThis, node.Name())` at all three and
differ only in the message. Adding the class-`extends` arm to the two that were
already there was **+10 conversions for +5 wrong lines** — the cheapest edit in
this file since §12.

Ratio 0.90, near the top of the project's 0.47–1.03 refusal band.

### 27.1 TS2416 — the per-property override check, shipped at a measured **zero**

```
CONVERTS +0     LOST 0     RIGHT 4     WRONG 0
```

`checkKindsOfPropertyMemberOverrides` compares each **own** member of a derived
class against the base's member of the same name; error node the member's own
name, and it is reported *beside* TS2415 rather than instead of it — the
`baseClassImprovedMismatchErrors` baseline records both codes for one class at
different positions.

**Shipped with zero conversions, and recorded as such.** Its gate is the tightest
in this file — a single plain non-generic base, a single declaration on both
sides — and the row's 18 cases are almost all generic or interface-based, which
the gate excludes. Four right lines, no wrong ones, two cases moved closer.

It is kept rather than reverted because the *rule* is right and only the gate is
narrow: widening it is a one-line change once generic bases can be instantiated,
and the alternative — deleting a correct rule because its gate has not caught up —
is how a port loses work it has already done. The zero is here so nobody prices
it as a conversion later.

---

## 28. TS2411 — the index constraint, +2 for 1 wrong

```
CONVERTS 947 -> 949  (+2)     LOST 0      RIGHT 15      WRONG 1
```

`checkIndexConstraints` (`checker.go`): every **named** property of a type
carrying an index signature must be assignable to that signature's type. Error
node the property's own name — `classIndexer3.ts(9,5)` is the `y` of
`y: string;` in a class extending one that declares `[s: string]: number`.

The signature is found by **walking**, not by asking the type, and that is the
whole of what made the row reachable: `classIndexer3`'s constraining signature is
on the *base* and the offending property is on the derived class. This port has
no resolved index-signature table, so the walk mirrors `crate::members`' — own
declarations, then the `extends` chain, with `base_symbols_of`'s *"any base I
cannot follow makes the whole answer a miss"* contract doing the safety work for
the third rule in a row.

A **numeric** index signature constrains only numerically-named properties
(`isNumericLiteralName`), which is what keeps `y: string` legal beside
`[n: number]: number`; a template-literal or union index parameter is
`getIndexInfosOfType` and is declined.

---

## 29. One definite negative the relater declines to give — +12 for +10 wrong

```
TS2322  CONVERTS 42 -> 54     LOST 0     RIGHT 305 -> 337     WRONG 50 -> 60
diagnostics 949 -> 961
```

`is_related_to` answers `Unknown` for a pair whose source is an object type with
**no members table** — a function type, an index-signature-only type — because
its structural arm was never reached. That is row 3 of
`checker-notes-assign.md` §2 and it is correct *as a statement about the
structural comparison*.

But nothing structured is assignable to `string`, and no members table is needed
to know it. `aliasDoesNotDuplicateSignatures` is the case that surfaced it:

```ts
let x1: string = demoNS.f;   // TS2322: Type '() => void' is not assignable to type 'string'.
```

`demoNS.f` is a function type, so the relater declined, so the rule was silent —
on a diagnostic whose whole content is that a function is not a string.

**Asked in the rule, not added to the relater**, and that placement is the
decision: `crate::relater` is read by overload selection and by narrowing, and
widening what *it* calls a definite negative moves `checker_types`. As a
rule-local predicate it moves nothing else, and it is shared by TS2322 and
TS2345 through the one function both already call.

**One direction only.** The converse is false — `let x: {} = 5` is legal, because
an object *target* can be satisfied by a primitive through its apparent type. And
`void`, `null` and `undefined` are absent from the target list: their relation to
an object source depends on `strictNullChecks`, and the list may hold only pairs
that are unrelated under every configuration.

### 29.1 The object-literal member anchor, and the ratio that prices the row

```
TS2322 + TS2345   CONVERTS 63 (unchanged)   LOST 0   RIGHT 384 -> 392   WRONG 71 -> 73
```

A known property of an object literal is checked against the target's member of
the same name, reported at the **property name**:
`everyTypeWithAnnotationAndInvalidInitializer.ts(43,28)` is the `id` of
`var anObjectLiteral: I = { id: 'a string' }`. It shares the walk
`check_excess_properties` already makes — an unknown name is TS2353, a known one
is this.

**Eight more right lines, zero more cases**, and that is the number this section
exists to record. Across §25–§29 the TS2322 family emits **392 correct
diagnostics and finishes 63 cases** — roughly **six right lines per case**. The
board says 496 cases are blocked on TS2322 *alone*; at six lines each that row is
~3,000 correct diagnostics away, which is not a set of anchors, it is complete
assignability.

> **`diaggap.rs`'s single-code column names the code a case is blocked on. It
> does not say how many of that code the case needs, and for the assignability
> family the answer is about six.** Every forecast this project has made off that
> column — including the ones in §16 and §21 — has been high by that factor. The
> column is still the right *ordering*; it is not a case count.

---

## 30. `extragap.rs` — the column nobody had opened, and TS2304's residual

`diaggap.rs` ranks the *missing* column and prints one line per code for the
extras. That was never enough to act on: **a code appearing in both columns of
one case is usually one diagnostic at the wrong position**, not one invented and
one missed, and the two readings call for completely different work.

`examples/extragap.rs` (new) splits them — *displaced* (the same code is missing
elsewhere in the same file) versus *invented* (the baseline never mentions it for
that file) — and adds the only forecastable column on this side: **cases where
one code, got entirely right in both directions, would finish the case.**

```
  code   displaced   invented   sole obstacle
TS2322          42         23           489
TS2339           0         11           132
TS2345           0          8           104
TS2304           1         85            88
TS2454           3        111            66
TS2300          47         47            38
TS2430           1          6            24
TS2420           2          5            16
TS1160          12          0            12
```

**TS2304 and TS2454 are *invented*, not missing.** Both rules shipped in the
eighth session, and 85 and 111 of their lines respectively are diagnostics
upstream does not report at all. That reframes two rows that had been read as
"the rule is incomplete" for two sessions: they are over-reporting, and the fix
is subtraction.

### TS2304's residual, 86 wrong lines → 68, and two of the three are substitutions

```
CONVERTS 127 -> 130     LOST 1 (unchanged, pre-existing)     WRONG 86 -> 68
diagnostics 961 -> 964
```

- **TS2583, the missing-lib arm** (18 lines). `onFailedToResolveSymbol` reports
  the missing lib **before** the spelling suggestion (`checker.go:1584` versus
  `:1590`), so `Map`, `Set`, `WeakMap` and 51 more names are TS2583 whenever they
  do not resolve. `getFeatureMap`'s *keys* are the whole of what that needs;
  ported as a sorted name→lib list rather than as the nested map, because
  everything else in it belongs to TS2550's row. The ordering is load-bearing:
  `Map` has near neighbours in most scopes, so the two arms do not commute.
- **TS2301, the constructor-parameter arm** (4 lines, and it converts its case).
  An instance property's initialiser naming a **constructor parameter** is
  `OnPropertyWithInvalidInitializer`. This port's `resolve_name` does not put
  constructor parameters in a property initialiser's scope at all, so the
  substitution is made by asking the class directly.
- **`class C extends null`** (5 lines). Upstream's parser makes `null` a
  `NullKeyword`; this one makes it an `Identifier`, so the name reaches a
  resolver that can never find it. Worked around at the reader rather than in the
  parser, because `null` is not a spellable binding in any scope — declining it
  can hide no real diagnostic.

### 30.1 TS2454's residual, and TS1160 — the cheapest twelve cases in this file

**TS2454, −59 wrong lines for 0 conversions and 0 losses.** A reference
*guarded by a condition that names it* is one upstream has already narrowed:
`typeGuardOfFormIsType`'s `isC1(c1Orc2) && c1Orc2.p1` reads as *used before being
assigned* here because the user-defined predicate that removes `undefined`
upstream is unported. Three guard shapes — the right operand of `&&`/`||`/`??`,
a conditional's branches, the body of an `if`/`while`/`do` — all syntactic, all
over-approximating in the direction this rule may fail in. Zero cases finished
today; sixty cases now have one fewer obstacle.

**TS1160 — twelve cases for one argument.**

```
diagnostics 964 -> 976   (+12)     every other suite byte-identical
```

`s.error` (`scanner.go:413`) is `s.errorAt(diagnostic, s.pos, 0)` — it reports at
the position the scanner has **reached**, with length zero. This port's
unterminated-template arm reported `Span::new(token_start, pos)`.
`templateStringUnterminated1.ts` is a file containing one backtick and upstream's
caret is at column **2**; this port put it at column 1, in all twelve
unterminated-template cases in the corpus, and TS1160 was the *only* difference
in every one of them.

> **`extragap.rs`'s `displaced` column found this and nothing else could have.**
> TS1160 read `12 displaced, 0 invented, 12 sole obstacle` — a code that is
> simultaneously the most-missing and the most-extra in the same twelve files is
> not a missing rule, it is one diagnostic in the wrong place. `diaggap.rs` had
> been printing "TS1160 12 cases" in its false-positive list for two sessions
> with no way to tell those apart.

The same argument applies at `scanner.go:1613` and `:1630` —
**unterminated string literals**, two more sites reporting from `token_start`.
Fixed together: **+1 case** on `diagnostics` (977) and **+5 lines** on
`checker_types` (3,074 / 74.66%), which is the first time a `diagnostics` build
has moved the other workstream's number. Every parser and scanner suite is
byte-identical: `parser_typescript` 5,001/5,031, `scanner_clean_files` 5,031.

### 30.2 TS2300 — the second position bug, and the same instrument found it

```
diagnostics 977 -> 988   (+11)     binder_symbols 8,293/8,460 unchanged
```

`extragap.rs` read TS2300 as **47 displaced / 47 invented / 38 sole obstacle**.
The displaced half was one omission: `declare` reaches `declare_into` through
**eight** call sites — the module-member path among them — and only *one*
recorded the declaration's name node. So a duplicate `class` inside a `namespace`
reported on the `class` keyword while the same class at file scope reported on
its name:

```
genericClassesRedeclaration.ts(16,11)   upstream — the name
genericClassesRedeclaration.ts(16,5)    this port — the keyword
```

`GetNameOfDeclaration` (`binder.go:245`) is what *every* redeclaration
diagnostic is positioned at, so the record belongs in `declare` itself, before
any of the eight paths. Eleven cases, and `binder_symbols` is unchanged —
positions are not part of that suite's comparison, which is why the defect
survived two sessions at 98.03%.

> **Both of this session's position bugs were invisible to every instrument
> except `extragap.rs`.** A code that is simultaneously the most-missing and the
> most-extra in the same files is not an incomplete rule; `diaggap.rs` printed
> "TS1160 12 cases" and "TS2300 32 cases" in its false-positive list for two
> sessions with no way to say so. **+23 cases for two arguments and one moved
> statement.**

**A measured negative in the same place**: applying the `s.pos` correction to the
remaining two `token_start` error sites — the **unterminated regular expression**
and the **JSX string literal** — cost **2 cases** and was reverted. Upstream's
`s.error` is at `s.pos`, but those two sites report through `errorAt` with the
token's own range. *The rule is not "the scanner always reports at `pos`"; it is
"report where upstream's call reports"*, and the two are only sometimes the same.
Both remaining sites are correct as written.


### 30.3 TS1125 — the position family's third member, +5

```
diagnostics 988 -> 993   (+5)     every parser and scanner suite unchanged
```

`Hexadecimal_digit_expected` and `Digit_expected` are reported through
`s.error` at `scanner.go:703`, `:1817` and `:1870` — the same `s.pos`, length
zero as TS1160. Five sites in this port spanned from the escape's or the
exponent's start instead. `extragap.rs` had priced the family at **86 displaced
lines**; five cases finished.

**The family is now exhausted from this instrument's data**: of the displaced
column's head, TS2300 (47) and TS1160 (12) and TS1125 (86) are done, TS1002's two
sites are done, and TS1005's 241 are *not* this shape — they are JSX and
conflict-marker parser **recovery** differences (32 lines at one EOF position in
`jsxUnclosedParserRecovery` alone), which is a parser project rather than a
position. TS1109's 18 are the only untried remainder.

### 30.4 TS2454's extras closed — 55 wrong lines to 10, and where the row actually is

A **destructuring** target is a definite assignment, and
`is_definite_assignment_target` only knew the `x = 1` spelling. `accessKind`
(`ast.go:1426`) already answers for every spelling — `({ x } = obj)`,
`[x] = arr`, `({ a: x } = obj)` — and `crate::unused` had ported it whole for its
own reasons (§15's `isUse` gate). Reusing it here is the same question asked
once: `shorthandPropertyAssignmentsInDestructuring_ES6`,
`destructuringAssignmentWithDefault2`, `destructuringAssignment_private` and
`noUnusedLocals_destructuringAssignment` were 12 wrong lines and all four are
that shape.

```
TS2454   WRONG 114 -> 55 -> 10     LOST 0     CONVERTS unchanged at 238
```

**Zero conversions from either decline, and that locates the row.**
`extragap.rs` says 66 cases have TS2454 as their *sole* obstacle; only ten wrong
lines remain, so ~60 of those are on the **missing** side. The rule's bound is
documented at `check_used_before_assigned` and excludes outer variables,
parameters, aliases and binding elements — the ninth session's named residual,
*"outer variables and assignment marking"*. **That, not the extras, is what the
row is worth**, and this is the measurement that says so.

---

## 31. TS2352 — assertion overlap, +4 for **zero** wrong, and a substitution that needed two declines

```
diagnostics 998 -> 1,003   (+4, past the thousand mark)     LOST 0     WRONG 0
```

`checkAssertionWorker`: an `as` or `<T>` assertion errors when **neither** type is
comparable to the other. `crate::relater` has no comparable relation, and
comparable is *weaker* than assignable — so "not assignable either way" is a
**superset** of upstream's condition and would over-report.

`relate_ternary` is what makes the substitution survivable at all: a pair this
port cannot decide answers `Unknown`, so the rule fires only where both
directions are a confident negative. But it is not sufficient, and the two
declines are exactly where the two relations part company:

- **The same primitive family.** `"foo" as "bar"` is not assignable in either
  direction and *is* comparable — both reduce to `string`.
  `stringLiteralsWithTypeAssertions01` and
  `stringLiteralsAssertionsInEqualityComparisons02` were 6 of the first 7 wrong
  lines.
- **A union or intersection on either side.** `fooOrBar as "baz"` with
  `fooOrBar: "foo" | "bar"` is comparable for the same reason, and this port's
  union carries `UNION` rather than its constituents' flags, so the family test
  cannot see through it. The last two wrong lines and the rule's only loss.

> **A weaker relation can stand in for a stronger one only where you can name
> the difference.** Here the difference is one sentence — comparability reduces
> literals to their base primitive — and it costs two declines. That is the whole
> of what `isTypeComparableTo` would buy on this row, which prices the real
> build.

---

## 32. TS18050 — a nullable operand, +10 for **zero** wrong

```
diagnostics 1,003 -> 1,013   (+10)     LOST 0      RIGHT 534      WRONG 0
```

`checkArithmeticOperandType` (`checker.go:12799`): an operand of a numeric
operator whose type *is* `null` or `undefined` is not a number and cannot become
one. Error node the operand — `binaryArithmatic1.ts(1,13)` is the `null` of
`var v = 4 | null;`.

**`+` is excluded, and it is the only decision in the rule.** Every other
arithmetic and bitwise operator takes numeric operands and nothing else; `+` is
overloaded with string concatenation, so its operand check runs after the
overload is chosen — a different arm with a different message set. Declining it
costs whatever `+` cases exist and cannot produce a wrong one.

534 right lines and zero wrong ones, which puts this beside §12 and §19 in the
"reports on a fact the port already computes exactly" family: the predicate is
`flags == NULL` or `flags == UNDEFINED` on a type, and there is no relation, no
members table and no inference between the question and the answer.

---

## 33. TS2552 — the suggestion arm, +10 for **one named loss**, and the only leg this session overrode

```
diagnostics 1,013 -> 1,022   (+9 net: 10 converts, 1 LOST)     WRONG 24
```

**The first build in this file to ship with `LOST != 0`, and it is overridden
loudly rather than quietly.** The lost case is
**`conformance/resolutionModeTripleSlash2`**, two lines, a triple-slash reference
whose resolution-mode attribute puts a name in scope this port does not model.
Net is +9. The alternative — discarding ten conversions to protect one case — is
the wrong trade at this ratio, and the rule this file has followed for
twenty-six builds is that a fired leg may be overridden *on independent evidence,
loudly*. The evidence is the net; the loudness is this paragraph and the case's
name.

### The rule is one message and no new machinery

The eighth session ported `getSuggestedSymbolForNonexistentSymbol`'s weighted
distance **in full** (§7) precisely because a near neighbour makes TS2304 a wrong
code at a right position — and then *declined* rather than reporting. With the
algorithm already exact, emitting the code it selects costs one `Diagnostic::with_args`.
`onFailedToResolveSymbol`'s order is honoured: missing lib (§30), then spelling,
then the bare TS2304.

### One decline, worth 15 wrong lines and paid for twice

`checkAndReportErrorForUsingTypeAsValue` (`checker.go:1681`) runs **before** the
suggestion arm, and a primitive type *keyword* in a value position is TS2693.
`class C extends string` (`conformance/classExtendingPrimitive`, 9 lines) and
`primitiveTypeAssignment` were the family. Those names resolve to no symbol here
because they are **keywords rather than globals**, so the existing "resolves as a
TYPE" decline never saw them — and the same decline now protects TS2304 as well.


### 33.1 TS2300's `export =` position — REFUSED at a net −3, and the probe could not see it

`duplicateExportAssignments` records `foo5.ts(4,10)` — the **expression** of
`export = x`, not the `export` at column 1 — so `name_node_of` should have an
`ExportAssignment` arm returning `n.expression`. Adding it measured, in
`diag2307.rs` with `RULE_CODES = [2300]`:

```
CONVERTS 28 -> 30     LOST 0     WRONG 61 -> 44
```

and the suite went **1,022 → 1,019**.

> **The counterfactual cannot see a position change.** Its *before* side removes
> the code entirely, so it compares "no TS2300" against "TS2300 at the new
> position" — and a case that was passing with TS2300 at the **old** position
> appears in neither column. Every measurement in this file up to §30 was of a
> rule being *added*, where before-is-absent is exactly right; a rule being
> *moved* needs a before that keeps the old behaviour, which `diag2307.rs` has no
> way to express.

Reverted. The arm is almost certainly correct for `export =` itself and there is
some other consumer of the declaration-start position that pays for it; finding
it needs a probe that diffs the suite's pass set across a commit rather than
across a code list. **That probe is the prerequisite for any further position
work**, and §30's three position fixes were lucky to be additive.

---

## 34. `new` expressions — the same two checks, +2

```
diagnostics 1,022 -> 1,024   (+2)
```

`resolveNewExpression` reaches the same `getArgumentArityError` and
`checkApplicableSignature` as a call; the only differences are where the
signature comes from — the class's **sole** constructor — and that
`getErrorNodeForCallNode` (`checker.go:9843`) unwraps only a `CallExpression`, so
a too-few-arguments error on `new C()` reports on the whole `new` expression
rather than on the `C`.

Declined: a generic class, a class with more than one declaration, an overloaded
constructor, a rest constructor, and a class with **no** constructor at all — the
last because its signature comes from the base through
`getBaseConstructorTypeOfClass`, which is `bd tsr-4sc.8`.

Two cases, and it is here for a reason beyond them: §20's and §26's gates were
written for one node kind and turned out to describe a *signature source*, not a
call syntax. Every remaining call-shaped site — a method call through a receiver,
a `super(…)`, a decorator — reuses the same two functions once its signature
source is nameable.

### 34.1 Excess properties at argument positions — a measured zero, kept

`check_excess_properties` is called from `check_argument_types`: an object literal
at an argument position is an excess-property site exactly as one at a
declaration is, and the contextual type is the parameter's. **+0 cases, 0 wrong,
0 lost** — the same function §23 wrote, one new caller, one line.

Kept for the reason §27.1 gives: the rule is right and only its reach is short.
The zero is recorded so nobody prices it as a conversion later.

---

## 35. TS2339 on a namespace or enum receiver — **+24**, the largest single edit since §15

```
diagnostics 1,024 -> 1,048   (+24)     checker_types and binder_symbols unchanged
```

`declared_members_are_complete` returned `false` for every `TypeData::Anonymous`
— `typeof X`, the static side of a class, a namespace's exports, an enum's
members — with the note *"its inherited statics are a documented gap, so the
table is not complete either."*

**That note over-generalised its own source.** `get_property_of_anonymous_symbol`
lists exactly three gaps and only the first is about inheritance:

- statics inherited from a base class (`getBaseConstructorTypeOfClass`),
- `globalThis`,
- an enum's numeric index signature.

**Only a *class* can have a base.** A namespace or an enum inherits nothing, so
its `exports` table *is* the whole table and a `None` from it is an absent
member — which is precisely what TS2339 needs to know. Admitting those two symbol
kinds, and only those, is twenty-four cases for a nine-line condition.

> The completeness predicate is a list of *named* unported mechanisms (§21), and
> the value of writing it that way is that each entry can be checked against the
> shapes it actually applies to. This one had been applied to three shapes and was
> true of one. **A decline inherits its justification from the mechanism it names,
> not from the type it happens to be attached to.**

---

## 36. The unnarrowed-reference decline is DELETED — +6, and §35's audit generalises

```
diagnostics 1,048 -> 1,054   (+6)     LOST 0
```

§16's fifth decline refused any assignability report whose source was a
**reference with a union type**, on the argument that narrowing applies only to
references and the two narrowing mechanisms this port lacks — aliased conditional
expressions and inferred type predicates — leave a union un-reduced. It was
measured at 18 wrong lines.

**That measurement was taken under the binary relation** (§16, before §25). Under
`relate_ternary` the un-reduced union reaches a relation that answers `Unknown`
for exactly the pairs it cannot decide, so the decline now removes conversions
and prevents nothing. Deleting it — and its three call sites, in the assignment,
declaration, return and argument arms — is worth six cases and zero losses.

> **§35's audit, generalised: a decline inherits its justification from the
> measurement that produced it, and a later build can invalidate that measurement
> without touching the decline.** Two of this session's own gates had gone stale
> that way within the same session. The cheap check is to ask, of every decline,
> *what would have to be true for this to still be needed* — and then run it.
> `pair_is_reportable`'s enum veto was audited the same way and is now a
> **measured no-op**: it is kept because it costs nothing and documents a real
> upstream gap, but it no longer decides anything.

---

## 37. The decline audit — four re-runs, two payments, one refused on faithfulness

§36 turned §35 into a procedure: **re-run every decline against the compiler that
exists now**, because a decline inherits its justification from the measurement
that produced it and a later build can invalidate that measurement silently. Four
were outstanding.

| decline | audited to | kept? |
|---|---|---|
| §18's contextual-typing position list (`implicit_any`) | **+2** — a `return` and an un-annotated property initialiser are uncontextual too | widened |
| §21's index-signature condition (`member_completeness`) | 0 | kept, still exact |
| §16.4's guarded-reference decline (TS2454) | **+2 available, REFUSED** | kept |
| `pair_is_reportable`'s enum veto | 0 — a measured no-op since §25 | kept, documented |

### The one refused, and it is refused on the goal rather than the number

Removing TS2454's guarded-reference decline is worth **+2 cases** and puts back
**45 wrong lines**. Those 45 are diagnostics upstream does not report: it narrows
by the type predicate and says nothing. So the +2 is bought by being *less*
faithful in 45 places, and the two cases it wins are ones where the guard —
a syntactic over-approximation — declines a TS2454 upstream really does report.

**Kept.** `docs/conventions.md`'s rule that a wrong diagnostic is worse than a
missing one is not a tie-breaker here; it is the whole answer, and the suite
number is the thing being traded away rather than the thing being served. The
right fix is a narrower guard, not no guard, and it is worth exactly 2 cases —
which is why it is recorded rather than attempted.


### 37.1 TS2339's dotted-name receiver — the audit's fifth payment, +2

```
diagnostics 1,056 -> 1,058   (+2)
```

§21 declined a **dotted-name receiver** because `narrowingOfDottedNames` narrows
`a.b` by a guard on `a.b` itself and this port's flow graph keys on a narrower
set of references. Still true — and §35 changed the balance underneath it, by
making a *namespace* receiver decidable. `N.x.y` is not narrowed by anything, and
those receivers now outnumber the narrowing family the decline was drawn for.

**Five declines re-run, four moved, +10 cases between them, no new machinery.**
The procedure is cheap enough to be routine: disable the condition, run coverage,
keep or revert. It should be the first thing a session does after any build that
changes what the port can decide — §25 and §35 each invalidated gates written
before them, and nothing announced it.

### 37.2 The JS declines were **borrowed**, not measured — +3

```
diagnostics 1,058 -> 1,061   (+3)
```

`crate::nonexistent_property` and `crate::type_argument_arity` both declined
`.js` files. Neither had ever measured it: they took the argument from §18 and
§16.2, where it *is* measured — JSDoc `@param` and `@type` supply annotations
this port does not parse, so an option-gated rule reports an implicit any on
every annotated parameter, and a declaration's type comes out wrong.

**That argument is about annotations, and neither of these rules reads one.**
TS2339 reports on a member's *absence* from a table the binder built from real
declarations, and TS2314 counts type parameters written on a class. **JSDoc adds
no members and no type parameters.** The declines were inherited by resemblance.

Three cases, and the shape is worth more than the three: a decline copied from a
sibling rule carries the sibling's justification, not its own, and nothing in the
code says which is which. The audit catches it; a reviewer reading either file in
isolation would not.

---

## 38. Namespace-member callees — a measured zero, kept

```
diagnostics 1,062 (unchanged)     LOST 0     WRONG 0
```

§20's and §26's gate resolved a callee only as a **bare identifier**. It now also
resolves a **property access**, through the receiver's type and only where
[`Checker::declared_members_are_complete`] certifies that table — which §35 made
true for a namespace or enum receiver. `N.f(…)` is therefore reachable now;
`obj.method(…)` on a class instance still is not, because a class's table is
certified complete only when nothing inherits.

**Zero cases.** Kept for §27.1's reason — the *rule* is right and only its reach
is short — and because the shape it removes is the one §34 named: these two
checks were written for a call syntax and are really about a **signature
source**, and this is the second source wired in. The third, a method on an
instance, arrives with the class-side completeness work rather than with any
change here.

### 38.1 TS2454's guard narrowed rather than removed — +1, and §37's refusal answered

```
diagnostics 1,062 -> 1,063   (+1)
```

§37 refused removing the guarded-reference decline: +2 cases for 45 wrong lines
is buying the suite number with unfaithfulness, and it named the right fix as
*"a narrower guard, not no guard"*. This is that guard.

The condition must now both **name** the reference and **contain one of the two
narrowing mechanisms this port does not model** — a call (a user-defined type
predicate) or an `instanceof`. Requiring the mechanism as well as the name is
what stops it declining a TS2454 upstream really does report, and it recovers a
case without putting any of the 45 lines back.

> A decline that over-approximates is worth auditing for *which half* is
> over-approximating. This one tested "is there a guard naming this?" when the
> question was "is there a guard naming this **that I cannot follow**?" — and the
> second is barely more code.

### 38.2 The class static side, when the class has no base — +4

```
diagnostics 1,063 -> 1,067   (+4)
```

§35's argument applied once more. `get_property_of_anonymous_symbol` names
exactly one inheritance gap for a `typeof C` receiver —
`getBaseConstructorTypeOfClass` — and **a class that extends nothing cannot reach
it**. So a non-generic class with no heritage clause has a complete static table,
and `C.missing` is a genuine TS2339.

That is the third shape §35's reading unlocked (namespace, enum, base-less
class), for the same nine-line condition each time. **The completeness predicate
should be read as a list of mechanisms and checked shape by shape**, which is
what its own header claims and what nothing had done until §35.

---

## 39. Where the audit ends

Every decline this session wrote has now been re-run against the compiler that
exists at the end of it. Seven moved, six were confirmed exact, and the totals
are **+18 cases for no new machinery at all**:

| moved | audited to |
|---|---:|
| §16's unnarrowed-reference (deleted) | +6 |
| §35's `Anonymous` receivers (namespace, enum) | +24 |
| §38.2's `Anonymous` receivers (base-less class) | +4 |
| §21's dotted-name receiver (deleted) | +2 |
| §18's contextual-typing positions (widened) | +2 |
| §37.2's borrowed `.js` declines (deleted) | +3 |
| §38.1's TS2454 guard (narrowed, not removed) | +1 |

Confirmed exact and left alone: the index-signature and computed-name conditions,
the enum veto (now a measured no-op), the optional-chain decline,
`implicit_any`'s `.js` decline (measured, unlike §37.2's), the `unknown` veto, the
empty-target and object-literal-versus-union declines.

**The procedure, for the next session:** disable one condition, run coverage,
keep or revert. Run it after any build that changes what the port can *decide* —
§25 (`relate_ternary`) and §35 (`Anonymous` completeness) each silently
invalidated gates written before them.

---

## 40. TS2551 — the property suggestion, +2

```
diagnostics 1,067 -> 1,069   (+2)
```

`reportNonexistentProperty`'s suggestion arm: a near-miss **member** name is
TS2551, not TS2339. §21 computed the suggestion in order to *decline*, exactly as
§7 had for TS2304 before §33 turned that into a report. The same move, the same
reason: the spelling algorithm is already exact, so reporting the code it selects
costs one message and converts its own row.

Two of the three "different code at the same position" arms §21 listed are now
reports rather than silences — TS2551 here, and the static-member arm stays a
decline because TS2576 needs a `typeof` distinction this port does not draw at
that site. The lib arms (TS2550, TS2812) remain silences for want of a
lib-version table.

### 40.1 TS2693 — the same move, REFUSED at −4

`check_value_identifier` declines a name that resolves in **type** space, and
§33's lesson said the resolution that made it a decline is the resolution the
report needs. Emitting TS2693 there measured **1,069 → 1,065**.

`checkAndReportErrorForUsingTypeAsValue` (`checker.go:1681`) is not the one-line
substitution TS2552 and TS2551 were: it has further conditions this port does not
draw — a type-only import, an `import type` alias, and the position tests that
separate it from TS2749 and TS2708. **The pattern generalises to the
substitutions whose upstream site is a single `if`, and TS2693's is not.**
Reverted; the decline stands.

### 40.2 The type-parameter condition drops out of completeness — +1

`declaration_members_are_complete` required `type_parameters.is_empty()` on every
class and interface. Removing it is **+1** and loses nothing, because the
condition was **already enforced one level up**:
`declared_members_are_complete` returns `false` for anything in
`type_reference_targets`, which is every *instantiated* reference. A generic
declaration reached without type arguments has an uninstantiated table, and its
member **names** — the only thing this predicate promises — are complete.

The tenth redundant or stale condition the audit has retired. Two levels of a
predicate testing the same thing is the shape to look for: the outer one is
usually the real gate.

### 40.3 TS2304's parse-error decline is DELETED — +6

The eighth session's §7 declined TS2304 in any file the parser reported a
diagnostic in, on the argument that *"a rule that reports on a node one parser
invented is reporting about a program the other never saw"*, and measured it as
the single largest family in that residual — `jsxUnclosedParserRecovery` 21
lines, `arrowFunctionsMissingTokens` 15, and a tail of `parserSkippedTokens`.

**Deleting it is +6 with the gates green.** The measurement was taken when
TS2304 was the *only* semantic rule in the traversal and every wrong line it
produced was unopposed. Thirty-eight builds later the recovered-tree cases are
mostly failing on other codes anyway, so the decline's wrong lines cost nothing
and its silences cost six cases.

This is the audit's eighth payment and its oldest target — a decline from a
different session, justified by a measurement that was correct when taken.
**Nothing marks a decline as re-checkable; only re-running it does.** The
condition itself is preserved on `Checker::file_has_parse_errors` and still read
by five other rules, each of which should be audited the same way.

### 40.4 TS2339's lib-receiver decline is DELETED — +2

§21 declined a receiver whose type was declared outside the program's own files,
because TS2550 and TS2812 replace TS2339 for a lib type and neither is
modellable. Measured then at 8 lines across three cases.

**Deleting it is +2.** Those three cases fail on other codes now; what the
decline was still costing was every *correct* TS2339 on a lib-declared receiver —
and `Checker::set_checked_files`, added in §21 solely to make the question
answerable, is now unused by any rule. It stays on the checker because the
question it answers is a real one that TS2550 will need.

The audit's eleventh payment, and the second whose target was a decline drawn
around a **different code's** territory rather than around this port's
incompleteness. Those are the ones that go stale fastest: the other code's
population moves and nothing tells the decline.

### 40.5 `+` belongs in TS18050 after all — +3

§32 excluded `+` from the numeric-operator set because it is overloaded with
string concatenation and upstream's operand check for it runs after the overload
is chosen. **That is sound about upstream's control flow and wrong about the
outcome**: `null` and `undefined` are not string-like either, so the `+` arm
reaches the same TS18050. Adding it is +3.

The twelfth payment, and the first whose target was not a stale measurement but
an **argument that was never measured at all**. §32 wrote "declining it costs
whatever `+` cases exist and cannot produce a wrong one" — true, and it cost
three. *An argument from upstream's structure is not a measurement*, and this
file has now been caught making that substitution once.

### 40.6 The relational operators too — +3

`checkArithmeticOperandType` is reached from every operator that requires a
numeric or comparable operand, and `<`, `>`, `<=`, `>=` are among them —
`checkBinaryLikeExpression`'s relational arm calls the same check. §32's set held
only the arithmetic and bitwise ones.

**+3**, and with §40.5's `+` that is **+6 from widening one `matches!` twice.**
The rule was right and its operator list was a guess dressed as a decision: §32
wrote "every other arithmetic and bitwise operator takes numeric operands and
nothing else", which is true and is not the same statement as "these are the
operators that reach `checkArithmeticOperandType`".

**A list is a claim about upstream and should be read off upstream**, the way
§19's `getMinTypeArgumentCount` and §30's lib table were. This one was written
from the inside out.

The compound arithmetic assignments — `-=`, `*=`, `/=`, `%=` — are one more
(+1). `checkAssignmentOperator` routes them through the same operand check as
their bare forms, and the set is now closed: **+7 across three widenings of one
`matches!`**, from a rule §32 shipped believing its operator list was the
decision it had made rather than a fact it had guessed.

---

## 41. The property-access assignment target — **+16, past 20%**

```
diagnostics 1,087 -> 1,103 = 20.10%
```

§16's third decline refused a property or element access as an assignment
target, because a `set` accessor whose parameter type differs from its getter's
return type makes the write type the setter's (`getWriteTypeOfSymbol`), and
`divergentAccessorsTypes2` is exactly that.

**Admitted, and the decline is retired by measurement: +16.** The divergent-
accessor cases it was drawn for are a handful; the ordinary `obj.field = value`
it was *also* refusing is sixteen. `getWriteTypeOfSymbol` would recover the
handful, and it is now priced.

**The largest single audit payment of the session, and the last one found.** It
sat behind the same error every other stale decline did — a real upstream
mechanism, correctly identified, used to justify a bound far wider than the
mechanism itself. §16 named `divergentAccessorsTypes2` and then declined *every*
property access, which is the whole class of defect §35 through §40.6 keep
finding: **the decline is drawn around the shape the counter-example belongs to,
not around the counter-example.**

### 41.1 The instantiated-reference decline — +4, and it was answering the wrong question

`declared_members_are_complete` opened by returning `false` for anything in
`type_reference_targets`, because *"an instantiated reference's members need
substitution this port only performs at `instantiate_for_reference`, one property
at a time; the **table** is the uninstantiated one."*

Every clause of that is true and none of it is about this predicate.
**Instantiation changes member *types*; it never changes member *names*** — and
the predicate's own doc comment says it promises the names and nothing else:
*"it says nothing about whether the member types are right, only about whether
the member names are all present. TS2339 needs exactly the second."*

Deleting it is **+4**. Fourteenth audit payment, and the second in a row where
the decline was a correct sentence about a different question than the one the
function asks.

### 41.2 §22's TS2741 refusal is RETIRED — the switch flips to `true`, +1

§22 refused TS2741 at **1 conversion for 8 wrong lines** and kept the machinery
behind `REPORT_MISSING_REQUIRED_PROPERTY` *"because everything it switches is
correct and four named families stand between it and a positive score."*

**It was measured against a `declared_property_table` that declined instantiated
references, generic declarations and every `Anonymous` receiver.** §35, §38.2 and
§41.1 removed all three. Flipping the constant is now **+1** with the gates
green — and the refusal is retired by the same procedure that retired §9's:
re-run it after the thing it was blocked on changes.

That is the second refusal in this file to be retired by measurement rather than
by argument (§21 was the first), and both times the retirement condition had been
written down at the point of refusal. **A refusal with a named prerequisite is an
asset; one without is a dead end.**

### 41.3 The merged-base decline in `sole_plain_base_type` — +1

§27 declined a **merged** base symbol in `check_property_overrides` by copying
the decline `check_heritage_conformance` needed, where it is measured (4 wrong
lines). The override check compares one *named member* at a time, and a member
found in a merged table is still the member upstream would have found — the
merge only changes which declarations contributed it, not what it is.

**+1**, and the sixteenth payment. Third instance of §37.2's shape: a decline
borrowed from a sibling that was answering a different question.

---

## 42. `isNeverInitialized` — the outer-variable refusal, lifted on a per-symbol assignment record

§8 bounded TS2454 by *requiring the shape in which the unported half cannot be
consulted*: the reference's control-flow container had to **be** the
declaration's, so `isOuterVariable` was false and `isNeverInitialized` — the sole
consumer of `isSymbolAssignedDefinitely` — was never reached. That bound is the
last one on the rule, and `extragap.rs` at `HEAD` prices what is behind it:

```
  code   displaced   invented   sole obstacle
TS2454           1         28             56
```

56 cases blocked on TS2454 alone against 28 invented lines, so the balance of the
row is on the **missing** side — the direction the §8 bound fails in, by
construction.

### The approach that is already refused, and why this one is different

The tenth session's last measurement lifted the same refusal in favour of a
*syntactic* `isNeverInitialized` — "no identifier of this name stands in a write
position anywhere in the file" — and measured **+5 converts, 4 LOST, wrong
10 → 65**. It was reverted. A name-based scan cannot tell one `x` from another's,
and TS2454 is a rule whose losses come from over-reporting.

Upstream does not scan names. `isSymbolAssignedDefinitely` (`flow.go:2655`) reads
`markedAssignmentSymbolLinks`, populated by `markNodeAssignments`
(`flow.go:2703`), which resolves **each assignment target to its symbol** during
one walk per function-or-source-file root.

### The half that is already built, and the half that is one field

`markNodeAssignments` **is ported** — `crate::flow::mark_node_assignments`
(`flow.rs:882`), built by the `.types` workstream for §13's past-last-assignment
extension. It records `last_assignment_pos` per symbol, with the same
`i64::MAX`-for-nested-functions rule and the same `extendAssignmentPosition`
walk.

What it does not record is upstream's **second** field on the same links:

```go
if assignmentKind == AssignmentKindDefinite {
    links.hasDefiniteAssignment = true
}
```

So this build is one set, one predicate and one changed disjunct:

1. `Checker::definitely_assigned`, a `SymbolId` set written by the existing walk;
2. `is_symbol_assigned_definitely` / `is_mutable_local_variable_declaration`;
3. `check_used_before_assigned`'s container equality becomes upstream's
   `isOuterVariable && !isNeverInitialized`.

**`last_assignment_pos` is not touched.** The definite flag is written from the
same identifier arm, but *outside* the `!= Some(&i64::MAX)` guard, which is where
upstream writes it — that guard governs the position, not the flag. Anything that
moved `last_assignment_pos` would move `checker_types`, which leg 2 watches.

### One deliberate divergence, named before it is measured

`isMutableLocalVariableDeclaration` (`utilities.go:1053`) excludes a `let` whose
variable statement sits in a **global** source file — `IsGlobalSourceFile`, which
is *"a source file that is not an external or CommonJS module"*. The port's
`is_parameter_or_mutable_local_variable` (`flow.rs:787`) excludes every
file-level `let`, module or not. That predicate belongs to the `.types`
workstream's §13 reader and changing it would move `checker_types`, so this build
writes its **own** faithful `is_mutable_local_variable_declaration` and leaves the
flow one alone. The two now disagree on one population — a top-level `let` in a
module file — and this paragraph exists so the next session does not read the
duplication as an accident.

### The bar, registered before the code

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1118** (baseline at `HEAD`). Forecast **+15 to +40** — the sole-obstacle row is 56 and no rule has ever converted its whole row |
| 2 | `checker_types` **byte-identical** — 3,645 / 9,538, 82.44%, snapshot unchanged |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2454]` |
| 4 | TS2454's own WRONG ≤ **60**, from the baseline **29** |
| 5 | every other snapshot unchanged |

Baseline for leg 3/4, `diag2307.rs` isolated to 2454 at `HEAD`:
**CONVERTS 252 · LOST 0 · STILL SHORT 89 · RIGHT 3,652 · WRONG 29.**

**Falsifier.** If the new arm reports on outer references to variables that *are*
assigned — the exact failure the syntactic scan produced — WRONG climbs past 60
and the per-symbol record is not doing the work the name scan could not. That
would mean the assignment walk's symbol resolution disagrees with the rule's, and
the answer would be to read the top row of the wrong column rather than to
re-tighten the container test.

### Scored — **+7**, and the arm's own yield was **+1**

| leg | registered | measured | |
|---|---|---:|---|
| 1 | `diagnostics` passes > 1,118 | **1,125 / 5,488 = 20.50%** | pass |
| 2 | `checker_types` byte-identical | **3,645 / 9,538, 82.44%**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | TS2454's own WRONG ≤ 60 | **30**, from 29 | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 2454: **CONVERTS 252 → 259 · RIGHT 3,652 → 3,793 ·
WRONG 29 → 30 · LOST 0.**

**The forecast was wrong in an instructive direction.** The bar said +15 to +40
for the outer-variable arm and the arm converted **one**. The reason is that the
capability was **already half-built somewhere else**: the flow walk's START arm
(`flow.rs:422`, `checker-notes-narrow.md` §9.7) had ported
`assumeInitialized = isOuterVariable && !isNeverInitialized` *inside the flow
graph* — a never-assigned outer `let` already returns the substituted initial
type there. So the rule's container test was refusing references the flow was
prepared to answer, and lifting it found almost nothing new because the same
question had been asked twice.

> **A refusal whose stated blocker is "the machinery is unported" is worth
> re-greping before it is worth building.** `markNodeAssignments` was ported
> eight builds ago by the parallel workstream and this file's handoff still
> named it as the missing piece. The cost of not checking was most of a build;
> the cost of checking would have been one grep, the same price §5's import-alias
> re-check paid.

The +1 is kept: it is upstream's disjunct, it costs nothing, and it is what makes
`isSymbolAssignedDefinitely` a real function rather than a stub. The seven that
moved the suite came from reading the *missing* column instead — §42.1.

### 42.1 The named-union printing guard was silencing the rule — **+7**

`diagmissing.rs` (new instrument, below) prints the lines the baseline records
and the port does not, restricted to cases that code alone blocks. TS2454's list
opened with two 60-line cases, `arithmeticOperatorWithEnum` and its `…Union`
twin, and a decline trace over one of them read:

```
 60 flow-error   initial_err=true declared=E
 40 report
```

`var c: E;` — every enum-typed declaration in the corpus. `get_optional_type(E)`
builds `E | undefined`, and `union_type_worker` answers **`errorType`** for any
union with a *named* constituent (`unions.rs:396`):

> *"Upstream would build a denormalised `origin` here so the named union prints
> unexpanded (`checker.go:25705`). Without it the constituents would be printed
> instead — `E.a | E.b | string` where upstream writes `E | string` — which is a
> wrong line rather than a missing one."*

Every word of that is true **about printing**, which is the whole of what it is
defending. Upstream has no such case at all; it is a deviation this port took to
protect the `.types` gradient, and it was written where every consumer pays it.

**A diagnostic that compares `(file, line, column, code)` never prints a type.**
So the guard turns a right answer into an `errorType` that silences the rule, for
a cost that consumer cannot incur. `get_union_type_unprinted` /
`get_optional_type_unprinted` are the same worker with the print guard off, and
`check_used_before_assigned` is their only caller — nothing on the query road may
call them, which is what keeps leg 2 green.

**+7 cases, +141 right lines, +1 wrong.** Fourth instance of §37.2's shape and
the sharpest yet: a decline that is a correct sentence about a *different*
question than the one the caller is asking. The three before it borrowed a
sibling's justification; this one was written for a real reason and then applied
to consumers that reason cannot reach.

### 42.2 The `declared == errorType` decline — REFUSED, and it is upstream's

The next family in `diagmissing.rs`'s list is eight cases, one line each —
`moduleAugmentation*` (7) plus `augmentExportEquals5` — and all eight decline at
`declared == self.intrinsics.error`: `let x: SomeImportedType;` whose annotation
this port cannot resolve.

**Deleting the decline is refused, on faithfulness rather than on a number.**
`errorType` upstream carries `TypeFlagsAny`, so `assumeInitialized`'s
`t.flags&(TypeFlagsAnyOrUnknown|TypeFlagsVoid) != 0` disjunct (`checker.go:11156`)
fires and upstream reports **nothing** for a variable whose declared type is an
error. The port's decline is that disjunct. It is also not *reachable* to fix
from here: `get_optional_type_unprinted(errorType)` is `errorType` again, because
`includes.error` wins over everything in `union_type_worker`, so there is no
initial type to run the flow with.

These eight cases are blocked on the annotation resolving at all, which is a
`checker_types` question with an owner. Recorded so the next session does not
re-derive it from the same list.

### The instruments this build added

| instrument | answers |
|---|---|
| `examples/diagmissing.rs` | **the missing half of one code.** `diagmissing -- 2454` prints every baseline line of that code the port does not emit, restricted to the cases the code alone blocks — so each case printed is exactly one conversion. `diaggap.rs` ranks codes and `extragap.rs` splits the *extra* column; neither says **which lines** are absent, and an under-reporting rule is diagnosed by reading them |
| `examples/diagcase.rs` | one case's expected and actual diagnostics side by side, through the suite's own `reported_for` |

Together they are the missing-side twin of `extragap.rs`, and §42.1 is what they
found in their first hour.

---

## 43. TS2564's name arm — the two kinds §6 quoted and did not port

`diagmissing.rs` on 2564 reads **41 cases blocked on it alone, 72 lines**, and
the case names are a family before any code is read:
`parserComputedPropertyName28/29/31`, `symbolProperty6`, `parserSymbolProperty5`,
`instanceMemberWithComputedPropertyName2`, `privateNameDeclaration`,
`privateNameNestedClassNameConflict`, `uniqueSymbols`,
`uniqueSymbolsDeclarations`, `classIndexer2`, `symbolDeclarationEmit1`.

`checkPropertyInitialization` (`checker.go:4944`) gates on

```go
if ast.IsIdentifier(propName) || ast.IsPrivateIdentifier(propName) || ast.IsComputedPropertyName(propName) {
```

and `check_property_initialization`'s comment **quotes that line** and then
matches `PropertyName::Identifier` alone:

```rust
// `IsIdentifier || IsPrivateIdentifier || IsComputedPropertyName`
// (`checker.go:4944`). A string- or number-named property is
// skipped by upstream too.
let name = match property.name {
    tsr_ast::PropertyName::Identifier(identifier) => identifier.text,
    _ => continue,
};
```

The comment is right and the code implements a third of it. The `_ => continue`
was reaching for the *string/number* exclusion the comment's second sentence
names, and swept the other two accepted kinds up with it. **Third instance this
session of a decline that is a correct sentence about a different question**
(§42.1, §41.3, §37.2) and the first where the correct sentence is sitting
directly above the wrong code.

### What the arm needs that the identifier arm did not

Only the message argument. `scanner.DeclarationNameToString` reads the node's
**source text**, and this checker has spans but no file text — ADR-0034 puts one
`NodeTable` across a program and the text stays with the `SourceFile` the
checker is not given. The argument is reconstructed from the tree instead:
`#x` for a private identifier, and the bracketed entity name for a computed one
where the expression is a name, a dotted name or a literal.

**This is safe to approximate and would not be safe to guess at**: the
`diagnostics` suite compares `(file, line, column, code)` and never the
arguments (§8's trap list). The reconstruction exists so the message is not
nonsense to a human reader, not because anything measures it. A computed name
whose expression is none of those shapes prints `(Missing)`, which is
upstream's own answer for a name node with an empty span.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,125**. Forecast **+6 to +15** of the 41 — the family above is ~13 cases and no rule has converted its whole row |
| 2 | `checker_types` byte-identical — 3,659 / 9,538, 82.62% |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2564]` |
| 4 | TS2564's own WRONG ≤ **1.5×** its baseline |
| 5 | every other snapshot unchanged |

**Falsifier.** A computed property name that upstream *late-binds* to a known
member has a symbol whose type this port may compute differently; if the wrong
column fills with `uniqueSymbol*` lines the arm is reporting on properties whose
type upstream resolves and this port does not, and the answer is a decline on
the computed kind rather than on the whole arm.

### Scored — **+14**, and the error test was wrong in the same function

| leg | registered | measured | |
|---|---|---:|---|
| 1 | `diagnostics` passes > 1,125 | **1,139 / 5,488 = 20.75%** | pass |
| 2 | `checker_types` byte-identical | **3,659 / 9,538, 82.62%**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | TS2564's own WRONG ≤ 1.5× baseline (6 → ≤9) | **4**, *below* the baseline | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 2564: **CONVERTS 214 → 228 · RIGHT 1,208 → 1,241 ·
WRONG 6 → 4 · LOST 0.**

**The wrong column went *down* while the rule's reach went up**, which is not
something the arm itself could do. The two accepted name kinds landed at 8 wrong
lines, and reading the top row named a second defect in the same three lines of
code: the type test was `declared == self.intrinsics.error`, and this port has
**two** error types. An unresolved type *reference* mints a
`TypeData::Named` carrying the written text and is recorded in
`Checker::unresolved_types` — `Checker::is_error` is the predicate, identity is
not. `class C { [e]: Type }` with neither name declared reached the rule with a
`Named` type that is not `intrinsics.error`, and so did two cases that were
already reporting wrongly before this build.

Switching to `is_error` is also the *faithful* reading: upstream has one
`errorType` and it carries `TypeFlagsAny`, so `checker.go:4946`'s
`t.flags&TypeFlagsAnyOrUnknown` **is** the error test. The port had written it as
an extra disjunct and then compared the wrong way.

> **`== self.intrinsics.error` is to `is_error` what `is_type_assignable_to` was
> to `relate_ternary`** (§25): a narrower question than the one the rule means,
> spelled so it looks like the right one. Both cost wrong lines in the
> over-reporting direction, and both are greppable. There are **31** other
> `== self.intrinsics.error` tests in the checker and each is a question about
> whether identity or `is_error` was meant.

### The parse-error gate — measured on the suite and REFUSED at −1

The four surviving wrong lines are all parser *recovery* shapes:
`class C2 extends { foo: string; } { }` (twice, `classExtendsEveryObjectType`),
`[public x: string]: string` in a class body, and `setFoo(#foo: string)`. The
standard gate this module uses everywhere else removes all four:

| | CONVERTS | WRONG | suite |
|---|---:|---:|---:|
| no `file_has_parse_errors` gate | 228 | 4 | **1,139** |
| with the gate | 227 | 0 | 1,138 |

**A zero wrong column is not the objective.** The four lines sit in cases that
fail for other reasons, so removing them converts nothing, and the gate costs a
real conversion. Same result as §40.3, which *deleted* TS2304's parse-error
decline for +6 — the gate is worth measuring per rule and is not a house style.

---

## 44. The `== errorType` audit — the grep §43 asked for

§43 found that `declared == self.intrinsics.error` is the **narrower** of two
questions this port can ask, and that the wide one — `Checker::is_error`, which
also answers for the `TypeData::Named` an unresolved type *reference* mints — is
the faithful reading of upstream's `t.flags&TypeFlagsAnyOrUnknown`. It cost two
wrong lines in one rule and named the audit: **every identity test against
`intrinsics.error` in a reporting path is a question about which was meant.**

Inside the diagnostics road there are three left after §43:

| site | direction | what the wide reading would do |
|---|---|---|
| `assignreport.rs:651` — `pair_is_reportable` | **decline** | stop reporting TS2322/2345/2352/2411/2415/2420/2430 for a pair whose source or target is an unresolved reference |
| `check.rs:1145` — TS2454's declared type | decline | §42.2 refuses this one: it is upstream's own any-flag disjunct and there is no initial type to run the flow with |
| `check.rs:1155` — TS2454's flow type | decline | the flow answer, not the declared one |

The first is the one with a population. `pair_is_reportable` is the gate every
conformance rule shares, and its own doc says *"`any` and the error type,
unchanged: neither can fail a relation, so admitting them can only produce
accidents."* **An unresolved reference is exactly that accident under a
different type id** — `Named("Foo")` where `Foo` never resolved is not a type
that failed a relation, it is a type the port could not build, and the relater
has no way to say so.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **1,139**. This is a *decline*, so the honest forecast is **0 to +6**: it can only remove wrong lines, and a wrong line converts a case only when it was that case's last defect |
| 2 | `checker_types` byte-identical — 3,659 / 82.62% |
| 3 | LOST == **0** |
| 4 | the assignment family's WRONG column **decreases** |
| 5 | every other snapshot unchanged |

**Falsifier.** If the suite *drops*, the unresolved-reference types were carrying
real reports — meaning the port resolves those names well enough that the
relation's answer was sound — and the narrow test was right for a reason nobody
wrote down.

### Scored — a **measured zero**, kept, and the zero is the useful part

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 1,139 | **1,139** | pass |
| 2 | `checker_types` byte-identical | **3,660 / 82.64%**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | the assignment family's WRONG decreases | **161 → 161** | **did not fire** |
| 5 | every other snapshot unchanged | **`diagnostics.snap` is byte-identical too** | pass |

`diag2307.rs` over `[2322, 2345, 2352, 2411, 2415, 2416, 2420, 2430]`, measured
on both sides of the edit: **CONVERTS 118, WRONG 161, LOST 0 — identical.**

**No unresolved-reference type reaches `pair_is_reportable` today.** The two
readings of the error test are the same function at this site, so the edit is
behaviour-neutral over the whole corpus, and `diagnostics.snap` is byte-identical
rather than merely equal in its total.

**Kept anyway, and this is a judgement rather than a measurement.** The reason is
the direction the port is moving: `checker-notes-narrow.md` §31–§32 mint
unresolved `Named` types deliberately and in growing numbers — TS2304's
provenance chain is 6,267 lines of them — and the moment one reaches an
assignment anchor the narrow test reports a type the port could not build. The
cost of keeping the wide reading is one set lookup on a path that already does
three intrinsic comparisons; the cost of the narrow one, when it fires, is the
same wrong line §43 measured in TS2564.

**A zero is a fact about the corpus at this commit, not about the predicate.**
Recorded here so the next session does not re-run it hoping for a number, and so
that if a future build *does* see this fire, the date it started is findable.

---

## 45. TS2367 — the comparison overlap, §31's substitution at its second site

The handoff's item 0: *"TS2367 (no-overlap comparison, 23 cases) remains and is
the same substitution at a different site."* `diaggap.rs` at `HEAD` reads
**49 cases contain it, 23 would convert alone.**

`checkBinaryLikeExpression`'s equality arm (`checker.go:12487`):

```go
c.reportOperatorErrorUnless(leftType, operator, rightType, errorNode, func(left, right *Type) bool {
    return c.isTypeEqualityComparableTo(left, right) || c.isTypeEqualityComparableTo(right, left)
})
```

and `isTypeEqualityComparableTo` (`checker.go:12861`) is

```go
return (target.flags&TypeFlagsNullable) != 0 || c.isTypeComparableTo(source, target)
```

Two facts fall straight out of that, and both are cheaper than the relation:

1. **Either side nullable is silence.** The predicate is asked in both
   directions, so `l == null` and `null == l` both take the `Nullable` disjunct
   before comparability is consulted. No relation runs.
2. **What is left is `isTypeComparableTo` in both directions** — exactly §31's
   substitution, and it comes with §31's two declines already measured: the same
   primitive family (`"foo" === "bar"` is comparable and not assignable), and a
   union or intersection on either side (this port's union carries `UNION`
   rather than its constituents' flags, so the family test cannot see through
   it).

`same_primitive_family` is `crate::assertion_overlap`'s and is reused **as is**.
That is the point of building this second: if the substitution needs a *third*
decline here that it did not need there, the difference is a property of the
site rather than of the relation, and that is worth knowing. If it needs none,
§31's two are the whole of what `isTypeComparableTo` would buy.

The error node is the **whole binary expression** (`checker.go:12333` passes
`node`), not the operator and not either operand.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,139**. Forecast **+8 to +20** of the 23 |
| 2 | `checker_types` byte-identical — 3,660 / 82.64% |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2367]` |
| 4 | own WRONG ≤ **20** — §31 shipped at 0, and this site sees every comparison in the corpus rather than every assertion, so a wider wrong column is expected and a much wider one is a refusal |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column is dominated by shapes §31 never saw —
enums, references, `typeof x === "…"` after a narrowing this port does not
perform — then the two inherited declines are not the difference between the
relations, they were the difference *at assertion sites*, and the rule needs its
own audit rather than §31's.

### Scored — **+15, past 21%**, and the falsifier fired on the first measurement

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,139, forecast +8 to +20 | **1,154 / 5,488 = 21.03%** | pass |
| 2 | `checker_types` byte-identical | **3,660 / 82.64%**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | own WRONG ≤ 20 | **0** — after the decline below; **36** before it | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 2367: **CONVERTS 16 · RIGHT 143 · WRONG 0 · LOST 0.**

**The falsifier fired exactly as written, and it is the build's finding.** The
first measurement, with §31's `same_primitive_family` inherited whole, read
**2 conversions**. The declined population was
`stringLiteralsWithEqualityChecks01–04` — `x === "bar"` with `x: "foo"` — which
upstream **does** report.

The reason is not a property of the relation. `checkAssertionDeferred`
(`checker.go:12317`) applies `getBaseTypeOfLiteralType` to the expression before
comparing, so `"foo" as "bar"` really compares `string` against `"bar"`. The
equality arm applies no widening at all. **§31's `same_primitive_family` is a
stand-in for that widening, not for the difference between comparable and
assignable** — and §31 says otherwise:

> *"`"foo" as "bar"` is not assignable in either direction and *is* comparable —
> both reduce to `string`."*

**Correcting the record: they reduce to `string` because `checkAssertionWorker`
reduces them, not because comparability does.** The sentence is right about the
outcome and wrong about the mechanism, and believing the stated mechanism cost
this rule 10 of its 16 conversions until the corpus contradicted it. §31's
*measurement* stands — the decline is correct where it is — and its explanation
is now this paragraph. The composite decline **is** shared and is the real
relation difference; it moved to `either_is_composite` and both rules call it.

### The one decline this site needed, and it is narrower than it first looked

Dropping `same_primitive_family` took the rule to 12 conversions and **36 wrong
lines**, all four of them `capturedLetConstInLoop6/7(_ES6)`, all the same shape:

```ts
const x = 1;
if (x == 1) { break; }
if (x == 2) { continue; }     // <- reported here, upstream does not
```

`.types` says why in one line: upstream reads **`x : never`** at the second test.
The failed `x == 1` empties a unit type, and `never` overlaps everything. This
port does not perform that narrowing, so `x` is still `1`, and `1` against `2` is
a confident negative. **Every narrowing this port does not perform leaves the
operand wider, and a wider operand in a no-overlap check invents a diagnostic** —
the same asymmetry §8 built `reference_is_guarded_by_a_condition_on` on.

Two versions of the decline were measured before the right one:

| decline | CONVERTS | WRONG | suite |
|---|---:|---:|---:|
| an earlier sibling `if` **mentions** the name | 10 | 0 | 1,148 |
| …and the test is an **equality** test | 10 | 0 | 1,148 |
| …and the compared literal **is** the operand's own unit type | **16** | **0** | **1,154** |

The first two also declined `capturedLetConstInLoop8`, whose `y : 0` meets
`if (y == 1)` four times — an equality test that does **not** empty `0`, so
upstream reports all four and so should this. Only the third version separates
them, and it is the faithful statement of the mechanism rather than a proxy for
it: *a failed equality test narrows to `never` exactly when the compared literal
is the operand's own type.* Getting from a proxy to the mechanism was worth
**six cases and eight of the eight `capturedLetConstInLoop*` cases**.

> Third time this session that the difference between +2 and +15 was a sentence
> about **why**, not a threshold. §42.1 (a printing guard applied to a
> non-printing consumer), §43 (a comment describing three kinds above code
> handling one), and now a decline copied with its conclusion and not its cause.

---

## 46. TS2352's literal decline becomes the widening it was standing in for

§45 established that `same_primitive_family` is `check_assertion_overlap`'s proxy
for `getBaseTypeOfLiteralType`, which `checkAssertionDeferred`
(`checker.go:12317`) applies to the *expression* before comparing:

```go
exprType := c.getRegularTypeOfObjectLiteral(c.getBaseTypeOfLiteralType(c.assertionLinks.Get(node).exprType))
targetType := c.getTypeFromTypeNode(typeNode)
if !c.isErrorType(targetType) {
    widenedType := c.getWidenedType(exprType)
    if !c.isTypeComparableTo(targetType, widenedType) {
        c.checkTypeComparableTo(exprType, targetType, errNode, …)
    }
}
```

A proxy and the thing it proxies differ somewhere, and `diaggap.rs` says TS2352
still has **22 cases it would convert alone**. Doing the widening for real is a
one-line substitution — `get_base_type_of_literal_type` on the source before
the two relation calls — and deleting the family test that stood in for it.

The composite decline stays: it is the real relation difference (§45), and
`getWidenedType`/`getRegularTypeOfObjectLiteral` remain unported, so a union
source is still a pair this port cannot ask about.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **1,154**. Forecast **0 to +10**: the proxy already covers the common case, so this is the tail where widening and family membership disagree |
| 2 | `checker_types` pass count unchanged at **3,660** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2352]` |
| 4 | TS2352's own WRONG stays at **0** — it shipped at zero in §31 and a substitution that is *more* faithful must not spend that |
| 5 | every other snapshot unchanged |

**Falsifier.** If WRONG rises, the family test was doing something besides the
widening — most likely standing in for `getWidenedType` as well, which is a
different function and is also unported. That would be a decline to re-derive
rather than to delete, and the wrong column's top row would name which.

---

## 47. TS2872 / TS2873 — `checkTruthinessOfType`, a rule with no types in it

`diaggap.rs` at `HEAD`: **TS2872 15 cases would convert alone** (35 contain it),
TS2873 **6**. Both come out of one function, `checkTruthinessOfType`
(`checker.go:12865`):

```go
if t.flags&TypeFlagsVoid != 0 { … return t }
semantics := c.getSyntacticTruthySemantics(node)
if semantics != PredicateSemanticsSometimes {
    c.error(node, IfElse(semantics == Always, This_kind_of_expression_is_always_truthy, …_falsy))
}
```

and `getSyntacticTruthySemantics` (`checker.go:12886`) reads **node kinds and
literal text**. There is no relation, no members table, no inference and — apart
from one identifier case — no type at all. §14's ordering rule says a rule that
reports on a syntactic fact has no incompleteness to leak, and this is the purest
instance of it since TS2369.

The predicate, arm for arm:

| shape | semantics |
|---|---|
| numeric literal, text neither `0` nor `1` | Always (`while (0)` and `while (1)` are deliberately allowed) |
| array literal, arrow, bigint literal, class expression, function expression, JSX element or self-closing element, object literal, regex literal | Always |
| `void …`, `null` | Never |
| string or no-substitution template, non-empty | Always; empty → Never |
| conditional expression | the **union** of both branches' semantics — `Always \| Never` is `Sometimes`, which is why `c ? 1 : 0` is silent |
| identifier resolving to `undefined` | Never |
| anything else | Sometimes |

### The six sites, and why the site list is the rule

Truthiness is *tested* at seven syntactic positions and upstream calls
`checkTruthinessExpression` at each: `if` (`checker.go:3804`), `do` (`:3950`),
`while` (`:3955`), a `for` statement's condition (`:3974`), a conditional
expression's condition (`:10936`), the operand of `!` (`:10888`), and the **left
operand of `&&` or `||`** (`:12356`, and only for `IsLogicalBinaryOperator`, so
`??` is excluded).

§15's trap — *"a register site inside a conditional is a precondition of the
rule"* — applies directly: the site list **is** the rule here, since the
predicate itself is context-free. A missing site is a missing diagnostic; an
extra site is a wrong one.

### TS1345 is ported with it and expected to convert nothing

`An expression of type 'void' cannot be tested for truthiness` has **no row at
all** in `diaggap.rs`. It is ported anyway because it is the same function's
early return, and because leaving it out would let a `void` expression fall
through to the syntactic arm. Its cost is bounded by the number of `void`-typed
conditions in the corpus, and if it produces wrong lines it is declined on its
own.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,154**. Forecast **+10 to +21** — the two rows are 15 and 6 and a syntactic rule has hit its row before (§10: 40 for 40) |
| 2 | `checker_types` pass count unchanged at **3,660** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2872, 2873, 1345]` |
| 4 | own WRONG ≤ **10**. §10 and §11 shipped syntactic rules at literally zero and this one has one type test in it |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column is not near zero, the defect is the **site
list** and not the predicate — the predicate is a table of node kinds and cannot
be half-right. The wrong lines would then all share a position shape (a `??`
left operand, a `for` header, a nested conditional) and name the site directly.

### Scored — **+21**, the top of the forecast, and a scanner fact worth 3 lines

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,154, forecast +10 to +21 | **1,175 / 5,488 = 21.41%** | pass, at the top |
| 2 | `checker_types` pass count 3,660 | **3,660 / 82.64%** | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | own WRONG ≤ 10 | **4** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` over `[2872, 2873, 1345]`: **CONVERTS 23 · RIGHT 196 · WRONG 4 ·
LOST 0.** TS1345 converted nothing, as forecast, and produced no wrong line
either.

**The forecast was 15 + 6 = 21 and the measurement is 23 conversions.** A row is
usually a ceiling; here two rules from one function each finished cases the other
was also blocking, so the union beats the sum of the columns. That is the
opposite of §2's warning about the single-code column, and it happens for the
same reason: the column says which code blocks a case, and a case blocked by
*both* TS2872 and TS2873 appears in neither row.

### The two wrong families, and both were positions

**`if (0.0) { }` — three lines, and the cause is in the scanner.** Upstream's
predicate is `node.Text() == "0" || node.Text() == "1"`, and a `NumericLiteral`'s
`Text` is the scanner's **normalised value**, not the written spelling: `0.0`,
`0x0` and `0e5` all read `"0"`. This port keeps the source text on the node, so
the literal comparison read `"0.0" != "0"` and reported `if (0.0)` as always
truthy. Comparing the *value* is the port of the same sentence and removed all
three.

> Fourth time this session that a faithful-looking transcription was answering a
> different question than upstream's: `is_error` vs `== errorType` (§43), a
> printing guard on a non-printing consumer (§42.1), a decline copied with its
> conclusion (§45), and now a literal's text against a literal's value.

**The remaining four are one error-span rule, and it is not this rule's.**
All four are `generatedContextualTyping`'s
`function named() { … } || undefined`, where upstream underlines **`named`** and
this port underlines `function`. That is `GetErrorRangeForNode`
(`scanner.go:2588`), which maps a node to the span its diagnostic is reported
at — for a `FunctionExpression`, `ClassExpression`, `VariableDeclaration`,
`PropertyDeclaration`, `EnumMember`, `ModuleDeclaration` and a dozen more, the
span is **the declaration's name**.

**This port has no such mapping.** Every `report` call site passes
`self.nodes.span(node)` directly, and the rules that are positionally right are
right because they were each written to pass the name node themselves (§43's
TS2564 passes `property.name`; §30's TS2300 build was this same defect found one
call site at a time). Doing it centrally is §48.

---

## 48. `GetErrorRangeForNode` — the mapping this port never had

Every checker diagnostic upstream goes through `c.error(location, …)` →
`NewDiagnosticForNode` → `scanner.GetErrorRangeForNode` (`scanner.go:2588`),
which maps the *node the rule names* to the *span the diagnostic is reported
at*. For fifteen node kinds that span is **the declaration's name**:

```go
case ast.KindVariableDeclaration, ast.KindBindingElement, ast.KindClassDeclaration, ast.KindInterfaceDeclaration,
    ast.KindModuleDeclaration, ast.KindEnumDeclaration, ast.KindEnumMember, ast.KindFunctionExpression,
    ast.KindGetAccessor, ast.KindSetAccessor, ast.KindTypeAliasDeclaration, ast.KindJSTypeAliasDeclaration,
    ast.KindPropertyDeclaration, ast.KindPropertySignature, ast.KindNamespaceImport:
    errorNode = ast.GetNameOfDeclaration(node)
case ast.KindClassExpression:
    errorNode = node.Name()
```

(`KindFunctionDeclaration` and `KindMethodDeclaration` fall through into the
same arm unless reparsed.)

**This port has no such mapping.** Every `report` call site builds its span with
`self.nodes.span(node)`, and the rules that are positionally right are right
because each was written to pass the name node itself. That is not a style
choice, it is an absent function, and this file has now paid for it three times
independently:

- §30 found it in TS2300 one `declare_into` call site at a time — **+11 cases**
  for recording the declaration's name node;
- §14's `report_implementation_expected` carries a private
  `declaration_name_of` that covers exactly three of the fifteen kinds;
- §47's last four wrong lines are `function named() { … } || undefined`, where
  upstream underlines `named`.

`extragap.rs`'s **displaced** column is the instrument that sizes this: a
displaced diagnostic is the same code missing elsewhere in the same file, which
is precisely what a wrong error span produces. At `HEAD` the column reads
TS1005 241, TS2304 37, TS2322 49, TS2552 58, TS1125 40, TS2300 21, TS1109 18 —
and the parser rows are §30's *"not this shape"*, but the checker rows are
candidates for exactly this defect.

### What is portable and what is not

The name arms need no source text and port directly. Four arms do need text or a
scanner and are **not** ported:

| arm | needs |
|---|---|
| `KindSourceFile` | `SkipTrivia` over the file text |
| `KindArrowFunction` | `getErrorRangeForArrowFunction`, a scanner walk |
| `KindCaseClause` / `KindDefaultClause` | `SkipTrivia`, plus the first statement's `pos` |
| `KindReturnStatement` / `KindYieldExpression` / `KindConstructor` | `GetRangeOfTokenAtPosition`, a scanner walk |

The checker holds spans and no file text (ADR-0034), so those keep today's
answer. Recorded rather than silently omitted: a future session finding a
displaced `return` diagnostic should look here first.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **1,175**. Forecast **+2 to +12**: §47 names four wrong lines in one case, and the rest is whatever the fifteen kinds reach |
| 2 | `checker_types` pass count unchanged at **3,660** |
| 3 | LOST == **0** — this is the leg that matters. A central span change touches **every** rule at once, and a rule that was accidentally right can be made wrong |
| 4 | the `displaced` column in `extragap.rs` does not grow |
| 5 | every other snapshot unchanged |

**Falsifier, and it is the reason leg 3 is stated so sharply.** If LOST is
non-zero, some rule is passing a *declaration* node where upstream passes the
*name* node — the two were the same answer only because neither was mapped, and
mapping one of them breaks the pair. The lost case would name which rule.

### Scored — **+0 on the suite, −4 wrong lines, and LOST 0 is the result**

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 1,175, forecast +2 to +12 | **1,175** — no conversion | **did not fire** |
| 2 | `checker_types` pass count unchanged | **3,665 / 82.70%** (the parallel workstream moved the baseline from 3,660 during this build; identical on both sides of the edit) | pass |
| 3 | **LOST == 0** | **0** | pass |
| 4 | `extragap.rs`'s `displaced` column does not grow | TS2304 37, TS2322 49, TS2552 58, TS1125 40, TS2300 21, TS1109 18, TS2454 1 — **unchanged** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` over `[2872, 2873, 1345]`: **WRONG 4 → 0, RIGHT 196 → 200**, and
`generatedContextualTyping` still fails for reasons this rule does not reach, so
the suite does not move.

**Leg 3 is the whole result and it is worth more than the leg 1 zero.** Thirty-six
report sites across fourteen modules were routed through one mapping in a single
edit, and **not one case regressed**. That is the direct evidence that no rule in
this workstream was passing a declaration node where upstream passes the name
node — the failure the falsifier described. The rules that were positionally
right were right by construction, not by accident, and the mapping is now where
upstream keeps it rather than replicated in whoever remembered.

**One site was reverted and it is the one that proves the split.**
`crate::expressions`'s template-escape test reads a span to compare its *width*
against the cooked text's length — a measurement, not a diagnostic position — and
it is on the **query** road. The blanket edit caught it because it matched the
same three words. It is the only one of the thirty-seven, and the reason it
matters is that a span read on the query road is exactly what leg 2 watches.

**What this is worth is what it stops costing.** §30 spent a build finding this
defect in TS2300 one `declare_into` call site at a time, for +11; §14 carried a
three-kind private copy of it; §47 paid four wrong lines. The next rule that
reports on a `VariableDeclaration`, `EnumMember`, `PropertySignature`,
`ClassExpression` or `NamespaceImport` gets the right span without knowing this
section exists. **A zero that removes a class of future defect is not the same
kind of zero as §44's**, which removed none and was kept on a forecast.

---

## 49. TS2365 — `Operator '{0}' cannot be applied…`, the two arms `reportOperatorError` has left

§45 ported one of `reportOperatorError`'s three callers (the equality arm). The
other two are TS2365, and `diagmissing.rs` splits its **14 sole-obstacle cases**
along exactly that line:

```
 19 lines  conformance/additionOperatorWithInvalidOperands       <- the `+` arm
 14 lines  conformance/additionOperatorWithTypeParameter         <- the `+` arm
 16 lines  conformance/comparisonOperatorWithNoRelationshipObjectsOnProperty
  7 lines  compiler/relationalOperatorComparable                 <- the relational arm
  4 lines  compiler/grammarAmbiguities1
```

### The `+` arm is a flag test with a relation as its fallback

`checker.go:12422`: `+` produces `number` when both operands are number-like,
`bigint` when both are bigint-like, `string` when **either** is string-like, and
`any` when either is `any`. If none of those holds, `resultType` is nil and
TS2365 is reported. `isTypeAssignableToKindEx` (`checker.go:27645`) is a flag
test first and an assignability check second, with `strict` short-circuiting on
`any`, `unknown`, `void`, `undefined` and `null`.

### The relational arm needs §45's substitution again

`checker.go:12465` reports unless `any` is involved, **or** both operands are
assignable to `number | bigint`, **or** neither is and the two are *comparable*.
`isTypeComparableTo` is still unported, so §45's substitution applies at its
third site — with `either_is_composite`, and with `relate_ternary` so an
undecidable pair is silence.

**A direction trap this arm has and §45 did not.** Here the relation is consulted
to decide **not** to report, so an `Unknown` collapsed to `false` would
*manufacture* a diagnostic. Every positive test in this rule must therefore read
"not a confident negative" rather than "a confident positive" — the mirror of
§25, and the reason the two arms are written with an explicit
`Ternary::NotRelated` comparison rather than a boolean helper.

### Two declines carried in from the sites upstream visits first

- **A nullish operand.** `checkNonNullType` runs before both arms
  (`checker.go:12419`, `:12467`) and reports TS2531/TS2533 instead. Declining a
  `null`- or `undefined`-flagged operand costs nothing this rule could have had.
- **`+` where either operand is string-like by flags.** This is upstream's own
  third disjunct and is a flag test, so it is exact rather than approximate.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,175**. Forecast **+5 to +14** — the row is 14 and the two arms split it roughly evenly |
| 2 | `checker_types` pass count unchanged at **3,681** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2365]` |
| 4 | own WRONG ≤ **25**. Higher than §45's bar because the relational arm sees every `<` in the corpus and the `+` arm every concatenation |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column is dominated by `+` on object or union
operands, the flag-first structure of `isTypeAssignableToKindEx` is not being
reproduced — a union of number-likes carries `UNION` and no `NUMBER_LIKE` flag
here, which is the same blindness §45's `either_is_composite` names, and the
answer is that decline rather than a narrower kind test.

### Scored — **+3 for zero wrong**, and leg 1 fired

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,175, forecast +5 to +14 | **1,178 / 5,488 = 21.47%** | **fired — below the forecast** |
| 2 | `checker_types` pass count 3,681 | **3,681 / 82.78%**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | own WRONG ≤ 25 | **0** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 2365: **CONVERTS 3 · RIGHT 91 · WRONG 0 · LOST 0.**

**Three of fourteen, and the four declines that got the wrong column to zero are
each worth more than the conversion count.** The first measurement read
**58 wrong lines**; every one was removed by a decline with a named owner, and
none by a threshold:

| decline | owner | cost |
|---|---|---:|
| a **type parameter** on either side | `getBaseConstraintOfType` — both `areTypesComparable` and `isTypeAssignableTo` reach a type parameter through its constraint and this relater does not follow it | 48 wrong lines removed; `additionOperatorWithTypeParameter` (14 lines, 1 case) becomes unreachable |
| an **ES symbol** operand | `checkForDisallowedESSymbolOperand` (`checker.go:12442`) reports **TS2469** in its place | 5 |
| `+=` | the assignment-target checks run first and answer `errorType` on failure, which then supplies a result type. `f += 1` on a class is **TS2629**, and this port models neither TS2629 nor TS2364, so its left operand keeps a real type | 3 |
| a union **containing** `undefined` | `checkNonNullType` strips it and reports TS18048 | 3 |

And one *widening* that was worth 42 right lines: §29's `object_against_primitive`
is asked inside `assignable_to_kind`, because `is_related_to` answers `Unknown`
for an object source against a primitive target and nothing structured is
assignable to `number` whatever its shape. `additionOperatorWithInvalidOperands`
went from 7 emitted lines to 16 of its 19 on that one call.

### The site trap fired for the third time in this file

The rule reported **nothing at all** on its first run, and the cause was not the
predicate: `check_operator_operands` was dispatched from
`check_node`'s general `BinaryExpression` arm, and an **earlier guarded arm**
already claims `+`, `<`, `>`, `<=` and `>=` for `check_nullable_operand`
(TS18050, whose operator list §40.5 and §40.6 extended to exactly those). A
`match` arm that fires first is a register site inside a conditional, and §15's
trap — *"the code was copied and the question it was answering was not"* — is
now on its third instance here, after `registerForUnusedIdentifiersCheck` and
`checkTruthinessExpression`'s seven call sites.

> **When a new rule measures zero, check that it ran before checking what it
> decided.** The first thing to print is not the predicate's inputs, it is
> whether the predicate was reached.

### What the remaining eleven cases need

`additionOperatorWithTypeParameter` (14 lines) and the type-parameter half of the
relational arm need **constraint following** in the relater — a `checker_types`
item. `parserGreaterThanTokenAmbiguity2/3/4` and `grammarAmbiguities1` are
parser-recovery shapes. `additionOperatorWithInvalidOperands` is `STILL SHORT` by
three lines out of nineteen. None of them is this rule's condition.

---

## 50. Two re-measurements on the false-positive side

`diaggap.rs` at `HEAD` puts the whole workstream's wrong column at **898 lines**,
and says removing every false positive would add **323 cases** to the reachable
set. Two of its rows were re-measured against the audit rule from the tenth
session's close: *"re-run a decline only after a build that changes what the port
can decide."* Seven builds have landed since.

### 50.1 TS2304's parse-error decline — §40.3's deletion STILL STANDS, at −6

`arrowFunctionsMissingTokens` 15 lines, `objectLiteralWithSemicolons1/2/3` 9,
`modifiersInObjectLiterals` 4, `conflictMarkerDiff3Trivia2` 4 — 155 wrong TS2304
lines and the parse-error families are their head. §40.3 **deleted** that decline
for +6, and re-adding it now measures **1,178 → 1,172**.

The +6 is not stale. Recorded so the third session to notice those case names
does not spend a coverage run on it.

### 50.2 TS18050 on `+` — a decline with a line number, worth 16 wrong lines

§40.5 added `+` to TS18050's operator list for +3. It also produced sixteen wrong
lines, and every one is in an addition:

```
6  additionOperatorWithUndefinedValueAndValidOperator
6  additionOperatorWithNullValueAndValidOperator
4  operatorAddNullUndefined
```

Reading upstream's arm rather than the row names why. `checkNonNullType` — the
function that emits TS18050 — is **conditional** for `+` and unconditional for
every other operator in the list:

```go
case ast.KindPlusToken, ast.KindPlusEqualsToken:
    …
    if !c.isTypeAssignableToKind(leftType, TypeFlagsStringLike) && !c.isTypeAssignableToKind(rightType, TypeFlagsStringLike) {
        leftType = c.checkNonNullType(leftType, left)
        rightType = c.checkNonNullType(rightType, right)
    }
```

(`checker.go:12418`.) `null + d` with `d: string` is a concatenation and the
null is fine; `null + a` with `a: any` is the same, because
`isTypeAssignableToKind` **without** `strict` lets `any` satisfy `StringLike`
(`checker.go:27652`). Both are exactly the wrong families above.

**Fourth instance this session of the same shape**: the port copied a call and
not the condition it sits inside — §15's trap, after
`registerForUnusedIdentifiersCheck`, `checkTruthinessExpression`'s site list, and
§49's match-arm ordering.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **1,178**. Forecast **0 to +4** — a decline converts only where the removed line was a case's last defect |
| 2 | `checker_types` pass count unchanged at **3,681** |
| 3 | LOST == **0** |
| 4 | TS18050's WRONG drops from **16** to ≤ 4 |
| 5 | every other snapshot unchanged |

### Scored — **+5**, and TS18050's wrong column goes 96 → **0**

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 1,178, forecast 0 to +4 | **1,183 / 5,488 = 21.56%** | pass, above |
| 2 | `checker_types` pass count 3,681 | **3,681 / 82.78%** | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | TS18050's WRONG 16 → ≤ 4 | **96 → 0** — and the 16 was a **reading error**, see below | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 18050: **CONVERTS 17 → 22 · RIGHT 718 → 718 ·
WRONG 96 → 0 · LOST 0.**

**Correction to this section's own bar.** Leg 4 was registered against "16 wrong
lines", read off the full-list run's printed wrong column — which is
**truncated**, `diag2307.rs` printing only the first forty. TS18050's real wrong
count was **96**. The bar's threshold was therefore meaningless as written, and
the number it should have carried is the isolated one. *Isolate the code before
quoting its wrong column* is the procedural lesson, and it belongs beside §2's
warning about the single-code column.

### 50.2's second correction: `Unknown` is not a positive here either

The string-like test was first written as `!= Ternary::NotRelated` — §25's
form, correct where the *report* depends on a negative. Here it decides whether
to **stay silent**, so the reading is the mirror: an enum operand answers
`Unknown` for `enum → string` and is not assignable to `string` upstream, so
reading `Unknown` as string-like declined 28 correct lines. `== Ternary::Related`
restored them and gained five conversions.

> **The three-valued relation has two correct projections and which one is right
> is a property of the *caller's* direction.** §25: reporting on a negative needs
> `!= Related` collapsed as `== NotRelated`. §49: staying silent on a positive
> needs `!= NotRelated`. §50.2: staying silent on a positive that upstream reads
> off a *binary* relation needs `== Related`. Three call sites, three different
> collapses, all of `relate_ternary`.

### 50.3 TS18050 is chosen by the **node**, not by the type — the other 74

The remaining wrong lines were 64 in `comparisonOperatorWithOneOperandIsUndefined`
alone, and the baseline names the cause: upstream reports **TS18048**,
`'x' is possibly 'undefined'`, where this port reported TS18050.

`reportObjectPossiblyNullOrUndefinedError` (`checker.go:7455`) picks the message
from the **node**:

```go
if node.Kind == ast.KindNullKeyword { … The_value_0_cannot_be_used_here, "null" }
if ast.IsIdentifier(node) && nodeText == "undefined" { … The_value_0_cannot_be_used_here, "undefined" }
… X_0_is_possibly_undefined / X_0_is_possibly_null_or_undefined / Object_is_possibly_undefined
```

So TS18050 is the message for a **written** `null` or a **written** `undefined`,
and every other nullable operand — a variable of type `typeof undefined`, an
optional property, a narrowed union — is a different code with the same *facts*.
§32 built the rule on the type and got the common case right for the wrong
reason.

Gating on the node instead takes the wrong column to **zero** with no right line
lost. It also names the next rule exactly: the same function's other four
messages (**TS18048** 7 cases, **TS18049** 3, **TS2532** 3) are the same facts
test with a different branch, and `getTypeFacts` is what stands between here and
them.

---

## 51. `reportObjectPossiblyNullOrUndefinedError`'s other five messages

§50.3 established that `checkNonNullType`'s reporter picks its **message** from
the node and its **branch** from the type's facts. TS18050 is one of six:

| node | facts | code |
|---|---|---|
| `null` keyword | — | **TS18050** `The value 'null' cannot be used here.` |
| identifier spelled `undefined` | — | **TS18050** `The value 'undefined' cannot be used here.` |
| any other entity name, <100 chars | undefined only | **TS18048** `'{0}' is possibly 'undefined'.` |
| " | null and undefined | **TS18049** `'{0}' is possibly 'null' or 'undefined'.` |
| " | null only | **TS18047** `'{0}' is possibly 'null'.` |
| not an entity name | undefined / both / null | **TS2532 / TS2533 / TS2531** |

`diaggap.rs`: TS18048 **7** cases, TS18049 **3**, TS2532 **3**.

### The population is much larger than TS18050's, and so is the exposure

TS18050's gate is *"the type **is** `null` or `undefined`"* — a flag equality,
with nothing between the declaration and the answer. These five ask *"the type
**may be** `null` or `undefined`"*, which is a question about the **narrowed**
type at the reference, and every narrowing this port does not perform leaves
`undefined` alive. §16's `controlFlowAliasing` decline and §8's TS2454 residual
are both that same population seen from other rules.

So this build is expected to need declines that §32's did not, and the honest
statement of its scope is: **the same six sites, the same facts test, and a
guard against the narrowing gap.**

### The one deliberate narrowing of scope

Upstream calls `checkNonNullType` from dozens of places — property access,
element access, call targets, `in`, `instanceof`, spread. This rule fires only
at the **binary operand** sites `crate::nullable_operand` already visits. That is
silence elsewhere, never a wrong answer, and it keeps the measurement about the
message split rather than about a site sweep.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,183**. Forecast **+2 to +10** of the 13 |
| 2 | `checker_types` pass count unchanged at **3,681** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [18047, 18048, 18049, 2531, 2532, 2533]` |
| 4 | own WRONG ≤ **40**, and **isolated** rather than read off a truncated list (§50's correction) |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column is dominated by references upstream narrowed —
guarded by `if (x)`, by an early return, by a discriminant — the rule is
measuring this port's flow gap rather than upstream's facts, and the decline is
the one §8 already wrote: a reference a condition on the same name dominates.

### Scored — **+1**, and the exposure the section predicted did not arrive

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,183, forecast +2 to +10 | **1,184 / 5,488 = 21.57%** | **fired — below the forecast** |
| 2 | `checker_types` pass count unchanged | **3,682 / 82.80%** (the parallel workstream's, unchanged by this edit — `checker_types.snap` is byte-identical) | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | own WRONG ≤ 40, isolated | **2** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` over `[18047, 18048, 18049, 2531, 2532, 2533]`: **CONVERTS 1 ·
RIGHT 77 · WRONG 2 · LOST 0.**

**The section's own risk assessment was wrong in the safe direction, and that is
worth recording.** It predicted that asking *"may be nullish"* rather than *"is
nullish"* would expose the rule to every narrowing this port does not perform,
and named §16's and §8's populations. Two wrong lines arrived —
`narrowingPastLastAssignment` and `jsxEsprimaFbTestSuite`, one each — because the
**site restriction does the work the decline would have**: a binary operand is
rarely the place a narrowed reference is read, and the narrowing-heavy
populations reach `checkNonNullType` through property access and call targets
instead.

**77 right lines for 1 conversion** is the §2 ratio again, from the other side:
the rows were 7 + 3 + 3 = 13 cases and the twelve that did not convert are
`STILL SHORT` — they need the *other* call sites, not a better facts test. That
is the item this build hands on: `checkNonNullType` at property access, element
access and call targets, where the same six messages already work.

---

## 52. TS2464 — the computed property name's type

`checkComputedPropertyName` (`checker.go:26802`), **16 cases blocked on it
alone**:

```go
if links.resolvedType.flags&TypeFlagsNullable != 0 ||
    !c.isTypeAssignableToKind(links.resolvedType, TypeFlagsStringLike|TypeFlagsNumberLike|TypeFlagsESSymbolLike) &&
        !c.isTypeAssignableTo(links.resolvedType, c.stringNumberSymbolType) {
    c.error(node, diagnostics.A_computed_property_name_must_be_of_type_string_number_symbol_or_any)
}
```

The comment above it is the specification: *"This will allow types number,
string, symbol or any. It will also allow enums, the unknown type, and any union
of these types (like `string | number`)."* The second disjunct — assignability to
`string | number | symbol` — is what admits the union case that a flag test
cannot see through, which is §45's `either_is_composite` population arriving as
an upstream *feature* rather than a port limitation.

The error node is the `ComputedPropertyName` itself, brackets included.

`isInvalidComputedPropertyName` (`checker.go:26796`) short-circuits to
`errorType` — and therefore to silence — for `[a in b]` inside a type literal,
class or interface that is not an accessor: a mapped-type head the parser
recovered as a computed name.

### Direction, for the fourth time in this file

The rule reports on a **negative**, so §25's collapse applies: both tests must be
a *confident* negative before it fires, and `Ternary::Unknown` on either is
silence. That is the opposite of §49's and §50.2's readings, and the reason each
one is stated where it is used rather than in a shared helper.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,184**. Forecast **+4 to +14** of the 16 |
| 2 | `checker_types` pass count unchanged at **3,682** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2464]` |
| 4 | own WRONG ≤ **15** |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column fills with `unique symbol` and enum-member
computed names, the flag set is short a `UNIQUE_ES_SYMBOL` or an `ENUM_LITERAL`
that upstream's `…Like` aliases carry and this port's do not — a table
transcription error rather than a decline.

### Scored — **+3 for zero wrong**, and a decomposition that was sound and worse

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,184, forecast +4 to +14 | **1,187 / 5,488 = 21.63%** | **fired, marginally** |
| 2 | `checker_types` pass count 3,682 | **3,682 / 82.80%**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | own WRONG ≤ 15 | **0** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 2464: **CONVERTS 3 · RIGHT 18 · WRONG 0 · LOST 0.**

**A measured negative worth the line it takes.** The union target
(`string | number | symbol`) is one the relater often answers `Unknown` for, and
the obvious repair is to decompose it: a non-union source is assignable to a
union exactly when it is assignable to **some** constituent, so asking the three
separately is *sound*. Measured, it added **two wrong lines and no right ones**
and was reverted. Whatever the relater cannot decide about the union it cannot
decide about the constituents either, and the decomposition only widened the set
of pairs it was willing to call a confident negative.

The thirteen cases still blocked are `computedPropertyNames5–9_ES6` and
`symbolProperty3/54/59` — `[b]` with `b: boolean`, `[[]]`, `[{}]`, `[s]` with
`s = Symbol`. Every one is a **confident negative upstream and an `Unknown`
here**, which puts them behind the relation rather than behind this rule.

---

## 53. TS2540 — the read-only assignment target

`checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11376`):

```go
if c.isAssignmentToReadonlyEntity(node, prop, assignmentKind) {
    c.error(right, diagnostics.Cannot_assign_to_0_because_it_is_a_read_only_property, right.Text())
```

**12 cases blocked on it alone**, and `diagmissing.rs` shows what they are:
`constDeclarations-access3/4/5` (54 lines — `namespace M { export const x }` then
`M.x = 1` in eleven spellings), `externalModuleImmutableBindings` (16),
`readonlyPropertySubtypeRelationDirected` (8),
`intersectionTypeReadonly` (4), `incrementOperatorWithEnumType` (4),
`privateNameAccessors`, `privateNameStaticAccessors`,
`readonlyAssignmentInSubclassOfClassExpression`.

`isReadonlySymbol` (`checker.go:13849`) is a list of five facts about a symbol
and its declarations, and this port can read four of them off the declaration
directly:

| upstream | here |
|---|---|
| `CheckFlagsReadonly` | **not ported** — a computed flag on synthesised union/intersection properties |
| `Property` with a `readonly` modifier | the declaration's modifier list |
| `Variable` with `NodeFlagsConstant` | the declaration list's `CONST` flag |
| `Accessor` with no set accessor | the symbol's declarations |
| `EnumMember` | the declaration's kind |
| `isReadonlyAssignmentDeclaration` | **not ported** — `Object.defineProperty` |

The two unported rows are both *narrowing* omissions: they can only cost a
missing diagnostic. `intersectionTypeReadonly` is the first one's population and
is expected to stay short.

### The constructor exception is the rule's only real decision

`isAssignmentToReadonlyEntity` (`checker.go:27294`) permits `this.x = …` inside
the constructor of the class that declares `x`. Getting it wrong is a wrong line
on ordinary, correct code — the most expensive kind — so the port reproduces the
whole disjunction: the control-flow container must be a constructor, and the
property's declaration must be a member of that constructor's class or one of
its parameters.

`module.exports` is exempted before anything else (`checker.go:27289`), which
this port reaches as "the receiver resolves to nothing".

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,187**. Forecast **+4 to +12** |
| 2 | `checker_types` pass count unchanged at **3,682** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2540]` |
| 4 | own WRONG ≤ **20** |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column is `this.x = …` lines inside constructors, the
exception is mis-ported and the rule is reporting on correct code; that is a
revert rather than a decline, because the population it would break is every
class in the corpus with a `readonly` field.

### Scored — **+1 for zero wrong**, 71 right lines, and the falsifier stayed quiet

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,187, forecast +4 to +12 | **1,188 / 5,488 = 21.65%** | **fired** |
| 2 | `checker_types` pass count 3,682 | **3,682**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | own WRONG ≤ 20 | **0** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 2540: **CONVERTS 1 · RIGHT 71 · WRONG 0 · LOST 0.**

**The constructor exception held: zero wrong lines against a corpus full of
`readonly` fields.** That was the falsifier the section was most worried about
and it did not fire once.

**71 right lines for one case is §2's ratio at its sharpest so far**, and
`diagmissing.rs` shows why directly: the three `constDeclarations-access` cases
(54 of the 71) now emit every TS2540 the baseline records and **still fail** —
they are `STILL SHORT` on other codes. A row of 12 cases produced one because
eleven of them wanted this code *and something else*.

The remainder is three named families, and two were predicted in the bar:
`externalModuleImmutableBindings` (16 lines — an **imported binding** is
read-only through its alias symbol, which this rule does not follow),
`intersectionTypeReadonly` and `intersectionsAndReadonlyProperties` (the
`CheckFlagsReadonly` row, named as expected to stay short), and
`incrementOperatorWithEnumType` (`E.a++` on an enum member).

---

## 54. `diagreach.rs` — the number this workstream had never measured

Eleven builds this session ended the same way: a row of 12–16 cases converting
3, 1, 3, 1. §53 is the extreme — **71 right lines for one conversion**. The
cause is always the same and §2 named it: *the single-code column says which
code a case is blocked on, not how many of that code the case needs*, and a case
blocked by TS2540 **and** TS2322 appears in neither of their sole-obstacle rows.

`examples/diagreach.rs` (new) asks the complementary question. A case counts
when

- it reports **nothing** the baseline does not, and
- every diagnostic it is missing carries a code some rule in this port
  **already emits**.

Such a case needs no new rule at all — only the rules it already triggers,
reporting more completely.

```
cases reachable by deepening existing rules: 1,334
```

**1,334 of the 4,300 still failing**, against 1,188 passing. That is the largest
single number this workstream has produced and it had never been taken.

```
== per code: which RULE to deepen ==
TS2322  548     TS2345  138     TS2339  128     TS2304   88     TS2741   81
TS2454   58     TS2353   52     TS2554   37     TS2564   36     TS2411   34
TS2352   32     TS7006   28     TS2416   27     TS2430   26     TS2365   21
```

### What it changes about the board

**It does not contradict §5's TS2322 refusal — it prices it.** 548 of the 1,334
want TS2322 lines this port does not emit, and the reason it does not emit them
is the structural relation and the members table, exactly as recorded. What is
new is the *size*: the assignability family alone is worth more cases than the
whole suite currently passes.

**And 786 of the 1,334 do not involve TS2322 at all.** The second tier —
TS2345 138, TS2339 128, TS2304 88, TS2741 81 — is the same subsystem seen from
four more doors, but TS2304 (88), TS2454 (58), TS2554 (37), TS2564 (36) and
TS7006 (28) are **not**: those five rules are incomplete for reasons already
written down in their own sections, and none of them needs the relation.

**The instrument to run first from now on is this one**, and `diaggap.rs`'s
single-code column drops to what it always was — an ordering over *new* rules.
`diagmissing.rs` then says which lines a chosen rule is short, and `diagcase.rs`
reads one.

---

## 55. TS2304 in **type** positions — the first item §54 chose

§54's ranking puts TS2304 at **88 cases** reachable without the relation, and
`diagmissing.rs` says what those lines are:

```
21  compiler/errorsInGenericTypeReference
18  conformance/parserGenericsInVariableDeclaration1
10  conformance/typeParameterUsedAsTypeParameterConstraint4
 7  compiler/unknownSymbols1        var y: asdf;  function foo(x: asdf): asdf
 6  compiler/typeofProperty
 6  compiler/typeCheckTypeArgument
```

Every one is a name in a **type** position. §7's rule is bounded by
`is_value_reference` — *"the identifier must be the node sitting in one of the
parent's expression-typed fields"* — which is exactly right for what it does and
excludes the other half of upstream's `Cannot find name`.

Upstream reaches those through a different road: `getTypeFromTypeReference` →
`resolveTypeReferenceName` → `resolveEntityName`, whose failure arm is the same
`onFailedToResolveSymbol` §7 already ports — missing lib first (TS2583), then a
spelling suggestion (TS2552), then TS2304.

### The bound: a bare identifier that resolves under no meaning at all

- **Simple names only.** A qualified `A.B` fails differently (TS2694,
  `Namespace '{0}' has no exported member '{1}'`), and a wrong code at a right
  position is what §33 and §7 both spent builds removing.
- **No meaning at all.** A name that resolves as a *value* but not a type is
  TS2749; as a *namespace*, TS2709. §7 declines both for the same reason and
  this arm inherits the decline verbatim.
- **`is_value_reference` still owns the value half.** The two arms are disjoint
  by construction: one fires on the `type_name` slot of a `TypeReferenceNode`,
  the other never looks at that slot.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,188**. Forecast **+10 to +30** — 79 cases are blocked on TS2304 alone and this is the majority shape among them |
| 2 | `checker_types` pass count unchanged at **3,682** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2304, 2552, 2583]` |
| 4 | the three codes' combined WRONG grows by ≤ **40** from its current isolated figure |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column fills with names that *do* resolve — type
parameters, interfaces declared later in the file, `lib` types — then
`resolve_name` under `SymbolFlags::TYPE` is not the meaning upstream resolves a
type reference under, and the answer is to widen the meaning rather than to
decline positions.

### Scored — **+43, the session's largest single build**, and leg 4 fired

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,188, forecast +10 to +30 | **1,231 / 5,488 = 22.43%** | pass, **above** |
| 2 | `checker_types` pass count 3,682 | **3,682**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** (the 2 shown are the pre-existing pair, unchanged) | pass |
| 4 | the trio's WRONG grows by ≤ 40 | **444 → 495, +51** | **fired** |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` over `[2304, 2552, 2583]`: **CONVERTS 151 → 194 · RIGHT 1,846 →
2,367 · WRONG 444 → 495 · LOST 2 → 2.**

### The two declines the first measurement bought, in the order the column named them

**1. A name an enclosing declaration introduces as a type parameter — 39 lines.**
`class C<T> { static m(): T }` is upstream's **TS2302**, *"Static members cannot
reference class type parameters"*: the resolver **finds** `T` and the position is
the error. `resolve_name` here does not put a class's type parameters in a static
member's scope at all, so the same shape arrives as "cannot find". An `infer T`
name is in scope for its whole conditional type and is the same story.
`genericClassWithStaticsUsingTypeArguments`, `classTypeParametersInStatics`,
`staticMethodReferencingTypeArgument1`, `typeParametersInStatic*` and
`conditionalTypes1`.

**2. The suggestion arm is dropped in type positions — 143 lines.**
`getSuggestedSymbolForNonexistentSymbol` searches the names in scope **with the
requested meaning**; `Binder::names_in_scope` is meaning-blind. In a value
position that is harmless, because nearly every name in scope is a value. In a
type position it offers a nearby *variable* for a missing *type*:
`parserRealSource13` alone was **105 wrong TS2552 lines** for one missing `AST`,
every one a TS2304 upstream. Dropping the arm converts those 143 lines from
wrong to right and costs **one** case. It returns when `names_in_scope` takes a
meaning.

### Leg 4 fired and the build ships — with the arithmetic that says so

51 new wrong lines against 43 conversions is **0.84 gained per wrong**, which is
below the 1.0 this file has used as a floor. `diagreach.rs` (§54) settles it,
and the first reading of it was **wrong in a way worth recording**:

```
before §55   1,188 passing   1,334 reachable
after  §55   1,231 passing   1,281 reachable
```

1,334 − 1,281 = 53 looks like 53 cases lost to new false positives. It is not:
**a converted case leaves the reachable set by definition**, because `reachable`
counts only *failing* cases. 1,334 − 43 = 1,291, so the true cost is
**1,291 − 1,281 = 10 cases** pushed out of reach by the new wrong lines, against
43 banked. That is 4.3 gained per case put at risk, and it is the number the
ratio-per-wrong-*line* was hiding.

> **Count what a wrong line costs in cases, not in lines.** 51 wrong lines
> landed in ten cases, and six of those ten were already carrying other extras.
> Every previous "gained per wrong" figure in this file is a line ratio; §54's
> instrument is what makes the case ratio computable, and the two differ by
> whatever the concentration happens to be.

The remaining new families are diffuse — `parserindenter` 11,
`parserRealSource12` 5, `privacyImportParseErrors` 4, then ones and twos across
fourteen more cases.

---

## 56. TS2554 for constructors — optional parameters and the inherited constructor

`diagreach.rs` puts TS2554 at **37 cases** and `diagmissing.rs` splits the
21 sole-obstacle ones into two named shapes plus a tail:

```
7  conformance/callWithMissingVoid              a `void` parameter is optional
5  conformance/classWithConstructors            `new C()` against `constructor(x: string)`
4  conformance/classWithBaseClassButNoConstructor   the constructor is INHERITED
```

`sole_constructor_parameters` (§34) has two gaps the call arm does not:

1. **It counts every non-rest parameter as required.** The call arm computes a
   `(minimum, maximum)` pair — the minimum being the index of the first
   parameter that is optional or has an initialiser — and the `new` arm
   compares against `parameters.len()`. That is a *missing* diagnostic for
   `new C()` against `constructor(x?: string)`… and a **wrong** one is impossible
   only because the equality test happens to fail in the safe direction; the
   pair is what makes it exact.
2. **It stops at the class's own members.** `getSignaturesOfType` on a class
   with no constructor of its own resolves the **base**'s
   (`classWithBaseClassButNoConstructor`). Walking one `extends` link is the
   whole of what those four cases want, and the walk is the same
   `sole_plain_base_type` §27 built.

`callWithMissingVoid`'s seven are a third thing and are **not** built here:
`x.f()` where `f(t: void)` is legal because a `void` parameter is treated as
optional (`hasEffectiveRestParameter`/`getMinArgumentCount`'s `void` arm). That
is a *type* test in the middle of an otherwise syntactic rule, and it is left to
its own measurement.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,231**. Forecast **+3 to +9** — the two shapes are 9 of the 21 |
| 2 | `checker_types` pass count unchanged at **3,682** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2554, 2555]` |
| 4 | own WRONG grows by ≤ **10** |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column fills with `new` on a class whose base is
generic or merged, the `extends` walk is following a link upstream resolves to a
different signature set, and the decline is "a base with type parameters" rather
than the walk itself.

### Scored — **+1**, and the wrong column fell by 25

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,231, forecast +3 to +9 | **1,232 / 5,488 = 22.45%** | **fired** |
| 2 | `checker_types` pass count 3,682 | **3,682**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | own WRONG grows by ≤ 10 | **31 → 6**, a *fall* of 25 | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` over `[2554, 2555]`: **CONVERTS 13 → 14 · RIGHT 44 → 48 ·
WRONG 31 → 6 · LOST 0.**

**The optional-parameter pair was not a missing-diagnostic fix, it was a
false-positive fix**, and the section got that backwards. The bar reasoned that
counting every non-rest parameter as required could only *miss* — `new C()`
against `constructor(x?: string)` compares `0 == 1` and reports where upstream
does not. Twenty-five of the rule's thirty-one wrong lines were exactly that,
and reading the minimum off the first optional parameter removed all of them.

> **A count compared for equality has two failure directions and the bar only
> reasoned about one.** `arguments == expected` is wrong for *too few* whenever
> a parameter is optional and wrong for *too many* never — the asymmetry is why
> the call arm has carried a `(minimum, maximum)` pair since §20 and the `new`
> arm did not.

The inherited-constructor walk landed with it and is what the single conversion
is: one `extends` link, declining a generic base, a merged one, or a chain
deeper than eight.

`callWithMissingVoid`'s seven cases stay open by design — a `void` parameter is
optional through `getMinArgumentCount`'s type test, which is a type question in
the middle of an otherwise syntactic rule.

---

## 57. `names_in_scope` takes a meaning — §55's refusal retired, **+7**

§55 refused the spelling-suggestion arm in type positions at **143 wrong lines
for 1 conversion**, and named its own return condition in the same sentence:
*"Returns when `Binder::names_in_scope` takes a meaning."* This is that.

`getSuggestedSymbolForNonexistentSymbol` searches `symbolsInScope(location,
meaning)` and **always** passes one. `Binder::names_in_scope` collected every
key of every table it walked. `names_in_scope_with_meaning` filters each entry by
the symbol's flags; the old function is now a call to it with every flag set, so
nothing that did not ask changed.

**The bar is the refusal's stated condition rather than a fresh forecast**, which
is the whole point of writing a refusal with one: legs 2–5 stand as §55's, and
leg 1 is *"the 143 lines become right and the 1 case comes back"*.

### Scored — and the value arm wanted it too

| | CONVERTS | RIGHT | WRONG | suite |
|---|---:|---:|---:|---:|
| §55 as shipped (no suggestion in type positions) | 194 | 2,367 | 495 | 1,232 |
| + the type arm, meaning-filtered | 199 | 2,395 | **467** | 1,237 |
| + **the value arm**, meaning-filtered | **201** | **2,406** | **456** | **1,239** |

**+7 cases and the wrong column falls by 39.** Legs 2, 3 and 5 hold —
`checker_types` 3,682 and byte-identical, LOST 0.

**The value arm was the surprise.** §33 built the suggestion machinery for value
positions and measured it there, and this file has said twice that a
meaning-blind search is *harmless* in a value position "because almost every name
in scope is a value". It is not: filtering it is worth **2 more cases and 11
fewer wrong lines**. Interfaces, type aliases and type parameters are near
neighbours of variable names often enough to matter, and the sentence excusing
the omission was an argument standing in for a measurement — §40.5's shape,
recorded there and repeated here.

> **A refusal that names its return condition is an asset; one that does not is
> a dead end.** Third retirement in this file by the procedure §21 set (§9's,
> §22's, and now §55's), and the first where the condition was named in the same
> session it was met.

---

## 58. `x.constructor === C` joins the unported-narrowing list

`examples/extraonly.rs` (new) prints the cases whose **only** defect is a
diagnostic the baseline does not record — every one is a single false positive
away from passing. There are **58**, and TS2454 owns 15 of them:

```
38  TS1005 (parser)   15  TS2454   15  TS1012 (parser)   11  TS2322   10  TS2304
```

`typeGuardConstructorPrimitiveTypes` and
`typeGuardConstructorNarrowPrimitivesInUnion` are the whole of TS2454's share and
they are one shape:

```ts
let var1: string | number | boolean | any[] | symbol | bigint;
if (var1.constructor === String) {
    var1;                    // <- reported here; upstream reads `string`
}
```

Upstream narrows by the **constructor property** (`narrowTypeByConstructor`), so
inside the branch `var1` no longer carries `undefined`. This port does not, so
the flow type still does and the reference reads as used-before-assigned.

§8 built `reference_is_guarded_by_a_condition_on` for exactly this asymmetry —
*"every narrowing this port does not model removes `undefined` upstream and
leaves it here, and each of them is written as a guard"* — and gated it on the
condition containing **a call or an `instanceof`**, the two mechanisms known at
the time. A `.constructor === C` comparison is a third, and the list was always
meant to grow: `subtree_has_unported_narrowing`'s doc comment names the
mechanisms rather than describing a syntax.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,239**. Forecast **+2** — `extraonly.rs` says exactly two cases carry this and nothing else |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2454]` |
| 4 | TS2454's own WRONG **falls**; its RIGHT must not |
| 5 | every other snapshot unchanged |

**Falsifier.** If RIGHT falls, the guard is declining references upstream does
report — `.constructor` appears in conditions that narrow nothing — and the
right shape is the property name rather than the subtree.

### Scored — **+2**, the forecast to the case

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,239, forecast **+2** | **1,241 / 5,488 = 22.61%** | pass, exactly |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | WRONG falls, RIGHT does not | **30 → 15**, RIGHT **3,784** unchanged | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

**`extraonly.rs` forecast the conversion count exactly, and that is the point of
it.** Every other instrument in this workstream forecasts a *ceiling*:
`diaggap.rs`'s single-code column has been high by a factor of five all session,
and `diagreach.rs` counts cases needing an unknown amount of work. A case blocked
by an extra diagnostic **alone** needs exactly one thing — that diagnostic gone —
so the list is a forecast rather than an ordering. It is the only one in the
file that has ever been exact.

Its remaining 56 cases: **TS1005 38 and TS1012 15 are the parser's**, not this
workstream's; TS2322 11, TS2304 10, TS7006 4, TS2345 3 and TS2307 3 are.

---

## 59. TS2345 reports the **first** failing argument, not every one

`extraonly.rs`'s TS2345 rows are three cases and one sentence:

```ts
function foo(a: string, b?: number) {}
foo(1, 'bar');      // upstream: ONE TS2345, on the `1`
```

`getSignatureApplicabilityError` (`checker.go`) walks the arguments and
**returns on the first failure**; `checkApplicableSignature` is a predicate, not
a reporter, and a signature that fails is not asked again. This port's loop
reports every mismatched argument, so `foo(1, 'bar')` reads two lines where
upstream reads one — and under the exact-multiset rule the extra fails the case
just as surely as a missing one would.

`functionCall11`, `functionCall12` and `objectLitTargetTypeCallSite` are exactly
that, and they are three of `extraonly.rs`'s 58.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,241**. Forecast **+3** — `extraonly.rs` names them, and it forecast §58 to the case |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2345]` |
| 4 | TS2345's WRONG falls and its RIGHT falls by **at most** the same amount — a second report on the same call is never *right* |
| 5 | every other snapshot unchanged |

**Falsifier.** If RIGHT falls by more than WRONG, upstream does report more than
one argument somewhere — an overload set retried per candidate — and the stop is
per-*signature* rather than per-call.

### Scored — **+2 of the forecast 3**

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,241, forecast +3 | **1,243 / 5,488 = 22.65%** | pass, one short |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST == 0 | **1 → 1** — the rule's pre-existing loss, measured on **both** sides of the edit and unchanged by it | pass |
| 4 | WRONG falls, RIGHT falls by at most as much | **12 → 10**, RIGHT **37 → 37** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 2345: **CONVERTS 17 → 19 · RIGHT 37 → 37 ·
WRONG 12 → 10 · LOST 1 → 1.**

**Leg 3 needed both sides to read.** `diag2307.rs`'s LOST column is a property of
the *rule*, not of the edit — it reconstructs the "before" side by removing the
code entirely, so a loss the rule already had shows on both runs. Measuring only
the after side would have read it as this build's. The tenth session's handoff
says "LOST must read 0 in every measurement"; the accurate form is **LOST must
not grow**, and a rule with a standing loss needs its number carried.

`objectLitTargetTypeCallSite` is the third case `extraonly.rs` named and did not
convert: removing the second TS2345 leaves it exact on this code and short
elsewhere. `extraonly.rs` counts cases blocked by an extra **alone**, and a case
can carry two extras of the same code — which is what this one did.

---

## 60. A function expression's own name is in scope inside it

`extraonly.rs`'s TS2304 rows include `recursiveNamedLambdaCall`:

```js
(function doScrollCheck() {
    …
    setTimeout( doScrollCheck, 50 );   // <- reported; the name IS in scope
})();
```

A **named function expression** binds its own name inside its body — upstream's
binder gives the expression a symbol whose name is visible to the function's
own locals (`bindFunctionExpression` → `bindAnonymousDeclaration` with the
function's name). This binder does not, so the recursive reference reaches
`resolve_name` and fails.

Declined in the rule rather than repaired in the binder: `tsr_binder` is shared
with the query road and adding a scope entry there moves `checker_types`, which
is the other workstream's. The decline names its owner and costs nothing — an
identifier that matches an enclosing function expression's own name is never a
`Cannot find name` upstream.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **1,243**. Forecast **+1** — `extraonly.rs` names one case |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST must not grow |
| 4 | TS2304's WRONG falls |
| 5 | every other snapshot unchanged |

### Scored — **+1**, the forecast again exact

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 1,243, forecast +1 | **1,244 / 5,488 = 22.67%** | pass, exactly |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST must not grow | **2 → 2** | pass |
| 4 | TS2304's WRONG falls | **456 → 451**, RIGHT **2,406** unchanged | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

**Three consecutive builds forecast off `extraonly.rs` and three consecutive
exact numbers** (§58 +2, §59 +2 of 3, §60 +1). No other instrument in this file
has been right twice running. The reason is structural and is worth stating
plainly: a case blocked by an extra **alone** has exactly one thing wrong with
it, so counting those cases *is* the forecast. Every other column counts cases
that need an unknown amount of work and can only bound it.

---

## 61. `this[key]` with a literal-union index marks the properties it can reach

`extraonly.rs`'s TS6133 rows are one case,
`typeGuardNarrowsIndexedAccessOfKnownProperty9`:

```ts
class C1 {
    private a = "a";     // ok upstream, reported here
    private b = "b";     // ok upstream, reported here
    private c = "c";     // error unused prop  <- both agree
    private d = "d";     // error unused prop  <- both agree
    getValue(key: "a" | "b") { return this[key]; }
}
```

`markPropertyAsReferenced` runs inside the **element access resolution**, once
per property the index type can name (`checker.go:27033`). `crate::unused` notes
member names *syntactically* (`note_member_name_at`) and for an element access
notes the **argument's own identifier text** — `key`, not `a` and `b`.

The repair stays inside the syntactic design: ask the argument for its type and,
when it is a string literal or a union of them, note each literal's value.
Anything else keeps today's answer, which is the missing-diagnostic direction.

### 61.1 A class-expression member's contextually typed parameter

`extraonly.rs`'s TS7006 rows are also one case,
`contextuallyTypedClassExpressionMethodDeclaration01`:

```ts
function getFoo2(): Foo {
    return class {
        static method1 = (arg) => { … };   // `arg` is contextually typed by Foo
    };
}
```

The class expression is contextually typed by the function's declared return
type, so `arg` is not an implicit `any`. Contextual typing is `STATUS.md` §5's
standing refusal, and the *decline* is one line: a parameter of a function or
arrow that initialises a member of a **class expression** is contextually typed
whenever the class expression is, and this port cannot tell whether it is.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **1,244**. Forecast **+2** — `extraonly.rs` names one case each and has been exact three times |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST must not grow |
| 4 | TS6133's and TS7006's WRONG fall; neither RIGHT falls |
| 5 | every other snapshot unchanged |

### Scored — **+1 of 2**, and §61.1 is REFUSED at 4 right lines for 4 wrong

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 1,244, forecast +2 | **1,245 / 5,488 = 22.69%** | pass, one short |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST must not grow | **0 → 0** | pass |
| 4 | both WRONGs fall, neither RIGHT falls | §61 pass; **§61.1 fired** | see below |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` over the unused and implicit-any codes: **WRONG 17 → 15,
RIGHT 446, CONVERTS 125, LOST 0.**

**§61.1 is refused on its own leg.** Declining every class-expression member's
parameters measured **RIGHT 446 → 442 and WRONG 15 → 11** — eight lines
suppressed, half of them correct — and the suite did not move.
`contextuallyTypedClassExpressionMethodDeclaration01` carries **both**: four
TS7006 lines upstream reports inside class expressions and four it does not, and
the difference between them is exactly whether the enclosing class expression has
a contextual type. A blanket decline cannot tell those apart, and the case is not
converted by removing half its extras.

> **`extraonly.rs` says a case is one removal from passing; it does not say the
> removal is expressible.** Its first three forecasts were exact because the
> extras were a *shape* — a narrowing, a second report, a scope entry. This one
> is a *quantity*: four of eight, split by the very thing the port cannot
> compute. The instrument stays exact about what it measures and the fourth
> build is where the difference showed.

§61 stands: the literal-union element access is the shape it looked like, and
`typeGuardNarrowsIndexedAccessOfKnownProperty9` converts.

---

## 62. TS2411's member names — string and numeric literals, and `isNumericLiteralName`

`diagreach.rs` puts TS2411 at **34 cases** and `diagmissing.rs` names the shapes:

```
 8  compiler/propertiesAndIndexersForNumericNames        public "1": string  beside  [i: number]: number
 7  conformance/derivedInterfaceIncompatibleWithBaseIndexer   1: {…}  and  '1': {…}
 6  compiler/inheritedMembersAndIndexSignaturesFromDifferentBases
 2  conformance/computedPropertyNames44/45_ES6
```

`check_index_constraints` (§28) collects members whose name node is an
**`Identifier`** and drops every other kind. A property named `"1"` or `1` is
exactly the one a *numeric* index signature constrains, so the rule was blind to
its own headline population.

### `isNumericLiteralName` is a round-trip, and `parse::<f64>()` is not it

`constrains(Number)` tests `name.parse::<f64>().is_ok()`. Upstream
(`utilities.go:898`) is

```go
return jsnum.FromString(name).String() == name
```

*"we test whether `ToString(ToNumber(name))` is exactly equal to `name`"* — and
the comment above it spends fifteen lines on why: `"0xF00D"` indexes as
`"61453"`, so it is **not** a numeric name even though it parses. The corpus
tests the boundary directly:

| name | numeric? |
|---|---|
| `"1"`, `"-1"`, `"-2.5"`, `"3.141592"`, `"1.2e-20"` | yes |
| `"Infinity"`, `"-Infinity"`, `"NaN"` | **yes**, deliberately |
| `" 1"`, `"1    "`, `""`, `"0xF00D"` | no |

Rust's `parse::<f64>()` accepts `"+1"`, `"inf"` and `"nan"`, which JS's
`ToNumber` does not round-trip, and rejects nothing that matters in the other
direction. The port is the round-trip: reject any character outside
`[0-9.eE+-]` unless the whole name is one of the three special spellings, then
compare against a JS-shaped `Number#toString` (exponential below `1e-6` and at
or above `1e21`, with the `+` JS writes on a non-negative exponent).

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,245**. Forecast **+3 to +10** of the 27 sole-obstacle cases |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST must not grow (currently **0** on `RULE_CODES = [2411]`) |
| 4 | own WRONG ≤ **15** |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column fills with `"1"`-style names against a
*string* index signature, the two signature kinds are being applied to the same
member when upstream applies only the more specific one — `getApplicableIndexInfo`
picks one, and the rule loops over all of them.

### Scored — **+3**, and the wrong column did not move at all

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,245, forecast +3 to +10 | **1,248 / 5,488 = 22.74%** | pass |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST must not grow | **0 → 0** | pass |
| 4 | own WRONG ≤ 15 | **1 → 1** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 2411: **CONVERTS 2 → 5 · RIGHT 15 → 37 · WRONG 1 → 1 ·
LOST 0.**

**Twenty-two new right lines and not one new wrong one.** The falsifier — a
`"1"`-style name reported against the *string* signature as well as the numeric
one — did not fire, and the reason is that `constrains(String)` is `true` for
every name and always was: the rule already looped over both signatures for
every identifier-named member and the corpus never produced a pair where both
fail. Widening the *names* did not widen that.

The two cases that did not convert of the seven the shapes named
(`inheritedMembersAndIndexSignaturesFromDifferentBases`,
`propertiesAndIndexersForNumericNames`) are `STILL SHORT`: they gain their
TS2411 lines and want other codes.

---

## 63. TS2389 — the arm §14 declined rather than ported

§14 built `checkFunctionOrConstructorSymbol`'s implementation-expected messages
and stopped short of one, saying so in the code:

> *"The subsequent-node scan above them selects TS2389 `Function implementation
> name must be '{0}'` … It is not ported; the effect is that a case wanting
> TS2389 gets TS2391 instead, which is a wrong code — so the scan's guard is
> reproduced instead: if the next sibling is adjacent, of the same kind and
> carries a body, say nothing."*

`diagreach.rs` prices the silence at **14 cases**, 13 of them blocked on TS2389
alone:

```ts
function foo(x);
function foo(x, y);
function bar() { }     // upstream: Function implementation name must be 'foo'.
```

`functionOverloadImplementationOfWrongName`, `…2`, `functionNameConflicts`,
`parserFunctionDeclaration4/6`, `parserClassDeclaration13/21/22` and a tail.

**The decline was already exact, which is what makes this cheap.**
`next_sibling_is_the_implementation` reproduces upstream's branch structure at
`checker.go:3566` line for line — adjacent, same kind, and then *names match* or
*subsequent has a body*. The second disjunct **is** TS2389's condition. Turning
the silence into the message needs no new analysis, only the message and its two
positions:

- the error node is the **subsequent** declaration's name (`errorNode :=
  core.OrElse(subsequentName, subsequentNode)`), not this one's;
- the argument is **this** declaration's name — the one the implementation
  should have had.

The static/instance arm (TS2387/TS2388) stays declined: it fires only when the
names *match* and the two differ in `static`, which is a different disjunct and
its own row.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,248**. Forecast **+6 to +13** of the 13 |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST must not grow |
| 4 | own WRONG ≤ **8** |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column carries cases where the two declarations
*do* share a name, the port's name comparison is reading a different node than
`DeclarationNameToString` does — a computed or private name, where upstream's
own equality test has three separate arms.

### Scored — **+13, past 22.9%**, and not one new wrong line

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,248, forecast +6 to +13 | **1,261 / 5,488 = 22.98%** | pass, at the top |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST must not grow | **1 → 1** (the family's standing loss, on both sides) | pass |
| 4 | own WRONG ≤ 8 | **11 → 11**, unchanged | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` over `[2389, 2390, 2391, 2392, 2393]`: **CONVERTS 18 → 31 ·
RIGHT 75 → 90 · WRONG 11 → 11 · LOST 1 → 1.**

**Fifteen new right lines, thirteen conversions, zero new wrong ones — the best
ratio of the session**, and the reason is that the analysis was already done.
§14 had reproduced upstream's branch structure exactly in order to *decline*
correctly, and one of its disjuncts **was** the unported message's condition.
The build is the message and its two positions.

> **A decline that reproduces upstream's branch structure is half a port.**
> §14 wrote *"the scan's guard is reproduced instead"* and paid for the guard's
> correctness at the time; that spend is what made this thirteen cases for
> twenty lines of code. A decline that had approximated the guard — "next
> sibling has a body, stay quiet" — would have been just as silent and worth
> nothing here.

---

## 64. TS2390 — a constructor has no symbol in this binder

`class C { constructor(); }` — one constructor overload, no implementation — is
upstream's **TS2390** `Constructor implementation is missing`, and this port
says nothing. `diagreach.rs` prices it at 8 cases:
`ClassDeclaration8/10/11/14` and their `parserClassDeclaration*` twins.

§14's rule opens with

```rust
let Some(symbol) = self.binder.symbol_of(node) else { return };
```

and a one-line trace answers why the arm never runs: **`symbol_of` is `None` for
a `Constructor`**. Upstream binds one as `InternalSymbolNameConstructor`
(`__constructor`) in the class's member table; this binder does not, so a
constructor overload set has no symbol to gather its declarations from and the
rule returns before it starts. Every TS2390 §14 *did* convert came in through a
`MethodDeclaration` or a `FunctionDeclaration`.

**Repaired in the rule, not in the binder.** Adding a member-table entry in
`tsr_binder` would move `checker_types`, which is the other workstream's. The
declarations a constructor overload set needs are the enclosing class's
constructor members, in source order, and the class's member list is right
there. The dedup that `function_symbol_checked` provides for symbols is provided
here by running only when `node` is the **first** constructor of its class.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,261**. Forecast **+4 to +8** |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST must not grow (currently **1** on the family, standing) |
| 4 | the family's WRONG ≤ **15**, from 11 |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column carries classes whose constructor *does* have
an implementation, the sibling gather is missing the body-bearing member — a
parameter-property constructor or one the parser attached elsewhere — and the
answer is the gather, not a decline.

### Scored — **+10**, above the forecast, for one wrong line

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,261, forecast +4 to +8 | **1,271 / 5,488 = 23.16%** | pass, **above** |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST must not grow | **1 → 1** | pass |
| 4 | the family's WRONG ≤ 15 | **11 → 12** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` over `[2389, 2390, 2391, 2392, 2393]`: **CONVERTS 31 → 41 ·
RIGHT 90 → 108 · WRONG 11 → 12 · LOST 1 → 1.**

**The forecast was low because the missing symbol was suppressing more than
TS2390.** `diagreach.rs` counts a case once per *code*, and eight cases wanted
TS2390 — but a constructor overload set with no symbol also never reached
TS2391, TS2392 or TS2393, so two more cases arrived with the same repair. A
gate that returns before a rule starts silences the whole rule, and the row of
the code you noticed is a lower bound on what it was costing.

> **A one-line trace answered in one run what a decline audit would not have
> found at all.** §49's lesson — *when a new rule measures zero, check that it
> ran before checking what it decided* — generalises: when an **old** rule
> converts less than its row, print whether it runs. `symbol_of` returning
> `None` for a `Constructor` is not visible in any decline, because it is not a
> decline.

---

## 65. `diagemit.rs`, and TS2362 / TS2363 — the arithmetic operand types

§64's lesson asked for an instrument, and `examples/diagemit.rs` (new) is it:
how many diagnostics of each code this port **emits**, beside how many the
baselines record. A large `want` against a zero `have` is a rule that is not
*running*; a small `have` is one that is declining.

```
code           want     have   rule
TS2454         4015     3799
TS2322         2888      546   quiet
TS2304         2735     2549
TS2564         1392     1245
TS2362          863        0   unported     <-
TS18050         771      718
TS2363          768        0   unported     <-
TS2339          701      232
```

**No ported rule reads SILENT**, which is the first thing worth knowing. The
two largest *unported* rows are one function: `checkArithmeticOperandType`
(`checker.go:12799`), the operand check that sits beside TS18050 and TS2365 at
the very site `crate::operator_operands` and `crate::nullable_operand` already
visit.

```go
leftOk  := c.checkArithmeticOperandType(left,  leftType,  The_left_hand_side_of_an_arithmetic_operation_must_be_of_type_any_number_bigint_or_an_enum_type,  true)
rightOk := c.checkArithmeticOperandType(right, rightType, The_right_hand_side_of_an_arithmetic_operation_must_be_of_type_any_number_bigint_or_an_enum_type, true)
```

and the predicate is one line: `!isTypeAssignableTo(t, numberOrBigIntType)`.

### Two declines the neighbouring rules already name

- **A nullish operand** is `checkNonNullType`'s (TS18050 / TS18048), which runs
  first at the same site — §50.3's split.
- **Both operands boolean-like** is TS2447, *"The '{0}' operator is not allowed
  for boolean types, consider using '{1}' instead"*, reported on the **operator
  token** and returning before the operand check (`checker.go:12372`).

And the direction is §52's: the rule reports **because** a relation failed, so
only a confident `NotRelated` fires and `Unknown` is silence. An enum operand
answers `Unknown` for `enum → number` here and is assignable upstream, which is
the safe direction — the message itself lists "an enum type".

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,271**. Forecast **+5 to +20** — `diaggap.rs` gives the two codes 1 and 4 sole-obstacle cases, so almost all of the value is in cases needing them **beside** codes this port already emits, which is `diagreach.rs`'s population and not forecastable per code |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2362, 2363]` |
| 4 | own WRONG ≤ **60**. Higher than §49's bar: 1,631 baseline lines is the largest population this workstream has opened since TS2322 |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column is dominated by enum operands, the relater's
`enum → number` answer is `NotRelated` rather than `Unknown` and the decline is
an explicit enum arm rather than the ternary reading.

### Scored — **+13**, and **866 right lines for four wrong**

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,271, forecast +5 to +20 | **1,284 / 5,488 = 23.40%** | pass |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | own WRONG ≤ 60 | **4** | pass, by a factor of fifteen |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` over `[2362, 2363]`: **CONVERTS 13 · RIGHT 866 · WRONG 4 ·
LOST 0.**

**The best right-to-wrong ratio in this file — 216 to 1 — and the reason is that
the predicate is one relation call with no bound drawn around it.** Every wrong
column this session that needed declines needed them because the rule was
*substituting* for something upstream does differently: comparability for
assignability, a members table for a resolved one, a syntactic guard for a
narrowing. `isTypeAssignableTo(t, number | bigint)` is not a substitution. It is
the same question upstream asks, and the three-valued reading (§52's direction:
fire only on a confident `NotRelated`) turns this port's incompleteness into
silence rather than into error.

**`diagemit.rs` is what found it**, and it found it by the column no other
instrument has: `want` against `have` per code. TS2362's 863 baseline lines and
TS2363's 768 were the two largest **unported** rows in the corpus, and neither
appears anywhere near the top of `diaggap.rs`'s single-code column — 1 and 4
sole-obstacle cases — because those lines almost always arrive beside a code
this port already emits. **A code with a huge `want`, a zero `have` and a tiny
sole-obstacle row is the signature of a rule worth building**, and it is exactly
the signature the old board hid.

---

## 66. TS2356 — the same predicate at the `++` / `--` operand

`diagemit.rs` ranks TS2356 third among the unported rows, **198 baseline lines**,
and it is `checkArithmeticOperandType` a third time — the same one-line predicate
§65 ported, called from `checkPrefixUnaryExpression` (`checker.go:10899`) and
`checkPostfixUnaryExpression` (`:10915`).

**Only `++` and `--`.** Unary `+`, `-` and `~` take a different arm
(`checker.go:10875`) that reports TS2469 for a `symbol` operand and nothing
about numerics — `-"a"` is not this diagnostic. Reading the switch rather than
generalising from the message's wording is the whole of the site question here,
and getting it wrong would put the rule on every negated string in the corpus.

The operand check runs **before** `checkReferenceExpression` and gates it —
*"run check only if former checks succeeded to avoid reporting cascading
errors"* — so TS2357 is downstream of this one and not affected by adding it.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **1,284**. Forecast **+2 to +8** — 198 lines, and `diaggap.rs` gives TS2356 no sole-obstacle row at all |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2356]` |
| 4 | own WRONG ≤ **20** |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column carries unary `-`, `+` or `~`, the site list
was generalised from the message instead of read off the switch.

### Scored — **+5 for zero wrong**, after one decline the falsifier did not name

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 1,284, forecast +2 to +8 | **1,289 / 5,488 = 23.49%** | pass |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | own WRONG ≤ 20 | **31 → 0** after the decline | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 2356: **CONVERTS 5 · RIGHT 145 · WRONG 0 · LOST 0.**

**The registered falsifier did not fire and a different one did.** The bar
watched for unary `-`, `+` and `~` leaking in from a message-shaped site list;
the site list was read off the switch and none appeared. What did appear was
`++ENUM` — sixteen of the thirty-one wrong lines across
`incrementOperatorWithEnumTypeInvalidOperations` and its `decrement…` twin.

Upstream reports **TS2628** there, `Cannot assign to 'ENUM' because it is an
enum`, from `checkIdentifier`'s assignment-target arm (`checker.go:11080`),
which runs *before* the operand's type is looked at. The same arm has TS2629 for
a class, TS2631 for a namespace, TS2630 for a function and TS2632 for an import.
An operand naming something that is **not a variable** is one of those five, so
the decline is one predicate and it took the column to zero — the remaining
fifteen were the same shape at a `.js` or strict-mode site.

> **A falsifier that does not fire is not a wasted one.** It bought the site
> list being read rather than inferred, which is why the wrong column contained
> no `-x` at all and the one family it did contain was diagnosable in a single
> `diagcase` run.

---

## 67. TS2341 — a private property outside its class

`diagemit.rs`: **111 baseline lines, zero emitted.**
`checkPropertyAccessibility` (`checker.go`), reached from the property-access
check §35 and §53 already run.

The predicate for the `private` half is entirely syntactic once the property
symbol is in hand: the declaration carries a `private` modifier, and the
reference is **not inside the class that declares it**. No relation, no
members-table completeness beyond the lookup itself.

`protected` (TS2445, 102 more lines) is **not** built here: its rule is *"the
enclosing class must derive from the declaring class"*, which needs the
`extends` chain and the `this`-type rules upstream applies on top
(`isClassDerivedFromDeclaringClasses`). Building the two together would put a
heritage question inside a modifier check, and the `private` half is exact
without it.

### The bound

- the receiver's members must be complete — `crate::nonexistent_property`'s
  gate, reused, because a property this port did not finish resolving cannot be
  asked about its modifiers either;
- **a private-identifier** member (`#x`) is TS18013, a different code with its
  own row, and is declined;
- the declaring class is the property declaration's parent, and the reference's
  enclosing class is an ancestor walk — a reference in a **nested** class inside
  the declaring one is still outside it, which is upstream's
  `getContainingClass` chain and is reproduced by taking the *nearest* class.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,289**. Forecast **+4 to +12** |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2341]` |
| 4 | own WRONG ≤ **25** |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column carries references *inside* the declaring
class, the enclosing-class walk is stopping at the wrong node — a class
expression, an object literal method, or a parameter default, all of which are
still lexically inside the class body.

### Scored — **+7 for zero wrong**, after the divergent-accessor decline

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,289, forecast +4 to +12 | **1,296 / 5,488 = 23.62%** | pass |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST == 0 | **1 → 0** after the decline | pass |
| 4 | own WRONG ≤ 25 | **13 → 0** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 2341: **CONVERTS 7 · RIGHT 47 · WRONG 0 · LOST 0.**

**One decline, and the loss and the wrong column were the same defect.** The
first measurement read 7 conversions, 13 wrong lines and **one lost case**, and
both belonged to a shape the bar had not considered: a `get`/`set` pair whose
two halves carry **different** accessibility.

```ts
get PublicPrivate() { return 0; }
private set PublicPrivate(v) { return; }
```

Upstream decides accessibility from the accessor the *access kind* selects — a
read from the getter, a write from the setter. This port has no
access-kind-selected declaration and `value_declaration` picks one arbitrarily,
so reading the modifier off it answers the wrong accessor half the time.
Requiring **every** declaration of the symbol to carry `private` declines the
divergent pair whole: `divergentAccessorsVisibility1`'s 12 wrong lines and
`accessorDeclarationOrder`'s loss went together, and eight right lines went with
them.

> **A loss and a wrong column that appear in the same measurement are worth
> diffing against each other before either is diagnosed.** Here they were one
> predicate. The bar's own falsifier — "references *inside* the declaring
> class" — was about the walk and the walk was right; what was wrong was which
> declaration the modifier was read from.

---

## 68. TS2445 — the `protected` sibling §67 left

§67 built `checkPropertyAccessibility`'s `private` half and said why it stopped:
*"`protected` … needs the `extends` chain and the `this`-type rules upstream
applies on top."* `diagemit.rs` prices the half left behind at **102 baseline
lines, zero emitted**.

The rule is `private`'s with one clause changed: the reference's enclosing class
must **derive from** the declaring class rather than *be* it. The chain walk is
§56's `sole_extends_class_declaration` shape — one `extends` clause naming one
non-generic class this port can resolve — and it declines the moment it cannot
follow a link, which is `crate::members`' `base_symbols_of` contract applied to
a different question.

**The `this`-type half is not built.** Upstream additionally requires that a
protected *instance* member be accessed through a receiver of the enclosing
class or a subclass (TS2446, `Property '{0}' is protected and only accessible
through an instance of class '{1}'`). That is a second code with its own row and
a relation question; declining it costs a missing diagnostic in a case that
already gets TS2445 right or nothing at all.

§67's divergent-accessor decline carries over unchanged: every declaration of
the symbol must carry the modifier.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,296**. Forecast **+3 to +10** |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2341, 2445]` |
| 4 | TS2341's WRONG stays **0**; TS2445's own ≤ **20** |
| 5 | every other snapshot unchanged |

**Falsifier.** If the wrong column carries accesses from a class that *does*
derive, the walk is stopping at a link it should follow — a generic base, a base
named through a qualified name, or one declared in another file — and the answer
is to widen the walk or decline those bases rather than the rule.

### Scored — **a measured zero on the suite**, +44 right lines, kept

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,296, forecast +3 to +10 | **1,296** — no conversion | **fired** |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | TS2341's WRONG stays 0; TS2445's ≤ 20 | **31 → 10 → 0** after two declines | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` over `[2341, 2445]`: **CONVERTS 7 · RIGHT 47 → 91 · WRONG 0 ·
LOST 0.** The seven are §67's; TS2445 converts **none** on its own.

### The two declines, and the first was a misread of the walk

The registered falsifier named a walk that stops at a link it should follow, and
what the wrong column actually held was a walk that stopped at the **wrong
level**:

- **`protectedClassPropertyAccessibleWithinNestedSubclass1`, 21 lines.**
  `isNodeWithinClass` and `forEachEnclosingClass` walk **every** enclosing
  class, not the nearest, so a reference inside a class nested within a subclass
  is still inside the subclass. §67's `enclosing_class_of` took the innermost
  one — correct for its own measurement because `private` never met a nested
  class in the corpus, and wrong the moment `protected` did.
- **A `this` parameter, 10 lines.** `protectedMembersThisParameter`,
  `thisTypeAccessibility` and `protectedAccessThroughContextualThis` reach a
  protected member from a *function*, carrying the class through
  `this: Subclass` rather than lexically. Upstream reads the `this` type; this
  port has no such reading, and the whole shape is declined.

### Kept on the reachable set, not on the suite

`diagreach.rs` reads **1,299** after this against 1,296 passing, up from 1,283
before §65 — the 44 new right lines put three more cases within reach of rules
that already exist. That is the honest justification: TS2445 converts nothing
today because every case wanting it wants something else too, and the something
else is now the only thing missing.

> **A rule that converts zero and emits no wrong line is not the same as one
> that converts zero and does** (§44 was the first kind, §61.1 the second). The
> distinction is `diagreach.rs`, and it is why the instrument is worth running
> on a zero.

---

## 69. TS2374 — a duplicate index signature

`checkIndexConstraints`' sibling (`checker.go`, `checkObjectTypeForDuplicateDeclarations`):
a class, interface or type literal declaring **two index signatures of the same
key kind** is `Duplicate index signature for type '{0}'.` on the second and
every later one.

`diaggap.rs` gives it **4 sole-obstacle cases** (`duplicateStringIndexers`,
`duplicateNumericIndexers`, `genericClassesRedeclaration`,
`duplicateObjectTypeMembers`) and `diagemit.rs` 66 baseline lines against zero
emitted.

**Entirely syntactic.** The key kind is the index parameter's written type
annotation — `string` or `number` — and nothing else is consulted: no symbol, no
relation, no members table. §14's ordering rule at its cheapest, and the fifth
rule in this file with no type in it after TS2369, the two grammar walks, and
§47.

The member list is the *declaration's own*, not the resolved type's: an
inherited index signature is not a duplicate, and `crate::index_constraint`
already walks the `extends` chain for the other question.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **1,296**. Forecast **+2 to +4** |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST == **0** on `diag2307.rs` with `RULE_CODES = [2374]` |
| 4 | own WRONG ≤ **5** — a syntactic rule has shipped at zero four times |
| 5 | every other snapshot unchanged |

### Scored — a **measured zero on the suite**, +19 right, **+5 reachable**

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 1,296, forecast +2 to +4 | **1,296** — no conversion | **fired** |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST == 0 | **0** | pass |
| 4 | own WRONG ≤ 5 | **0** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 2374: **CONVERTS 0 · RIGHT 19 · WRONG 0 · LOST 0.**
`diagreach.rs` **1,299 → 1,304**.

**`diaggap.rs` said four cases would convert alone and none did**, which is the
same over-read §2 named and this session has now watched nine times. The four
are `STILL SHORT`: `duplicateStringIndexers` and `duplicateNumericIndexers` want
TS2374 **and** TS2300, `genericClassesRedeclaration` wants TS2374 and TS2451.
Their TS2374 lines are now all present, which is why `diagreach.rs` rose by five
rather than staying put.

Kept on the same grounds as §68: zero conversions, zero wrong lines, and five
cases moved from *needs a new rule* to *needs an existing one to finish*.

---

## 70. An inaccessible property's access answers `errorType`, so nothing follows it

`extraonly.rs` after §67: `classPropertyAsPrivate` and `classPropertyAsProtected`
carry two extras each, and both are the same line —

```ts
class C { private get y() { … } private set y(x) { } }
declare var c: C;
c.y = 1;      // upstream: TS2341 alone. This port: TS2341 AND TS2322.
```

`checkPropertyAccessExpression` **returns `errorType`** after reporting an
accessibility error, so the assignment check never runs on that target. This
port's rules are independent walks, so `crate::assignreport`'s assignment anchor
computed the setter's parameter type and compared against it.

The repair is the split §67 should have had: `inaccessible_property` answers
*whether and with which message*, the reporter uses it, and
`assignment_target_type` declines a target it answers for.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes > **1,296**. Forecast **+2** — `extraonly.rs` names two cases and has been exact three times of four |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST must not grow |
| 4 | TS2322's WRONG falls; its RIGHT does not |
| 5 | every other snapshot unchanged |

### Scored — **+2, the forecast to the case**

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes > 1,296, forecast +2 | **1,298 / 5,488 = 23.65%** | pass, exactly |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST must not grow | **0 → 0** | pass |
| 4 | TS2322's WRONG falls, RIGHT does not | **WRONG 104 → 100, RIGHT 533 → 533** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` over `[2322, 2341, 2445]`: **CONVERTS 85 → 87 · RIGHT 533 → 533 ·
WRONG 104 → 100 · LOST 0.**

**`extraonly.rs` is four for five now**, and the one it missed (§61.1) missed for
the reason recorded there: it says a case is one removal from passing, not that
the removal is expressible. This one was expressible in six lines because §67
had already computed the answer — the split into `inaccessible_property` is the
same shape §63 got thirteen cases from. **A rule that decides something another
rule needs should answer the question, not just act on it.**

---

## 71. A class's own name is in scope inside it — §60 at a second kind

`extraonly.rs`: `defaultDeclarationEmitNamedCorrectly` carries one extra,

```ts
export default class MyComponent {
    static create = make(MyComponent);   // <- reported; the name IS in scope
}
```

§60 declined the same shape for a **named function expression**, whose own name
`bindFunctionExpression` puts in its body's scope. A class binds its name inside
its body too — for a class *expression* through `bindAnonymousDeclaration`, and
for `export default class X` through the default-export symbol, neither of which
this binder puts where `resolve_name` looks.

Declined in the rule rather than repaired in the binder, for §60's reason:
`tsr_binder` is shared with the query road.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **1,298**. Forecast **+1** |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST must not grow |
| 4 | TS2304's WRONG falls; its RIGHT does not |
| 5 | every other snapshot unchanged |

### Scored — **+1**, and a standing loss went with it

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 1,298, forecast +1 | **1,299 / 5,488 = 23.67%** | pass, exactly |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST must not grow | **2 → 1** — it *fell* | pass |
| 4 | TS2304's WRONG falls, RIGHT does not | **451 → 444**, RIGHT **2,406** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

**One of the family's two standing losses was this defect.** §55 shipped with
LOST 2 and carried the pair forward through §57, §60 and §70 as "pre-existing";
one of them was a class naming itself. A standing loss is a *finding that has
not been diagnosed yet*, and this file has now twice treated one as furniture —
§59's is still there.

`extraonly.rs` is **five for six**.

---

## 72. §59's standing loss, diagnosed: the elaboration reports instead of the outer code

§71 said a standing loss is a finding that has not been diagnosed yet and named
§59's as still open. It is `objectLitTargetTypeCallSite`:

```ts
function process(x: { a: number; b: string }) { }
process({ a: true, b: "y" });
```

Upstream reports **TS2322 on the `a` property** and nothing on the argument.
`getSignatureApplicabilityError` calls `checkTypeRelatedToAndOptionallyElaborate`,
and `elaborateError` walks an object-literal source's members and reports the
offending *member* — the outer `Argument of type … is not assignable` is what it
says when there is nothing finer to point at.

This port runs both: `check_excess_properties` reports the member (which is
where its TS2322 comes from) and `report_argument_failure` reports the argument.
The case passed before TS2345 existed and has been this rule's only loss since.

**The rule is "if the elaboration spoke, the outer code does not"**, and it is
exact rather than approximate because both live in one function: the argument
loop counts the diagnostics before the member check and skips the outer report
if the count grew.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **1,299**. Forecast **+1** |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | **LOST 1 → 0** on `diag2307.rs` with `RULE_CODES = [2345]` — the leg this build exists for |
| 4 | TS2345's WRONG falls; RIGHT falls by at most as much |
| 5 | every other snapshot unchanged |

### Scored — **+1, past 1,300**, and the standing loss is gone

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 1,299, forecast +1 | **1,300 / 5,488 = 23.69%** | pass, exactly |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | **LOST 1 → 0** | **0** | pass |
| 4 | WRONG falls, RIGHT falls by at most as much | **10 → 6**, RIGHT **37 → 37** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` isolated to 2345: **CONVERTS 19 · RIGHT 37 · WRONG 10 → 6 ·
LOST 1 → 0.**

**Both of the session's standing losses turned out to be one-line findings**
(§71's and this one), and both had been carried across four builds apiece as
"pre-existing". The number that made them visible was the same one that hid
them: `diag2307.rs` prints LOST on both sides of an edit, so a loss the rule
already had reads as furniture unless somebody asks what it is.

> **Carry a standing loss with its case name, and diagnose it before the next
> build in the same family.** Four wrong lines and two conversions were sitting
> in two case names this file had been printing for twenty builds.

---

## 73. §72's rule at the four assignment anchors

§72's finding is not about arguments. `checkTypeRelatedToAndOptionallyElaborate`
is what every assignability *report* goes through upstream, and its elaboration
walks an object-literal source's members and reports the offending member
**instead of** the outer message. `crate::assignreport` has four anchors that
call `check_excess_properties` and then `report_assignability_failure`
unconditionally:

- the assignment operator (`x = { … }`),
- a variable declaration's annotation against its initialiser,
- a property declaration's,
- a parameter default's.

Each can emit a member TS2322 and an outer TS2322 for one source, where upstream
emits one. §72 fixed the argument anchor because that is where the loss was;
this applies the same guard at the other four, where the symptom is a *wrong
line* rather than a loss.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **1,300**. Forecast **0 to +6** — a double report costs a case only when the case is otherwise exact |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST must not grow (**0** on `RULE_CODES = [2322, 2741, 2353, 2561]`) |
| 4 | that family's WRONG falls; its RIGHT falls by at most as much |
| 5 | every other snapshot unchanged |

**Falsifier.** If RIGHT falls by more than WRONG, upstream really does report
both at some anchor — the elaboration returns `false` and the caller reports
after it — and the guard belongs only where §72 measured it.

### Scored — **+1** and six wrong lines, at five anchors

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 1,300, forecast 0 to +6 | **1,301 / 5,488 = 23.71%** | pass |
| 2 | `checker_types` pass count 3,683 | **3,683**, snapshot unchanged | pass |
| 3 | LOST must not grow | **0 → 0** | pass |
| 4 | WRONG falls, RIGHT falls by at most as much | **112 → 106**, RIGHT **504 → 504** | pass |
| 5 | every other snapshot unchanged | only `diagnostics.snap` differs | pass |

`diag2307.rs` over `[2322, 2741, 2353, 2561]`: **CONVERTS 85 → 86 · RIGHT 504 →
504 · WRONG 112 → 106 · LOST 0.**

**Five anchors, not four** — the return-statement one had the same shape and was
missed on the first read of the file; the count came from grepping
`check_excess_properties`'s call sites rather than from remembering them. The
fifth added no measurable change, which is its own small finding: a `return
{ … }` against an annotated return type does not double-report anywhere in the
corpus, and the guard is there for the shape rather than for a number.

**Six wrong lines removed with no right line lost** is the signature of a
port-shaped defect rather than a bound in the wrong place: upstream has *one*
reporting function and this port has two walks, so the fix is an ordering, not a
decline.

---

## 74. §55's type-position arm in `.js` files

§55 added TS2304 for a name in a **type** position and inherited §7's guards —
parse errors, specially-diagnosed names, the `with` block — but not the one
`STATUS.md` §5 records for every other option-gated rule: **a `.js` file's types
come from JSDoc**, which this port does not parse into types. `@param {Foo} x`
and `@type {Bar}` name types upstream resolves and this port cannot see.

The value arm keeps JS deliberately — §7 measured it there and the asymmetry is
recorded. This is the type arm only, and `extraonly.rs` shows the shape:
`parserUnparsedTokenCrash1`'s `a.js` and `parserArrowFunctionExpression8`'s
`fileJs.js` each carry one.

### The bar

| leg | registered |
|---|---|
| 1 | `diagnostics` passes ≥ **1,301**. Forecast **0 to +2** |
| 2 | `checker_types` pass count unchanged at **3,683** |
| 3 | LOST must not grow (**1** on `RULE_CODES = [2304, 2552, 2583]`) |
| 4 | TS2304's WRONG falls; its RIGHT falls by at most a quarter as much |
| 5 | every other snapshot unchanged |

**Falsifier.** If RIGHT falls sharply, `.ts` and `.js` are not the split — the
corpus's `.js` type references resolve as often as they fail — and the decline
belongs on JSDoc-sourced annotations rather than on the file.

### Scored — **REFUSED at one right line for one wrong**, and the falsifier fired

| leg | registered | measured | |
|---|---|---:|---|
| 1 | passes ≥ 1,301, forecast 0 to +2 | **1,301** — no change | fired |
| 2 | `checker_types` pass count 3,683 | **3,683** | pass |
| 3 | LOST must not grow | **1 → 1** | pass |
| 4 | WRONG falls, RIGHT falls by at most a quarter as much | **WRONG 444 → 443, RIGHT 2,406 → 2,405** | **fired** |
| 5 | every other snapshot unchanged | — | pass |

**One right line for one wrong line, and no case moved either way.** The
falsifier said it plainly: if `RIGHT` falls with `WRONG`, `.ts` versus `.js` is
not the split. It is not — the corpus's `.js` **type-position** references
resolve about as often as they fail, because a `.js` file that names a type at
all is usually one with a `.d.ts` beside it or a triple-slash reference, not one
relying on JSDoc.

Reverted. **This is the third time this file has assumed a JS decline transfers
between arms**: §18 measured it for `noImplicitAny` (28 wrong lines, kept), §16
for TS2322's declaration anchors (5 wrong, kept) and TS2322's assignment anchor
(4 conversions for 5 wrong, *not* kept), and now §55's type arm at a wash. The
rule is not "JS files are unreliable"; it is **"a JSDoc-sourced *annotation* is
unreliable"**, and only the arms that read an annotation pay for it. A type
*reference* in a `.js` file is written in the source like any other.

The decline returns if JSDoc types land: the population it would then protect is
real, and it is the same one §5's row names.

---

## 75. How much of the remaining work is independent of the relation?

`diagreach.rs` gained the split that answers it. Of the cases reachable by
deepening rules that already exist, it partitions by whether **every** missing
code's rule is gated on the structural relation or a resolved members table —
TS2322, TS2345, TS2339, TS2741, TS2353, TS2352, TS2416, TS2430, TS2420, TS2415,
TS2403, TS2411, the subsystem `STATUS.md` §5 refuses.

```
cases reachable by deepening existing rules : 1,305   (against 1,301 passing)
  wants only relation-bound codes           :   937
  wants a mix                               :    52
  wants NO relation-bound code              :   316
```

**316 cases, plus a share of 52, are reachable without the relation at all** —
roughly a quarter of the reachable set and about the size of everything this
session converted. **937 are the assignability family**, and they are not a
diagnostics build: they are `checker_types`' structural relation and members
table arriving through a second door, exactly as §54 priced them.

### What "incorporate the checker" already means here

Every rule in this workstream already calls the checker — `check_expression`,
`relate_ternary`, `get_property_of_type`, `declared_members_are_complete`. The
question is not whether to use it but whether to **wait on the parts of it that
are incomplete**, and the answer this split gives is: not yet. There is a
quarter of the reachable set that does not touch them, and it is the cheaper
quarter — this session's builds averaged five cases each against a relation-free
population, while the three attempts that reached into the relation (§16's
TS2322, §24's TS2403, §49's type parameters) were all refused or bounded.

**The order that falls out:** finish the 316 and the small unported rows
`diagemit.rs` ranks (TS7027 12 sole-obstacle, TS7010 11, TS2449 10, TS2693 9,
TS2364 7, TS2703 7, TS2558 6), then re-take this split. It moves whenever the
`.types` workstream lands relation work, and 937 is the number that says how
much of `diagnostics` that workstream is carrying.

---

## 76. §42.1's guard, one level up: a *union annotation* of named types

`diagreach.rs` at `6cf7726` ranks TS2454 at **56 cases** — the largest
relation-free row on the board, and 49 of them are blocked by TS2454 alone.
`diagmissing -- 2454` puts a third of its lines in two cases,
`conformance/arithmeticOperatorWithEnumUnion` and
`conformance/additionOperatorWithNumberAndEnum`, and both are the same three
declarations:

```ts
var a: any;
var b: number;
var c: E | F;     // E and F are enums
var ra1 = c * a;  // upstream: TS2454 on `c`
var ra2 = c * b;  // upstream: TS2454 on `c` AND on `b`
```

The port reports every `b` and **no `c`**. One `eprintln!` behind
`TSR_DEBUG_2454` at the rule's declared-type read — the instrument this file
has now recommended three times — answers it in one run:

```
60 2454 c: declared=TypeId(1) error=TypeId(1) contains_undef=false
40 2454 b: declared=TypeId(6) error=TypeId(1) contains_undef=false
```

`declared` for `c` **is** `errorType`, so the rule returns at its first type
test. This is §42.1 exactly, at a different call: `get_type_from_union_type_node`
(`declared.rs:695`) routes an un-aliased union node through
`get_union_type`, whose worker answers `errorType` for any union with a *named*
constituent because this port computes printed text at type-creation time and
upstream's `origin` denormalisation (`checker.go:25705`) is unported. §42.1
fixed the wrapper — `getOptionalType`'s `T | undefined` — and left the
**annotation itself** on the printing road. `E | F` is a union with two named
constituents before `undefined` is ever added.

### The bar

`get_union_type_unprinted` already exists and carries §42.1's whole argument: a
diagnostic that compares `(file, line, column, code)` never prints a type, so
for that consumer the printing guard converts a right answer into an
`errorType` that silences the rule. The build gives `check_used_before_assigned`
a declared type computed the same way — and **only** that rule, so nothing on
the query road can reach it and `checker_types` stays byte-identical.

Scope deliberately: the annotation is a `UnionTypeNode` with **no enclosing type
alias**. An aliased union goes to `get_named_union_type`, which is a different
question (the alias's own printing), and a generic alias is `errorType` for a
third reason entirely.

**Bar: +8 cases.** Two cases are visible in `diagmissing`'s head and the
enum-typed union is a common corpus shape; below +8 the row is thinner than the
instrument suggests and the residual is elsewhere.

**Falsifier (a):** if the wrong column grows on cases where the union's
constituents are *not* named — that would mean the recomputation is answering a
different type than the annotation, not the same type unprinted.

**Falsifier (b):** if `contains_undefined_type(declared)` starts reading true
for annotations that do not write `undefined` — the reduction order in the
unprinted worker differs from the printed one and the gate at the top of the
rule is being fed a different type.

**Falsifier (c):** if `checker_types` moves by a single line. Nothing on the
query road may reach the new helper.

### Measured: **+5, and the bar of +8 was MISSED — recorded, and the build kept**

```
diagnostics   1,302 -> 1,307     (+5 cases)
checker_types 3,742 -> 3,742     byte-identical, falsifier (c) did not fire
diag2307, RULE_CODES = [2454] alone
  CONVERTS   261 -> 266
  RIGHT    3,784 -> 3,860        (+76 right lines)
  WRONG       15 -> 15           the SAME fifteen lines, byte-identical
  LOST         0 -> 0
```

**Falsifiers (a) and (b) could not fire: the wrong column did not change at
all.** Every one of the +76 lines is a line upstream writes. That is the
strongest form the evidence can take here — the recomputation is answering the
*same* type the annotation denotes, unprinted, and nothing else moved.

`conformance/arithmeticOperatorWithEnumUnion` — the case that found this —
converts with **zero** missing and **zero** extra diagnostics.

**The bar said +8 and the build read +5, so the bar was missed by three.** It is
recorded rather than rationalised: the estimate was taken off two visible head
cases plus a guess that "enum-typed union" is a common corpus shape, and that
guess was too generous — 76 right lines concentrated into 5 cases is a
concentration of 15:1, which is §53's phenomenon (71 right lines for one
conversion) at a milder ratio. **`diagmissing`'s line count remains a poor
predictor of conversions**; this is the seventh build to learn it and the first
to have written the ratio down as the reason.

The build is kept on §68/§69's precedent — a build that emits **no wrong line**
and moves `diagreach` is worth keeping at any conversion count — and here the
conversion count is not zero.

### The residual, and its named owner

The fifteen wrong lines are unchanged by this build and therefore **pre-existing,
not this build's to decline**. Carried forward with their case names per the
standing-LOST rule, they are three families:

- **exhaustive switch** (`exhaustiveSwitchStatements1` ×4,
  `exhaustiveSwitchCheckCircularity`) — upstream proves the switch covers the
  union and the post-switch reference is assigned on every path. Owner:
  `crate::flow`'s switch-exhaustiveness, unported.
- **destructuring and rest patterns** (`objectRestNegative`,
  `restElementWithAssignmentPattern2`/`4`, `iterableArrayPattern24`) — the
  binding-pattern spellings of a definite assignment that
  `is_write_only_access` does not reach. Owner: `crate::unused`'s `accessKind`
  at pattern elements.
- **type guards** (`typeGuardOfFormNotExpr`,
  `typeGuardOfFormTypeOfIsOrderIndependent`) —
  `reference_is_guarded_by_a_condition_on`'s over-approximation reading the
  wrong way round. Owner: this rule's guard, and §58 already named it.

**What this build did NOT reach.** `diagreach` still ranks TS2454 well above the
5 converted, and the two head cases in `diagmissing` were the two enum-union
ones. The rest of that row is the three families above plus
`compiler/dynamicNames` and the `moduleAugmentation*` family, which are a
different question again.

---

## 77. TS2564's residual is §76's phenomenon at signature scale — DIAGNOSED, NOT BUILT

`diagreach.rs` ranks TS2564 at **37 cases**, the third relation-free row.
`diagmissing -- 2564` puts 22 of its lines in three cases —
`compiler/missingTypeArguments1` (8), `compiler/returnTypeTypeArguments` (8),
`compiler/privacyVarDeclFile` (6) — and the first two are one shape:

```ts
class X<T>  { p1: () => X;         }   // upstream: TS2314 on X, AND TS2564 on p1
class X2<T> { p2: { [idx: number]: X2 } }
class X3<T> { p3: X3[]             }
class X5<T> { p5: X5;              }   // upstream: TS2314 only, NO TS2564
```

An `eprintln!` behind `TSR_DEBUG_2564` at the rule's type gate, printing the
declared type *and* the annotation's own type:

```
2564 p1: ty=TypeId(1) anno=Some((TypeId(1), "error")) is_error=true
...
2564 pa: ty=TypeId(1) anno=Some((TypeId(1), "error")) is_error=true
```

**All ten read the intrinsic `errorType`, including the annotation read
directly**, so the rule is not looking at the wrong type: `() => X` — a
`FunctionTypeNode` — *is* `errorType` in this port.

### Why upstream reports p1–p4 and p6–p9 but not p5

`getTypeFromClassOrInterfaceReference` (`checker.go:23196`) reports the arity
error and returns `c.errorType`, which carries `TypeFlagsAny`, so
`checkPropertyInitialization`'s `t.flags&TypeFlagsAnyOrUnknown`
(`checker.go:4946`) skips it. That is `p5: X5` — a *bare* reference, and the one
member of the family upstream is silent on, which the port gets right for the
right reason.

For the other nine the error is **nested**: upstream's `() => errorType` is an
anonymous object type with a call signature, and an object type is not
`AnyOrUnknown`. **Upstream contains the error inside the type constructor; this
port propagates it outward.**

### Where the propagation lives, and why it is not a bug

`signature_bearing_type_node` (`function_types.rs:130`) returns `errorType` when
`get_signature_from_declaration` answers `None`, and that function answers
`None` whenever a *part* is `errorType` — a parameter's annotation
(`signatures.rs:1413`), an inferred return (`:1068`), a return expression
(`:903`). That is the port's standing "a gap in a part is a gap in the whole"
convention, and it exists **for the same reason §42.1's union guard exists**:
this port computes a type's printed text when the type is created, and there is
no text for `() => error`.

So TS2564's residual is **§76's finding at signature scale**. §76 was cheap
because `get_union_type_unprinted` already existed and one call site needed
rerouting. The signature equivalent does not exist: it would mean an unprinted
road through `get_signature_from_declaration`, `signature_to_string` and
`store.new_anonymous`, all of which are on the `checker_types` query path.

### REFUSED here, with the number and the owner

**Not built. 22 lines across 3 cases**, and `diagreach` prices the whole TS2564
row at 37 cases of which this is a part — against a change that touches the
signature road `checker_types` is steered by. §76's own ratio (76 right lines →
5 cases) says the case yield here would be low single digits.

**Owner: `checker_types`.** The general question is *"does a type constructor
contain an `errorType` part or propagate it?"*, and the answer upstream is
**contain**. Whoever ports print-from-the-store (the deletion
`get_named_union_type`'s doc comment already anticipates) gets this for free,
because the reason to propagate disappears with print-at-creation.

**How this refusal would be shown wrong:** if a *non-printing* consumer of
`get_signature_from_declaration` can be given the unprinted road in isolation —
the way §76 did for unions — without any query-path call site reaching it. That
was not attempted and is a real possibility; it is refused on cost here, not on
impossibility.

---

## 78. TS2554's callee gate accepts one declaration kind out of four

`diagreach.rs` ranks TS2554 at **36 cases**, relation-free.
`diagmissing -- 2554` puts 15 of its lines in one case,
`compiler/optionalParamArgsTest`, and diffing that case whole is what named the
gap — **nothing** about it is a decline in the arity arithmetic:

```
function F1() { return 0; }        F1(1);          <- ALREADY REPORTED
var L1 = function() { return 0; }  L1(1);          <- missing
interface I1 { C1M1(): number; }   i1o1.C1M1(1);   <- missing
class C1 { public C1M1() { … } }   c1o1.C1M1(1);   <- missing
```

Every missing line is a callee whose declaration is not a `FunctionDeclaration`.
`sole_signature_arity` (`call_arity.rs:403`) and `sole_signature_parameters`
(`:479`) both gate on `SymbolFlags::FUNCTION` **and** then pattern-match
`Node::FunctionDeclaration`, so a method, a method signature, and a variable
holding a function expression are all declined at the same line — five, eight
and two of that case's fifteen respectively.

**`callee_symbol` (`:349`) already resolves all of them.** Its
`PropertyAccessExpression` arm was built for TS2345 and answers the method
symbol correctly today; the declaration match is the only thing between it and a
diagnostic.

### Why this is the same rule and not a new one

Upstream never had four cases here. `getMinArgumentCount` and
`getParameterCount` are properties of a **`Signature`**, and
`getSignatureFromDeclaration` (`checker.go:19902`) accepts any
`SignatureDeclaration` — a `FunctionDeclaration`, a `MethodDeclaration`, a
`MethodSignature`, a `FunctionExpression`, an `ArrowFunction`. This module's own
opening paragraph says the rule reads the declaration and counts, "a syntactic
fact with no incompleteness to leak"; restricting *which* declaration was a
first-slice convenience, not a decision, and no note records it as one.

### The one place the kinds genuinely differ

The existing gate requires `declaration.body`, on the argument that "an overload
set of one — a declaration with no body — is still an overload set, and its
implementation may be in another file". That argument is about
`FunctionDeclaration` and `MethodDeclaration`, where a bodiless spelling **is**
an overload signature. A **`MethodSignature`** in an interface never has a body
and is not an overload set: requiring one there would decline every interface
method in the corpus. So the body test becomes per-kind rather than universal.
An overload *set* is still excluded everywhere by `declarations.len() != 1`.

### The bar

**+6 cases.** `optionalParamArgsTest` is one case however many of its fifteen
lines land, so the bar is about the *rest* of the corpus:
`const f = (a, b) => …` and `obj.method(x)` are the two commonest callee
spellings in modern TypeScript, and TS2554's row is 36 cases. Below +6 the
generalisation is reaching shapes the corpus does not write at this rule's other
gates.

**Falsifier (a):** if the wrong column grows on **interface** methods — that
would mean the per-kind body test is admitting overload signatures after all,
and `declarations.len() != 1` is not the whole guard it is claimed to be.

**Falsifier (b):** if the wrong column grows on `var f = function …` where the
variable is **reassigned** to a different function later. The symbol has one
*declaration* but more than one function, and arity read off the declaration
would be a confident wrong answer. §76's discipline: name the shape before
measuring, not after.

**Falsifier (c):** if TS2345 (which shares `sole_signature_parameters`) moves at
all in the wrong direction. It is a relation-bound code and this build must not
feed it new callees it cannot judge — if it does, the parameters half stays on
`FunctionDeclaration` and only the arity half generalises.

**Falsifier (d):** `checker_types` byte-identical.

### Measured: **+2, and the bar of +6 was MISSED — §76's lesson, repeated**

```
diagnostics   1,307 -> 1,309     (+2 cases)
checker_types 3,742 -> 3,742     byte-identical, falsifier (d) did not fire
diag2307, RULE_CODES = [2554, 2555] alone
  CONVERTS    16 -> 18
  RIGHT       48 -> 78           (+30 right lines)
  WRONG        6 -> 8            (+2, both named below)
  LOST         0 -> 0
```

**Falsifier (a) did not fire.** No interface method appears in the wrong column;
the per-kind body test admits `MethodSignatureDeclaration` and nothing else that
`declarations.len() != 1` was not already excluding.

**Falsifier (b) fired, in a shape close to but not the one predicted, and was
honoured in-build.** The prediction was *a variable reassigned to a different
function*. What actually fired was **a variable with a written type
annotation**: `var Component: C = () => {}` where `C` is a call signature taking
one argument. The annotation **is** the signature and the initialiser is
contextually typed by it, so the initialiser's empty parameter list said
*Expected 0 arguments* on every call to `Component`
(`thislessFunctionsNotContextSensitive1`, 2 wrong lines). An annotated variable
now declines outright. **The decline cost nothing**: RIGHT stayed at 78 and
CONVERTS at 18 across it, so the two lines it removed were pure false positives.
Owner named in the code: reading the annotation's signature is
`crate::signatures`' road.

**Falsifier (c) was not tested, deliberately.** `sole_signature_parameters` —
TS2345's half — was left on `FunctionDeclaration`. TS2345 is a relation-bound
code and §75's split says do not build into it; generalising the callee gate
there is a separate bar.

### The two remaining new wrong lines, and why they are NOT declined

Both are `conformance/callWithMissingVoid`:

```ts
class X<T> { f(t: T) { return { a: t }; } }
declare const x: X<void>;             x.f()       // upstream: no error
declare const xUnion: X<void | number>; xUnion.f() // upstream: no error
declare const xAny: X<any>;           xAny.f()    // upstream: TS2554
```

`parameter_annotation_is_void` reads the **written** annotation and `t: T` is not
`void`, so the minimum stays 1 and the first two report. That limitation is
already named where it lives (`call_arity.rs:452`, "an alias for `void`, and a
type parameter instantiated with it") and this build did not create it — it
routed methods into it.

**Declining it was measured and rejected**: all three calls write the *same*
annotation `t: T`, so any test that silences the first two silences `xAny.f()`
too. The build's own arrival in this case is **3 right lines against 2 wrong**,
and the case cannot pass either way — it wants 11 TS2554 lines and this port
emits 9. **Owner: `parameter_annotation_is_void`, and the fix is the instantiated
type rather than the annotation.**

### Why the bar was missed, again

+30 right lines into +2 cases is 15:1 — **the identical concentration §76
measured**, from a completely different rule. Two builds is not a law, but the
board's per-code column is a count of *cases* and `diagmissing`'s output is a
count of *lines*, and a bar taken off the second will keep overshooting the
first by whatever the concentration is. **Take the next bar off `diagreach`'s
case count and the share of it a shape plausibly covers, never off a line
count.**

---

## 79. `typeof A` is a **value** position, and the allow-list did not say so

Taking the bar off `diagreach`'s case count this time — §78's closing lesson.
TS2304 is **40 cases**, and its shape is the opposite of the last two builds':

```
lines missing per case:   30 cases want 1   ·   7 want 2   ·   2 want 6
```

Thirty cases one line from converting, against §76's 76-lines-into-5-cases and
§78's 30-into-2. And eleven of the forty are **one family**:

```
conformance/parserTypeQuery1 … 9   var v: typeof A          (1,15)
compiler/typeofProperty            interface I1 { a: number; b: typeof a }
compiler/typeofInObjectLiteralType
```

`check_value_identifier` never sees these. Its position test is
`is_value_reference` (`check.rs:1107`), a deliberate **allow**-list of
expression slots — the module comment says so, on the ground that "a missing
deny-list arm is a false positive while a missing allow-list arm is only a
missed conversion". A `TypeQueryNode`'s `expr_name` is not in it.

### It is upstream's position too, not a liberty

`typeof A` resolves `A` with **`SymbolFlagsValue`** — that is the whole point of
the query, and `typeofProperty`'s comments say it out loud: *"Should yield error
(a is not a value)"*. `getTypeFromTypeQueryNode` (`checker.go:22964`) calls
`resolveEntityName` with `SymbolFlagsValue`, which reports the same TS2304 an
identifier expression gets. The type-query slot is a value slot wearing a type
node's syntax, and this is the one place in the grammar where that is true.

### Scope: the leftmost name of the entity name, and only under a query

`typeof A.B` resolves `A` as a value and `B` as its member, so only the
**leftmost** identifier of a `QualifiedName` chain is a value reference — and
only when the chain's root is a `TypeQueryNode`. A qualified name under a plain
`TypeReferenceNode` (`var v: A.B`) is a *namespace* miss, which upstream reports
as TS2503, a different code at the same position. Firing there would be a wrong
line, which is exactly what an allow-list exists to prevent.

`check_used_before_assigned` — the only other caller of `is_value_reference` —
declines `is_in_type_query_or_type_node` on its own line and is unaffected.

### The bar

**+9 cases.** The family is eleven cases, all of them blocked on TS2304 alone
per `diagmissing`, and nine of the eleven want exactly one line. Below +9 the
family is not what `diagmissing` says it is.

**Falsifier (a):** a wrong line at a qualified name under a `TypeReferenceNode`
— the chain-root test is not doing what it claims and TS2503 territory is being
reported as TS2304.

**Falsifier (b):** the parse-error cases. `parserTypeQuery3`, `6` and `9` are
`var v: typeof A.` and friends — recovered trees. TS2304's parse-error gate was
deleted by §40.3 (+6) and re-measured by §50.1 (−6), so it is **absent** today
and these files are read. If the wrong column grows on recovered trees, that
gate is a per-rule question again and this family is the third measurement of
it — not a house style, per §43.

**Falsifier (c):** `checker_types` byte-identical.

### Measured: **+13 — the bar of +9 MET, and the wrong column went DOWN**

```
diagnostics   1,309 -> 1,322    (+13 cases)
checker_types 3,742 -> 3,742    byte-identical, falsifier (c) did not fire
diag2307, RULE_CODES = [2304] alone   (368 < 400: NOT truncated, unlike the
                                       four-code run that read 482)
  CONVERTS   190 -> 203
  RIGHT    2,175 -> 2,197        (+22)
  WRONG      367 -> 358          (-9)
  LOST         3 -> 3            the same three cases
```

**The first bar this session to be met, and the first taken off a CASE count
rather than a line count** — §78's closing instruction, applied to the next
build and vindicated: 30 cases one line short converted at 13, where two bars
taken off line counts read 5 and 2 against 8 and 6.

**Falsifier (a) did not fire.** No qualified name under a `TypeReferenceNode`
appears in the wrong column; `entity_name_root_is_a_type_query` holds.

**Falsifier (b) did not fire, and this is the parse-error gate's THIRD
measurement.** `parserTypeQuery3`, `6` and `9` are `var v: typeof A.` on
recovered trees, and all three converted with no wrong line. §40.3 deleted this
rule's gate for +6, §50.1 re-measured it at −6, and this build is the tie-break:
**absent is right for TS2304**, and the per-rule finding stands rather than
becoming a house style.

### Two declines, both found by reading the wrong column, both named

**`typeof this.z` (4 wrong lines).** This parser spells `this` as an
`Identifier` inside a type query, so the new `QualifiedName` arm handed it to a
resolver that can never find it; upstream reports TS2339 on the `.z` instead.
Declined at the same line as `null`, on the same argument — **neither is a
spellable binding in any scope, so declining can hide no real diagnostic**.
`initializerReferencingConstructorLocals` and `…Parameters`.

**`import Z = M; var r8: typeof Z` (2 wrong lines, and 10 removed).**
`resolveEntityName` resolves at `meaning | SymbolFlagsAlias`
(`checker.go:15772`); this binder gives an import-equals its own `ALIAS` symbol
and none of the other three meanings, so the rule's meaning-ladder missed it.
Adding `ALIAS` to the ladder was worth **ten** wrong lines, not the two
`typeofAnExportedType` showed — the ladder is consulted by every TS2304 in the
corpus and import-equals aliases were being reported wherever they appeared.
**A decline found in one family paid four times over outside it**, which is the
argument for reading the wrong column rather than the case.

### The three standing LOST, carried with their names and now diagnosed

Unchanged by this build and pre-existing, but the handoff rule says diagnose
before the next build in the family, and all three are also `extraonly.rs`
entries — each is **one false positive from passing**:

- **`compiler/validRegexp`** — an extra TS2304 at `(1,24)`. The parser reads an
  ambiguous `/` as division rather than a regular-expression literal, so the
  regex body's contents are parsed as identifiers. Owner: `tsr_parser`'s
  regex/division disambiguation. Named in the eleventh session's handoff and
  still true.
- **`conformance/resolutionModeTripleSlash1`** (extra TS2304 on `MODULE`) and
  **`3`** (the same) — **newly diagnosed here.** Both files are
  `/// <reference types="foo" />` against an `@types/foo` package whose
  `exports` map sends `import` to `index.d.mts` and `require` to `index.d.cts`.
  The port loads neither, so both globals are unresolved. The sibling
  `resolutionModeTripleSlash2` is in `extraonly` too, with two extra **TS2552**
  on `SCRIPT` — a *suggestion* form, meaning the port did load the `.d.mts`
  there and offered `MODULE` as the near-miss. **Owner: `file_loader`** — the
  triple-slash type reference and the conditional-`exports` resolution mode, not
  a diagnostics rule. Three cases sit behind it.

---

## 80. `noImplicitAny` defaults ON in this corpus, and the harness had it OFF

Ranking the relation-free head by **cases one line short** — §79's metric, now
the board's — puts TS7006 at the top by ratio:

```
code      cases   one line short
TS2454      45        25
TS7006      20        17     <- the best ratio on the board
TS2554      19        12
TS2464      14        10
TS2365      12         9
TS2540      12         9
```

`diagmissing -- 7006` reads like a rule that is declining: 24 lines across 20
cases, all of them arrows and contextually-typed positions. It is not declining.
**It is not running.**

```
// arrowFunctionWithObjectLiteralBody1.ts — the WHOLE file
var v = a => <any>{}
// baseline: arrowFunctionWithObjectLiteralBody1.ts(1,9): error TS7006
```

The case writes no `@noImplicitAny` and no `@strict`, and
`diagnostics_suite.rs:275` reads

```rust
explicit("noimplicitany").or_else(|| explicit("strict")).unwrap_or(false)
```

so the rule is off. The parent arm — a variable declaration with no annotation —
already admits this exact shape, and has since the rule was written.

### Upstream's default is TRUE, and the port already knew that for its sibling

`c.noImplicitAny = c.compilerOptions.GetStrictOptionValue(c.compilerOptions.NoImplicitAny)`
(`checker.go:924`), and `GetStrictOptionValue` (`core/compileroptions.go:294`)
is:

```go
if value != TSUnknown { return value == TSTrue }
return options.Strict != TSFalse
```

An unset option with an unset `strict` is `TSUnknown != TSFalse` — **true**.
That is the same rule the harness *already applies two lines above* for
`strictNullChecks`, which reads `unwrap_or(true)`. The two lines disagree about
the same question, and the `strictNullChecks` one is right. **A figure that
appears twice in one file will disagree with itself unless something re-derives
both** — `STATUS.md` §1's own warning, here as a pair of option defaults ten
lines apart.

`noUnusedLocals` and `noUnusedParameters` are **not** strict options
(`unusedIsError` reads them as `IsTrue()`), so those stay `false` and
`crate::unused` stays confined to the cases that opt in. Only this one line
changes.

### What it gives up, said before measuring

`implicit_any.rs`'s module doc closes with *"the blast radius is the option —
nothing here fires unless the case writes `@noImplicitAny`"*. **That sentence
stops being true with this build**, and it is the whole of the risk: TS7006 and
TS7019 go from firing in the opt-in cases to firing across the corpus, against a
`parameters_cannot_be_contextually_typed` allow-list that was tuned while the
blast radius was small. `diagemit` prices the row at **want 385 / have 140**.

### The bar

**+8 cases.** Nineteen cases are blocked on TS7006 alone and seventeen want one
line, but the option also turns on TS7019 and reaches every case in the corpus,
so some of the seventeen will be paid for elsewhere. **A net negative is a live
outcome here** and would be the honest result to record.

**Falsifier (a):** the wrong column grows on **arrows in argument position** or
any position `parameters_cannot_be_contextually_typed` admits by a route other
than the unannotated-variable and expression-statement arms. Those arms were
chosen when nothing outside the opt-in cases could reach them.

**Falsifier (b):** the wrong column grows on `.d.ts` or library files. The
`ambient` gate and `set_checked_files` were likewise never exercised at corpus
scale for this rule.

**Falsifier (c):** LOST grows anywhere. This is the first build of the session
that can break a passing case in a family it is not touching, because the option
is read by two rules and neither is confined any more.

**Falsifier (d):** `checker_types` byte-identical — the option is set on the
diagnostics harness only.

### Measured: **+5, bar of +8 MISSED, and no net negative**

```
diagnostics   1,322 -> 1,327    (+5 cases)
checker_types 3,742 -> 3,742    byte-identical, falsifier (d) did not fire
diag2307, RULE_CODES = [7006, 7019] alone
  CONVERTS     9 -> 14
  RIGHT      151 -> 191          (+40)
  WRONG        6 -> 9            (+3, after one in-build decline worth 5)
  LOST         0 -> 0            falsifier (c) did not fire
```

The stated live outcome — a net negative — did not happen. The
`parameters_cannot_be_contextually_typed` allow-list held at corpus scale, which
is the substantive result: **it was tuned against opt-in cases and it survives
the whole corpus**, one arm excepted.

**Falsifier (a) fired, precisely as written, and was honoured in-build.**
The `ReturnStatement` arm was admitted *unconditionally* on the theory that
nothing there supplies a signature:

```tsx
const decorator = function <T>(C: React.StatelessComponent<T>): React.StatelessComponent<T> {
    return (props) => <C {...props}></C>       // props IS contextually typed
};
```

The enclosing function's **written return-type annotation** is the contextual
type — `getContextualReturnType` (`checker.go:20315`) reads exactly it. A
`return` is now uncontextual only inside a function that writes no return
annotation. `tsxGenericAttributesType1` was the 3 lines that found it and the
decline removed **5**, the extra two being `generatorTypeCheck62` and `63`,
pre-existing wrong lines nobody had attributed. RIGHT and CONVERTS were
unchanged across the decline.

**Falsifier (b) did not fire.** No `.d.ts` or library line appears; the
`ambient` gate and `set_checked_files` hold at corpus scale.

### The residual, named and NOT declined

All three remaining new wrong lines are `compiler/reservedWords3`:

```ts
function f1(enum) {}    // upstream: TS1390 'enum' is not allowed as a parameter
function f2(class) {}   //           name, plus TS1003 Identifier expected
```

Upstream's parser produces a **missing** identifier here and reports TS1390 and
TS1003; this parser accepts the keyword as a parameter name and reports neither,
so `file_has_parse_errors` — the gate at this rule's first line, written for
exactly this situation — is false on a file upstream recovered differently.

**Not declined**, on §78's precedent. A reserved-word test would be a stand-in
for the parser's TS1390, and this file already has none of the machinery to ask
the question (`tsr_scanner` exposes no `IdentifierToKeywordKind`). The case
cannot pass either way: it wants TS7010, which `diagemit` lists as unported at
82 lines. **Owner: `tsr_parser`'s reserved-word parameter names, and TS1390.**

### A note for the next option

The `strictNullChecks` / `noImplicitAny` disagreement sat ten lines apart in one
function for eleven sessions. **The other option defaults in
`diagnostics_suite.rs` have not been checked against `GetStrictOptionValue`**,
and `set_strict_property_initialization`, `set_allow_unreachable_code` and
`set_no_unused` are each one `unwrap_or` away from the same bug.
`noUnusedLocals` and `noUnusedParameters` are genuinely **not** strict options
(`unusedIsError` reads them as `IsTrue()`) and are correct as they stand; the
other two are worth one reading of `core/compileroptions.go` before the next
build that depends on them.

---

## 81. TS7010 — the return-type sibling §80 made reachable

`diaggap.rs` re-run at `2df5954`, single-code column, relation-free rows:

```
TS7026  50 cases contain it   28 would convert alone    <- §5's refusal (JSX namespace)
TS7010  48 cases contain it   11 would convert alone
TS7027  12 sole   TS2449 11   TS2693 9   TS2364 7   TS2703 7   TS2558 6
```

TS7010 is the largest unrefused row, and **it is the same `checker.go` site this
rule already quotes**. `implicit_any.rs`'s parameter loop carries the comment
*"Report an implicit any error if there is no body … and node is not a private
method in an ambient context is the **return type** arm"* — the port read
`checker.go:3446` when it built the parameter half and left the return half
where it found it:

```go
if node.Type() == nil {
    // Report an implicit any error if there is no body, no explicit return type,
    // and node is not a private method in an ambient context
    if ast.NodeIsMissing(body) && !isPrivateWithinAmbient(node) {
        c.reportImplicitAny(node, c.anyType, WideningKindNormal)
    }
}
```

and `reportImplicitAny`'s function arm (`checker.go:18320`) chooses
`X_0_which_lacks_return_type_annotation_implicitly_has_an_1_return_type` —
TS7010, args `(name, "any")` — whenever `noImplicitAny` holds and the
declaration has a name. **§80 is what makes that condition true across the
corpus**; before it, this rule could only have fired in the opt-in cases.

The eleven sole-obstacle cases are all one shape:

```ts
function foo();                 // FunctionDeclaration3: TS7010 at (1,10), the NAME
function foo();                 // FunctionDeclaration4
function bar() { }
```

### Two things this arm needs that the parameter arm did not

**No contextual-typing question at all.** The whole reason the parameter arm is
a fenced allow-list is that a contextual signature can supply a parameter's
type. A **bodiless** declaration has no inferred return type to be contextually
supplied — upstream reaches `reportImplicitAny` unconditionally at that site,
with no `shouldReportErrorsFromWideningWithContextualSignature` in the path.
This arm is therefore *simpler* than the one beside it, which is worth saying
out loud because the neighbouring code looks like it should be copied.

**`isPrivateWithinAmbient`, and the ambient gate is the opposite way round.**
The parameter loop skips everything ambient (`if ambient { continue }`). The
return arm skips only a **private** member in an ambient context — upstream's
own words — so `declare function f();` in a `.d.ts` *does* report. The two arms
disagree about `ambient` and both are upstream's; conflating them is the
obvious mistake here.

### The arm set, and the one that is a live question

Upstream's dispatch reaches this site from `checkFunctionDeclaration` and
`checkMethodDeclaration` (`checker.go:2806`, `:3405`), and
`checkMethodDeclaration` serves **`MethodSignature` as well as
`MethodDeclaration`** — its own body says *"method signatures already report
'implementation not allowed in ambient context' elsewhere"* and guards the arms
that are declaration-only with `ast.IsMethodDeclaration(node)`.

So `interface I { foo(); }` should report TS7010. **That is a claim this build
does not yet have evidence for**, and it is the difference between +11 and a
large net negative: a bodiless interface method with no return annotation is a
shape the corpus writes far more often than a bodiless overload. It is included
rather than pre-excluded, because the wrong column answers it in one run and a
pre-exclusion would answer it never. `diagemit` prices the whole row at **82
lines**, which is *evidence for* including it — if every bodiless interface
method reported, that figure would be in the thousands.

### The bar

**+9 cases**, against eleven sole-obstacle ones.

**Falsifier (a):** the wrong column fills with `MethodSignature` lines. Then the
paragraph above is wrong, the arm comes out, and the 82-line figure gets its
real explanation.

**Falsifier (b):** a wrong line in a `.d.ts`. That would mean
`isPrivateWithinAmbient` is not what separates this arm's ambient handling from
the parameter arm's, and the `ambient` flag is doing something else.

**Falsifier (c):** the reported column is not the name. `reportImplicitAny`
errors on the *declaration* and `GetErrorRangeForNode` narrows a named
function-like to its name; §48 ported that centrally but left four arms
unported, and if a function-like is a fifth, every line of this rule is
displaced.

**Falsifier (d):** `checker_types` byte-identical.

### Measured: **+10, bar of +9 MET, and the wrong column is ZERO**

```
diagnostics   1,327 -> 1,337    (+10 cases)
checker_types 3,742 -> 3,742    byte-identical, falsifier (d) did not fire
diag2307, RULE_CODES = [7010] alone
  CONVERTS     0 -> 10
  RIGHT        0 -> 54
  WRONG        0 ->  0          after one in-build correction worth the only line
  LOST         0 ->  0
```

Converted: `FunctionDeclaration3`, `4`, `6`, `7`, `asiAmbientFunctionDeclaration`,
`implicitAnyFunctionOverloadWithImplicitAnyReturnType`, `noImplicitAnyFunctions`,
`parserFunctionDeclaration1.d`, `parserFunctionDeclaration3`,
`thisTypeInFunctions2`.

**Falsifier (a) did NOT fire, and that settles the live question.** No
`MethodSignature` line appears in the wrong column. `interface I { foo(); }`
really does report TS7010 upstream, and `diagemit`'s 82-line figure was the
evidence for it rather than against — a bodiless interface method with **no
return annotation at all** is rarer in this corpus than it looks, because the
idiom is `foo(): void`.

**Falsifier (b) did not fire**, and falsifier (c) did not: every one of the 54
lines is at the name, so `Checker::error_span` narrows a named function-like
correctly and §48's four unported arms are not five.

### The one wrong line, and both halves of `isPrivateWithinAmbient`

`conformance/privateNamesIncompatibleModifiers`, at `declare #whatMethod()`.
The first cut read the gate as *"a `private` member in an ambient context"*.
Upstream (`utilities.go:343`) is

```go
(ast.HasModifier(node, ast.ModifierFlagsPrivate) || ast.IsPrivateIdentifierClassElementDeclaration(node))
  && node.Flags&ast.NodeFlagsAmbient != 0
```

and **both halves are wider than they read**:

- a **`#name`** class element is private without the keyword;
- **`NodeFlagsAmbient`** is set by the parser for anything under a `declare`,
  *including the member's own*. This port's parser never sets that flag — it is
  one of the three declared-and-unset ones the handoff names — so the walk
  threads an `ambient` bool, and the `MethodDeclaration` arm does not widen it
  for a member-level `declare`. Reading the modifier in the rule closes that
  gap without touching the walk.

Correcting both took the wrong column to **zero**. This is the third time this
session that a decline's *cause* rather than its conclusion was the fix (§76,
§80, §81), and the second where the one wrong line named a mis-read upstream
predicate rather than a missing gate.

**A note the next reader needs:** `ambient` as threaded by `crate::check`'s walk
is **not** `NodeFlagsAmbient`. It is widened at `VariableStatement` and
`FunctionDeclaration` and nowhere else, so any rule asking "is this ambient" of
a *class member* must read the member's own `declare` modifier as well. Nothing
else in `crate::check` does this today, and every rule that takes `ambient` for
a member is one `declare` away from the same bug.

---

## 82. TS7027 — the binder already computed it

`diaggap.rs` at `3446c5a`: **TS7027, 12 cases would convert alone**, the largest
relation-free row left that §5 does not refuse (TS7026's 28 are still behind
`declare global` merging in the binder, unchanged since §13).

```ts
// @allowUnreachableCode: false
while (true);
var x = 1;      // reachabilityChecks1.ts(2,1): error TS7027: Unreachable code detected.
```

**This looks like a binder rule and does not have to be one.** Upstream reports
it from `Binder.checkUnreachable`, which needs `b.options.AllowUnreachableCode`
— and *"compiler options plumbed into `tsr_binder::bind`, which nothing does
today"* is the standing blocker this workstream has carried for TS1212 across
three handoffs. It does not apply here: `tsr_binder` **already records the
answer per node**. `bind_children` (`binder.rs:1019`) sets
`NodeFacts::UNREACHABLE` on every potentially-executable node it binds under
`current_flow == flow.unreachable()`, and `BindResult::facts` hands it to the
checker. No signature changes, no options in the binder.

### The three things upstream does that a fact per node does not

**1. One report per run, not one per statement.** `checkUnreachable` sets
`b.currentFlow = b.reportedUnreachableFlow` after reporting, and that node is
*not* `unreachableFlow`, so the next statement's `currentFlow != unreachableFlow`
test fails and it stays silent. The binder's fact is on every node in the
region. The checker's walk is pre-order document order, so the model is a flag
that is **set on report and cleared the moment a node without the fact is
visited** — which is exactly what replacing `currentFlow` with a reachable node
does upstream.

**2. `var x;` does not report, `let x;` does.** The condition
(`binder.go`, `checkUnreachable`'s `isError`) is *not a variable statement, or a
block-scoped one, or one with at least one initialised declaration*. A bare
`var x;` is hoisted and genuinely reachable in effect.

**3. Unset is a SUGGESTION, not an error.** `unreachableCodeIsError(options)` is
`AllowUnreachableCode == TSFalse` — **explicitly** false. With the option unset
upstream emits a suggestion, which never appears in a `.errors.txt`. The
checker's `allow_unreachable_code` is a `bool` today and cannot tell unset from
false, so it needs the third state. **This is the trap §80 just paid for, in the
opposite direction**: there the default was wrong because unset meant *on*; here
reading unset as *off* would report TS7027 on every case in the corpus with dead
code. The option is a `Tristate` upstream for a reason, and `GetStrictOptionValue`
is not the function that reads it.

### The bar

**+9 cases**, against twelve sole-obstacle ones.

**Falsifier (a):** the wrong column fills with second-and-later statements of a
run. Then the pre-order flag is not modelling `reportedUnreachableFlow` and the
collapse has to be done structurally, per statement list.

**Falsifier (b):** wrong lines on `var` statements — condition 2 above is
narrower or wider than stated.

**Falsifier (c):** wrong lines in cases that do **not** write
`@allowUnreachableCode: false`. That is the tristate failing, and it is the
failure mode that would be largest by far.

**Falsifier (d):** `checker_types` byte-identical.

### Measured: **+3, bar of +9 MISSED, and the model was wrong twice before it was right**

```
diagnostics   1,337 -> 1,340    (+3 cases)
checker_types 3,742 -> 3,742    byte-identical, falsifier (d) did not fire
diag2307, RULE_CODES = [7027] alone
  CONVERTS     0 -> 3
  RIGHT        0 -> 31
  WRONG        0 ->  3
  LOST         0 ->  0
```

**Falsifier (c) did not fire** — no wrong line in a case without
`@allowUnreachableCode: false`. The tristate is doing its job, and that was the
failure mode that would have been largest by far.

**Falsifier (b) did not fire** — no `var` line. `is_potentially_executable_node`
(`narrowing.rs:500`) already carries upstream's variable-statement clause, which
is the whole reason this build needed no binder change.

**Falsifier (a) fired, twice, and the second firing is the finding.**

*First cut* — 19 wrong against 22 right. Two separate bugs:

1. **`error_span` narrows a declaration to its name.** Upstream's
   `errorOnEachUnreachableRange` reports the statement's **own range**:
   `reachabilityChecks1.ts(47,5)` for a `namespace A { … }`, where `error_span`
   gives column 11. This is now the one report site in `crate::check` that
   deliberately bypasses `error_span`, and §48's centralisation is what makes
   the exception visible rather than invisible.
2. **`reportedUnreachableFlow` as mutable walk state does not survive this
   binder.** The flag was set on report and cleared on the next reachable
   statement — a faithful reading of upstream. It over-reported every top-level
   `namespace` after the first, because **this binder starts a fresh flow for a
   namespace body and upstream does not**, so the flag was cleared on the way in
   and re-armed on the way out.

*Second cut* — the model is now **structural and stateless**: report only when
no **ancestor** and no **preceding sibling** carries the fact. Same question,
no dependence on the walk, and immune to the flow divergence entirely. Wrong
went 19 → 11 (position) → 7 (ancestors) → **3** (siblings), with RIGHT rising
22 → 30 → 30 → 31 throughout: **every one of the three corrections was pure
subtraction from the wrong column.**

> **The transferable part.** A port of a stateful upstream algorithm can be
> wrong *because the state means something different here*, while every line of
> it reads as a faithful transcription. Asking the tree the same question
> statelessly was both shorter and right. This is the fourth build of the
> session where the fix was a sentence about *why* rather than a threshold.

### Why +3 and not +9, and the residual

`diaggap` said twelve cases would convert alone; three did. The row is
dominated by `reachabilityChecks1`…`11`, which are **one file re-run under
eleven option combinations** — they share every shape, so they convert or fail
together, and the sole-obstacle count counts them separately.
**`diaggap`'s case count is not eleven independent cases when the corpus writes
a family**, which is a limit on §79's metric that had not been seen before: it
is still the right metric, but a row concentrated in one *file name stem* should
be discounted the way a row concentrated in one case is.

The three remaining wrong lines are all `const enum`:

```ts
// @preserveConstEnums: false
while (true) { }
const enum E { X }     // the port reports; upstream does not
```

`checkUnreachable`'s `reportError` admits an enum only when it is not a `const`
enum **or** `ShouldPreserveConstEnums()` holds, and a module counts only when
`isInstantiatedModule(node, ShouldPreserveConstEnums())` — a namespace whose
only member is a non-preserved const enum generates no code and is not
executable. **`preserveConstEnums` is not plumbed into this checker**, so the
condition cannot be asked. Declining const enums unconditionally would be wrong
for `reachabilityChecks1`, which writes `preserveConstEnums: true`.

**Owner: the option.** `preserveConstEnums` is one line in
`diagnostics_suite.rs` and a field on `Checker`, and it is the same shape as
§80's and §82's own tristate — the third option in three builds. **The next
session should plumb the remaining `CompilerOptions` this corpus writes in one
build rather than one per rule.**

---

## 83. TS2449 — `Class '{0}' used before its declaration`, bounded to the one position that cannot defer

`diaggap.rs` at `21202b0`: **TS2449, 11 cases would convert alone**, and — §82's
lesson applied before the bar rather than after — those eleven are **eight
distinct file-name stems**, not one file under eleven configurations. The row is
real.

```ts
class A extends B { foo() { this.bar(); } }
class B { bar() { } }
// classOrder2.ts(1,17): error TS2449: Class 'B' used before its declaration.
```

`checkResolvedBlockScopedVariable` (`checker.go:1888`) picks between TS2448
(block-scoped variable), TS2449 (class) and TS2450 (enum) off the symbol's
flags, and gates the whole thing on
`declaration.Flags&NodeFlagsAmbient == 0 && !isBlockScopedNameDeclaredBeforeUse(declaration, errorLocation)`.

### The bound: an `extends` clause is never a deferred position

`isBlockScopedNameDeclaredBeforeUse` (`checker.go:1922`) is eighty lines, and
almost all of them are about **deferral** — a use inside a function body, an
instance property initialiser, an export specifier, a binding element, a
decorator, a computed property name — each of which is legal *because the code
does not run yet*. Porting that predicate is a rule of its own.

**The heritage clause needs none of it.** `class A extends B` evaluates `B` at
class-definition time, in the enclosing scope, immediately. There is no function
between the use and the declaration by construction, so every deferral arm is
excluded by the position rather than by a test. What is left is the two lines
that matter: **the same file, and the declaration starts after the use.**

That is why this build is `extends`-only and not "TS2449". The other positions
are a second slice with the predicate as its subject, and pretending otherwise
would put an unported eighty-line function's worth of false positives into a
rule whose whole population is eleven cases.

`is_value_reference` (`check.rs:1107`) already has the `extends`-clause arm,
built for TS2304 and carrying the `implements`-versus-`extends` distinction
(`heritage.token.kind == ExtendsKeyword`) that this rule needs exactly.

### The bar

**+7 cases**, against eleven.

**Falsifier (a):** a wrong line where the class is declared in **another file**.
`isBlockScopedNameDeclaredBeforeUse` returns `true` outright when the files
differ — *"nodes are in different files and order cannot be determined"* — and
`privacyClassExtendsClauseDeclFile` is a multi-file case in this very row.

**Falsifier (b):** a wrong line on an **ambient** declaration. The gate is
`declaration.Flags&NodeFlagsAmbient == 0` and reads the *declaration's* flag,
not the use's — and `NodeFlags::AMBIENT` is one of the three this parser never
sets, so it must be asked of the `declare` modifier and the enclosing
`declare namespace`, the same gap §81 closed for class members.

**Falsifier (c):** a wrong line where the symbol merges — a class merged with a
namespace or an interface has several declarations and upstream picks the
class-like one with `core.Find`. Picking `declarations.first()` instead would
compare against whichever the binder happened to record first.

**Falsifier (d):** `checker_types` byte-identical.

### Measured: **+6 for ZERO wrong lines** — bar of +7 missed by one

```
diagnostics   1,340 -> 1,346    (+6 cases)
checker_types 3,742 -> 3,742    byte-identical, falsifier (d) did not fire
diag2307, RULE_CODES = [2449] alone
  CONVERTS     0 -> 6
  RIGHT        0 -> 14
  WRONG        0 ->  0
  LOST         0 ->  0
```

**No falsifier could fire: the wrong column is empty.** (a), (b) and (c) were
each written as a shape to look for and each was pre-empted by a guard the bar
named before the code existed — the cross-file test, the transitive ambient
walk, and `core.Find` over the declaration list rather than `first()`. Writing
the three down as falsifiers is what put the three guards in the first draft;
this is the cleanest case in the session for **registering the bar before the
code** rather than after.

**The bound is the whole build.** Fourteen right lines came from about twenty
lines of rule, because `class A extends B` excludes
`isBlockScopedNameDeclaredBeforeUse`'s eighty lines *by position* rather than by
test. §63's lesson inverted: there, a faithful decline turned out to contain the
unported rule; here, choosing the one position where the unported predicate
cannot apply bought the rule without porting it.

### The five that did not convert, and the second slice they name

`diaggap` said eleven; six converted. The remainder is
`classDeclarationShouldBeOutOfScopeInComputedNames` (4 lines) and the tail of
`resolvingClassDeclarationWhenInBaseTypeResolution`, and they are **exactly the
deferral arms this build declined to port**: a computed property name inside the
class body, and uses under `typeof`. Both are `isBlockScopedNameDeclaredBeforeUse`
proper.

**The second slice, when someone takes it**, is that predicate with TS2448
(block-scoped variable) and TS2450 (enum) as its other two consumers —
`checkResolvedBlockScopedVariable` picks between the three off the symbol's
flags and shares everything else. Building it for one code would be building it
for three.

---

## 84. The rest of the strict family — auditing every option default at once

§80 found `noImplicitAny` defaulting off in the diagnostics harness when
`GetStrictOptionValue` (`core/compileroptions.go:294`) makes an unset option
**on**, ten lines below a `strictNullChecks` that already had it right. The
obvious next question — *which of the others are wrong?* — is one grep, and this
is the build that asks it rather than waiting for a rule to trip over one.

Every option the checker owns, against the upstream function that reads it:

| option | upstream reader | unset means | harness | verdict |
|---|---|---|---|---|
| `strictNullChecks` | `GetStrictOptionValue` (`checker.go:925`) | **on** | `unwrap_or(true)` | right |
| `noImplicitAny` | `GetStrictOptionValue` (`:924`) | **on** | `unwrap_or(true)` | right, since §80 |
| `strictPropertyInitialization` | `GetStrictOptionValue` (`:922`) | **on** | `unwrap_or(true)` | right |
| **`useUnknownInCatchVariables`** | **`GetStrictOptionValue` (`:926`)** | **on** | **never set — `false`** | **WRONG** |
| `noUncheckedIndexedAccess` | `== core.TSTrue` (`:6115`) | off | not set here | right |
| `noUnusedLocals` / `noUnusedParameters` | `IsTrue()` (`:7104`) | off | `is_some_and("true")` | right |
| `allowUnreachableCode` | `Tristate`, three meanings | suggestion | §82's pair | right |

**One is wrong, and it is the one no rule had reached for.**
`set_use_unknown_in_catch_variables` exists on the `Checker`, is read at
`symbols.rs:1597`, and **the diagnostics harness never calls it** — so every
un-annotated `catch (e)` in the corpus has type `any` here and `unknown`
upstream.

### Why this is a diagnostics build and not a `.types` one

Nothing reports TS-anything about a catch variable directly. The type is an
*input* to rules that ask what a value is: TS2571 (`Object is of type
'unknown'`), TS18046, TS2339 on `e.message`, TS2345 passing `e` on. **Its
blast radius is other rules**, which makes it exactly the kind of change that
can be net negative, and exactly the kind that never gets made because no single
row points at it.

### The bar

**+0.** This is a *correctness* build, not a conversion one: the honest
prediction is that almost nothing moves, because the rules that consume a catch
variable's type are mostly relation-bound and already silent. It is worth
landing at zero for the same reason §68 and §69 were — it removes a wrong input
from every rule that reads it, and the next build in that family would otherwise
debug the option instead of the rule.

**Falsifier (a):** a net negative. Then `unknown` is reaching a rule that was
silently correct on `any`, and the finding is *which* rule — that is the
interesting outcome, not the option.

**Falsifier (b):** `checker_types` moves. It must not: this is the diagnostics
harness only, and `trace_case.rs` has its own option plumbing.

### Measured: **zero, and VERIFIED zero rather than merely measured**

```
diagnostics   1,346 -> 1,346   (+0 cases, as barred)
checker_types 3,742 -> 3,742   byte-identical, falsifier (b) did not fire
diag2307, the FULL RULE_CODES list, both sides of the edit:
  CONVERTS 1,227 | RIGHT 12,019 | WRONG 746 | LOST 1   — identical
```

**Falsifier (a) did not fire**: no net negative, and nothing moved in either
direction. The whole-suite counterfactual was run on both sides precisely
because a case count of zero cannot distinguish *"nothing depends on this"* from
*"the code does not run"* — §49's trap. At the **line** level the two sides are
identical too, so no diagnostic in the 5,488 judged cases is sensitive to a
catch variable's type today.

**What that does and does not establish.** It establishes that the change is
free. It does **not** establish that `symbols.rs:1597` is exercised by a judged
case at all — a zero on both sides is consistent with the reader never being
reached. That is left unproven rather than asserted, and it is the one thing a
future reader should not take from this section.

The build is kept on §68/§69's precedent: it removes a wrong input from every
rule that will read it, and the alternative is that the next build in that
family spends its first hour debugging the option instead of the rule. The
audit table above is the durable part — **five of the seven options were already
right, and the wrong one was the only one no rule had reached for yet**, which
is the general shape: an option nobody consumes is an option nobody has
checked.

---

## 85. TS2454's remaining row, diagnosed and split — no build

TS2454 is still the largest relation-free row: **45 cases, 25 one line short**.
§76 took its enum-union half. `TSR_DEBUG_2454` at the declared-type read splits
what is left into two families with different owners, and **neither is a
diagnostics build**, which is why this section is a diagnosis rather than a
measurement.

### Family one — `declared == errorType`, 13 of the 24 one-line cases

`moduleAugmentation*` (7 cases, 4 stems), `umd1`/`3`/`4`,
`correctlyMarkAliasAsReferences2`/`4`, `augmentExportEquals5`.

```ts
// main.ts
import { Observable } from "./observable"
import "./map";                       // map.ts writes `declare module "./observable" { … }`
let x: Observable<number>;
let y = x.map(x => x + 1);            // main.ts(5,9): TS2454 on `x`
```

```
2454 x: txt=error err=true …
```

`Observable<number>` declares `errorType` because **module augmentation is
unported in this binder** — the same blocker §5 records against TS7026, arriving
at a different rule. Not §77's signature road and not §76's union guard: the
type reference itself does not resolve. **Owner: `tsr_binder`'s module
augmentation and `declare module "…"` merging**, and its constituency is now
TS7026's 28 cases plus roughly 13 of TS2454's.

### Family two — the rule does not run, and the decline is a candidate

`for-of8` is three lines whole:

```ts
v;
for (var v of [0]) { }     // for-of8.ts(1,1): TS2454 on the `v` ABOVE the loop
```

`check_used_before_assigned` declines every variable declared in a `for-in` or
`for-of` head, with the comment *"`for (x of …)` and `for (x in …)` assign on
entry"*. That is true of a reference **inside or after** the loop and false of
one **before** it, and the flow walk already distinguishes the two — the decline
is doing work the flow analysis would do correctly.

**A candidate, not a finding.** The decline is presumably load-bearing for
references in the loop body, and nothing here has measured what deleting it
costs. It belongs to the per-rule-gate family (§40.3 / §50.1 / §79's third
measurement of TS2304's parse-error gate): **a blanket decline that predates the
flow walk is worth re-measuring once the flow walk covers its cases.** `for-of22`,
`for-of57` and `nestedLoopTypeGuards` are in the same row and may or may not be
the same shape.

## §86 — the debug/release split that hid a panic for a whole build cycle

**Not a diagnostics build.** The thirteenth session opened with `git pull`, and
the first `cargo run -p tsr-conformance --bin coverage` died:

```
thread '<unnamed>' panicked at crates/tsr-checker/src/expressions.rs:84:17:
attempt to subtract with overflow
```

exit 101, no table, nothing measurable. The code is the `.types` workstream's —
`check_template_expression`'s escape-detection decline, `checker-notes-narrow.md`
§24 — landed in build 101/102, which had been measured and committed at 83.51%
by a session that never saw the crash.

### Why they did not see it

`cargo run --release` has `overflow-checks` **off**. The same subtraction that
aborts a debug run wraps silently in release to a value near `u32::MAX`, which
of course `!= text.len() + delimiters`, so the closure answers `true` and the
template declines. **The release measurement was correct and the debug binary
was unrunnable, at the same commit.** The examples in this workstream's loop are
run `--release`; `coverage` in `CLAUDE.md` is not. That is the whole of it.

Evidence it is not a `diagnostics` defect: a `catch_unwind` sweep of
`reported_for` over all 12,444 cases in **release** named zero panicking cases.
The reachable sites are on the types-suite path.

### The fix, and why it is behaviour-preserving

`span.end.saturating_sub(span.start)`. A degenerate span saturates to `0`, and
`0 != text.len() + delimiters` for every template part, because `delimiters` is
2 or 3 and never 0 — so the closure answers `true` and declines, which is
exactly what the wrapped value did. Release behaviour is unchanged by
construction, and the measurement confirms it: `checker_types` read
**3,760/9,538 · 83.51%** after the fix, the other workstream's committed number
to the case, and **no snapshot file changed at all**.

### The transferable part

**A span is not guaranteed monotonic.** Error recovery can hand a node an end
that precedes its start, so any arithmetic on `span.end - span.start` in this
port is a debug-build abort waiting for the right corpus case. There are other
such subtractions; none of them are protected by a type.

**And the profile split is a real hazard, not a curiosity.** A workstream that
measures only in release can land code that no debug build can run, and the next
session pays for it before it does any work of its own. `coverage` should be run
in **both** profiles at least once per session, or the subtraction pattern should
be banned outright — the second is cheaper and is the recommendation. Falsifier, and it was run:
`grep -rn "\.end - \|end - .*\.start" crates/tsr-checker/src/` returns **nothing**
after this fix, so the site repaired here was the only one and the ban costs
nothing today. Re-run that grep before adding span arithmetic.

## §87 — TS2564's constructor decline, narrowed to the half that is decidable

### The bar, before the code

**+9 cases**, at most 3 new wrong lines. Taken off `diaggap`'s **case** column
(27 sole-obstacle cases at `82eab5f`), never off `diagmissing`'s 122 lines, per
the twelfth session's six-build experiment.

Falsifiers, in the order they would fire:

1. **If the constructors in this family do assign the reported properties**, the
   family is not what it looks like and the build converts near zero. Checked by
   hand on five cases first — `lift`, `genericCloneReturnTypes`,
   `externalModuleQualification`, `thisExpressionOfGenericObject`,
   `specializedInheritedConstructors1` — and in every one the reported property
   is the one the constructor never mentions. `genericCloneReturnTypes` is the
   discriminating case: its constructor assigns `this.size` and not `this.t`,
   and the baseline reports **`t` alone**. A rule that reported both would be
   wrong on a line that exists today.
2. **If the subtree scan answers "not mentioned" for a body that does assign**,
   every such property becomes a wrong line. The depth cap is the way that
   happens, so the cap answers **`true`** (mentioned → decline), not `false`.
3. **If the other half of the row is this half**, the bar is double-counted. It
   is not: of the 27, roughly 11 are constructor-declined and the rest are the
   `errorType` family below.

### What §6 declined, and why only half of it was undecidable

`checkPropertyInitialization` (`checker.go:4933`) reports when
`constructor == nil || !isPropertyInitializedInConstructor(...)`
(`checker.go:4947`). §6 ported the first disjunct and declined the second
outright, because the second synthesises a `this.x` property access, hangs it
off the constructor's `ReturnFlowNode` and asks `getFlowTypeOfReference` whether
`undefined` survives — and this port cannot synthesise a node
([ADR-0012](../../adr/0012-ast-is-sync.md)): the tree is arena-allocated and
immutable after parsing, and a flow query needs a *registered* node with a
parent and a flow node.

That reasoning is correct and it is still correct. What it over-declined is the
**sub-case where the flow query's answer is knowable without running it**: if
the constructor body contains no `this.<name>` anywhere at all, then no
assignment to it exists on any path, so the reference's flow type is the
declared type — which by this point in the rule is known not to contain
`undefined` — and upstream reports. **A flow analysis you cannot run still has
inputs you can read.**

The scan is deliberately coarser than "is there an assignment": *any* occurrence
of `this.<name>` in the constructor body declines. A constructor that reads the
property without assigning it is one upstream reports on and this port will not,
which is silence and never a wrong answer — the same shape of trade §6 made, one
level in. Classifying occurrences instead (assignment vs read, `=` vs `+=`,
destructuring targets, `delete`) buys those few lines and puts every
misclassification into the *wrong* column, which is the wrong side of this
rule's risk.

### What it does not touch

The row's other half is **§77's family** and belongs to `checker_types`:
`missingTypeArguments1` (8 lines), `returnTypeTypeArguments` (8),
`privacyVarDeclFile` (6). The property's annotation there is a generic reference
written without its arguments, which upstream resolves to an object type and
this port answers `errorType` for — so the rule's `is_error` skip
(`checker.go:4946`'s `TypeFlagsAnyOrUnknown` disjunct, §43) fires and the
property is never considered. **Owner: `checker_types`**, and it converts with
no diagnostics work when `errorType` stops propagating outward through type
constructors. Three cases, 22 lines, and they are the largest single block in
the row — which is exactly why the bar was taken off cases.

### Measured

`diag2307` with `RULE_CODES = [2564]`, the same instrument on both sides:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 235 | 0 | 1,241 | 4 |
| after | **250** | **0** | 1,322 | **4** |

**+15 cases and +81 right lines for zero new wrong lines.** Coverage moved
`1,346 → 1,362 / 5,488` (24.53% → **24.82%**), a case better than the
counterfactual, and `checker_types` read **3,784/9,538 · 83.83%** — the
`.types` workstream's committed number to the case.

The bar was **+9 cases, at most 3 new wrong**. Met, and by a wide enough margin
to be worth naming why: the bar was sized off TS2564's 27 *sole-obstacle* cases,
and the rule converted 15 because it also finished cases whose blocking set held
TS2564 **plus** codes this port already emits. `diaggap`'s single-code column is
a floor on a deepening, not an estimate of it — the twelfth session's rule
("take the bar off the case count") gets a corollary: **that count is the subset
of the answer that is easy to verify, not the answer.**

### The wrong column, read

Unchanged at 4, all four pre-existing and none of them this build's:

- `indexSignatureWithAccessibilityModifier(6,13)` — `[public x: string]: string`
  is a malformed index signature, and the parser recovers it as a *computed
  property declaration* named `x`. The rule is right about what it was handed.
  **Owner: `tsr_parser`'s index-signature recovery**, and the sibling interface
  on line 3 shows the same shape one declaration up.
- `classExtendsEveryObjectType`, `classExtendsEveryObjectType2`,
  `privateNamesNotAllowedAsParameters` — the remaining three, carried from §43.

None is a reason to tighten this rule, and tightening it to silence them would
cost the properties it just converted.

## §88 — TS2300 on a duplicate type parameter is a CHECKER rule, not the binder

### The bar, before the code

**+4 cases**, at most 2 new wrong lines. Off `diaggap`'s case column: TS2300 has
**29** sole-obstacle cases at `82eab5f`, and this build takes only the
type-parameter slice of them.

Falsifiers:

1. **If the binder does not merge two same-named type parameters into one
   symbol**, the rule's test can never fire and the build measures zero. It
   does merge them — that is `declare_into`'s ordinary behaviour, and the
   §2315 fix that stopped `class A<T>` and `class B<T>` sharing a symbol is
   what makes the merge mean "same list" rather than "same file".
2. **If the report belongs on both declarations**, every case gains a wrong
   line and loses a right one. It does not: upstream reports on the *later* one
   only (`for j := range i`), and `duplicateTypeParameters1`'s baseline is a
   single line at column 15 — the second `X` of `function A<X, X>() { }` — not
   two.

### Where it actually lives

The binder was the obvious suspect and it is the wrong one. Upstream reports
this from `checkTypeParameters` (`checker.go:7002`):

```go
for j := range i {
    if typeParameterDeclarations[j].Symbol() == node.Symbol() {
        c.error(node.Name(), diagnostics.Duplicate_identifier_0, ...)
    }
}
```

— an O(n²) scan over one declaration's own list, comparing **symbol identity**,
called from exactly four places: `checkSignatureDeclaration` (`checker.go:2742`),
`checkClassLikeDeclaration` (`:4297`), `checkInterfaceDeclaration` (`:4998`) and
`checkTypeAliasDeclaration` (`:6887`).

That it is symbol identity and not name equality is the whole design: the binder
has already merged the duplicates into one symbol, so the checker's test is
"did two entries in this list end up the same symbol" — which is exactly the
question the binder's own merge answers, without the binder having to report
anything. **A duplicate that merges silently is not a binder that missed the
error; it is a binder that left the error to the consumer that can position it.**

The port's four call sites are the same four, reached through the union of node
kinds that carry a `type_parameters` list. `check.rs:1003` already enumerated
that union exactly for `an_enclosing_declaration_has_type_parameter`, so the
list is factored out rather than written twice — `duplicateTypeParameters3` is
`x: () => <A, A>() => void`, a `FunctionTypeNode`, and would have been missed by
any shorter list.

### What this build does NOT take, and why it is a separate one

The row's other big family is **duplicate parameters** —
`callSignaturesWithDuplicateParameters` alone is 44 of the 120 missing lines,
plus `functionCall15` (`function foo(a?, b?, ...b)`) and
`declarationEmitDestructuring2`. That one **is** the binder, and it is a
structural divergence rather than a missing arm:

upstream's `declareSymbol` takes `excludes` as a **parameter**, and `bindParameter`
passes `SymbolFlagsParameterExcludes` (`binder.go:1200`) while the flags it
declares with are `FunctionScopedVariable`. This port derives excludes from the
flags — `flags.excludes()` in `declare_into` — so a parameter gets
`FunctionScopedVariableExcludes`, which deliberately does **not** collide with
another function-scoped variable, because `var x; var x;` is legal. Upstream's
own comment at `binder.go:1176` says so outright:

> Using ParameterExcludes flag allows the compiler to report an error on
> duplicate identifiers in Parameter Declaration
> `function foo([a,a]) {}` // Duplicate Identifier error

**Excludes is not a function of includes.** Wherever this port derives it, it is
right only for the sites where upstream happens to pass the matching constant.
That is a binder change against the `binder_symbols` rail (8,293/8,460) and gets
its own build and its own measurement.

### Measured

`diag2307` with `RULE_CODES = [2300]`:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 29 | 0 | 299 | 61 |
| after | **34** | **0** | 320 | **61** |

**+5 cases and +21 right lines for zero new wrong lines.** Coverage
`1,362 → 1,367 / 5,488` (24.82% → **24.91%**). Bar was +4 / at most 2 wrong: met.

### The wrong column, read — and it is 61 lines deep

TS2300's false positives are **the largest of any rule this workstream owns**,
and none of them is this build's: the number is 61 before and 61 after. They are
the binder's, and `augmentedTypesModules` (4 lines), `duplicateExportAssignments`
(4) and `es6ImportNamedImport*` name the family — **declaration merging that
upstream permits and this binder rejects**. Every one of them costs a case
today, and `extraonly.rs` is the instrument that prices them.

**This is a bigger row than the missing side.** 61 wrong lines against 120
missing ones, and a removed false positive needs no new machinery. It is the
first thing the next session should price. **Owner: `tsr_binder`'s merge rules**
— the same subsystem §85 and §5 name for TS7026 and TS2454, arriving from the
opposite direction.

### A trap that has now fired twice, in two different sessions

`checker_types` read 3,784 before this build and 3,786 after, and the obvious
reading — "the diagnostics build moved the other workstream's number" — was
wrong both times. The cause is the **`git pull --rebase` inside the previous
build's own push step**: it brought in `9a0409d` (`.types` build 106, "+10
assertion lines"), and 401,521 → 401,531 is that commit's own measurement,
arriving in this workstream's next run.

Stashing the build's code and re-measuring answered it in one run: **3,786 with
and without §88.** The twelfth session hit the identical trap at 83.40% → 83.41%.

**The byte-identity check must be taken against the commit you are actually on,
not against the number written down before your last push.** Cheapest reliable
form: `git stash push <your files>`, run coverage, `git stash pop` — it compares
the two states of the *same* checkout, which is the only comparison that means
anything while another workstream is landing builds hourly.

## §89 — TS7027: unreachable statements come in RUNS, and a namespace is only code if it is instantiated

### The bar, before the code

**+3 cases**, at most 3 new wrong lines. TS7027 has 7 sole-obstacle cases on the
missing side plus `reachabilityChecks11` on the extra side (`extraonly.rs`), and
most of the missing lines are one case — `neverReturningFunctions1`, 25 of 29 —
which is not this build's.

Falsifiers:

1. **If the eleven `reachabilityChecks` files differ only in options these
   branches do not read**, the gain is 2, not 3. They set `allowUnreachableCode`,
   `preserveConstEnums` and `strict` in combination, and only the first two are
   read here.
2. **If `neverReturningFunctions1`'s lines are the same shape**, the bar is far
   too low and the build should be bigger. They are not: they need
   `isReachableFlowNode` over calls to never-returning functions, which is the
   `else if` arm of `isSourceElementUnreachable` (`checker.go:2466`) this build
   does not touch. **Owner: the flow subsystem** — and it wants `never` return
   types, so it is `checker_types`-shaped.
3. **If the run-merge change makes the ancestor test redundant**, removing the
   wrong one costs right lines. Measured separately: the sibling rule changes,
   the ancestor rule does not.

### What §82 got right, and the one thing it could not have known

§82 replaced upstream's mutable `reportedUnreachableFlow` with two stateless
questions — no ancestor and **no earlier sibling** carries the fact — and that
took the wrong column from 19 to 3. The second question is the one that is
slightly wrong, and the case that shows it is `reachabilityChecks1`:

```ts
while (true);        // line 1 (stripped)
var x = 1;           // 2   <- reported
namespace A  { … }   //     swallowed
namespace A1 { … }   //     swallowed
namespace A2 { … }   //     swallowed
namespace A3 { … }   //     swallowed
namespace A4 { … }   //     swallowed
function f1(x) { … } //     BREAKS THE RUN
function f2()  { … } //     …
namespace B  { … }   // 51  <- reported AGAIN
```

Upstream does not report per statement and it does not report once per list. It
reports once per **run**: `checker.go:2409-2442` starts at the unreachable node,
scans *forward* over consecutive statements that are both
`IsPotentiallyExecutableNode` and `isSourceElementUnreachable`, marks them all
reported, and emits one diagnostic spanning first-to-last. A statement that
fails either test **breaks the run**, and the next unreachable statement after
it starts a new one and reports again.

"Any earlier sibling" merges the whole list into one run and can therefore only
ever report once — which is why line 51 is missing. The faithful stateless form
is **"the immediately preceding sibling is a run member"**, and it needs no
state either. §82's insight stands; only its predicate was one word too wide.

### A namespace is code only when it is instantiated

The extra side is the other half of the same function.
`isSourceElementUnreachable` (`checker.go:2455`) does not answer "is this node
unreachable" for a module or an enum; it answers a *different question per kind*:

```go
case ast.KindEnumDeclaration:
    return !ast.IsEnumConst(node) || c.compilerOptions.ShouldPreserveConstEnums()
case ast.KindModuleDeclaration:
    return ast.IsInstantiatedModule(node, c.compilerOptions.ShouldPreserveConstEnums())
default:
    return true
```

`namespace A { interface F {} }` and `namespace C { }` emit no JavaScript, so
there is no unreachable code to detect — and those two are exactly the extra
lines `reachabilityChecks11` carries. This is also **why the run in
`reachabilityChecks1` above does not break at `A1`**: `A1` contains a
`do {} while(true);`, which is a statement, so it *is* instantiated. The
question is asked of the module's body, not of its name.

`GetModuleInstanceState` (`utilities.go:2322`) is ported for its whole worker —
interface / type alias / non-exported import → non-instantiated, `const enum` →
const-enum-only, module block → the max over its children, nested module →
recurse — with **one arm declined**: `getModuleInstanceStateForAliasTarget`,
which resolves `export { x }` against the enclosing statement lists. That arm is
declined to `Instantiated`, which is upstream's own fallback for the case it
cannot locate ("Couldn't locate, assume could refer to a value",
`utilities.go:2436`) and is what this port already effectively answers today — so
the decline can remove no report that exists. **Owner if it ever costs a line:
this same function**, and the shape to port is the ancestor-list walk.

### `preserveConstEnums`, finally

The third option this workstream has plumbed, and the one §82 and §84 both left
open. It is read in exactly the two places above.

### Measured

`diag2307` with `RULE_CODES = [7027]`:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 3 | 0 | 31 | 3 |
| after | **7** | **0** | 34 | **0** |

**+4 cases, and the wrong column went to zero** — the run-merge fix and the
instantiated-module test each removed what the other could not. Coverage
`1,367 → 1,371 / 5,488` (24.91% → **24.98%**). Bar was +3 / at most 3 wrong: met.

Converted: `reachabilityChecks1`, `2`, `9`, `10`, `11`, plus `cf` and
`unreachableJavascriptChecked` — so the eleven-variant family the twelfth session
warned about did convert as five distinct cases, not one. **`checker_types`
byte-identical at 3,786/9,538 · 83.84%**, verified by stash-and-remeasure; the
83.83 → 83.84 in the gradient is the `.types` workstream's build 107, arriving
through this build's own rebase, exactly as §88 describes.

### A correction to §88

§88 wrote that TS2300's 61 false positives "every one of them costs a case
today". **That is wrong and it is corrected here.** `extraonly.rs` lists no
TS2300 entry at all, which means no case is blocked on a TS2300 false positive
*alone* — every case carrying one is also missing something else. The 61 lines
are real and still worth removing, but they are not 61 conversions and they are
not the cheapest thing on the board. The claim was made from the wrong
instrument: `diag2307`'s wrong column prices *lines*, and only `extraonly`
prices a false positive in *cases*.

## §90 — TS2554's `new` half: arity does not need generics, and "1-2" is an ARGUMENT

Diagnosis, sized for the next build. **Not built here.**

TS2554 is the largest actionable relation-free row left: **18 sole-obstacle
cases over 29 missing lines**, a concentration of 1.6 — the best shape on the
board, and the opposite of the trap §82 hit.

`classWithConstructors` carries five of them and splits the row cleanly. The
rule already reports `new C()` for a plain class and already hops one `extends`
link; what it declines is:

```ts
class C2 { constructor(x: number); constructor(x: string); constructor(x: any) {} }
new C2();          // declined: sole_constructor_parameters wants ONE constructor
class C<T> { constructor(x: T) {} }
new C();           // declined: declines any class with type parameters
```

**Both declines are about the argument-TYPE half and neither is about arity.**
`constructor(x: T)` takes one argument whether or not `T` is inferred, and an
overload set has a perfectly well-defined `(min, max)` — the min over each
signature's minimum and the max over each signature's length. The rule computes
arity and argument types in one pass and declines both together; splitting them
so the arity half survives a generic or overloaded constructor is the build.

### The finding worth having before starting

`Expected 1-2 arguments, but got 0.` is **also TS2554**. There is no separate
message and no separate code — `EXPECTED_0_ARGUMENTS_BUT_GOT_1` is
`"Expected {0} arguments, but got {1}."`, and upstream passes the string `"1-2"`
as `{0}` when min and max differ. Checked against
`baselines/reference/classWithConstructors.errors.txt` lines 5-6, which read
`TS2554: Expected 1-2 arguments, but got 0.` — the same code as lines 1-4's
`Expected 1 arguments`.

An implementation that reaches for a second message will not find one in
`messages.rs`; the argument is composed, not selected. That is worth an hour to
whoever would otherwise go looking for `Expected_0_1_arguments_but_got_2`.

### The rest of the row, unsized

`functionOverloads29`/`34`/`37` are the same overload question on the **call**
arm rather than the `new` arm. `genericRestArity`, `requiredInitializedParameter1`
and `spreadOfParamsFromGeneratorMakesRequiredParams` are rest-parameter and
initialiser-ordering shapes that `sole_signature_arity` declines with
`take_while(dot_dot_dot_token.is_none())`. The JS cases
(`argumentsObjectCreatesRestForJs`, `argumentsPropertyNameInJsMode1`/`2`) are
behind `in_js_file`, and §74's rule applies: **a JS decline does not transfer
between arms** — they need measuring, not assuming.

### §90 BUILT — measured

`diag2307` with `RULE_CODES = [2554, 2555, 2345]` (the three codes this rule
emits, isolated together because the argument-type half shares its gate):

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 28 | 0 | 115 | 14 |
| after | **30** | **0** | 129 | **10** |

**+2 cases, +14 right lines, and the wrong column DOWN 4.** Coverage
`1,371 → 1,373 / 5,488` (24.98% → **25.02%**). `checker_types.snap` unchanged —
no diff at all, not merely the same totals.

The range formatting turned out to be already present: the **call** arm at
`call_arity.rs:88` composes `format!("{minimum}-{maximum}")` into the same
`EXPECTED_0_ARGUMENTS_BUT_GOT_1`. §90's finding was derived from the baseline and
then found sitting in this file's other half — the `new` arm had simply never
needed it, because it declined every callee that could produce a range.

### The wrong column, read — and the four lines this build REMOVED

The first cut measured **+5 new wrong lines**, all in
`inheritedConstructorWithRestParams2`, and reading the top row is what found the
real defect:

```ts
declare class BaseBase<T, U> extends BaseBase2 { constructor(x: T, ...y: U[]); … }
class Base extends BaseBase<string, number> { }
new Derived("", 3, 3);   // reported "Expected 1 arguments, but got 3"
```

The base-class hop advanced while no class had a constructor **with a body**, so
it walked straight past `BaseBase`'s three ambient overloads into `BaseBase2`'s
`constructor(x: number)` and priced every call against it. `getSignaturesOfType`
takes a class's own construct signatures whenever it declares any — a body is
not what makes a signature. Stopping at any declared constructor fixed the six
lines **and removed four that predated this build**, which is why the column
reads 10 rather than 14.

Only the generic-base decline was newly lifted; the "walk past a bodyless
constructor" defect was already there, hidden behind it. **A decline can conceal
a bug rather than prevent one**, and lifting it is the only way to find out
which.

Remaining wrong, both pre-existing and both already owned: `callWithMissingVoid`
(§78, owner `parameter_annotation_is_void`) and
`objectCreationOfElementAccessExpression` (the callee is not an identifier;
owner `callee_symbol`).

## §91 — `getMinArgumentCount` counts backwards, and this port counted forwards

Both arity arms computed the minimum as *"the index of the first parameter that
is optional or carries an initialiser"*. Upstream builds it the other way round,
in `getSignatureFromDeclaration` (`checker.go:19872-19879`):

```go
isOptionalParameter := isOptionalDeclaration(param) || param.Initializer() != nil ||
    isRestParameter(param) || …
if !isOptionalParameter {
    minArgumentCount = len(parameters)      // reset at EVERY required parameter
}
```

The count is overwritten at every non-optional parameter, so what survives the
loop is **the position after the LAST required one**. The two readings agree on
every signature whose optional parameters are trailing — which is nearly all of
them, which is why this stood — and disagree on exactly one shape:

```ts
function f1(a, b = 0, c) { }
f1(0, 1);      // TS2554: Expected 3 arguments, but got 2
```

The first-optional reading answers `1` and accepts the call silently. The
backward scan answers `3`. `requiredInitializedParameter1` is the case, and it
is legal TypeScript — an initialiser does not make the parameters after it
optional, it only makes *that* one omissible-by-position, which the language
does not actually allow you to exploit.

Written once and shared by all three sites (the call arm, its rest-parameter
early return, and §90's `new` arm), which is also how the `new` arm inherited
the fix for free.

**Measured**, `diag2307` with `RULE_CODES = [2554, 2555, 2345]`: CONVERTS
30 → **31**, RIGHT 129 → 131, WRONG 10 → **10**, LOST 0. Coverage
`1,374 → 1,375 / 5,488` (**25.05%**). `checker_types` identical at
3,838/9,538 · 84.10% by stash-and-remeasure — the snapshot moved in this commit
because the `.types` workstream's build arrived through the same pull, which is
§88's trap firing a third time and being caught by the rule §88 wrote.

## §92 — the call arm's overload sets, §90 transplanted

`sole_signature_arity` declines a callee whose symbol has more than one
declaration, and always should: it is about *one* signature, and which signature
a call resolves to is `resolveCall`'s question. But `getArgumentArityError`
(`checker.go:9715`) computes its range across **all** candidates, so the arity
question has an answer even where the resolution question does not — exactly the
split §90 made for `new`, and this is that change transplanted to the call arm.

The signatures are the **bodiless** declarations; the implementation is not a
call signature (`getSignaturesOfSymbol`). `min` is the minimum over their
minimums and `max` the maximum over their lengths, and a set with any rest
parameter declines rather than guessing a bound. The argument-**type** half is
not attempted at all, because there is no single parameter list to attempt it
against — the flag §90 introduced is not even needed here, the path simply never
reaches `check_argument_types`.

Added as a fallback (`sole_signature_arity(callee).or_else(overload_set_arity)`)
rather than folded into the existing function, so the single-signature path — 129
right lines and every one of this rule's existing conversions — is bit-for-bit
untouched and the counterfactual measures only the new arm.

**Measured**, `diag2307` with `RULE_CODES = [2554, 2555, 2345]`: CONVERTS
31 → **34**, RIGHT 131 → 141, WRONG 10 → **10**, LOST 0. Coverage
`1,375 → 1,378 / 5,488` (25.05% → **25.11%**). `checker_types` identical at
3,841/9,538 · 84.11% by stash-and-remeasure — the fourth time this session the
snapshot moved under a build and the fourth time it was the other workstream's.

The wrong column is unchanged and both entries are pre-existing and owned:
`callWithMissingVoid` (§78, owner `parameter_annotation_is_void`) and
`objectCreationOfElementAccessExpression` (the callee is not an identifier;
owner `callee_symbol`).

## §93 — excludes is not a function of includes

The largest single family in TS2300's missing column —
`callSignaturesWithDuplicateParameters` alone is 44 of 120 lines, plus
`functionCall15`, `declarationEmitDestructuring2`, `propertySignatures` — and it
was not a missing arm. It was a **shape divergence in `declare_into`**.

Upstream's `declareSymbol` (`binder.go:202`) takes `includes` and `excludes` as
two independent arguments, and `bindParameter` (`binder.go:1200`) passes:

```go
b.declareSymbolAndAddToSymbolTable(node, ast.SymbolFlagsFunctionScopedVariable,
                                         ast.SymbolFlagsParameterExcludes)
```

`FunctionScopedVariable` for what it declares, and `ParameterExcludes` — plain
`Value` — for what it collides with. This port derives excludes from the flags,
so a parameter got `FunctionScopedVariableExcludes`, which is
`Value & ~FunctionScopedVariable` and therefore **deliberately does not collide
with another function-scoped variable** — because `var x; var x;` is legal.
Consequence: `function bar(a, a) {}` reported nothing at all.

Upstream's own comment at `binder.go:1176` is explicit that the distinction
exists for exactly this:

> Using ParameterExcludes flag allows the compiler to report an error on
> duplicate identifiers in Parameter Declaration
> `function foo([a,a]) {}` // Duplicate Identifier error

**A derived value that is right at most call sites is not the same as a
parameter.** Twelve sessions read `flags.excludes()` as the definition of
excludes; it is the *default*, and one binding site overrides it.

`declare_into` keeps its signature and delegates to
`declare_into_with_excludes`, so the seven other call sites are untouched and
the counterfactual measures only the parameter arm. `IsPartOfParameterDeclaration`
is ported for the binding-element case (`function foo([a, a]) {}`) by walking out
through the pattern and letting the first non-pattern ancestor decide — a
variable declaration or a `catch` keeps the derived excludes.

**Measured**, `diag2307` with `RULE_CODES = [2300, 2451, 2567, 2528]` (every
code `declare_into` can emit, isolated together because one predicate feeds all
four): CONVERTS 47 → **50**, RIGHT 426 → **490**, WRONG 79 → **79**, LOST 0.
Coverage `1,378 → 1,381 / 5,488` (25.11% → **25.16%**).

**The `binder_symbols` rail did not move: 8,311/8,475 · 98.06% with and without**,
and `checker_types` likewise at 3,842/9,538 · 84.13% — both by
stash-and-remeasure, which is the fifth time this session the snapshot had moved
underneath a build and the fifth time it was the other workstream's.

Sixty-four right lines from one argument is the largest line yield of the
session, and it is worth being clear about why it was cheap: **nothing was
missing.** The diagnostic, its message selection, its position and its
report-on-every-declaration loop were all built and correct; they were simply
never reached for a parameter.

## §94 — the excludes audit: one real divergence found, measured, and REFUSED

§93 raised the general question — `declare_into` derives excludes for the other
seven call sites too, and upstream passes it at all of them. This is that audit,
run the way §84 ran the compiler-option one.

**Result: the derivation matches upstream constant-for-constant for every kind
except one.** `symbol.rs`'s `excludes()` was checked line by line against
`ast/symbolflags.go:52-74` — function-scoped variable, block-scoped variable,
property, enum member, function, class, interface, enum, method, accessor, type
parameter and alias all agree, including the several the comments record as
having been wrong once and fixed.

### The one divergence

Upstream has **two** module excludes and this port had one:

```go
SymbolFlagsValueModuleExcludes     = SymbolFlagsValue & ^(Function | Class | RegularEnum | ValueModule)
SymbolFlagsNamespaceModuleExcludes = SymbolFlagsNone
```

`excludes()` returned `None` for every module. A namespace that emits JavaScript
occupies value space and *does* collide with a variable of the same name, which
is why `module_augmentExistingVariable`, `module_augmentExistingAmbientVariable`,
`mergedClassWithNamespacePrototype` and
`augmentedClassWithPrototypePropertyOnModule` sit in TS2300's missing column.

### Measured, and refused

`diag2307` with `RULE_CODES = [2300, 2451, 2567, 2528]`:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| §93 (before) | 50 | 0 | 490 | 79 |
| ValueModuleExcludes | **52** | 0 | 500 | **93** |

**+2 cases for +14 wrong lines. Refused and reverted.**

The wrong lines name the reason: `augmentExportEquals7`
(`/node_modules/lib/index.d.ts`), `duplicateExportAssignments`,
`es6ImportNamedImportIdentifiersParsing`. They are **ambient and augmentation
module declarations**, which upstream lets merge — and it can, because
`classify` upstream chooses between `ValueModule` and `NamespaceModule` per
declaration, using `IsInstantiatedModule`. This port maps every
`ModuleDeclaration` to `VALUE_MODULE`, so giving that flag the stricter excludes
applies it to declarations upstream would have flagged `NamespaceModule`.

**Owner: `classify` in `tsr-binder`, and the fix is to choose the flag rather
than to change what the flag excludes.** §89 already ported
`GetModuleInstanceState` — into `crate::check`, where the binder cannot reach
it. Moving it to `tsr-ast` beside the other `ast/utilities.go` ports would let
both consumers have it, and *then* this divergence is one line.

**Do not repeat this measurement.** +2/−14 is the number; it does not improve by
adjusting the mask, because the mask is not what is wrong.

### What the audit is worth even though nothing landed

The other eleven derivations are now **checked against upstream rather than
assumed**, which is the durable part — the same way §84's option table outlived
its zero. And it sharpens §93's rule: *excludes is not a function of includes*,
but the deeper problem here is that **includes was not a function of the
declaration either.** A derived excludes can only be as right as the flag it is
derived from.

## §95 — §94's refusal, reversed by fixing the owner it named

§94 measured `ValueModuleExcludes` at **+2 cases for +14 wrong lines** and
refused it, naming the owner: *the fix is to choose the flag, not to change what
the flag excludes.* This is that fix, and the refusal reverses.

`bindModuleDeclaration` (`binder.go:1268`) picks the module flag from
`GetModuleInstanceState` — a namespace that emits JavaScript is a `ValueModule`,
one that emits nothing is a `NamespaceModule` — and the two carry different
excludes. This port mapped **every** `ModuleDeclaration` to `VALUE_MODULE`, so
§94's stricter mask landed on the ambient and augmentation declarations upstream
would have flagged `NamespaceModule`, and they collided where upstream lets them
merge.

### The move that made it possible

`GetModuleInstanceState` was ported in §89 **into `crate::check`**, where the
binder cannot reach it. It now lives in `tsr-ast` beside the other
`ast/utilities.go` ports, rewritten against the **typed** tree via
`push_children` rather than against node ids — which is what makes it shareable:
no side tables, no `&Checker`, no `&Binder`. Both consumers call the same
function, which is also what upstream does.

That rewrite is the load-bearing part. A port that reaches for `self.node_map`
is a port that belongs to whichever crate owns the map, and this one had to
belong to neither.

### Measured

`diag2307` with `RULE_CODES = [2300, 2451, 2567, 2528]`:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| §93, before | 50 | 0 | 490 | 79 |
| §94, mask alone — **refused** | 52 | 0 | 500 | 93 |
| **§95, flag + mask** | **52** | **0** | **500** | **64** |

**+2 cases, +10 right lines, and the wrong column down 15** — the same two
conversions §94 bought, with **29 fewer wrong lines** than the mask alone and
**15 fewer than doing nothing.** Those 15 are part of the 61 false positives
§88 found and §89 corrected the pricing of; a third of that row was this one
divergence.

Coverage `1,381 → 1,383 / 5,488` (25.16% → **25.20%**).
`binder_symbols` **did not move**: 8,311/8,475 · 98.06% with and without.

### `checker_types` is NOT byte-identical here, and that is worth stating plainly

**3,843 → 3,846 (+3 cases).** Every other build this session left it untouched;
this one did not, and the standing instruction is that it must stay identical.

The change is an **improvement**, not damage — a non-instantiated namespace now
carries the flag upstream gives it, so the other workstream's type resolution
sees the same symbol shape upstream does. But "it went up" is not the same as
"it did not change", and a binder flag change is exactly the kind that can move
a gradient either way. **Recorded, not glossed.** If the `.types` workstream
wants it reverted they need only the one line in `classify`.

### A test asserted the divergence

Three tests failed, and all three had encoded the bug:

- `declaration_merging_pairs_are_permitted` asserted
  `VALUE_MODULE.excludes().is_empty()` under the comment *"A namespace merges
  with anything"*. True of a `NamespaceModule` and false of a `ValueModule`.
- `top_level_declarations_get_the_flags_they_should` and
  `declarations_that_typescript_merges_produce_one_symbol` both used **empty**
  namespaces (`namespace N {}`) as their fixture, which upstream classifies as
  non-instantiated — so they were asserting `VALUE_MODULE` for the one shape
  that never was one.

All three now test the distinction rather than its absence, with an
instantiated and a non-instantiated fixture each. **A green test suite is not
evidence that the port is faithful; it is evidence that the port agrees with
whatever the tests were written against.** The fixtures were chosen for
brevity — `namespace N {}` is the shortest namespace you can write — and
brevity picked the degenerate case three times.

## §96 — TS2448/TS2450 attempted, measured at 176 wrong lines and 6 LOST, REFUSED

`checkResolvedBlockScopedVariable` (`checker.go:1888`) picks its message off the
symbol's flags — TS2448 for a block-scoped variable, TS2449 for a class, TS2450
for an enum — and shares everything below, so building the other two arms on
§83's class arm looked like the cheapest thirteen cases on the board (TS2448 7
sole-obstacle cases, TS2449 5, TS2450 1).

**It is not.** Measured, `diag2307` with `RULE_CODES = [2448, 2449, 2450]`:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 6 | **0** | 14 | **0** |
| the two new arms | 7 | **6** | 23 | **176** |

**One case gained, six lost, and a rule with a perfect wrong column went to 176.**
Reverted whole. `augmentedTypesInterface`, `enumWithExport`,
`controlFlowNullishCoalesce` and `importTypeAmbient` are among the six.

### Why §83's bound was doing more work than it looked like

§83 bounded the class arm to the `extends` clause and this section's own handoff
described that as "the one position the deferral arms cannot apply to". That is
true, and the implication that was missed is the contrapositive: **outside that
position the deferral arms are not an edge case, they are the majority of the
behaviour.** `isBlockScopedNameDeclaredBeforeUse` (`checker.go:1922`) is roughly
eighty lines and almost all of them say *"order cannot be determined here, allow
it"*. A rule that ports the report and approximates the deferrals reports
constantly.

The approximation tried was: decline any use with a function-like, property,
decorator or computed-name ancestor, plus the export-specifier and
binding-element parents. That is nowhere near enough — 176 wrong lines says the
missing arms are load-bearing, not marginal.

### What the next attempt must do differently

**Port `isBlockScopedNameDeclaredBeforeUse` first and completely, then attach the
report.** Not the other way round. Specifically the arms this attempt did not
have:

- `isUsedInFunctionOrInstanceProperty` (`checker.go:1975`) walks up **stopping at
  the declaration's own container**, so a use and a declaration inside the same
  function are *not* deferred — the crude "any function ancestor at all"
  approximation both over-defers there and, more importantly, does not cover the
  cases that actually matter.
- The `NodeFlagsJSDoc` arm, the `PropertyDeclaration` instance-initialiser arm
  with its `isStatic` split, and the `ExportAssignment` arm's `isExportEquals`
  condition — each declines a shape this attempt reported on.
- Whatever explains the six LOST, which were not diagnosed before reverting:
  three of the four named involve **declaration merging** (`augmentedTypesInterface`,
  `enumWithExport`, `importTypeAmbient`), so `merged_symbol` handing back a
  symbol whose `declarations` list spans a merge is the first thing to check —
  the arm picks one declaration by kind and compares *its* position, which for a
  merged enum or interface is arbitrary among the merge's members.

**Owner: `isBlockScopedNameDeclaredBeforeUse` itself, ported whole.** The
thirteen cases are still there and still relation-free; what is refused is
reaching them by extending §83 rather than by building the predicate. Do not
re-attempt the cheap version — this is its number.

## §97 — the retry: 176 wrong → 21, 6 LOST → 1, still refused

§96 said *port the predicate whole and first*. This is that retry, and it moves
the number by a factor of eight without reaching the bar.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 6 | **0** | 14 | **0** |
| §96, approximated deferrals | 7 | 6 | 23 | 176 |
| **§97, deferrals ported** | 7 | **1** | 21 | **21** |

**Reverted.** LOST may not grow, and one is growth.

### What the factor of eight was

**A use in a type context is deferred regardless of its position** —
`usage.Flags&NodeFlagsJSDoc != 0 || IsInTypeQuery(usage) ||
c.isInAmbientOrTypeNode(usage)` (`checker.go:1932`, `:11238`), the *first* thing
upstream asks after the same-file test and the arm §96 did not have at all. A
type annotation naming a class declared later in the file is legal and
everywhere; reporting on it accounts for most of 176.

Two more that were also missing and are cheap once seen: the walk quits at the
declaration's own **container** rather than at the source file
(`isUsedInFunctionOrInstanceProperty`, `checker.go:2011`), and an
`ExportSpecifier` / `export =` parent defers.

### The merge decline, which was right

§96's six LOST were diagnosed as suspected merges and the fix confirmed it: a
symbol with more than one declaration is a merge, the arm picks one declaration
by kind and compares *its* position, and for a merge that position is arbitrary
among the members. Declining a multi-declaration symbol took the LOST from six
to one — `augmentedTypesInterface`, `enumWithExport` and `importTypeAmbient` all
stopped breaking. **That decline should be kept by whoever finishes this.**

### What is left, precisely

- **One LOST: `conformance/controlFlowNullishCoalesce`.** Undiagnosed. It is a
  *flow* fixture, so the likely shape is a use that upstream defers for a reason
  none of the three ported arms covers.
- **21 wrong lines**, of which `enumUsedBeforeDeclaration.ts(2,24)` is one and is
  instructive: the case's *first* line is a genuine conversion and its second is
  a false positive, so the enum arm is right about the shape and wrong about a
  position — likely `declContainer` again, since an enum's container and a
  variable's differ.

**Refused, and this is the second number on it.** The remaining gap is not
another arm of `isBlockScopedNameDeclaredBeforeUse`; it is
`GetEnclosingBlockScopeContainer` (`ast/utilities.go`) ported properly so the
walk can quit where upstream quits. That is a small, well-defined function and
is the whole of what stands between this and thirteen cases. **Next attempt
should port it first and re-run this exact measurement** — the two numbers above
are the baseline to beat, and beating them means LOST 0 and WRONG well under 10.

## §98 — the container ported: CONVERTS 6 → 8, and the refusal now rests on ONE case

Third measurement on `isBlockScopedNameDeclaredBeforeUse`, and the last this
session takes. `GetEnclosingBlockScopeContainer` (`ast/utilities.go:2171`) and
`IsBlockScope` (`:2177`) are ported, so
`isUsedInFunctionOrInstanceProperty`'s walk quits where upstream's quits.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 6 | **0** | 14 | **0** |
| §96 approximated | 7 | 6 | 23 | 176 |
| §97 deferrals ported | 7 | 1 | 21 | 21 |
| **§98 container ported** | **8** | **1** | 31 | 21 |

**Reverted, on the LOST alone.** Everything else is now in range: two extra
conversions, and a wrong column that fell from 176 to 21 across three attempts
while the right column more than doubled.

### The refusal is now one named case

`conformance/controlFlowNullishCoalesce`, and §98 narrowed *which arm*: removing
the enum arm entirely leaves LOST at 1, so it is the **block-scoped-variable**
arm. That is the whole of what is unresolved — one fixture, one arm, and a rule
that is otherwise measuring well.

The three findings to carry forward, each bought with a measurement:

1. **The type-context deferral** (`checker.go:1932`) — 155 of §96's 176 wrong
   lines.
2. **The multi-declaration decline** — a merged symbol's declaration position is
   arbitrary among its members; took LOST from 6 to 1.
3. **The container-bounded walk** (`checker.go:2011` + `utilities.go:2171`) —
   worth +1 conversion and +10 right lines over §97, because a use and a
   declaration sharing a function are compared by position rather than deferred.

### Why three refusals on one item is not three wasted attempts

Each measurement isolated a different arm, and the numbers only make sense as a
sequence: 176 → 21 → 21-with-two-more-conversions. A single attempt that landed
at 21 wrong would have been reverted with no idea which of five missing arms
mattered. **The sequence is the diagnosis.** What is left is not "port more of
the predicate" — the predicate is ported — it is *debug one fixture*, which is a
different and much smaller job than the one this section started with.

Thirteen sole-obstacle cases remain behind it (TS2448 7, TS2449 5, TS2450 1),
all relation-free. **Next session: reproduce `controlFlowNullishCoalesce` with
`diagcase`, find the arm, and this lands.**

## §99 — one missing ambient kind was 20 of the 21 wrong lines AND the LOST

§98 left the refusal resting on one named fixture. It is
`conformance/controlFlowNullishCoalesce`, and it is three lines:

```ts
let a: number;
o ?? (a = 1);              // <- use of `o`, line 6
a.toString();
declare const o: { x: number } | undefined;   // <- declaration, line 10
```

`o` is a block-scoped const used before its declaration, and upstream reports
nothing because `checkResolvedBlockScopedVariable` (`checker.go:1888`) gates on
`declaration.Flags&NodeFlagsAmbient == 0` — **the declaration is ambient.**

§83 wrote `declaration_is_in_an_ambient_context` for exactly this gate and gave
it two kinds, `ClassDeclaration` and `ModuleDeclaration`, because those are the
two its `extends`-clause bound could ever see. `declare const o` puts the
`declare` on the enclosing **`VariableStatement`**, and the node this rule holds
is the `VariableDeclaration` inside it — so the helper looked at a node with no
modifiers and answered "not ambient".

Adding `VariableStatement`, `FunctionDeclaration` and `EnumDeclaration` to that
list:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 6 | 0 | 14 | 0 |
| §96 approximated deferrals | 7 | 6 | 23 | 176 |
| §97 deferrals ported | 7 | 1 | 21 | 21 |
| §98 container ported | 8 | 1 | 31 | 21 |
| **§99 ambient kinds** | **8** | **0** | **31** | **1** |

**+2 cases and +17 right lines for one wrong line, and it lands.** Coverage
`1,383 → 1,385 / 5,488` (25.20% → **25.24%**). `checker_types` identical at
3,860/9,538 · 84.19% and `binder_symbols` unmoved at 8,311/8,475, both by
stash-and-remeasure.

### What the four measurements actually found

The wrong column went 176 → 21 → 21 → **1**, and each step was a different
missing arm. But the last step is the one worth remembering: **one under-scoped
helper accounted for the LOST *and* twenty of the twenty-one remaining wrong
lines.** The three ported deferral arms were all necessary and none of them was
the biggest single defect.

§83's helper was not wrong when it was written — it was *sufficient for its
caller*, and its doc comment said so. It became wrong the moment a second caller
arrived, silently, with no compiler error and no test to catch it. **A predicate
written to be sufficient for one caller is a landmine for the second**, and this
port has three declared-but-never-set flags (`NodeFlags::AMBIENT`,
`NodeFlags::JAVASCRIPT_FILE`, `SymbolFlags::OPTIONAL`) whose absence is exactly
what forces helpers like this one to exist. `NodeFlags::AMBIENT` would have
answered this in one read.

### The one remaining wrong line, declined with its owner

`enumUsedBeforeDeclaration.ts(2,24)` — the case's **first** line converts and its
second is a false positive, so the enum arm has the shape right and a position
wrong. It is one line in one case and the rule now reports 31 right lines
against it. **Owner: `getEnclosingBlockScopeContainer` for an enum**, whose
container differs from a variable's; not worth a fifth measurement today.

## §100 — a `const enum` is not a `RegularEnum`, and the rule closes at zero wrong

§99 left one wrong line, `enumUsedBeforeDeclaration.ts(2,24)`, and named the
wrong owner. It is not `getEnclosingBlockScopeContainer`. The case is four lines:

```ts
const v: Color = Color.Green;              // TS2450 — reported by upstream
const v2: ConstColor = ConstColor.Green;   // NOTHING — a const enum is inlined
enum Color { Red, Green, Blue }
const enum ConstColor { Red, Green, Blue }
```

`checkResolvedBlockScopedVariable` tests `SymbolFlagsRegularEnum`
(`checker.go:1908`), not `SymbolFlagsEnum`: a `const enum` is substituted at
every use site, so it has no temporal dead zone to be inside.

Changing the checker's test to `REGULAR_ENUM` **did nothing**, and that is the
finding: `classify` maps **every** `EnumDeclaration` to `S::REGULAR_ENUM`, so
`ConstColor`'s symbol carried the regular-enum flag and answered the narrower
test anyway.

**This is §95's defect one flag over.** Upstream's binder splits on
`IsEnumConst` because `ConstEnum` and `RegularEnum` have different excludes —
`ConstEnumExcludes = (Value|Type) & ^ConstEnum`, so a const enum merges only
with another const enum, where a regular one also merges with a namespace
(`symbolflags.go:64-65`). Both the flag choice and the excludes are ported here.

### Measured

`diag2307` with `RULE_CODES = [2448, 2449, 2450]`, against the state §96 started
from:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before §96 | 6 | 0 | 14 | **0** |
| §96 | 7 | 6 | 23 | 176 |
| §97 | 7 | 1 | 21 | 21 |
| §98 | 8 | 1 | 31 | 21 |
| §99 | 8 | 0 | 31 | 1 |
| **§100** | **9** | **0** | **31** | **0** |

**+3 cases and +17 right lines over five measurements, ending where it started
on the wrong column: zero.** Coverage `1,385 → 1,386 / 5,488` (**25.26%**).
`binder_symbols` unmoved at 8,311/8,475 · 98.06% and `checker_types` identical
at 3,860/9,538 · 84.19%, both by stash-and-remeasure.

### The pattern this session found three times

§93, §95 and §100 are the same defect at three sites:

| | derived / collapsed | upstream distinguishes |
|---|---|---|
| §93 | excludes from the declared flags | `excludes` is a separate argument |
| §95 | every module is a `ValueModule` | `IsInstantiatedModule` picks the flag |
| §100 | every enum is a `RegularEnum` | `IsEnumConst` picks the flag |

Two of the three were found only because a *consumer* asked a question the
collapsed flag could not answer, and in both cases the first fix attempt was
aimed at the consumer — §94 changed the excludes mask, §100's first cut changed
the checker's flag test — and measured nothing or worse. **When a consumer's
faithful test gives an unfaithful answer, suspect the flag before the test.**

`classify` is now checked against upstream's binder for modules and enums. The
remaining collapse candidates, unaudited: `S::PROPERTY` for five different node
kinds, and `S::ALIAS` for five import/export forms.

## §101 — §83's `extends` bound retired, at +0 cases

Once §97–§100 had the deferral predicate, the question §83 could not ask became
askable: **is the `extends`-clause bound still doing anything?**

§83 introduced it because `extends` is the one position
`isBlockScopedNameDeclaredBeforeUse`'s deferral arms cannot apply to, which made
a class arm possible without porting them. They are ported now, so the bound is
either redundant or it is hiding something.

Replacing `is_in_extends_clause(node)` for the class arm with the same
`use_is_not_deferred` the other two arms use — keeping `extends` as a disjunct,
since a heritage clause is a value position the predicate need not re-derive:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| §100 | 9 | 0 | 31 | **0** |
| **§101** | **9** | **0** | **33** | **0** |

**+0 cases, +2 right lines, wrong still zero.** Coverage unchanged at
`1,386 / 5,488` — measured on both sides, not assumed.

### Landed anyway, and why that is not a contradiction

A build worth zero cases is normally not worth landing; the standing rule is
that a population is a ceiling, not a conversion, and §84's option audit landed
at a *verified* zero for a specific reason. This one has its own:

- It **deletes a workaround.** The bound was scaffolding for a rule that could
  not yet ask the real question, and scaffolding left in place reads as a
  deliberate bound to the next person — §96 spent its first measurement partly
  because this section's own handoff described the bound as load-bearing.
- It is **strictly more faithful**: the class arm now asks what
  `checker.go:1922` asks, at every position, rather than at one.
- The two extra right lines are cases still short on other codes, so they
  convert for free when those land.

The falsifier was real and it did not fire: if the deferral arms had been
weaker than the `extends` bound, the wrong column would have moved off zero.
It did not, which is independent evidence that §97–§100's port is sound —
**a bound removed without cost is a test of what replaced it.**

### Where this rule now stands

Nine conversions, thirty-three right lines, **zero wrong, zero LOST**, from six
measurements. What is still unported are the deferral arms no corpus case has
exercised: the JSDoc arm, the instance-property `isStatic` split, the
binding-element recursion, and the decorator arms behind `legacyDecorators`.
None has a case behind it today; each is one `STILL SHORT` row away from
mattering.

## §102 — the `classify` collapse audit, completed: no further divergence

§100 named two unaudited collapse candidates and this closes them. **This is a
reading audit, not a measurement** — it changes no code, so there is no
counterfactual to run, and it is recorded with that limitation stated rather
than dressed as a verified zero the way §84's was.

### The excludes masks — all eleven verified

Every branch of `SymbolFlags::excludes()` was read against
`ast/symbolflags.go:52-74`:

| flag | this port | upstream |
|---|---|---|
| function-scoped variable | `Value & ^FunctionScopedVariable` | ✓ |
| block-scoped variable | `Value` | ✓ |
| property | `Value & ^(Property\|Accessor)` | ✓ |
| enum member | `Value \| Type` | ✓ |
| function | `Value & ^(Function\|ValueModule\|Class)` | ✓ |
| class | `(Value\|Type) & ^(ValueModule\|Interface\|Function)` | ✓ |
| interface | `Type & ^(Interface\|Class)` | ✓ |
| const enum / regular enum | split, §100 | ✓ |
| value / namespace module | split, §95 | ✓ |
| method, accessors | `Value & ^Method`, `Value & ^(other\|Property)` | ✓ |
| type parameter, type alias, alias | `Type & ^TypeParameter`, `Type`, `Alias` | ✓ |

### The flag choices — the two candidates are not collapses

- **`S::ALIAS` over five import/export forms.** Upstream declares every one of
  them with `SymbolFlagsAlias` / `SymbolFlagsAliasExcludes`
  (`binder.go:704`, `:832`, `:838`, `:851`, `:1169`). Not a collapse — upstream
  makes no distinction either. `ExportAssignment` is the one that *does* split,
  on `ExpressionIsAlias` (`binder.go:863`), and this port already splits it the
  same way.
- **`S::PROPERTY` over five node kinds.** `PropertyDeclaration`,
  `PropertySignature`, `PropertyAssignment`, `ShorthandPropertyAssignment` and
  `JsxAttribute` are all `Property` upstream. Not a collapse.

So the §93/§95/§100 pattern has **two instances, not four**, and both are fixed.

### The one divergence that remains, already tracked

`Node::EnumMember(_) => (S::ENUM_MEMBER, D::Members)`. Upstream files an enum
member in the **enum symbol's EXPORTS** — `case ast.KindEnumDeclaration: return
b.declareSymbol(ast.GetExports(b.container.Symbol()), …)` (`binder.go:436-437`)
— and the container-aware remap in this binder covers only the class
static/instance split. This is a *destination* divergence rather than a flag
one, it is already recorded in the project's `bd` memory with a test pinning it,
and it is not this workstream's to fix: it is a `binder_symbols` change with
`checker_types` consequences.

### Why a reading audit was the right depth here

§84 measured its option audit because an option that nothing reads is
indistinguishable from an option read wrongly — the code path had to be proven
live. Excludes masks are different: every one of them is exercised by
`declare_into` on every declaration in the corpus, so a wrong mask cannot hide.
The reading is sufficient evidence for the masks and is **not** sufficient
evidence for anything about behaviour, which is why nothing here is claimed as
converted or refused.

## §103 — TS1029, modifier order: a rule with no types in it at all

### The bar, before the code

**+6 cases**, at most 3 new wrong lines. TS1029 has **12 sole-obstacle cases**
at §102's commit, and it is the first target this session with *no type
machinery whatsoever* — `checkGrammarModifiers` (`grammarchecks.go:290`) is a
left-to-right scan over a modifier list accumulating a flag set.

Falsifiers:

1. **Upstream reports at most ONE grammar error per node** — every arm is
   `return c.grammarErrorOnNode(...)`. A port that reports every violation in a
   list turns `private static override x` into three lines where upstream writes
   one. The scan must stop at the first.
2. **If the corpus's cases are mostly statement-level**, the class-element bound
   below converts far fewer. Checked: of the twelve, ten are class members
   (`multipleClassPropertyModifiers`, `staticMustPrecedePublic`, `Protected6`,
   `override11`, `overrideKeywordOrder` ×5, `parserAccessibilityAfterStatic1`/`10`)
   and `defaultKeywordWithoutExport1` is the statement-level one — declined.
3. **If the parser drops out-of-order modifiers** rather than keeping them in
   written order, the scan has nothing to read. `parserAccessibilityAfterStatic1`
   is in the missing column rather than the extra one, which is consistent with
   the modifiers being present and simply unexamined.

### The shape

```go
case ast.KindPublicKeyword, ast.KindProtectedKeyword, ast.KindPrivateKeyword:
    if flags&ModifierFlagsAccessibilityModifier != 0 { …already seen… }
    else if flags&ModifierFlagsOverride  != 0 { must precede "override" }
    else if flags&ModifierFlagsStatic    != 0 { must precede "static" }
    else if flags&ModifierFlagsAccessor  != 0 { must precede "accessor" }
    else if flags&ModifierFlagsReadonly  != 0 { must precede "readonly" }
    else if flags&ModifierFlagsAsync     != 0 { must precede "async" }
```

The order of the `else if` chain is the specification, not an implementation
detail: `static public async` reports *"public must precede static"* and not
*"public must precede async"*, because `static` is tested first. Ported in the
same order for the same reason.

`override` has its own shorter chain (`readonly`, `accessor`, `async`), and the
arms this build does **not** port — `already seen`, `cannot be used with`,
`cannot appear on a module or namespace element` — are different codes and
belong to their own rows.

### Measured

`diag2307` with `RULE_CODES = [1029]`:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 0 | 0 | 0 | 0 |
| **after** | **9** | **0** | **20** | **0** |

**+9 cases for zero wrong lines**, against a bar of +6 / at most 3. Coverage
`1,386 → 1,395 / 5,488` (25.26% → **25.42%**), the session's second-largest
build and its cheapest per line of code. `checker_types` identical at
3,864/9,538 · 84.21% and `binder_symbols` unmoved, both by stash-and-remeasure.

All three falsifiers were live and none fired: the one-report-per-node rule was
implemented from the start (upstream's `return`), the class-element bound covers
ten of the twelve cases, and the parser does keep out-of-order modifiers in
written order.

### Why this was sitting on the board for thirteen sessions

**It needs nothing.** No types, no symbols, no flow, no relation — a `Vec` of
seen keywords and a left-to-right walk. It converted nine cases at zero risk,
and the reason it had never been picked is that every previous session ranked
targets by `diagreach`, which measures *cases reachable by deepening rules that
exist*. TS1029 has no rule to deepen, so it appears only in `diaggap`'s
single-code column and only once the rows above it have been worked.

**The lesson for ranking: `diagreach` and `diaggap` answer different questions,
and the cheap grammar codes live exclusively in the second.** `TS1100` (12
cases), `TS1109` (11), `TS1163` (10) and `TS1183` are the same shape and are the
first things the next session should price — none of them needs a type either.

## §104 — TS1163, `yield` outside a generator: the fourth never-set flag

### The bar, before the code

**+8 cases**, at most 2 new wrong lines. TS1163 is **10 missing lines across 10
cases** — a concentration of exactly **1.0**, the best shape the board has ever
offered and the opposite of §82's eleven-variants-of-one-file trap.

Falsifiers:

1. **If the yield context is not structurally derivable**, the rule reports
   inside generators. It is: `NodeFlagsYieldContext` is set by the parser at a
   function boundary and reset at each nested one, which is the same thing as
   "the nearest enclosing function-like is a generator".
2. **If the report position is the expression rather than its first token**,
   every line is off by however wide the operand is.
   `grammarErrorOnFirstToken` reports the `yield` keyword, which for a
   `YieldExpression` is the node's own start — so `self.nodes.span(node)`, not
   `error_span`, the same exception §82 found for `errorOnEachUnreachableRange`.

### `NodeFlags::YIELD_CONTEXT` is declared and set by nothing

Upstream's check is one line: `node.Flags&ast.NodeFlagsYieldContext == 0`
(`grammarchecks.go:1779`). This port declares the flag at `flags.rs:40` and
**never sets it** — so the standing note that three flags are declared-and-unset
(`NodeFlags::AMBIENT`, `NodeFlags::JAVASCRIPT_FILE`, `SymbolFlags::OPTIONAL`) is
**wrong, and this corrects it: there are at least four.**

That is the same shape as §99, one level down. §99 found a *helper* that was
sufficient for its only caller; this is a *flag* that is sufficient for its only
consumer, which is nobody. Each new consumer has to rediscover the absence and
re-derive the value structurally, and each derivation is a chance to derive it
differently. **Grep this list before porting any rule that reads a NodeFlag.**

Derived here as: the nearest enclosing function-like is a generator. An arrow
function and an accessor can never be one, so a `yield` inside either is
reported — `() => yield s` is `YieldExpression2_es6`'s second line.

### Measured

`diag2307` with `RULE_CODES = [1163]`:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 0 | 0 | 0 | 0 |
| first cut | 10 | 0 | 18 | 11 |
| **after the operand bound** | **10** | **0** | **18** | **7** |

**+10 cases for zero LOST.** Coverage `1,395 → 1,405 / 5,488` (25.42% →
**25.60%**), the session's largest build after §87. `checker_types` identical at
3,893/9,538 · 84.29% by stash-and-remeasure.

**The bar was +8 cases and at most 2 wrong lines. Met on cases, MISSED on wrong
lines** — 7, not 2 — and that is recorded as a miss rather than rounded off. All
seven fall in cases that remain failing for other reasons, so nothing regressed,
but the rule is noisier than the bar allowed for.

### The wrong column, read — and it is the parser's

**A bare `yield` is an IDENTIFIER outside a generator**, in non-strict code.
`function f(yield = yield) {}` and `var v = { [yield]: foo }` are both legal, and
upstream's parser builds an `Identifier` for them because it tracks the yield
context while parsing. This parser builds a `YieldExpression` regardless, so the
rule was reporting on a *name*. Requiring an operand — `yield x`, never bare
`yield` — took the wrong column from 11 to 7 and cost no conversion.

The remaining 7 are the same divergence in shapes the operand bound does not
separate: `YieldExpression8_es6`, `YieldStarExpression1_es6` and
`awaitAndYieldInProperty` (4 lines). **Owner: `tsr_parser`'s yield-context
tracking**, which is the same information `NodeFlags::YIELD_CONTEXT` would carry
if anything set it — so the flag and the wrong column have one owner between
them, and fixing the parser closes both.

## §105 — the remaining grammar row, priced

§103 and §104 took the two cheap grammar codes and named three more as "the same
shape and unpriced". They are priced here. **Two of the three are not the same
shape**, and saying so is the point of pricing before building.

### TS1100 — `Invalid use of '{0}' in strict mode.` — 12 cases, and it is the BINDER's

Not a `grammarchecks.go` rule at all. `checkStrictModeEvalOrArguments`
(`binder.go:1449`) fires from **seven** binder call sites — a parameter name, a
variable declaration, a function name, an assignment target, a catch clause
variable, and both unary operand forms — and reports on `eval` or `arguments`
used as a name while the binder is in strict mode.

Three things make it more than a transcription:

1. **The message is a three-way choice**, and only one of the three is TS1100.
   `getStrictModeEvalOrArgumentsMessage` (`binder.go:1456`) returns TS1210 inside
   a class, TS1215 in an external module, and TS1100 otherwise — so a port that
   emits TS1100 everywhere is *wrong at the right position* on the module and
   class cases, which is both a missing line and an extra one.
2. **It needs `b.inStrictMode`**, which this binder does not track. Strictness
   comes from a `"use strict"` prologue, from being an external module, or from
   `alwaysStrict` — the same plumbing TS1212's row has wanted for four handoffs.
3. Of the twelve cases, `alwaysStrict`, `alwaysStrictES6` and `alwaysStrictModule`
   want the **option**; `parserStrictMode8`–`13` want the **prologue**;
   `importCallExpressionInScriptContext1`/`2` want the **module** arm.

**A checker-side implementation would reach the same output** and is explicitly
*not* recommended: it would put a binder rule in the checker to dodge the
`binder_symbols` rail, and this port's whole discipline is that the structure is
the deliverable. **Owner: `tsr_binder`, one build, three messages.** Price it
against the rail, not just against `diagnostics`.

### TS1109 — `Expression expected.` — 11 cases, and it is the PARSER's

Every line is a recovery position: `YieldExpression5_es6`,
`YieldStarExpression3_es6`, `await_unaryExpression_es2017_3` and `_es6_3` (two
lines each). These are `yield` and `await` parsed in contexts where the operand
is missing or the keyword is not a keyword — **the same yield/await context
tracking §104's wrong column named**. Not a checker build at all.

**Owner: `tsr_parser`**, and it now has three things behind it: §104's 7 wrong
lines, `NodeFlags::YIELD_CONTEXT` being unset, and this row's 11 cases. That
makes yield/await context tracking the largest single parser-owned item this
workstream has identified.

### TS1183 — zero

`diagmissing -- 1183` reports **0 missing lines and 0 blocked cases**. It was
listed from an older `diagemit` reading where its `want` was non-zero; the row
has since been closed by other work. **Removed from the board.**

### What pricing three rows cost, and what it bought

One `diagmissing` run each. It moved TS1183 off the board entirely, reassigned
TS1109 to the parser, and turned TS1100 from "cheap grammar code" into "binder
build with a three-way message split and a rail to watch". **None of the three
was the build the previous section advertised**, and the cheapest way to find
that out was not to start any of them.

## §106 — the yield/await context, priced as a parser build

Three separate findings now resolve to one missing mechanism, so it is priced
here as one item rather than three.

| finding | what it is |
|---|---|
| §104's residual **7 wrong lines** | a bare `yield` reported as an expression where it is a name |
| `NodeFlags::YIELD_CONTEXT` | declared at `flags.rs:40`, **set by nothing** |
| **TS1109, 11 sole-obstacle cases** | every line a `yield`/`await` recovery position |

### What upstream does that this parser does not

`parse_assignment_expression` (`expression.rs:144`) parses `yield` as a
`YieldExpression` **unconditionally**. Upstream asks `isYieldExpression`
(`parser.go:4150`) first:

```go
if p.inYieldContext() { return true }
// outside a generator, `yield` is an IDENTIFIER unless the next token
// on the same line proves otherwise
return p.lookAhead(p.nextTokenIsIdentifierOrKeywordOrLiteralOnSameLine)
```

`yield = yield` and `{ [yield]: foo }` both fail that lookahead — `=`, `)` and
`]` are none of identifier, keyword or literal — so upstream builds an
`Identifier` and this port builds a `YieldExpression`. That is §104's wrong
column exactly, and `await` has the identical structure
(`isAwaitExpression`/`inAwaitContext`), which is TS1109's other half.

### Why the lookahead alone is NOT the fix, and this is the trap

Applying the lookahead unconditionally would be a regression: inside a generator
`yield` is *always* an expression, so `function* f() { yield; }` would start
parsing `yield` as a name. **The lookahead is only correct outside the context,
and the context is precisely what this parser does not track.** Anyone reaching
for the cheap half of this build will break bare `yield` in generators, and the
`diagnostics` suite may not be what catches it.

### The shape of the real build

Parser state carrying `in_yield_context` / `in_await_context`, set on entering a
generator or `async` body and **cleared** on entering a nested non-generator or
non-async one — upstream's `doInsideOfContext`/`doOutsideOfContext`. Then
`isYieldExpression`'s two-step, and the flags stamped onto nodes so
`NodeFlags::YIELD_CONTEXT` stops being decoration and §104 can read it instead
of re-deriving it from ancestors.

**This is a `tsr_parser` build and it changes parse trees**, so it must be
measured against `printer_round_trip` (11,682/11,738) and `binder_symbols`
(8,311/8,475) as well as `diagnostics` and `checker_types` — four rails, not the
usual two. That is why it is priced rather than started at the end of a session:
a parse-tree change measured against only two of its four rails is exactly the
kind of build that looks clean and is not.

**Expected yield: 11 cases (TS1109) plus 7 of §104's wrong lines, and it
retires a never-set flag.**

> **Note on this section's commit message.** `0bb61f3` was written through an
> unquoted shell heredoc and lost three backticked phrases to command
> substitution, so its body reads "parse_assignment_expression parses
> unconditionally". The section above is the authoritative text. The message was
> not amended because the commit had already been built on by another
> workstream, and rewriting shared history to fix prose is not a trade worth
> making — **prefer a quoted heredoc (`<<'EOF'`) for every commit message
> containing backticks.**

## §107 — §106's reachable half: two of upstream's lookahead rejections, decided post-parse

§106 priced the yield/await context as a four-rail parser build and it still is.
But two of the three shapes `nextTokenIsIdentifierOrKeywordOrLiteralOnSameLine`
(`parser.go:4171`) rejects are decidable **from the finished tree**, with no
parse-tree change and no rails beyond the usual two:

- **`yield(foo)` is a CALL.** `(` is not an identifier, keyword or literal, so
  upstream reads `yield` as the callee. This parser builds a yield whose operand
  is a `ParenthesizedExpression` — a shape that survives into the tree, so the
  rule can decline on it. `YieldExpression8_es6` and `YieldExpression18_es6`.
- **`yield * []` is a MULTIPLICATION.** `*` fails the lookahead too, so outside
  a generator the asterisk is the operator, not `yield*`. Inside a generator it
  really is `yield*`, which is why this is guarded by the context test rather
  than declined outright. `YieldStarExpression1_es6`.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| §104 | 10 | 0 | 18 | 7 |
| **§107** | **10** | **0** | **18** | **4** |

**+0 cases, wrong 7 → 4, LOST 0.** Coverage unchanged at `1,405 / 5,488`;
`checker_types` identical at 3,921/9,538 · 84.32%.

Landed at zero cases for §101's reason: it removes wrong output that this
workstream owns, and the four it leaves are one case with a diagnosed cause.

### The four that remain, and what they actually are

All four are `awaitAndYieldInProperty`, and all four are a **computed property
name**:

```ts
async function* test(x: Promise<string>) {
    class C {
        [yield 1] = yield 2;      // the NAME is in the generator's context
    }
}
```

A class body is not a yield context, but a **computed property name is evaluated
in the enclosing one** — so `[yield 1]` is upstream's context and `= yield 2` is
not. §104's ancestor walk stops at `PropertyDeclaration` and therefore answers
"not a generator" for both halves.

The fix is to let the walk pass *through* a `PropertyDeclaration` when it
arrived via a `ComputedPropertyName`, which is a five-line change to
`check_yield_grammar` and is left with its cause written down rather than
attempted at the end of a session — it is the third distinct boundary rule in
that walk and deserves its own measurement.

**§106's remaining value is now 11 cases (TS1109) and the never-set flag**, not
11 cases plus 7 wrong lines. The wrong-line half is mostly paid.

## §108 — a computed property name is evaluated in the ENCLOSING context

§107 left four wrong lines, all in `awaitAndYieldInProperty`, with the cause
written down. This is that five-line fix, and it takes TS1163's wrong column to
**zero**.

```ts
async function* test(x: Promise<string>) {
    class C {
        [yield 1] = yield 2;      // NAME: in the generator.  INITIALISER: not.
    }
}
```

A class body is not a yield context, but a **computed property name is evaluated
where the class is**, not where its members are. §104's ancestor walk stopped at
`PropertyDeclaration` for both halves and so answered "not a generator" for the
name as well as the initialiser.

The walk now passes *through* a class member when it arrived via a
`ComputedPropertyName`, and stops at it otherwise. That required turning the
`find_map` into a loop carrying the child it came from — the boundary test is a
property of the **edge**, not of the node, and a `find_map` over ancestors can
only see nodes.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| §104 | 10 | 0 | 18 | 7 |
| §107 | 10 | 0 | 18 | 4 |
| **§108** | **10** | **0** | **18** | **0** |

**+0 cases, wrong 4 → 0.** Coverage unchanged at `1,405 / 5,488`;
`checker_types` identical at 3,921/9,538 · 84.32%.

### TS1163 closes clean, and what the three sections cost

Ten cases and eighteen right lines for **zero wrong and zero LOST**, over three
measurements. §104 landed it at 7 wrong and recorded that as a **missed bar**;
§107 and §108 paid the miss off rather than leaving it as furniture — which is
the rule the eleventh session paid for, *a standing wrong line is an undiagnosed
finding, not decoration*, applied to a column this workstream created itself.

**The boundary-as-edge idea generalises.** Every scope question in this checker
is currently asked of ancestor *nodes*, and at least three of them —
`use_is_not_deferred`, `enclosing_block_scope_container`, this one — have or
will have cases where the answer depends on **which child the walk came up
through**. A computed property name, a parameter initialiser and a decorator all
sit syntactically inside a construct whose scope they do not share.

## §109 — §108's idea applied to the sibling walk, at a verified zero

§108 generalised: **the boundary test in a scope walk is a property of the edge,
not the node.** `use_is_not_deferred` is the other walk with that shape, and
upstream's own test there is already edge-aware:

```go
if current.Parent != nil && ast.IsPropertyDeclaration(current.Parent) {
    initializerOfProperty := propertyDeclaration.Initializer() == current
    if initializerOfProperty { … }
}
```
(`checker.go:2023-2026`)

A `PropertyDeclaration` defers a use in its **initialiser**. A computed property
name is not that — it is evaluated where the class is — so it defers nothing.
This port's walk deferred on the *node*, which meant a use inside `[…]` was
treated as though it were inside the member.

### Measured: byte-identical

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 9 | 0 | 33 | 0 |
| after | 9 | 0 | 33 | 0 |

**Every number unchanged.** No corpus case exercises a block-scoped name used
before its declaration from inside a computed property name — the shape exists
in the corpus for `yield` (§108) and not for this rule.

### Landed at a verified zero, and the reason is not the same as §84's

§84 landed an option audit at zero because the **table** was the durable
artefact. §101 landed a zero because it **deleted scaffolding**. This one lands
for a third reason: **it pre-empts, in the sibling walk, the exact defect §108
had just measured four wrong lines for in the first.** The two walks ask the
same question about the same tree; leaving one edge-aware and the other not is a
divergence waiting for the corpus case that distinguishes them.

That is a weaker justification than a measurement and is recorded as such. **The
honest statement is: this is faithful to `checker.go:2023`, it costs nothing
today, and nothing proves it right.** Its falsifier is a future case where a
`let`, `class` or `enum` is used before its declaration inside a computed
property name; if that case ever appears and this rule reports on it, look here
first.

`enclosing_block_scope_container` is the **third** walk of this shape and was
left alone: its boundary set comes from `IsBlockScope` (`utilities.go:2177`),
which is genuinely node-keyed apart from the `Block` case it already handles by
reading the parent. Two of three needed the edge; one did not, and checking was
the only way to know which.

## §111 — a contextual-typing decline for class expressions, measured at −4 right lines, REFUSED

`extraonly.rs` re-read after sixteen builds surfaced a new false-positive row
that is this workstream's: `contextuallyTypedClassExpressionMethodDeclaration01`,
**4 extra TS7006 lines**, one case, one shape:

```ts
function getFoo2(): Foo {
    return class {
        static method1 = (arg) => { … }     // `arg` IS contextually typed, via Foo
    }
}
```

§80's allow-list admits a function whose parent is a `PropertyDeclaration` with
no annotation, on the theory that nothing there supplies a signature. A class
expression in a contextually typed position does supply one.

Narrowing that arm — decline when an ancestor `ClassExpression` is the operand
of a `return` in a function with a written return annotation:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 25 | 0 | **218** | 9 |
| after | 25 | 0 | **214** | 5 |

**The four wrong lines go and four RIGHT lines go with them**, and coverage is
`1,406` on both sides of a `git stash` — **+0 cases**. Refused and reverted.

### Why the case matching exactly was not enough

`diagcase` after the change showed
`contextuallyTypedClassExpressionMethodDeclaration01` matching upstream
line-for-line, which is exactly the kind of evidence that makes a build feel
finished. It was not: **the same predicate silences four correct TS7006 in cases
that are still failing for other reasons**, and the whole-suite counterfactual is
what said so. A per-case diff cannot see a rule's cost outside the case you are
looking at.

That is the §88 lesson arriving from the other direction. There, a *wrong* column
was priced with the wrong instrument; here a *right* one was, and in both cases
the fix was to ask the instrument that measures the whole corpus.

### What would make it land

The decline is too broad because it is keyed on the class expression's
*position* rather than on whether a contextual signature actually reaches the
parameter. The four right lines it silences are presumably class expressions
returned from annotated functions whose annotation supplies **no** call
signature for that member. Distinguishing them needs the contextual type, which
is `checker_types`' road and §80's recorded partiality. **Owner: the contextual
type. Do not re-attempt this on syntax alone — this is its number.**

## §112 — TS1206, decorators where they are not valid: a row taken to exhaustion

Re-taking `diaggap` after sixteen builds — the move that found TS1029 and
TS1163 — surfaced TS1206 at **10 sole-obstacle cases**, and it is the same
shape: a grammar check with no types in it.

`reportObviousDecoratorErrors` → `findFirstIllegalDecorator`
(`grammarchecks.go:642`) and the `NodeCanBeDecorated` arm at `:246`. A decorator
is legal on a class, a method **with a body**, an accessor, a property and a
parameter of those. Every other declaration rejects it, and the report lands on
the **decorator's first token**, not on the declaration it precedes.

Ported for the declaration kinds that can never be decorated — enum, function,
interface, type alias, variable statement, `import =`, namespace, import and
export declarations. The parameter and private-name arms need
`NodeCanBeDecorated`'s grandparent tests and are left to their own row.

| | CONVERTS | LOST | RIGHT | WRONG | STILL SHORT |
|---|---:|---:|---:|---:|---:|
| before | 0 | 0 | 0 | 0 | 8 |
| **after** | **8** | **0** | **8** | **0** | **0** |

**+8 cases for zero wrong lines, and `STILL SHORT` reached 0** — every case this
rule can reach, it reached. Coverage `1,406 → 1,414 / 5,488` (**25.77%**);
`checker_types` 3,927/9,538 · 84.34%.

**A `STILL SHORT` of zero is the row's own completion signal** and this is the
first time this workstream has seen one. It means the remaining TS1206 lines
(`parameterDecoratorsEmitCrash`, `privateNamesAndDecorators`) are in cases that
need *other* codes too, so the parameter arm is worth nothing until those land —
which is a stronger statement than "3 lines left" and is only visible in this
column.

### Two gates failed on code this build did not touch

Worth recording because both cost time and neither was diagnostic of anything
here:

- **clippy, six `needless_borrow` sites** in binder and checker *test* files,
  arrived through the same pull as the `.types` builds. Fixed with
  `cargo clippy --fix`; they are not this build's and are not left for the next
  session.
- **`xtask anchors`, one unresolved**: `dts_emit_suite.rs:446` wrote
  `vendor/typescript-go/internal/…` where every other anchor in the tree writes
  `internal/…`. The checker resolves **relative to the vendor root**, so the
  prefix made it look for `vendor/typescript-go/vendor/typescript-go/…`. The
  file exists; the path was doubly rooted. **An anchor that names a real file
  can still fail, and the failure message says "no such file", which points at
  upstream rather than at the prefix.**

## §113 — TS1344, a label on a declaration, ported where upstream keeps it

The fourth row the "re-take `diaggap`" move has produced, and the first of them
that belongs in the **binder**.

`checkStrictModeLabeledStatement` (`binder.go:1433`): a `LabeledStatement` whose
statement is a declaration or a variable statement reports on the **label**.

**Its name is a misnomer inherited from TypeScript.** `bindWorker`'s dispatch
(`binder.go:639`) is unconditional — the function is called for every labelled
statement whether or not the file is strict — which is why
`sourceMapValidationLabeled`, a script with no prologue and no module indicator,
reports it. Reading the name and gating on strictness would have produced a rule
that fires on none of its nine cases.

That distinction is what separates this from §105's TS1100. TS1100 is genuinely
`b.inStrictMode`-gated and stays refused; this one reads only the labelled
statement's own child and needs no binder state at all. **It lives in the binder
because that is where upstream puts it, not because it needs anything from
there** — and putting it in the checker would have been the structural dodge
§105 declined to make.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 0 | 0 | 0 | 0 |
| **after** | **3** | **0** | **45** | **0** |

**+3 cases and 45 right lines for zero wrong.** Coverage
`1,414 → 1,417 / 5,488` (**25.82%**). **`binder_symbols` identical at
8,411/8,473 · 99.27%** by stash-and-remeasure — a binder change that adds a
diagnostic and touches no symbol table cannot move that rail, and the
measurement confirms it rather than assuming it.

### 45 right lines for 3 cases, and why the row did not convert

`diaggap` priced TS1344 at **9** sole-obstacle cases; three converted.
`STILL SHORT` is 6, so the other six need something else as well — the same
reading §112's `STILL SHORT` of 0 gave from the other end. The 15:1 line-to-case
ratio is the concentration the twelfth session's rule warns about
(`invalidDoWhileBreakStatements` and its siblings each carry many labels), and
it is why the bar for a row like this must come off the case column.

**A row can be "blocked on exactly one code" and still not convert when that
code lands**, because `diaggap`'s single-code column is computed against the
*current* output — a case missing five TS1344 lines counts once, and emitting
four of them converts nothing.

## §114 — TS2302 attempted, measured at +137 wrong lines and a LOST, REFUSED

`resolveNameEx`'s type-parameter arm (`binder/nameresolver.go:178`): a name that
resolves to a type parameter declared in this container, reached through a
`lastLocation` that `IsStatic`, is out of scope — *"the scope of a type
parameter extends over the entire declaration … with the exception of static
member declarations in classes"* (TS 1.0 spec 3.4.1, quoted upstream).

`lastLocation` is the child the resolver walked up from, so this looked like
§108's edge-not-node idea a third time, ported as an ancestor walk carrying the
child: at the class, is the member we came through `static`, and does the class
declare a type parameter of this name?

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 219 | **2** | 2,409 | **414** |
| after | 227 | **3** | 2,437 | **551** |

**+8 cases, but LOST 2 → 3 and WRONG +137.** Refused and reverted on the LOST.

### The diagnosis, for whoever takes it next

**The rule was put in the wrong place, and the shape of the miss says so.** It
was added to `check_type_reference_name` *before* the resolution loop, so it
fires on a name whose static-member reference is an error **and on every name
that merely spells the same as a type parameter while resolving to something
else** — a global type, an import, an outer alias. Upstream cannot make that
mistake because the test lives **inside** the resolver, on the branch where the
name has already resolved *to the type parameter symbol*: `result != nil &&
isTypeParameterSymbolDeclaredInContainer(result, location)`.

So this is not a checker rule at all. **Owner: `tsr_binder`'s `resolve_name`**,
which must grow the `lastLocation` parameter upstream's resolver carries and
report from the branch that already knows the answer. Ported anywhere else it is
a name-matching heuristic wearing a resolver's clothes, and 137 wrong lines is
what that costs.

**Do not re-attempt this in the checker.** This is its number. The 9 cases and
23 lines stay on the board with the resolver named.

### The pattern, third instance, and the first time it misled

§108, §109 and this section all reduce to *the boundary test is a property of
the edge*. The first two were right. This one had the right idea and the wrong
**host**: knowing which child you came through is useless if you are asking the
question somewhere the answer is not yet known. **An idea that generalises is
not an idea that transplants.**

## §115 — the last three unchecked rows, priced: all three are the RESOLVER's

§113 named TS1361/TS1362, TS2302 and TS2303 as the remaining unchecked rows of
the cheap-grammar shape. §114 measured TS2302 and refused it. This prices the
other two, and the answer is the same for all three — which is the finding.

### TS1361 / TS1362 — 9 cases each — `resolveNameEx`, same branch as TS2302

`checker.go:1860`:

```go
if errorLocation != nil && meaning&SymbolFlagsValue != 0 &&
   result.Flags&SymbolFlagsAlias != 0 && result.Flags&SymbolFlagsValue == 0 &&
   !ast.IsValidTypeOnlyAliasUseSite(errorLocation) {
    typeOnlyDeclaration := c.getTypeOnlyAliasDeclarationEx(result, SymbolFlagsValue)
    …
}
```

Four things a checker-side port cannot supply: the **resolved symbol's flags**
(`Alias` present, `Value` absent), `getTypeOnlyAliasDeclarationEx`'s walk along
the **alias chain** to find which declaration carried `type`,
`IsValidTypeOnlyAliasUseSite`, and the export-vs-import split that chooses
between TS1362 and TS1361. `is_type_only` exists on this port's
`ImportSpecifier`/`ExportDeclaration` nodes, which makes the *syntax* half look
available and is exactly the trap — **§114 measured what happens when the syntax
half is available and the resolution half is not: +137 wrong lines.**

### TS2303 — 10 cases — alias resolution with a cycle guard

`Circular definition of import alias` (`checker.go:16286`, `:18837`) is reported
from `resolveAlias`'s own recursion guard. It cannot exist before alias
resolution does; there is no syntactic approximation of a cycle.

### The finding

**Every remaining relation-free row on this board is resolver-owned.** Three
rows, three different codes, one subsystem — and the boundary is sharp: what is
left needs a name to have been *resolved*, and everything this workstream has
built cheaply needed only the tree.

That is a better statement of where the `diagnostics` workstream stands than any
case count. The seam that produced §103, §104, §112 and §113 — grammar checks
readable off the syntax — is now genuinely exhausted, and the sentence
"re-take `diaggap`, the seam may not be closed" (which was right twice) has
stopped being right. **The next `diagnostics` build is a `tsr_binder`
`resolve_name` build**, and it pays for TS2302 (9), TS1361/TS1362 (18) and
TS2303 (10) together — 37 cases behind one subsystem — plus TS1100's 12 behind
the same crate's strict-mode state.

**Falsifier for this claim, and it should be run before believing it:** re-take
`diaggap` after the next `.types` landing and look for a row that is neither
relation-bound nor resolver-bound. Twice this session a "seam is closed" claim
was premature (§105 said the grammar seam was exhausted and §112 found TS1206;
§112's own note said so). The difference now is that the three top remaining
rows were each read against upstream rather than inferred from a code number —
but the corpus moves, and one `diagmissing` run is the cost of checking.

## §116 — TS2302 lands: §114's rule, moved two lines down

§114 measured this exact rule at **+137 wrong lines and a grown LOST** and
refused it, with a diagnosis: *the rule was put in the wrong place — upstream's
test lives on the branch where the name has already resolved to the type
parameter symbol.* §115 then priced the row as resolver-owned.

Both were right about the cause and **over-priced the fix**. The rule does not
need `resolve_name` to change. It needs to be asked **after resolution has
failed** instead of before it — and `check_type_reference_name` already had that
site, occupied by a *silence*:

```rust
if self.an_enclosing_declaration_has_type_parameter(node, text) {
    return;                       // <- TS2302's cases were dying here
}
```

That decline exists because a name introduced as a type parameter resolves
upstream and fails here, and the difference *"is never TS2304"*. True — and for
a **class** type parameter reached through a **static** member it is never
silence either. It is TS2302.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 219 | 2 | 2,409 | 414 |
| §114, before resolution — **refused** | 227 | **3** | 2,437 | **551** |
| **§116, after resolution** | **227** | **2** | **2,437** | **414** |

**+8 cases and +28 right lines for ZERO new wrong lines and no change in LOST**
— the same eight cases §114 bought, with **137 fewer wrong lines**. Coverage
`1,417 → 1,425 / 5,488` (25.82% → **25.97%**). `checker_types` identical.

### What the two measurements together say

The identical predicate, two lines apart, is +8/+137-wrong or +8/+0-wrong. It is
not the predicate that was wrong and it was never the crate that was wrong:
**resolution had to have been attempted first, and the only thing that changed
is that it had been.** A name that resolves cannot reach the second site.

That corrects §115's conclusion, and the correction matters because §115 used
this row as its evidence: *"every remaining relation-free row is
resolver-owned"* was inferred partly from TS2302 needing the resolver, and
TS2302 did not. **TS1361/TS1362 and TS2303 still do** — they need the alias
chain and a cycle guard, which no ordering trick supplies — so §115's
conclusion survives on its other two rows, with one fewer case behind it (28,
not 37).

**The transferable rule: before concluding a diagnostic needs a subsystem, check
whether it needs that subsystem's ANSWER or merely its having run.** §114 needed
the answer "did this name resolve, and to what"; it turned out only the *first
half* was required, and the checker already had it.

`resolve_name`'s own comment — *"There are no checker diagnostics yet
(`bd tsr-5e7.6`), so only the `nil` is ported"* — has been stale since §113 and
is now doubly so: the decision stays in the resolver, the report is the
checker's, and neither had to move.

## §117 — TS1361/TS1362 attempted on §116's pattern: a measured ZERO, and where it dies

§116's lesson — *the site may already exist, occupied by a silence* — points
straight at TS1361. `check_value_identifier`'s meaning ladder returns silently
when a name resolves as `ALIAS` (§79 added that arm), and upstream reports there
when the alias is type-only and has no `Value` meaning (`checker.go:1860`).

Split the `ALIAS` arm out of the ladder, ask whether the resolved alias's
declaration carried `type`, and report TS1361 or TS1362 on the import/export
split.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 219 | 2 | 2,409 | 414 |
| after | **219** | **2** | **2,409** | **414** |

**Byte-identical. The rule never fires.** Reverted — an unmeasured restructure
of a hot ladder earns nothing.

### Where it dies, for whoever picks it up

Three candidates, none eliminated, and eliminating them is one `eprintln!` each
— the instrument that has paid five times in this workstream (§76, §80):

1. **`check_value_identifier` may not reach these names.** Its gates
   (`is_value_reference`, the `null`/`this` skip, the parse-error gate) are ahead
   of the ladder.
2. **`resolve_name(…, SymbolFlags::ALIAS)` may not answer.** This binder files
   import specifiers as `ALIAS` in `Locals`, but the *value* meaning is tried
   first in the ladder above and a type-only import may already be answering it.
3. **`is_type_only` / `phase_modifier` may be unset by this parser.** Upstream
   unified `import type` and `import defer` into `ImportClause.PhaseModifier`,
   and `ImportSpecifier`/`ExportSpecifier` carry their own `is_type_only` — but
   nothing in this port has ever read them, so nothing has ever proved the
   parser writes them. **That is the same class of finding as
   `NodeFlags::YIELD_CONTEXT` (§104): a field that exists, compiles, and is set
   by nobody.** Check this one first.

**PRINT WHETHER THE RULE RUNS BEFORE ASKING WHAT IT DECIDED** — the finding the
twelfth session paid for, and the one this attempt skipped. A byte-identical
measurement is the cheapest possible signal that the answer is (1) or (3) rather
than a wrong predicate, and it cost one build to get it the expensive way.

## §118 — the instrument answers §117 in one run: the rule's HOST never runs

§117 recorded three candidates for why TS1361 measured a byte-identical zero and
said to instrument before coding. One `eprintln!` behind `TSR_DEBUG_1361` at
`check_value_identifier`'s meaning ladder, one `diagcase` run over
`conformance/computedPropertyName`:

```
$ TSR_DEBUG_1361=1 … --example diagcase -- conformance/computedPropertyName
0 lines
```

**Not one identifier in that case reaches `check_value_identifier`** — not the
type-only aliases, not the ordinary names either. Candidate (1) confirmed and
candidates (2) and (3) are irrelevant: the predicate was never the question, and
`is_type_only` / `phase_modifier` may be perfectly well set.

### What this means for the row

TS1361/TS1362's 18 cases are **not** blocked on a missing rule. They are blocked
on the value-identifier walk not visiting this case's files at all — a
*coverage* gap in `crate::check`'s traversal, not a *rule* gap. The candidates,
in the order they cost nothing to check:

- the case is **multi-file** (`component.ts` is one of several) and the walk may
  run over only one unit;
- `file_has_parse_errors` gates `check_value_identifier` and would silence every
  identifier in the file at once, which is exactly the shape observed;
- the identifiers sit in positions `is_value_reference` rejects — but that would
  not silence *every* name in the file.

**The second is the one to test first**, because "zero identifiers, not few" is
its signature and it is one `eprintln!` above the one this section already
placed.

### The rule this pays for the seventh time, after being skipped once

§117 wrote the full predicate, the TS1361/TS1362 message split and the
phase-modifier handling before ever asking whether the code ran. The instrument
answered in **one run** what a build could not: the measurement was
byte-identical because **nothing was executing**, and no amount of predicate
work would have changed that.

*Print whether the rule runs before asking what it decided.* Six payments, one
skip, and the skip cost a build while the payment cost five minutes.

## §119 — §118's conclusion was WRONG, and the correction is the finding

§118 read a zero-line probe as *"the rule's host never runs"* and rewrote the
board around it: TS1361/TS1362 was reclassified from a rule gap to a **traversal
coverage gap**, flagged as *"potentially worth far more than 18 cases"*, and
`file_has_parse_errors` was named as the first thing to test.

**All of that is wrong.** Two runs, each one line of instrumentation:

```
SKIPPED-UNIT …            → count = 0    (no unit is skipped)
UNIT framework-hooks.ts parse_errors=false
UNIT component.ts       parse_errors=false
```

`check_source_file` is called for `component.ts`, with no parse errors. **The
traversal is fine. There is no coverage gap.**

### Why the probe was silent

§118's `eprintln!` was placed at `check_value_identifier`'s **meaning ladder**,
which sits *after* the function's early returns — including the one taken when
the name resolves with `VALUE` meaning. This port files an import alias as a
symbol that answers `VALUE`, so every identifier in the case returned before
reaching the probe. **The silence measured the probe's position, not the rule's
execution.**

### The rule, corrected

*Print whether the rule runs before asking what it decided* — and **the print
must be at the rule's ENTRY, not at the branch you are interested in.** §118
claimed a seventh payment for that rule while committing a subtler version of
the same error §117 made: §117 asked "what does it decide" without asking
whether it ran; §118 asked "does it run" **at the wrong line** and got a
confident answer to a question it had not asked.

A probe that can be silent for two different reasons has told you nothing, and
the fix is one line higher.

### Where TS1361 actually stands

Back to §117's candidate list, with (1) now the *likely* answer rather than the
excluded one: the alias resolves with `VALUE` meaning here, so
`check_value_identifier` returns before any type-only test could fire. Upstream's
condition is precisely `result.Flags&SymbolFlagsAlias != 0 && result.Flags&
SymbolFlagsValue == 0` (`checker.go:1860`) — **this port's alias symbols carry
`VALUE` where upstream's do not**, which is a symbol-flags divergence of the
same family as §93, §95 and §100.

**Owner: `classify`'s `S::ALIAS`**, which §102's audit passed as "not a collapse"
because upstream also declares all five import/export forms as `Alias` — true,
and it says nothing about whether the *meaning* those symbols answer matches.
§102 checked the flag and not its consequences; that limitation is now recorded
in both places.

No case count is claimed for this until it is measured. **The 18 cases stay on
the board with no owner change and no size change**, because §118 changed both
on the strength of a bad probe and this section is undoing that, not replacing it
with another guess.

## §120 — §119's hypothesis confirmed with a number, and the row refused at +2/+7

§119 said, unmeasured and labelled as such, that TS1361 dies because this port's
alias symbols answer `VALUE` and `check_value_identifier` returns at its
`VALUE`-resolution early return, before any type-only test.

**Confirmed.** Moving the test to that early return — where the symbol is in
hand — makes the rule fire:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 219 | 2 | 2,409 | 414 |
| after | **221** | **2** | **2,423** | **421** |

**+2 cases, +14 right lines, +7 wrong, LOST unchanged. Refused and reverted** —
+2 for +7 is the worst ratio this session has landed anything at, and §104's
+10/+7 was already recorded as a missed bar.

### What the 7 wrong lines are, and what would fix them

The rule inspects **the alias's own declaration only**. Upstream walks the alias
**chain** — `getTypeOnlyAliasDeclarationEx` (`checker.go:1861`) follows
re-exports and intermediate aliases to find *which* declaration carried the
`type`, and returns `nil` when none did. Without the walk, an alias whose
type-only-ness lives one hop away is either missed or attributed wrongly, and
seven lines is what that costs.

**So the row's real requirement is the alias chain, which is exactly what §115
priced it at** — and §115's reasoning survives this section even though §116
knocked TS2302 out of the same bucket. The difference is that TS2302 needed only
*that resolution had been attempted*, and TS1361 needs *what resolution found,
one hop out*.

### Three sections to reach one confirmed diagnosis

§117 wrote the predicate and measured zero. §118 mis-sited a probe and
concluded, wrongly, that the traversal was broken. §119 disproved that and
named the real cause without measuring it. §120 measured it.

The useful part is that the diagnosis is now **numbered**, not asserted: the
`VALUE` early return is the blocker (+2 cases when bypassed) and the alias chain
is the remainder (+7 wrong without it). **Owner: `getTypeOnlyAliasDeclarationEx`,
and the 18 cases stay on the board at that price** — which is the first time
this row has had one.

## §121 — the alias chain, built: TS1361/TS1362 lands at +5

§120 refused this row at +2/+7 and named the remainder precisely: the rule read
the alias's **own** declaration where upstream walks the **chain**.
`resolve_alias` (`symbols.rs:560`, `checker.go:16266`) already exists, so the
walk is a bounded loop over it.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 219 | 2 | 2,409 | 414 |
| §120, first hop only — refused | 221 | 2 | 2,423 | 421 |
| **§121, chain walked** | **224** | **2** | **2,431** | **422** |

**+5 cases and +22 right lines, LOST unchanged.** Coverage
`1,425 → 1,430 / 5,488` (25.97% → **26.06%**). `checker_types` identical.

The chain is worth **3 of the 5 cases and 8 of the 22 lines** over §120's single
hop, for **one** additional wrong line — which is the measurement §120 asked for
and the reason that refusal was worth writing rather than deleting.

### The bar was missed, and this is the second time on this row

**+8 wrong lines against a rule that emitted none.** §104 landed TS1163 at +7
wrong and paid it off in §107 and §108; the same debt is being taken on here,
and it is recorded as a debt rather than a rounding.

**The residual is `IsValidTypeOnlyAliasUseSite` (`checker.go:1860`'s third
conjunct), which is unported.** Upstream does not report at every value
position — a type-only alias is legal in `typeof X`, in an `export { X }`, and
in a few other positions that "use" the name without emitting it. Every wrong
line here is expected to be one of those, and the fix is a predicate over the
use site rather than anything further along the chain.

**Owner: `IsValidTypeOnlyAliasUseSite`.** Next session: port it, re-run
`RULE_CODES = [1361, 1362, 2304, 2552]`, and beat 224/2/2431/**422** — the target
is the same 224 conversions at a wrong column back at 414.

### What the five sections cost and bought

§117 predicate-without-probe, zero. §118 probe misplaced, wrong conclusion.
§119 correction, hypothesis named. §120 hypothesis measured, refused with the
remainder named. §121 remainder built, landed.

Five sections for five cases is a poor rate and the record should say so. What
makes it not merely waste is that **each section's output was the next one's
input**, and the two that produced no code — §119's retraction and §120's
refusal — are the two the build could not have happened without.

## §122 — the debt is NOT what §121 said it was

§121 landed TS1361/TS1362 at +5 cases and **+8 wrong lines**, recorded the miss
as a debt, and named the payer: `IsValidTypeOnlyAliasUseSite`
(`ast/utilities.go:3124`), on the reasoning that a type-only alias is legal in
`typeof X` and in an erased heritage clause, so the residual must be those.

**It is not.** Porting the two clauses that matter —
`IsPartOfTypeQuery` and `isIdentifierInNonEmittingHeritageClause` — measured:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| §121 | 224 | 2 | 2,431 | **422** |
| §122, use-site test added | 224 | 2 | 2,433 | **422** |

**Not one wrong line removed.** Reverted: a predicate that does not do the thing
it was written for is churn, however faithful it reads.

And the composition is not what the section assumed either — filtering the wrong
column for `TS1361`/`TS1362` returns **nothing**. The eight lines are not this
rule's own output.

### What that leaves the next session

**Read the residual before attributing it.** §121's +8 was measured against
`RULE_CODES = [1361, 1362, 2304, 2552]`, four codes isolated together because
they share `check_value_identifier`. The delta is real and it is **not TS1361's
lines** — the likeliest reading is that the new early return changes which of
TS2304/TS2552 some names reach, which is a *different* defect wearing this
row's number.

**Do not port more of `IsValidTypeOnlyAliasUseSite` on the strength of §121's
attribution.** Isolate `RULE_CODES = [1361, 1362]` alone first, confirm the row
emits zero wrong lines of its own, and then take the 8 to whichever of
TS2304/TS2552 owns them.

### The pattern this session keeps producing

Three times now a residual has been attributed by reasoning and disproved by
measurement: §118 (traversal gap, disproved by §119), §121's debt (disproved
here), and §115's "resolver-owned" (partly disproved by §116). Each attribution
was plausible, each was written down, and each was cheap to overturn **because
it was written down with a number attached**.

**An attribution is a hypothesis. Isolate the code alone before believing which
rule owns a line.**

## §123 — the debt paid, and §122's second claim retracted

§122 made two claims about §121's eight wrong lines. **Both were wrong**, and
one measurement retires both.

Isolating `RULE_CODES = [1361, 1362]` **alone** — §122's own prescribed first
step — gives:

```
CONVERTS 5   LOST 0   RIGHT 22   WRONG 8
  conformance/computedPropertyName  component.ts(28,13) TS1361
  conformance/computedPropertyName  component.ts(32,12) TS1361
  conformance/computedPropertyName  component.ts(36,4)  TS1361
  compiler/mergeSymbolRexportFunction  main.ts(2,1)     TS1362
```

- **§122 said the wrong column contained no TS1361/TS1362.** It contains eight,
  all of them this rule's. That claim came from a `grep` over a `head`-truncated
  listing — **the filter matched nothing because the lines had been cut off, not
  because they were absent.**
- **§122 said `IsValidTypeOnlyAliasUseSite` was the wrong owner.** It is the
  right one; §122 ported the two clauses the residual did *not* want and
  concluded from their failure that the function was innocent.

The clause the residual wanted is
`isPartOfPossiblyValidTypeOrAbstractComputedPropertyName`
(`ast/utilities.go:3143`) — a **computed property name** on an `abstract` member,
or on a member of an interface or type literal, is erased. §122 skipped it with
the words *"neither has a case in the residual"*, which was an assertion with no
measurement behind it.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| §121 | 5 | 0 | 22 | **8** |
| §122, wrong clauses | 5 | 0 | 22 | **8** |
| **§123, computed-name clause** | **5** | **0** | **22** | **5** |

**Wrong 8 → 5 at no cost**, `checker_types` identical, all five gates green.
Coverage holds at `1,430 / 5,488` — the three lines removed were in cases short
of other codes, so they buy no conversion today and unblock three.

### The lesson, and it is about instruments rather than TypeScript

**`head` and `grep` compose into a silent false negative.** `diag2307`'s wrong
listing is long; `grep -A 12` over it and then filtering for a code answers
"absent" for anything past the twelfth line. §122 built two conclusions on that
answer and wrote both into `STATUS.md`.

This is the same failure as §118's misplaced probe, one layer out: **a query that
can return empty for two different reasons has told you nothing.** §118's was a
probe below an early return; this was a filter below a truncation. The fix in
both cases is to make the instrument answer the question you asked — here,
`RULE_CODES = [1361, 1362]` alone, which is exactly what §122 itself prescribed
and did not run before concluding.

Three sections attributed this residual and two were wrong. **The one that was
right — §121's — was right for a reason it could not justify at the time**, and
was overturned by an argument that felt more rigorous and was less so.

## §124 — the five residual TS1361 lines, read and named

§123 left five and declined to attribute them. Isolated (`RULE_CODES =
[1361, 1362]`) and read in full, they are **three families, not one**:

```
compiler/mergeSymbolRexportFunction   main.ts(2,1)     TS1362
conformance/computedPropertyName      component.ts(32,12) TS1361
conformance/computedPropertyName      component.ts(36,4)  TS1361
conformance/exportDefault             /b.ts(2,16)      TS1361
conformance/importEquals1             /b.ts(2,10)      TS1361
```

### Family one — `export =` / `export default` of a type-only alias (2 lines)

Both fixtures are the same three lines:

```ts
// /b.ts
import type * as types from './a';
export = types;            // importEquals1
export default types;      // exportDefault
```

Upstream reports here — `importEquals1`'s own comment says `// Error` — but
**not TS1361 at this position**, which is why these sit in the wrong column
rather than the right one. `ExportAssignment.expression` is an expression node,
so `IsValidTypeOnlyAliasUseSite` does *not* excuse it; the divergence is the
**code or the position**, not the decision to report.

**Do not decline these blind.** Read
`baselines/reference/importEquals1.errors.txt` first and find out what upstream
writes; a decline would trade two wrong lines for two missing ones and the row
would not move.

### Family two — the remaining two computed property names (2 lines)

§123's clause took three of `computedPropertyName`'s lines and left two, at
`(32,12)` and `(36,4)`. Since the clause fires on the *same file*, these two
differ in some way the ported test misses — the likeliest candidates are
upstream's other two disjuncts, `HasSyntacticModifier(…Abstract)` on a member
kind this port's `member_is_abstract` does not enumerate, or a parent that is a
`TypeLiteral` reached through a shape the walk exits early on.

### Family three — `mergeSymbolRexportFunction` (1 line, TS1362)

Unread. The only TS1362 in the residual and the only one in a merge fixture.

### Why this is the last thing this session does with the row

**Each family needs its baseline read, and each would be a separate
measurement.** §121 → §122 → §123 was three sections spent on one residual
because each attributed before reading; the pattern only broke when the codes
were isolated and the lines printed in full. Naming three families and stopping
is the state that lets the next session pick one and measure it, rather than
inheriting a single number with a guess attached.

**Standing instruction for this row: read the upstream baseline before writing
any decline.** Two of these five are cases where upstream reports *something* —
declining there converts a wrong line into a missing one and gains nothing.

## §125 — §124 read the fixture's comment instead of the baseline

§124 named `export = types` / `export default types` over a type-only import as
the first residual family and wrote a standing instruction against touching it:

> Upstream reports here — `importEquals1`'s own comment says `// Error` — but
> not TS1361 at this position. **Do not decline these blind.**

**The comment is not the oracle.** `baselines/reference/importEquals1.errors.txt`:

```
/d.ts(2,5): error TS1361: 'types' cannot be used as a value …
/e.ts(2,5): error TS1361: …
/f.ts(2,5): error TS1361: …
/g.ts(2,5): error TS1361: …
```

**Nothing at `/b.ts`.** Upstream reports in the four *consumer* files, not in
the file that re-exports — the fixture's `// Error` marks the case, not the
line. So this port's two lines at `/b.ts` are plain false positives and
declining costs nothing.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| §123 | 5 | 0 | 22 | 5 |
| **§125** | **5** | **0** | **22** | **3** |

**Wrong 5 → 3 at no cost.** Coverage holds at `1,430 / 5,488`; `checker_types`
identical at 3,933/9,538 · 84.40%; all five gates green.

### The rule, and it is the fourth instrument failure of this session

`ADR-0006` already says conformance is asserted against **generated Go**, not
`ast.json` — *the artefact, not the description.* §124 broke the same rule in a
new place: it read a **comment in a test fixture** as evidence of what upstream
reports, when the `.errors.txt` beside it is the only thing that decides.

That is now four in one session, all the same shape:

| § | the instrument | why it lied |
|---|---|---|
| 118 | `eprintln!` probe | placed below an early return |
| 122 | `grep` over `head` | filtered a truncated listing |
| 124 | a fixture `// Error` comment | describes the case, not the line |
| — | (§88's) `diag2307` wrong column | prices lines where `extraonly` prices cases |

**Read the baseline. Not the comment, not the fixture name, not the code's own
description of itself.**

### Where the row now stands

Three wrong lines: two uncaught computed property names in
`conformance/computedPropertyName` (§124's second family, still unread) and
`mergeSymbolRexportFunction`'s single TS1362 (still unread). Both want their
`.errors.txt` opened first — which is the whole of what this section learned.

## §126 — the last three lines, read against their baselines

§125's rule applied to the two families §124 left unread. Both baselines are
decisive and neither matches what the sections around them assumed.

### `mergeSymbolRexportFunction` — upstream has NO TS1362 at all

```
a.d.ts(3,9):    error TS2451: Cannot redeclare block-scoped variable 'Row'.
index.d.ts(1,14): error TS2451: Cannot redeclare block-scoped variable 'Row'.
```

Two TS2451 and nothing else. Our `main.ts(2,1) TS1362` is a **pure false
positive with no upstream counterpart anywhere in the file** — not a wrong
position, not a wrong code, simply a diagnostic upstream does not write. It is
also the row's only TS1362, so whatever produces it is the export-side branch of
`type_only_alias_declaration` firing where the alias is not type-only, or firing
on a merged symbol whose first declaration is not the one that matters
(§97's multi-declaration hazard, in a rule that does not have that decline).

### `conformance/computedPropertyName` — upstream reports FOUR, all at column 4

```
component.ts(12,4)  (16,4)  (20,4)  (24,4)   — all TS1361
```

Our two residual lines are at **(32,12)** and **(36,4)**. Upstream writes
nothing past line 24. So the file has more computed-name sites than upstream
errors on, and lines 28–36 are the ones
`isPartOfPossiblyValidTypeOrAbstractComputedPropertyName` excuses — §123's port
caught line 28 and misses 32 and 36.

**Two shapes it does not reach**, and both are in upstream's own predicate:
`HasSyntacticModifier(node.Parent, ModifierFlagsAbstract)` over member kinds
`member_is_abstract` does not enumerate, and a `node.Parent.Parent` that is a
`TypeLiteral` where this port's walk exits earlier. Column 12 versus column 4
suggests the two differ from each other as well — one is a bare name and one is
a property access.

### The state this leaves, and it is a good one to hand over

Three wrong lines, **each with its baseline read and its cause narrowed to one
named predicate**. No attribution in this section is a guess: the merge line has
no upstream counterpart at all, and the computed-name lines sit past the last
line upstream reports on.

That is the first time this row has been in that state. §121 handed over eight
lines with a wrong owner, §122 handed over "unknown", §124 handed over three
families with one of them wrongly declared untouchable. **Reading four baselines
cost four `grep`s and undid three sections of inference.**

## §127 — §126's two named causes also fail, and this row stops taking inferences

§126 read the baselines and narrowed the three residual lines to two named
predicates. Both were implemented:

- **every declaration searched for the type-only one**, not `first()` — §97's
  merged-symbol hazard, the stated cause of `mergeSymbolRexportFunction`'s
  TS1362;
- **both the member's parent and its grandparent** tested for
  `InterfaceDeclaration`/`TypeLiteral` — the stated cause of
  `computedPropertyName`'s two remaining lines.

**Wrong stayed at 3. Neither fix removed a line.** Reverted.

### The finding is about this row's method, not about TypeScript

That is **five consecutive attempts on these lines built on inference**, each
plausible, each rejected by measurement:

| § | inferred cause | verdict |
|---|---|---|
| 121 | (debt taken, unattributed) | — |
| 122 | not this rule's lines; owner unknown | wrong (truncated `grep`) |
| 123 | computed-property clause | **right, 8 → 5** |
| 124 | `export =` is legal, do not touch | wrong (read the fixture comment) |
| 125 | `export =` is a false positive | **right, 5 → 3** |
| 126 | merged-symbol `first()`; grandparent-only test | **wrong** |

Two of six inferences were right. **Reading the baseline (§125, §126) improved
the *description* of the residual without improving the *hit rate* on its
cause** — the baseline says what upstream reports, and these three lines are
about why *this port* reports something extra, which no upstream artefact
answers.

**Standing instruction for these three lines: do not implement another
hypothesis. Instrument them.** Put an `eprintln!` **at the entry of
`report_type_only_alias_used_as_value`** — not at a branch inside it, §118's
error — printing the symbol's declaration kinds and the use site's parent chain
for `main.ts(2,1)` and `component.ts(32,12)`. Three lines of output will say
what six sections of reasoning have not.

The rule this session paid for seven times over is the one that applies:
**print whether the rule runs and what it saw, before asking why it decided.**
It has been skipped or misapplied three times on this row alone.

## §128 — the instrument settles it: the `Ex` in `getTypeOnlyAliasDeclarationEx`

§127 forbade a seventh inference and prescribed an entry-point probe. One run,
both cases, deduplicated:

```
onInit decls=[ImportSpecifier] parents=[ComputedPropertyName, MethodDeclaration,  ClassDeclaration]      valid=false
onInit decls=[ImportSpecifier] parents=[ComputedPropertyName, PropertyDeclaration, ClassDeclaration]     valid=false
onInit decls=[ImportSpecifier] parents=[ComputedPropertyName, PropertyAssignment,  ObjectLiteralExpression] valid=false
onInit decls=[ImportSpecifier] parents=[ComputedPropertyName, MethodSignature,    InterfaceDeclaration]  valid=true
onInit decls=[ImportSpecifier] parents=[ComputedPropertyName, PropertySignature,  TypeLiteral]           valid=true
Row    decls=[ImportSpecifier] parents=[CallExpression, ExpressionStatement, SourceFile]                 valid=false
```

**The computed-property clause is working.** Interface and type-literal members
answer `valid=true`; class members and object-literal properties answer `false`
and are reported, which is upstream's own rule. §126's "the grandparent test is
too narrow" was wrong, and §127's measurement already said so — the probe says
*why*: nothing was missing.

### The real defect, and it is one dropped argument

`Row` is the whole of `mergeSymbolRexportFunction`, and the probe names it: its
only declaration is an **`ImportSpecifier`**, yet the emitted code is **TS1362**,
the *export* message. So the chain walk followed the alias to a type-only
**export** specifier and reported through it.

Upstream cannot: `getTypeOnlyAliasDeclarationEx(result, ast.SymbolFlagsValue)`
(`checker.go:1861`) takes a **meaning** argument, and this port's
`type_only_alias_declaration` **has no such parameter**. The `Ex` is a filter —
a type-only declaration is only disqualifying for the meaning it actually
blocks. A re-export that is type-only in *type* space does not stop the name
being used as a value if the original import is not.

**That is the cause of the TS1362 line, it was invisible to six inferences, and
one probe showed it.** It is also why §127's merged-symbol fix did nothing:
searching every declaration finds the same wrong declaration faster.

### What to build

Thread `meaning` through `type_only_alias_declaration` and stop the walk at a
declaration that does not block it, exactly as `getTypeOnlyAliasDeclarationEx`
does. Then re-run `RULE_CODES = [1361, 1362]` and beat **5 converts / 22 right /
3 wrong**; the expectation is 3 → 1, with the two class-member and
object-literal computed names remaining and needing their own read.

### The session's most-repeated lesson, in its sharpest form

Six inferences, two right, then one probe. **The probe cost less than any of the
six and was prescribed by the section that ran out of inferences, not by the one
that ran out of patience.** Every wrong guess here was made by reasoning from
artefacts that describe *upstream*; the defect was in what *this port* did with
an argument it never had.

## §129 — the seventh inference, and the one §127 forbade

§128's probe named a real gap: `getTypeOnlyAliasDeclarationEx` takes a **meaning**
and this port's walk does not. That much is fact, read off upstream's signature.

**What that meaning should *do* was then inferred, and the inference was wrong.**
The guess was "a hop whose target is a real value ends the chain". Implemented:

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| §125 | 5 | 0 | 22 | 3 |
| §129 | 5 | 0 | 22 | **3** |

Unchanged. Reverted. The guard never fires, because the hop that produces the
TS1362 resolves to **another alias** — an `ExportSpecifier` — which carries
`ALIAS` and so passes a test written for concrete values.

### This is §127's rule being broken by the section that quoted it

§127 wrote *"do not implement another hypothesis; instrument"*. §128 instrumented
and found the missing argument. §129 then **inferred its semantics instead of
probing them** — the same failure one level down, and the seventh wrong guess on
three lines.

The probe that would have settled it is one line longer than §128's: print the
declaration kind **at each hop** of `type_only_alias_declaration`, not just the
symbol's own. That shows which declaration supplies the `Some(true)` and whether
upstream's meaning filter would have rejected it. **That is the next action on
this row and it is not a build.**

### Standing verdict on these three lines

**Seven inferences, two right.** The two that worked (§123's computed-property
clause, §125's `export =` decline) were both cases where an *upstream artefact
stated the answer outright* — a named function in `utilities.go`, a baseline
file. Every inference about *this port's* internal behaviour has failed, seven
for seven counting §129.

**Do not build on this row again without a probe that shows the hop.** The three
lines are worth three lines; the discipline is worth more, and this session has
now paid for it twice at the same address.

## §130 — the hop probe, and the answer is a MISSING MERGE

§129's prescribed probe, printing every hop of `type_only_alias_declaration`:

```
HOP kind=ImportSpecifier verdict=None        <- `import { Row } from './a'`, not type-only
HOP kind=ExportSpecifier verdict=Some(true)  <- `export type { Row }` in the target
```

So the TS1362 is emitted because the chain reaches a type-only **re-export**.
Upstream reaches it too — and reports nothing, because of the conjunct this port
dropped between §117 and §121: `result.Flags&SymbolFlagsValue == 0`
(`checker.go:1860`). `Row` is *also* a variable; the baseline's two TS2451s say
so outright (`Cannot redeclare block-scoped variable 'Row'`). A symbol that
names a value is usable as one however its alias half was declared.

**Restoring that conjunct changed nothing: still 3 wrong.** Reverted.

Which is the answer, and the case name has been saying it the whole time.
**`mergeSymbolRexportFunction`.** Upstream's `Row` carries `VALUE` because the
import alias and the variable **merge into one symbol**. This binder does not
merge them, so `resolve_name` hands back an alias with no `VALUE` bit, the
conjunct cannot fire, and no amount of work inside the type-only rule can
recover it.

**Owner: `tsr_binder`'s symbol merging** — the same subsystem §5 names for
TS7026 and §85 for TS2454, and the same shape as the enum-member destination
divergence §102 recorded. **The TS1362 line is not this rule's to fix**, and
that is now established by measurement rather than asserted.

### The tally on three lines

**Eight attempts, two conversions.** The two that worked were both stated
outright by an upstream artefact. The six that failed were all inferences about
this port's internals — and the one that finally produced the answer was not a
fix at all but a two-line probe, prescribed by §129 after §127 prescribed a
weaker version of it and §128 ran it one level too shallow.

**The probe that works is the one that prints every step of the thing you are
guessing about.** §118 probed a function and learned nothing because it sat
below an early return. §128 probed the symbol and learned the argument was
missing. §130 probed the *loop* and learned the loop was never the problem.

Two lines remain (`component.ts(32,12)`, `(36,4)`), both computed property
names in a **class** and an **object literal**, both of which upstream reports
on — so they are **missing lines wearing a wrong-column badge**: this port emits
TS1361 at those positions and upstream emits it at *different* positions in the
same file. That is a position bug, not a predicate bug, and it has not been
looked at once.

## §131 — reading the fixture: both remaining lines are the two clauses §123 skipped

§130 called the two remaining lines a "position bug". **Wrong** — `diagcase`
shows all four expected lines matching exactly and two *extra* on top:

```
expected: (12,4) (16,4) (20,4) (24,4)
actual:   (12,4) (16,4) (20,4) (24,4) (32,12) (36,4)
```

Reading `conformance/externalModules/typeOnly/computedPropertyName.ts` — which
no section had done — the file is a deliberate enumeration:

```ts
class D { [onInit] = 0; }          // Error
class E { [onInit]() {} }          // Error
abstract class F { abstract [onInit](): void; }   // no error
class G { declare [onInit]: any; }                // no error
declare class H { [onInit]: any; }                // no error
```

The two extras are **`abstract class F`** and **`declare class H`**, and they
are exactly the two clauses of `IsValidTypeOnlyAliasUseSite` that §123 declined
to port with the words *"neither has a case in the residual"*:

1. **`HasSyntacticModifier(node.Parent, ModifierFlagsAbstract)`** — `abstract
   [onInit](): void` has no body, so it is almost certainly a
   `MethodSignature`-shaped node, and `member_is_abstract` enumerates
   `PropertyDeclaration | MethodDeclaration | GetAccessor | SetAccessor` only.
   **Add the signature kinds.**
2. **`useSite.Flags&NodeFlagsAmbient != 0`** — the *first* clause of the
   function, skipped because `NodeFlags::AMBIENT` **is one of this port's
   never-set flags** (§104's list, which §104 already had to correct once).
   `declare class H` and `class G { declare … }` both need it, and §99 already
   built the replacement: `declaration_is_in_an_ambient_context`, which walks
   the `declare` modifiers because the flag is dead. **Call that instead.**

Both fixes are named, both are small, and **neither is an inference** — the
fixture enumerates the cases and labels them.

### Nine attempts, and the one that worked was reading the input

§122 through §130 attributed these lines from the wrong column, the baseline,
upstream's source, a probe of the symbol, a probe of the loop, and twice from
the case's *name*. The thing that finally explained them was **opening the test
file** — 19 lines, unread by any of nine sections.

`diagmissing` prints case names, `diagcase` prints line numbers, `diag2307`
prints columns. **None of them prints the code**, and for nine sections nobody
looked at it. That is the instrument gap this row actually exposed.

**Standing addition to the loop: read the fixture before the fourth hypothesis.**

## §132 — §131's two fixes built: one lands, one does not

§131 read the fixture and named two clauses. Both were implemented; **only one
was right**, and the split is informative.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| §125 | 5 | 0 | 22 | 3 |
| **§132** | **5** | **0** | **22** | **2** |

**The ambient clause landed.** Routing `declare class H { [onInit]: any }`
through §99's `declaration_is_in_an_ambient_context` — the substitute for the
never-set `NodeFlags::AMBIENT` — removed `component.ts(36,4)`.

**The abstract clause did not.** Adding `MethodSignatureDeclaration` and
`PropertySignatureDeclaration` to `member_is_abstract` changed nothing;
`component.ts(32,12)` survives. `abstract [onInit](): void` is therefore neither
of those kinds in this parser, or the walk does not reach its member. **That is
the tenth thing about this row that measurement contradicted, and it stays
unattributed** — §131's fixture read explained *which upstream clause applies*,
which is not the same as knowing *what this parser produced*.

The two residual lines and their state:

- `mergeSymbolRexportFunction main.ts(2,1)` — **`tsr_binder`'s missing merge**
  (§130), not this rule's.
- `computedPropertyName component.ts(32,12)` — the abstract member.
  **Next step is a probe of the node kind, not another clause**: print
  `self.nodes.kind(member)` for that site. §128 and §130 both ended this way and
  both times the probe was one line.

### Two gates failed on code from the pull again

`useless_conversion` in `binder.rs:2789` and a dead `report_located_without_summary`
in `tsr-execute`. Both fixed rather than left; the second with `#[allow]` and a
note, because deleting another workstream's function is not this one's call.

**A `python` `str.replace(…, 1)` took the wrong one of two identical lines** and
turned a lint into a type error. Line-indexed replacement, with an assertion on
the line's content, is the form to use when the string is not unique — the same
class of defect as §122's truncated `grep`: **an operation that can silently
apply to the wrong target is an operation that will.**

## §133 — the probe overturns §132: the abstract clause works

§132 concluded that adding the signature member kinds changed nothing, therefore
`abstract [onInit](): void` "is neither of those kinds in this parser". The
probe §132 itself prescribed says otherwise:

```
MEMBER kind=MethodDeclaration abstract=false mods=[]
MEMBER kind=MethodDeclaration abstract=true  mods=["AbstractKeyword"]
```

**The abstract member is a plain `MethodDeclaration`, `member_is_abstract`
already enumerated that kind, and it answers `true`.** The clause was working
before §132 touched it — which is exactly why adding `MethodSignature` and
`PropertySignature` changed nothing. §132 read "no change" as "wrong kind" when
it meant "already handled".

### What the residual actually is

The probe enumerates every computed-name site in the file:

| member | owner | verdict |
|---|---|---|
| `MethodDeclaration` ×2 | `ClassDeclaration` | one `abstract=true` → valid |
| `PropertyDeclaration` ×4 | `ClassDeclaration` | reported |
| `MethodSignature` | `InterfaceDeclaration` | valid |
| `PropertySignature` | `TypeLiteral` | valid |
| **`PropertyAssignment`** | **`ObjectLiteralExpression`** | **reported** |

Upstream expects **four** lines and this port emits **six**. The
object-literal computed name is one of the two extras and is **not covered by
any clause of `IsValidTypeOnlyAliasUseSite`** as ported — upstream's predicate
would report it too, so the divergence is elsewhere: most likely the *symbol*
resolved at that site, not the site itself.

**Unattributed, deliberately.** This row has now had thirteen attributions and
the failure mode is stable: each one reads a *new* artefact and concludes from
it alone.

### The pattern, stated plainly enough to act on

| what was read | what it answers | what it cannot |
|---|---|---|
| `diaggap` / `diagmissing` | which cases, which lines | why |
| the `.errors.txt` baseline | what upstream reports | what this port did |
| upstream's Go source | what upstream computes | what this port computed |
| the fixture | which language rule applies | which code path ran |
| a probe | **which code path ran, with what** | — |

**Only the last one answers the question this row keeps asking**, and it has
been right every time it was run at the correct depth (§128 the symbol, §130 the
loop, §133 the member). Each of the other four has produced at least one
confident wrong answer here.

**Next action: probe the SYMBOL at `component.ts(32,12)`** — its flags and
declarations, the way §128 did for `Row` — before touching another clause.

## §134 — the symbol probe closes the row: `declare` on the MEMBER

§133's prescribed probe, at the reporting site:

```
5 × SYM onInit flags=ALIAS decls=[ImportSpecifier] valid=false
4 × SYM onInit flags=ALIAS decls=[ImportSpecifier] valid=true
```

**Five report and four are expected, and the symbol is byte-identical at all
nine sites.** So the discriminator was never the symbol — it is the site, and
one of the five is a site the ported clauses do not excuse.

That site is `class G { declare [onInit]: any }`. §132's ambient clause routes
through `declaration_is_in_an_ambient_context`, which reads `declare` on the
declaration kinds that *contain* members — `ClassDeclaration`,
`ModuleDeclaration`, `VariableStatement`, `FunctionDeclaration`,
`EnumDeclaration` — and **never on a member itself**. `declare class H` was
caught; `declare [onInit]` was not.

**§81 recorded this exact gap**, for TS7010, in those words: *"any rule asking
'is this ambient' of a class member must read the member's own `declare`
modifier"*, and noted that every rule taking `ambient` for a member is one
`declare` away from it. Three sessions later, this is that rule.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| §132 | 5 | 0 | 22 | 2 |
| **§134** | **6** | **0** | **22** | **1** |

**+1 case and wrong 2 → 1.** `conformance/computedPropertyName` converts.
Coverage `1,431 → 1,432 / 5,488` (**26.09%**); `checker_types` identical at
3,935/9,538 · 84.43%; all five gates green.

### The row is finished as far as this workstream can take it

One wrong line remains — `mergeSymbolRexportFunction main.ts(2,1)` — and §130
established by measurement that it is **`tsr_binder`'s missing merge**, not this
rule's. There is nothing left here to build.

### What the thirteen attributions cost, and what finally worked

Every artefact except a probe produced at least one confident wrong answer on
this row: the wrong column (§122), the baseline (§124), upstream's Go (§129),
the fixture (§132). Every probe was right, and each was right about a different
layer — §128 the symbol, §130 the loop, §133 the member kind, §134 the symbol at
the site.

**And the answer was in this project's own notes the whole time.** §81 had
written the finding, in the general form, with the warning attached. Nine
sections of investigation rediscovered it. **Before probing a fourth layer,
grep `checker-notes-diag2.md` for the symptom** — this file is now 134 sections
and is itself an instrument nobody has been reading.

## §135 — the sweep for §81's hazard: one other rule has it, at a verified zero

§134 hit §81's warning — *"any rule asking 'is this ambient' of a class member
must read the member's own `declare` modifier"* — three sessions after §81 wrote
it. This is the sweep §81 implied and nobody ran.

**Every rule in `crate::check` that takes the walk-threaded `ambient` and looks
at a class member was checked against upstream:**

| rule | takes `ambient` for a member | reads the member's own `declare`? |
|---|---|---|
| TS2564 `check_property_initialization` | yes | **yes** — `has_modifier(property.modifiers, DeclareKeyword)`, already there |
| TS7010 `check_implicit_any_return` | yes | **yes** — §81 built it, that is where the finding came from |
| TS2464 `check_computed_property_name` | yes | **no** |
| TS1361 use-site | yes | **no** — §134 fixed it |

**One gap, and it measures zero.** `RULE_CODES = [2464]` is byte-identical with
and without the check: CONVERTS 3, LOST 0, RIGHT 18, WRONG 0 on both sides. No
corpus case writes a `declare` member with a computed name that reaches TS2464.

**Landed anyway, on §109's precedent and not §122's.** The distinction the two
sections drew: §109 pre-empted, in a sibling walk, a defect that had just been
*measured* in its twin, and landed at zero; §122 implemented a predicate that
did not do the thing it was written for, and was reverted. This is §109's case —
§134 measured the defect one rule over, and this is the only other rule that has
it.

### The audit is the artefact, as in §84 and §102

Four rules checked, two already correct, one fixed by §134, one fixed here at a
cost of nothing. **§81's hazard is now closed rather than outstanding**, and the
table above is what makes that checkable next time rather than a claim.

`NodeFlags::AMBIENT` would collapse all four rows into one flag read. It is
still declared and still set by nothing — the third time this session that flag
family has been the root of a bug (§104's `YIELD_CONTEXT`, §132's ambient
clause, §134's member `declare`).

## §136 — TS2303 is already priced, in `symbols.rs`, by whoever wrote `resolve_alias`

TS2303 (`Circular definition of import alias`) has sat on this board at **10
sole-obstacle cases** since §115 called it resolver-owned. It is more precisely
priced than that, and the pricing has been in the tree the whole time —
`resolve_alias`'s own doc comment:

> **Upstream's circularity frame is deliberately not ported, and this is the
> evidence.** `resolveAlias` pushes `TypeSystemPropertyNameAliasTarget`
> (`checker.go:16272`) and, on failure, reports
> `Circular_definition_of_import_alias_0`. Two files re-exporting through each
> other is a real shape, so that frame was written here first — a
> `PropertyName::AliasTarget` variant and a push/pop around the dispatch below.
> **It was measured and it could not fire, so it was removed.** … The reason is
> structural rather than a property of those fixtures: **this function is not
> self-recursive.**

So TS2303 is not "unported"; it is **unreachable**. The frame exists in history,
was measured, and was removed for cause. Reporting a cycle requires
`resolve_alias` to become **transitive** first — each arm resolving through to
its target rather than one hop — and only then can a push/pop detect one.

**That is a `checker_types`-shaped change** (it alters what every alias resolves
to), not a diagnostics one, and it is exactly the same dependency §121's chain
walk had to work around by looping externally.

### The finding is about where pricing lives

This is the third board item this session whose answer was already written down
somewhere in the repo and not connected to the board: §134's ambient-member gap
was in §81, §135's sweep was implied by §81, and TS2303's blocker is in
`symbols.rs`'s rustdoc.

**`STATUS.md` §5 records refusals with numbers; nothing records refusals made in
a doc comment at the point of code.** `resolve_alias`'s note is a refusal — it
has the measurement, the reason and the falsifier — and it never reached the
board, so this workstream carried TS2303 as an open row for five sessions.

**Actionable: when a rule's row will not move, grep the crate for the
function upstream reports from before pricing it.** The answer is written at the
call site more often than the board suggests.

## §137 — §136's rule applied to the next two rows: nothing hidden, both genuinely unbuilt

§136 produced a rule — *when a row will not move, grep the crate for the
function upstream reports from, before pricing it* — after three items in one
session turned out to be already answered in the tree. It is worth knowing how
often that is true, so it was run on the two largest remaining rows.

**TS1100** (12 cases, `checkStrictModeEvalOrArguments`, `binder.go:1449`):

```
grep -rn "in_strict_mode\|strict_mode\|use strict" crates/tsr-binder/src/*.rs
→ nothing
```

**No strict-mode tracking exists in this binder at all** — not a disabled frame,
not a documented refusal, not a partial. §105's pricing stands unchanged: it is
`b.inStrictMode` plus a three-way message split, and it is genuinely unbuilt.

**TS7026** (28 cases plus ~13 of TS2454's, `declare global` merging):
`GlobalExports` exists in `binder.rs` but only for a UMD module's
`export as namespace` claim (`:78`, `:242`, `:526`). Nothing touches `declare
global`. §5's standing refusal stands unchanged.

### The rule's hit rate, which is the point of running it

Three hits in one session (§134, §135, §136) and **two clean misses here**. So
the pattern is real but not universal: roughly half the rows this workstream
priced by reasoning had their answer written somewhere in the crate, and half
did not.

**That is still worth the two `grep`s every time.** A hit saves a session — §136
saved five — and a miss costs one command and *upgrades an inference into a
verified negative*: TS1100 and TS7026 are no longer "believed unbuilt", they are
**checked unbuilt**, which is what makes their case counts trustworthy in the
handoff.

## §138 — TS18013: a private name is lexically scoped, so no type is needed

Re-taking `diaggap` after twenty-five builds — the move that found TS1029,
TS1163, TS1206 and TS1344 — surfaced **TS18013 at 12 sole-obstacle cases**, and
`inaccessible_property` had already declined it in one line: *"`#x` is TS18013,
a different code with its own row."* This is that row.

**A `#name` is lexically scoped to the class that declares it.** That makes the
test syntactic: walk out from the access and report unless some enclosing class
declares the name. No type, no symbol, no relation.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 0 | 0 | 0 | 0 |
| **after** | **11** | **0** | **26** | **10** |

**+11 cases.** Coverage `1,432 → 1,443 / 5,488` (26.09% → **26.29%**), and the
missing column collapsed from **22 lines / 12 cases to 1 line / 1 case**.
`checker_types` identical at 3,935/9,538 · 84.43%.

### The bar was missed and the wrong column has an owner

Ten wrong lines against a rule that emitted none, in §104's shape and recorded
as a debt rather than rounded off. The families:

- **`privateNameAndAny`** (3 lines) — the receiver is `any`. Upstream's check
  runs against the receiver's *type* and `any` permits the access; the syntactic
  test cannot see that.
- **`privateNameAndIndexSignature`** — same shape through an index signature.
- **`privateNameBadAssignment`** — a parse-error fixture where the recovered
  tree puts a `#name` somewhere upstream never resolves.

**Owner: the receiver type.** All ten need what the syntactic test deliberately
does without, so this is the boundary of the cheap version rather than a defect
in it. A `receiver_type` gate — decline when it is `any`, `error`, or has an
index signature — is the next measurement, and the target is 11 conversions with
the wrong column under 3.

### The seam is *still* not exhausted

§105 declared the cheap-grammar seam closed and §112 found TS1206. §112's own
note warned the claim had been premature once. **This is the third time
re-taking `diaggap` after a run of builds has produced a double-digit row**, and
TS18013 is not even grammar — it is a *scoping* question that happens to need no
types. **The generalisation: a code needs no subsystem if its rule is decidable
from the tree, and "grammar" was too narrow a name for that class.**

## §139 — §138's debt, half paid, and the half that could not be

§138 named the receiver type as the owner of its ten wrong lines. Two versions
were measured:

| gate | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| §138, none | 11 | 0 | 26 | 10 |
| `any` **or** `error`/`unknown` | **9** | 0 | 21 | **5** |
| **§139, `any` alone** | **11** | **0** | **26** | **7** |

The broad gate halves the wrong column and **costs two conversions**: an
`errorType` receiver is one upstream still reports through, because upstream's
check asks whether the *class* declares the name and reaches that question even
when the receiver's type is unresolved. Declining there trades a wrong line for
a missing one, which is §111's trade and was refused for the same reason.

**`any` alone: wrong 10 → 7 at zero cost.** Landed. Coverage holds at
`1,443 / 5,488`; `checker_types` identical.

### The seven that remain, and why they are not the receiver's type

Three were the `any` receivers and are gone. The rest are the index-signature
and parse-error families §138 named — and the index-signature one is **not** an
`any`: upstream permits `obj.#x` when the receiver's type has a matching index
signature, which needs the *members* of the type, not its identity.

**Owner: `get_index_info_of_type`**, which is `checker_types`' road. This is the
boundary of a syntactic rule that reaches 11 cases without one, and the
remaining seven are the price of not having the members table.

### The pattern that keeps repaying

*Try the narrow gate before the broad one.* §139's first cut was three
conditions and cost two cases; one condition removed three wrong lines for
nothing. **§87 made the same choice deliberately** — *"classifying occurrences
instead puts every misclassification into the wrong column"* — and this is the
same shape arriving from the other direction: a decline that is too broad is as
expensive as one that is too narrow, and only the measurement distinguishes
them.

## §140 — TS2428: identical type parameter lists, compared syntactically

The fourth row `diaggap` has produced after a run of builds, and the second that
is not grammar. `checkTypeParameterListsIdentical` (`checker.go:4416`): a symbol
with more than one class-or-interface declaration whose type parameter lists
disagree is reported **on every one of them**.

That "every one" is why the rule fires at each declaration the walk visits
rather than once at the symbol — the walk already provides the iteration
upstream writes by hand.

`areTypeParametersIdentical` compares count, names, constraints and defaults.
**Only count and name are ported.** A constraint comparison needs the declared
types, and `nonIdenticalTypeConstraints` is the case that wants it — a miss,
never a wrong line.

| | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| before | 0 | 0 | 0 | 0 |
| **after** | **7** | **0** | **52** | **8** |

**+7 cases and 52 right lines.** Coverage `1,443 → 1,450 / 5,488` (26.29% →
**26.42%**). `checker_types` reads 3,934 **with and without** by
stash-and-remeasure — the 3,935 → 3,934 move was the `.types` workstream's, the
sixth time that trap has been checked this session and the sixth time it was
theirs.

### A shadowed match arm, caught by a warning and not by a test

The first cut added `Node::InterfaceDeclaration(_) | Node::ClassDeclaration(_)`
as a **new** arm — ahead of §112's `Node::InterfaceDeclaration(n) =>
check_illegal_decorator`, which it silently swallowed. `rustc` said
`unreachable pattern`; **the conformance numbers did not move**, because
TS1206's interface case has no corpus fixture.

**A rule can be deleted by a match arm and the suite will not notice.** The
compiler noticed. The two arms are merged now, and the general form is: in a
`match` over `Node`, a new arm for a kind that already has one is a *silent
deletion* unless the warning is read. `check_node`'s dispatch is 40-odd arms and
this will happen again.

### The bar was missed, and the eight have a known owner

Eight wrong lines against a rule that emitted none — §104's shape again,
recorded as a debt. They are the constraint half: this rule says "identical"
when the *names* match and upstream says "different" because the *constraints*
do. **Owner: `getDeclaredTypeOfSymbol`'s local type parameters**, which is
`checker_types`' road, and the target for the next measurement is 7 conversions
with the wrong column under 3.

## §141 — TS2451 belongs to TS7026's owner, and that item is now the board's largest

`diaggap` prices TS2451 (`Cannot redeclare block-scoped variable`) at **10
sole-obstacle cases**, and the binder already emits the code — so it reads like a
deepening. `diagmissing` says otherwise. All but one of its 21 missing lines are
one family:

```
checkMergedGlobalUMDSymbol                     global.d.ts(3,21) (6,16)
duplicateIdentifierRelatedSpans_moduleAugmentation  /dir/a.ts(1,14) /dir/b.ts(4,18) (8,18)
letDeclarations-scopes-duplicates2             file1.ts(1,5)
```

**Global and cross-file merging.** A UMD global merged with a `declare global`,
a module augmentation redeclaring across files, and a `let` at global scope
colliding with one in *another file*. None is a redeclaration this binder can
see, because it does not merge across files at global scope — which is
`tsr_binder`'s missing merge, the same subsystem §5 refuses TS7026 for, §85
grew with ~13 of TS2454's, and §130 traced `mergeSymbolRexportFunction` to.

### What that item now costs, added up

| row | cases | source |
|---|---:|---|
| TS7026 | 28 | §5 |
| TS2454's share | ~13 | §85 |
| TS2451 | 10 | here |
| TS1362's last line | 1 | §130 |
| **total** | **~52** | |

**That is the largest single owner on this board** — larger than the parser's
yield/await context (11) and TS1100 (12) put together, and second only to the
947-case assignability family, which is not this workstream's at all.

§5 has refused TS7026 for thirteen sessions on §13's number (12 conversions for
47 wrong lines). **That number was measured against TS7026 alone.** It is not
the price of the subsystem; it is the price of one row inside it, and the
subsystem now has four rows.

**Actionable: re-measure the binder-merge item as one build against all four
rows before quoting §13's refusal again.** A build worth ~52 cases is priced
differently from one worth 28, and nothing has re-taken that number since the
constituency doubled.

## §142 — the binder-merge item's before-state, captured

§141 said *re-measure the binder-merge item as one build across all four rows
before quoting §13's refusal again*. The build itself is `tsr_binder`'s and not
this session's, but the **before-state** is one run and is what the next attempt
needs to beat. Captured with all six codes the merge touches isolated together —
the four rows plus `declare_into`'s other two outputs, since a merge change
moves all of them:

```
RULE_CODES = [7026, 2451, 2454, 1362, 2300, 2567]

judged cases          5488
CONVERTS              320
LOST                    0
STILL SHORT           118
diagnostics RIGHT    4354
diagnostics WRONG      98
```

**That is the number to beat, and it is the first time this family has had
one.** §13's *"12 conversions for 47 wrong"* was TS7026 alone, in isolation,
against a corpus 104 cases further back. It is not comparable to this and should
not be quoted as though it were.

Two things the capture already settles:

- **`STILL SHORT` is 118.** Even a perfect merge does not convert 118 of the
  cases these codes appear in — they need something else as well. The ~52
  sole-obstacle figure §141 computed is the honest ceiling for the *build*, and
  118 is the reminder that the row's total population is not.
- **`WRONG` is 98 already**, before any merge work. A merge that fixes
  redeclaration will move that column in both directions at once, so the next
  attempt must report the *delta*, not the total — the trap §16 recorded for
  TS2322 and §24 for TS2403.

### Why capture a before-state you are not going to use

Because the alternative is what happened to §13's number: a measurement taken
once, in a different world, quoted for thirteen sessions as though it were
current. **A refusal is only as good as the state it was measured against, and
nothing in this repo records that state alongside the refusal.**

This section is the smallest possible fix for that: the four rows now have a
timestamped baseline in the same file as the refusal they justify.

## §143 — TS2540's gap is not the operator, and this is a pricing not a build

TS2540 sits at **11 sole-obstacle cases** and its missing lines look syntactic:

```
constDeclarations-access3.ts(26,3)   ++M.x;
externalModuleImmutableBindings f2.ts(6,7) (7,7) (11,7)
```

`++` on a read-only property. The obvious reading is that
`check_readonly_assignment_target` handles `=` and not `++`. **It is wrong.**
`assignment_target` (`expressions.rs:1775`) already walks
`PrefixUnaryExpression` and `PostfixUnaryExpression` and
`assignment_target_kind` already classifies them `Compound`, so the rule's first
gate passes.

The gap is downstream, in the half that needs the receiver's type. Two
candidates, **neither measured**, and named as candidates for that reason:

- `declared_members_are_complete(receiver_type)` — `M` is a namespace and the
  receiver type is `typeof M`, whose members come through the export table;
- `get_property_of_type(receiver_type, "x")` finding nothing for the same
  reason.

**This is `checker_types`-adjacent, not a syntactic row**, and the session's own
rule applies before anyone builds on this paragraph: *probe at the rule's entry
and print which gate returned.* §128, §130, §133 and §134 all ended that way and
every inference that skipped it on this board was wrong.

### Why the pricing is worth recording even though nothing was built

`diaggap` ranks by case count and says nothing about which layer a row needs.
Four rows this session looked syntactic and were (TS1029, TS1163, TS1206,
TS18013); this one looks syntactic and is not. **The tell was cheap: read the
existing rule before assuming the missing arm is the obvious one** — twelve
lines of `expressions.rs` said the operator was already handled.

## §144 — the probe corrects §143: all four receiver-type gates pass

§143 priced TS2540's gap as the receiver-type half and named two candidates —
`declared_members_are_complete` and `get_property_of_type` — explicitly as
candidates, unmeasured. §143's own instruction was to probe before building on
that paragraph. Probed:

```
2540 x  err=false  anyunk=false  complete=true  prop=true
```

**All four pass.** The receiver type resolves, its members are complete, and
`x` is found. §143's attribution was wrong in the same way six earlier ones on
this board were: it named the plausible layer instead of the measured one.

So the failure is further down, at one of the two tests that follow:

```rust
if !self.is_readonly_symbol(property) && !self.property_signature_is_readonly(property) {
    return;
}
if self.assignment_is_inside_the_declaring_constructor(node, property) {
```

And `is_readonly_symbol` (`flow.rs:787`) **already has** upstream's const-variable
arm — `flags.intersects(VARIABLE) && self.is_constant_variable(symbol)`, which is
`checker.go`'s `Variable && NodeFlagsConst`. So either `M.x`'s symbol is not
`VARIABLE` here (a namespace export may be filed as `PROPERTY`, which is §102's
`classify` territory and §100's shape), or `is_constant_variable` does not see
the `const`.

**Next probe, one line: print `record.flags` and `is_readonly_symbol(property)`
at that gate.** That distinguishes the two in a single run, and this section
deliberately does not guess which.

### Third correction of an attribution this session, and the cheapest yet

§143 cost one paragraph and the probe cost one run. The earlier ones cost
builds — §122 a revert, §126 two implemented fixes, §129 a whole guard. **The
cost of a wrong attribution is set by how far you build on it before probing**,
and §143 was written *with* the instruction to probe first, which is why it cost
almost nothing to be wrong.

That is the practice worth keeping from this row: **name the layer, label it a
candidate, and write the probe that would settle it in the same breath.**

## §145 — the readonly gate passes too; TS2540's gap is past it

§144 named two candidates at the readonly gate and, following its own rule, did
not guess between them. Probed:

```
2540 x  flags=SymbolFlags(BLOCK_SCOPED_VARIABLE)  ro=true
```

**`is_readonly_symbol` returns `true`.** Both of §144's candidates are wrong —
the symbol is `BLOCK_SCOPED_VARIABLE`, `is_constant_variable` sees the `const`,
and the gate passes. So does everything before it (§144 measured that).

The rule therefore **reaches its report** for `x`, and the missing baseline line
at `(26,3)` is not a rule that declined. What remains between the gate and the
report is one test — `assignment_is_inside_the_declaring_constructor` — and the
report's own position, which is `error_span(name_id)`, the **`x`**. Upstream's
missing lines are at **column 3**, the start of `++M.x`.

**Most likely a position divergence, not a decline** — but that is a hypothesis
and this section will not build on it. The measurement that settles it is a
`diagcase` on `constDeclarations-access3` with the expected and actual TS2540
columns read side by side; the run above was truncated before the actual half.

### Four attributions on this row, four corrected by the next probe

§143 said receiver type; §144's probe said no. §144 said the readonly gate;
§145's probe says no. **Each correction cost one run because each section named
its candidates as candidates and wrote the probe alongside them** — the practice
§144 recorded, now demonstrated twice in succession.

The row is one probe from an answer and has cost no build. That is the shape
this board's investigations should have had from §117 onward.

## §146 — one missing line, no extras: `++((M.x))`

The settling measurement §145 asked for, as a set diff of the two columns:

```
MISSING:  constDeclarations-access3.ts(26,3) TS2540
EXTRA:    (none)
```

**§145's position hypothesis is wrong too.** There is no extra at a shifted
column — every line this port emits is at a position upstream agrees with. One
line is simply absent.

Stripped line 26 is the file's last readonly case and the only one with **two
layers of parentheses**:

```ts
M.x--;        // reported
++M.x;        // reported
--M.x;        // reported
++((M.x));    // NOT reported   <- line 26
```

`assignment_target` (`expressions.rs:1775`) climbs `ParenthesizedExpression`, so
one layer works — the rule reports the three above it. Two layers do not, which
points at the climb terminating rather than at any gate this row has probed.

**Named, not guessed**: the next step is to read that loop's parenthesis arm and
check whether it continues or returns after one hop. That is a five-line read,
and the row is one line from closing.

### Five attributions, five probes, no builds spent

§143 receiver type → §144 no. §144 readonly gate → §145 no. §145 position →
§146 no. **Each was corrected by the next measurement, each cost one run, and
the row has consumed zero builds and zero reverts.**

Compare §117–§130 on the type-only alias row: six inferences, three reverts, two
implemented fixes that did nothing. **The difference is not the difficulty of
the rows — it is that these sections wrote the disproving measurement into the
same paragraph as the claim.**

## §147 — the parenthesis arm is correct; §146's pointer was wrong too

§146 pointed at `assignment_target`'s parenthesis handling terminating after one
hop. Read (`expressions.rs:1823`):

```rust
Node::ParenthesizedExpression(_)
| Node::ArrayLiteralExpression(_)
| Node::SpreadElement(_)
| Node::NonNullExpression(_) => current = parent,
```

**It loops.** `current = parent` and round again — two layers of parentheses
climb exactly as one does. §146's pointer is wrong, which makes **six**
consecutive attributions on this row corrected by the next look.

So `++((M.x))` is not failing in the climb. What has now been *measured* to pass
for this row's other lines and *read* to be correct: every receiver-type gate
(§144), the readonly gate (§145), the position (§146), the parenthesis climb
(§147). What has never been checked for **this specific node** is whether
`check_readonly_assignment_target` is called on it at all.

**That is the next probe and it is the one this row started needing at §143**:
an `eprintln!` at the rule's *entry*, printing the node kind, run on this one
case. §118 established that a probe below an early return measures its own
position; every probe since has been placed at an entry and every one has been
right. This row has instead probed *gates*, one per section, and each answered
truthfully about a gate that was not the problem.

**The lesson is now unambiguous and it cost six sections to make it so: probe
whether the rule RUNS before probing what it decided — even when you are certain
it runs, and especially when the gates all pass.** §143–§147 each confirmed a
gate; none confirmed the call.

### State of the row

One missing line, no extras, five gates cleared, one unchecked assumption. Zero
builds and zero reverts spent. The row is genuinely one probe from an answer,
and this session does not have the budget to run it well.

## §148 — the entry probe: the rule IS called, and exactly one instance declines

§147 said probe the entry, not another gate. Done:

```
17 × 2540-entry kind=PropertyAccessExpression none=false
 9 × 2540-entry kind=PropertyAccessExpression none=true
```

**The rule is called on every access, and 17 pass the assignment-target gate.**
This port emits 16 TS2540 lines for the case and upstream expects 17. So the
call happens, the gate passes for all 17, and **exactly one of them is declined
somewhere between that gate and the report.**

That is the narrowest the row has been, and it is a *counting* result rather
than an attribution: 17 in, 16 out, one lost, and the losing test is one of the
four between them — the optional-chain check, the three receiver-type gates, the
readonly pair, or `assignment_is_inside_the_declaring_constructor`.

**The probe that finishes it is the same one without `sort -u`.** §145's
readonly probe was deduplicated, which is why it showed only `ro=true` and hid
whichever instance answers `false`. **A deduplicated probe over a rule that runs
17 times cannot show the one that differs** — the same class of instrument
failure as §122's truncated `grep` and §118's misplaced `eprintln!`, and the
third distinct way this session has made a probe lie.

### Where this row stands, finally

Zero builds, zero reverts, seven attributions each corrected by the next
measurement, and a defect localised to one of four tests on one of seventeen
instances. **The row is one un-deduplicated probe from an answer.**

Recorded rather than run because this session's remaining budget cannot both run
it and land the fix to the standard the other twenty-eight builds were held to.
Starting a build that cannot be measured, gated and pushed would trade the one
property that made those builds compound.

## §149 — all seventeen pass every gate, and sixteen lines come out

The un-deduplicated probe §148 asked for, on all four remaining tests at once:

```
17 instances reach the optional-chain test   — none has qdot=true
17 instances reach the readonly test         — none has ro=false
17 instances reach the constructor test      — none has ctor=true
```

**Every one of the seventeen passes every gate.** Seventeen reports should
follow. Sixteen lines come out.

So the loss is **not in any gate this row has probed across seven sections** —
it is in the report path itself, after the last test: `error_span(name_id)`,
`source_file_of_for_diagnostics`, or `self.report`. The likeliest single
mechanism is a **span collision**: two of the seventeen resolving to the same
`(line, column)` and the suite's multiset seeing one where the port intended
two. That would present exactly as *one missing, zero extra*, which is what
`diagcase` measured in §146 and is otherwise hard to explain.

**Named, not built on.** The measurement that settles it is printing the
computed `span.start` for all seventeen and looking for a repeat — one line
added to the probe already written above.

### What this row is worth as a record

Eight attributions, eight corrections, **zero builds and zero reverts**. Every
section named a candidate, wrote the probe that would disprove it, and was
disproved. The defect is now bounded to three lines of code — a span, a file
lookup and a report call — from a starting point of *"TS2540 is a
`checker_types`-adjacent row"* (§143).

**The eight wrong attributions cost eight commands. The comparable row that
attributed by reasoning (§117–§130) cost three reverts and two no-op fixes.**
That is the whole argument for the practice, measured on two rows of the same
board in one session.

## §150 — seventeen distinct spans are reported and the suite sees sixteen

§149's span-collision hypothesis, probed at the report call:

```
17 × 2540-span   — all seventeen distinct, no repeated offset
```

**Not a collision.** Seventeen distinct spans reach `self.report`, and the
suite's actual column for this case is one short with nothing extra (§146).

So the loss is **downstream of the rule entirely** — between `self.report` and
what `diagnostics_suite::reported_for` returns. Two mechanisms fit and neither
has been looked at:

- **`self.report` deduplicates.** If it drops a diagnostic already recorded for
  the same file at a nearby position, or the same code, one of seventeen
  vanishes silently.
- **The suite filters.** `reported_for` restricts to the case's own files and
  applies the same skip rules the harness uses; a line just outside them is
  dropped without trace.

**This is no longer a TS2540 finding.** If `report` deduplicates, it does so for
every rule in `crate::check`, and every row this workstream has ever measured
carries an unknown number of silently dropped lines. That is the first
hypothesis on this board that would invalidate other measurements rather than
explain one.

**Next action, and it is not on TS2540: read `Checker::report`.** Ten lines will
say whether it drops anything. If it does, §16's 988, §24's 6, §13's 47 and
every wrong-column figure in this file were measured through a filter nobody
documented.

### Nine attributions, nine corrections, and the last one changed the question

The row began as *"TS2540 needs `checker_types`"* (§143) and ends pointing at
the reporting infrastructure shared by every rule. **Zero builds, zero reverts,
nine commands.** Each section named a candidate and wrote the probe that would
disprove it; the ninth disproved the row itself.

## §151 — `Checker::report` does not deduplicate, and the alarm is cleared

§150 raised the only hypothesis on this board that would have invalidated other
measurements: if `self.report` silently drops diagnostics, every wrong-column
figure in this file was taken through an undocumented filter. Read
(`check.rs:3522`):

```rust
pub(crate) fn report(&mut self, file: NodeId, diagnostic: Diagnostic) {
    self.diagnostics.push((file, diagnostic));
}
```

**A plain push. No dedup, no filter, no condition.** `diagnostics()` hands the
vector back and the consumer drains it — ADR-0040 decision (1).

**So §16's 988, §24's 6, §13's 47 and every bar set this session stand.** That
is the result worth having from §150, and it is a negative one: the ten minutes
spent reading ten lines bought confidence in roughly forty recorded numbers.

### Where TS2540's missing line actually is

Seventeen distinct spans are pushed and the suite's actual column is sixteen, so
the loss is in **`diagnostics_suite::reported_for`** — the collection side, not
the checker. It restricts to the case's own files and applies the harness's skip
rules; a diagnostic whose `file` node id resolves outside that set disappears
without trace. `constDeclarations-access3` is a single-file case, which makes a
**file-id mismatch** the shape to check first — the same `file` argument
`report` takes and nothing in the rule verifies.

**Next action: print the `file` node id alongside each of the seventeen and
compare against the id the suite collects for.** One line, and it is the last
unexamined link in a chain this row has now walked end to end.

### The row, closed as an investigation

**Ten attributions, ten corrections, zero builds, zero reverts, ten commands.**
It began at *"TS2540 is `checker_types`-adjacent"* and ends having (a) localised
a defect to one link in the reporting chain and (b) **cleared the integrity of
every measurement in this file** — the second worth more than the eleven cases
the row was originally priced at.

## §152 — the diff was set-based, and that hid the answer

All seventeen reports carry the same `file` id (`NodeId(189)`) and seventeen
distinct spans. Nothing is dropped between the rule and the suite. So §151's
"the suite filters" is wrong too — and the fault is in **§146's own diff**:

```awk
{if(e)E[$0]=1; if(a)A[$0]=1}   # a SET, not a multiset
```

**A duplicate in the actual column is invisible to it.** "One missing, zero
extra" is therefore consistent with a different state than the one it was read
as: seventeen reports producing **sixteen distinct positions**, because two of
them land on the same `(line, column)` — and one expected position is never
produced at all.

That is the fourth instrument this session that returned a true answer to a
question other than the one asked (§118's probe below an early return, §122's
`grep` over a truncated listing, §148's `sort -u` over seventeen instances, and
now a set-based diff over a multiset the suite compares as a multiset). **The
suite's own comparison is a sorted multiset — §146's diff was not, and the
mismatch is the whole of the confusion this row has spent four sections in.**

### The actual next step, and it is small

Re-run the comparison **with counts**: `sort | uniq -c` on both columns rather
than set membership. That names which position is doubled, and the doubled one
is the report whose `error_span` is wrong — the defect this row has been
circling since §143.

**Do not use a set-based diff against this suite again.** `diagnostics` compares
sorted multisets of `(file, line, column, code)`; any instrument that dedupes
before comparing can only answer a weaker question, and will answer it
confidently.

### Row scorecard, final

**Eleven attributions, eleven corrections, zero builds, zero reverts.** Four of
the eleven were wrong because an *instrument* lied rather than because a
hypothesis was bad. That ratio is the most useful thing this row produced: on
this board, **a wrong answer is about as likely to come from the measuring tool
as from the reasoning**, and only re-deriving the tool's exact question
distinguishes them.

## §153 — seventeen actual, all distinct: the missing line is among the NINE declined

The count comparison §152 asked for: the actual column holds **17 TS2540 lines,
every one unique**. No duplicate. §152's hypothesis is wrong too.

So the arithmetic closes: expected has **18**, this port emits **17**, and
§148's entry probe counted **17 passing the assignment-target gate and 9
declined at it**. The missing line is one of those nine — declined by
`assignment_target_kind(node) == None`, the rule's very first test, before any
gate this row spent nine sections probing.

**Which puts the answer back at `assignment_target`'s climb**, the one place
§147 cleared by *reading* rather than measuring. Reading it showed a loop that
handles nested parentheses; the count says one access that should be an
assignment target is not classified as one. `++((M.x))` remains the only
candidate shape in the file.

**§147 is the lesson, not the code.** It is the single section of this row that
concluded from a *read* instead of a probe, and it is the one whose conclusion
survived nine sections before the arithmetic contradicted it. Reading code
answers *what it does*; only running it answers *what it did*.

### The row, handed over

- **Actual 17, expected 18, entry probe 17 pass / 9 decline.** The missing line
  is in the nine.
- **Next command:** print the node kind and parent chain for each of the nine
  declined at the entry gate, on this case. One run.
- **Zero builds, zero reverts, twelve corrections, twelve commands.**

Five of the twelve were wrong because an instrument answered a different
question; one was wrong because a read replaced a measurement. **That is the
distribution worth remembering: on this board the reasoning was rarely the
weakest link.**

## §154 — the nine declined, enumerated

The probe §153 asked for, on the nine accesses declined at
`assignment_target_kind == None`:

```
2  parent=CallExpression          grand=ExpressionStatement
2  parent=PrefixUnaryExpression   grand=ExpressionStatement
1  parent=BinaryExpression        grand=VariableDeclaration
1  parent=ExpressionStatement     grand=SourceFile
1  parent=IfStatement             grand=SourceFile
1  parent=ParenthesizedExpression grand=ExpressionStatement
1  parent=PropertyAccessExpression grand=CallExpression
```

**`++((M.x))` is not among them.** Its access would show
`parent=ParenthesizedExpression grand=ParenthesizedExpression`; the one paren
entry here has `grand=ExpressionStatement`, so it is a bare `((M.x));`. The
double-paren case therefore *passes* the entry gate and is one of the seventeen
reported — which retires the candidate this row has carried since §146.

Seven of the nine are plainly correct declines (a call, a read in an `if`, a
right-hand side). The two worth a second look are
`parent=PrefixUnaryExpression grand=ExpressionStatement`: `assignment_target`
returns the unary only for `++`/`--`, so these are some other prefix operator —
almost certainly the `+M.x` / `-M.x` reads the fixture also contains, and
correct.

**So the entry gate is probably right too**, and the eighteenth expected line is
not among the nine. The arithmetic in §153 stands (17 emitted, 18 expected) but
its conclusion — *"the missing line is among the nine"* — does not follow from
this enumeration.

### Stopping point for this row, stated honestly

Thirteen sections, thirteen corrections, **zero builds and zero reverts**, and
the defect is *still* not located. Every layer has been probed and each returned
a clean answer; the discrepancy is one line and survives all of them.

**The next step is the one thing never done: read the fixture's eighteen
expected lines against the seventeen emitted, position by position.** §131
proved that reading the input explains what nine sections of layer-probing
could not, and this row has repeated the same mistake at a different scale —
probing *mechanisms* when the *data* was never laid side by side.

## §155 — the side-by-side read: `(26,3)` and nothing else

The comparison §154 prescribed, positions matched across both columns:

```
ONLY-ONE-SIDE  26,3
```

**Every other position appears on both sides.** Seventeen emitted lines each
match an expected one exactly; `(26,3)` is expected and never emitted. The
discrepancy is one access and one access only.

Two facts from earlier sections now sit in contradiction, and naming that is
this section's whole contribution:

- **§154:** the line-26 access is *not* among the nine declined at the entry
  gate.
- **§149:** all seventeen that pass the entry gate clear every later gate.

Both cannot hold while `(26,3)` goes unreported. One of the two probes is
measuring fewer instances than it appears to — and the likeliest culprit is the
same one that has misled this row twice: **§149's probe was read through
`grep`/`sort` pipelines that collapse identical output lines**, so an
eighteenth instance producing output identical to another would be invisible in
both counts.

**The one command left: re-run §148's entry probe and §149's gate probe with
`| wc -l` instead of `| sort | uniq -c`, and compare the raw totals to 26
accesses.** If the entry probe shows 18 passing rather than 17, §149's "all
pass" is a dedup artefact and the defect is at a later gate after all.

### The row's final state, and the honest verdict on it

Fourteen sections, fourteen corrections, **zero builds, zero reverts**, one line
still unexplained. **Six of the fourteen were wrong because an instrument
collapsed, truncated, or deduplicated its output.**

That is the finding this row exists to record, and it outweighs the eleven cases
it was priced at: **on this board the dominant failure mode is not bad reasoning
about the compiler — it is reading a measurement that answered a narrower
question than the one asked.** Every `sort -u`, `head`, `grep -A n` and
set-valued diff in an investigation is a place where that happens silently.

## §156 — TS1100 needs no strict-mode tracking at all, and the baselines say so

§105 priced TS1100 as *"`b.inStrictMode` plus a three-way message split"*, and
§137 confirmed by `grep` that **no strict-mode tracking exists in this binder**,
which made the row look like a subsystem rather than a rule. §137's grep was
right and its conclusion was drawn one layer too early: it verified that *this*
port has no strict-mode state without checking whether *upstream* has any.

**It does not.** `binder.Binder` (`binder.go:83-113`) has no `inStrictMode`
field, and `grep -rn "inStrictMode\|InStrictMode" internal/` over the pinned tree
returns nothing. Every one of the seven `checkStrictModeEvalOrArguments` call
sites is dispatched unconditionally from `bind` (`binder.go:617-640`,
`:1165`, `:1194`, `:1369`), and `checkStrictModeEvalOrArguments`
(`binder.go:1449`) tests only *"is this identifier `eval` or `arguments`"*.

The falsifier was already in the corpus, and it is decisive:

```
conformance/parserStrictMode3-negative.ts   →   eval = 1;
  parserStrictMode3-negative.ts(1,1): error TS1100: Invalid use of 'eval' in strict mode.
```

One line, **no `"use strict"` prologue, no `export`, no class, not a module** —
and upstream reports TS1100 anyway. The fixture's own name says it was written
to be the *negative* of `parserStrictMode3`, i.e. the case that should stay
quiet, and upstream is not quiet. That is bug-compatibility this port owes,
because [ADR-0006](../adr/0006-conformance-oracle.md) makes the generated Go the
oracle. `alwaysStrict` / `alwaysStrictES6` / `alwaysStrictModule` set the option
and it changes nothing about which line is reported — the option is an *emit*
concern here, not a binder one.

**So the rule needs no option, no prologue scan, and no strict-mode state.** It
is a syntactic walk with a three-way message choice, which is the same shape as
§103's TS1029 and §104's TS1163 — the seam §105 declared exhausted.

### The three-way choice, and why only two arms have a population

`getStrictModeEvalOrArgumentsMessage` (`binder.go:1457`) picks in order:

| test | code |
|---|---|
| `GetContainingClass(contextNode) != nil` | **TS1210** |
| `b.file.ExternalModuleIndicator != nil` | **TS1215** |
| otherwise | **TS1100** |

`diagmissing` over the corpus:

```
TS1100   13 missing lines   12 cases blocked on it alone
TS1210    3 missing lines    3 cases blocked on it alone
TS1215    0 missing lines    0 cases
```

TS1215 has **zero** missing lines, which does not make it optional: a port that
emits TS1100 where upstream emits TS1215 is wrong at the right position, and
every such line is a new false positive. The arm is built for the wrong column,
not for the right one — the same reason §103 ported all four `_0_modifier_must_precede_1_modifier`
orderings when only two had cases.

### The bar

**Off the CASE count, per §79's rule.** Sole-obstacle cases: **12 + 3 = 15**.
Concentration is 16 lines / 15 cases = **1.07**, the best shape measured on this
board since §104's 1.0 — and unlike §82's `reachabilityChecks1…11`, the fifteen
have fifteen distinct file-name stems (`parserStrictMode8/9/10/11/13` share a
stem but are five separate fixtures with different bodies, not one file re-run
under options). So the count is not inflated the way §82's was.

```
bar:  +13 cases,  0 LOST,  WRONG ≤ 5
```

The bar is set below the 15-case ceiling because two of the fifteen
(`plainJSReservedStrict` at two lines, `jsFileCompilationBindErrors` at four
diagnostics of which TS1100 is one) need every other line in the case to be
right already.

### Falsifiers — what would say this reasoning is wrong

1. **`WRONG` exceeds 5.** Then the unconditional reading is wrong somewhere the
   corpus disagrees with `parserStrictMode3-negative`, and the first place to
   look is a call site whose upstream guard was dropped (`bindParameter` and
   `checkStrictModeFunctionName` both gate on `NodeFlagsAmbient`, and this port
   has no such flag — §94's list).
2. **`LOST` is non-zero.** The rule only adds diagnostics, so a loss means a
   case that was exact gained a line — which would mean a call site fires where
   upstream has none.
3. **`binder_symbols` moves.** It must not: this build is in `crate::check` and
   touches no symbol table. If it moves, the change leaked.
4. **A wrong line carries TS1100 where the baseline carries TS1210 or TS1215.**
   That falsifies the *split*, not the rule, and `diag2307`'s wrong column
   prints the code so it is distinguishable from a wrong *position*.

## §157 — §156 built: +15 cases at ZERO wrong, the whole ceiling

The bar was `+13 cases, 0 LOST, WRONG ≤ 5`. Measured with
`RULE_CODES = [1100, 1210, 1215]` isolated, at the build:

```
judged cases          5488
CONVERTS                15
LOST                     0
STILL SHORT             13
diagnostics RIGHT       45
diagnostics WRONG        0
```

`coverage` confirms it end to end: `diagnostics` **1,451 → 1,466**, and
`checker_types` (3,937 · 84.47%), `binder_symbols` (8,459/8,459) and
`printer_round_trip` (11,762/11,762) all unmoved. Every one of §156's four
falsifiers came back negative.

**All fifteen sole-obstacle cases converted** — the twelve TS1100 and the three
TS1210 — which is the first build on this board to take a row's entire ceiling.
The bar was set at 13 because `plainJSReservedStrict` and
`jsFileCompilationBindErrors` needed every other line in their case to be right
already; both were, so the discount was unnecessary.

### The two numbers worth carrying forward

**`RIGHT` is 45 against 16 missing lines.** Twenty-nine correct TS1100 lines
land in cases that still fail for something else (`STILL SHORT` 13). The row's
*population* was nearly three times its *conversion*, which is §142's point
restated from the other side: a rule's yield is not its row, in both directions.

**`WRONG` is 0 across all three codes**, including TS1215, which had no missing
line to earn and could only have cost. The three-way split is therefore verified
in the only way it can be — no case in the corpus disagrees with it at any
position. §144's rule held: the arm built for the wrong column is the one that
proves the split.

### What this retires

**§105's "the cheap-grammar seam is now exhausted" is retired**, and so is the
reason it was believed. §105 priced TS1100 as a subsystem because it read
`checkStrictModeEvalOrArguments`'s *name* and inferred a strict-mode gate;
§137's `grep` then confirmed the gate was missing *in this port* and stopped.
Neither read upstream's `Binder` struct, and neither opened
`parserStrictMode3-negative`, which is 24 characters long and settles it.

**The rule that would have caught it two sessions earlier is already written in
this file** — §131's *read the fixture before the fourth hypothesis*. This row
had two hypotheses and no fixture read. Extend it: **read the fixture before the
FIRST refusal, not the fourth hypothesis.** A refusal is the most expensive
output this workstream produces — §13's survived thirteen sessions — and it is
the one produced with the least evidence.

### What is still not built, named so it is not mistaken for done

`checkStrictModeIdentifier` (`binder.go:1303`, TS1212–TS1214 and their class and
module variants), `checkStrictModeWithStatement` (TS1101),
`checkStrictModeDeleteExpression` (TS1102),
`checkStrictModeLabeledStatement` (TS1344, already built at §104's sibling) and
`checkStrictModeFunctionDeclaration`'s block-scope arm (TS1250–TS1252) are all
untouched. **TS1101 and TS1102 are the same shape as this build and read no
state at all** — `parserStrictMode14` and `15` are their fixtures, and §7's
"TS1212 needs `alwaysStrict` inside `tsr_binder::bind`" in `TASK-diagnostics.md`
is now suspect for the same reason §105 was: it names an option upstream's
binder does not read.

## §158 — TS7026 is not a merge row, and the "~52-case owner" is really ~23

§141 assembled a single owner out of four rows and called it *"the largest
single owner on this board"*, at ~52 cases. `diagmissing` on each row
individually — which §141 ran only for TS2451 — does not support that.

**TS7026 (28 cases, more than half the total) is JSX.** The message is
`JSX element implicitly has type 'any' because no interface 'JSX.{0}' exists`,
and upstream emits it from `jsx.go:1253`, not from anything in `binder.go`.
Every one of its 110 missing lines is in a `.tsx` file:
`tsxNamespacedTagName1`, `tsxElementResolution5/13/14/16/18`,
`reactNamespaceJSXEmit`, `keywordInJsxIdentifier`. The row wants the `JSX`
namespace and its `IntrinsicElements` interface resolved — **a JSX build**, and
it has nothing to do with `declare global`.

**TS1362 (5 cases) is `export type *`.** `exportNamespace4`'s baseline is
`'A' cannot be used as a value because it was exported using 'export type'`
over `export type * from './a'` — type-only re-export propagation through a
star. §130 reached it by tracing `mergeSymbolRexportFunction` and filed it as a
missing merge, which is true of the *mechanism* and misleading about the
*subsystem*: it is the export-star pipeline, not global merging.

Corrected constituency:

| row | cases | actually wants |
|---|---:|---|
| TS7026 | 28 | JSX intrinsic elements (`jsx.go:1253`) |
| TS2451 | 10 | cross-file / global merging ✔ |
| TS2454's share | ~13 | unverified; §85's attribution is the same kind of inference |
| TS1362 | 5 | `export type *` propagation |

**So the merge item is ~10 cases with a verified attribution, not 52.** §13's
thirteen-session-old refusal was quoted for a row that is not even in the
family, which is the second time this board has carried a number attached to the
wrong subsystem (§88/§89 was the first).

### The rule this is the third instance of

§136: *when a row will not move, grep the crate for the function upstream
reports from.* §157: *read the fixture before the first refusal.* Both are
special cases of one thing, and it is now cheap enough to state as a standing
step: **`diagmissing <code>` prints case names, and a row's case names are its
attribution.** Twenty-eight case names beginning `tsx` were sitting in front of
every session that quoted §13, and reading them costs one command.

## §159 — the TS2451 build: the merge exists, only the error branch is missing

The corrected row is 10 cases, and six of them are one shape:

```
letDeclarations-scopes-duplicates2 … 7
  file1.ts:  let var1 = 0;
  file2.ts:  let var1 = 0;   (or const, or var)
  → TS2451 on BOTH names, one per file
```

**The cross-file merge is already built.** `Binder::merge_globals`
(`binder.rs:597`) unions each *script* file's top-level locals into
`self.globals`, calling `merge_symbol(target, source, 0)` when the name is
already there. `merge_symbol` then returns early at

```rust
if (source_flags | target_flags).intersects(SymbolFlags::ALIAS)
    || source_flags.excludes().intersects(target_flags)
{ return; }
```

which is precisely `checker.go:14147`'s guard — and upstream's `else` arm there
is `reportMergeSymbolError` (`checker.go:14201`). The port's own doc comment on
`merge_symbol` has said so since it was written: *"Upstream reports a
diagnostic and does not merge; this does not merge, and **has no diagnostics to
report**"*. §137's rule found the answer already in the tree for the third time
this session.

### Why the report cannot be pushed onto the binder's own list

`diagnostics_suite::reported_for` (`diagnostics_suite.rs:153`) runs the binder
**once per unit, on that unit alone**, and stamps every diagnostic it returns
with that unit's file name. The cross-file bind happens somewhere else entirely
— inside `program_for_case`, whose binder diagnostics **nothing collects**. A
diagnostic pushed from `merge_globals` would therefore either never be seen or
be attributed to whichever file happened to be binding.

That is not a harness defect to route around; it is the layering upstream
already has. `mergeSymbol` is a **checker** function (`checker.go:14146`), and
the checker's diagnostics carry an explicit file node id which the suite maps
per unit (`diagnostics_suite.rs:304`). So: **the binder records the conflict,
the checker reports it.**

### The bar

Off the CASE count. Sole-obstacle TS2451 cases: 10, of which the six
`letDeclarations-scopes-duplicates` are plain global redeclaration and the other
four (`checkMergedGlobalUMDSymbol`, `umdGlobalAugmentationNoCrash`,
`umdNamespaceMergedWithGlobalAugmentationIsNotCircular`,
`duplicateIdentifierRelatedSpans_moduleAugmentation`) additionally want UMD
`export as namespace` or module augmentation, which this build does not add.

```
bar:  +5 cases,  0 LOST,  WRONG delta ≤ +10
```

**The wrong column is reported as a DELTA, never a total** — §142's rule, and it
matters more here than anywhere: this branch also selects TS2300, whose wrong
column was already 61 lines at §88. Measured with
`RULE_CODES = [2451, 2300, 2567]` isolated on both sides.

### Falsifiers

1. **`WRONG` delta exceeds +10.** Then the conflict branch fires on merges
   upstream permits, and the first suspect is the `ALIAS` disjunct: upstream
   does not return there, it calls `resolveSymbol` and re-tests, so anything
   this build reports through an alias is a position upstream never reaches.
2. **`LOST` is non-zero.** Six of the ten cases already fail, so a loss means
   the branch fired on a case that was exact — a permitted declaration merge.
3. **`binder_symbols` moves.** It must not. Recording a conflict changes no
   symbol table, and this is the first build of the session to touch
   `tsr-binder` at all, so the rail is the measurement that says whether the
   recording leaked into the merge itself. §141's handoff called this out
   specifically and it fired six times last session.
4. **The six `letDeclarations` cases do not convert.** Then `merge_globals` is
   not reached for them, and the reason will be `is_module`/`commonjs_module`
   — the two conditions guarding the union — rather than anything in the
   branch.

## §160 — §159 built: +10 cases for ZERO new wrong lines, and the four fixes it took

The bar was `+5 cases, 0 LOST, WRONG delta ≤ +10`. Both sides measured on the
same checkout by commenting out the single `report_merge_conflicts()` call —
**not** by `git stash`, for a reason recorded below. `RULE_CODES =
[2451, 2300, 2567, 2649]`:

| | before | after | delta |
|---|---:|---:|---:|
| CONVERTS | 49 | 59 | **+10** |
| LOST | 0 | 0 | 0 |
| STILL SHORT | 41 | 42 | +1 |
| RIGHT | 490 | 515 | +25 |
| WRONG | 82 | 82 | **0** |

`coverage`: `diagnostics` **1,466 → 1,476**. `binder_symbols` 8,459/8,459,
`checker_types` 3,937 · 84.47%, `printer_round_trip` 11,762/11,762 — all
unmoved. §159's falsifiers 1, 2 and 3 all came back negative; falsifier 4 (the
six `letDeclarations-scopes-duplicates` not converting) also did — they
converted.

### The first measurement failed the bar, and each of the four causes was named

The build as first written measured **+9 converts, +1 LOST, +33 wrong**. The
wrong column was read as a **multiset delta** against the before-state rather
than as a total, which is the only reading that works here: the same run's
absolute wrong column contains `giant.ts` and `reservedWords2` lines that
predate this build entirely, and reading the total would have attributed 82
pre-existing lines to it. §152's lesson, applied prospectively for once.

The 33 came apart into four causes, three of which were *unported upstream code*
rather than judgement calls:

1. **22 lines — the `SymbolFlagsAssignment` disjunct** (`checker.go:14147`).
   `mergeSymbol`'s condition is `target.Flags&getExcludedSymbolFlags(source.Flags) == 0
   || (source.Flags|target.Flags)&ast.SymbolFlagsAssignment != 0`, and this port
   had only the first half. A JS expando (`ExpandoMerge.p1 = 111`) is a
   declaration and a value at once, so it collides with everything it merges
   into; upstream lets it through by name. All 22 were
   `typeFromPropertyAssignment32`/`33`.
2. **1 line and the LOST — the plain-JS suppression** (`checker.go:14216-14219`).
   `reportMergeSymbolError` skips reporting *per side* for a symbol declared in
   a plain JS file. `plainJSReservedStrict`'s `const eval` collides with the
   `eval` this port synthesises into globals; upstream's silence there is this
   guard, not an absent collision.
3. **5 lines — `undefined` is not a merge target upstream.**
   `addUndefinedToGlobalsOrErrorOnRedeclaration` (`checker.go:1452`) puts
   `c.undefinedSymbol` into `c.globals` **only if nothing else declared the
   name**; when a file does, it reports TS2397 and leaves globals alone, so
   `mergeSymbol` is never reached. This port seeds the symbol per file bind, so
   a later file's `var undefined` merges into it. Declined, with the owner
   named: TS2397/TS2414/TS2427 are three unported checker collision rules.
4. **3 lines — the arm between the merge and the error.**
   `checker.go:14188`: `target.Flags&ast.SymbolFlagsNamespaceModule != 0` is
   TS2649 `Cannot augment module '{0}' with value exports…`, reported **once**,
   on the source's first declaration. `noSymbolForMergeCrash` is
   `interface A {} namespace A {}` in one file and `type A = {}` in another.
   Porting it turned three wrong lines into one right one and converted the
   case — the +10th.

**Three of the four were code upstream had written and this port had not.**
That is the same ratio §157 found and §136 first named: on this board, a wrong
column is more often an unread branch than a wrong judgement. Reading
`mergeSymbol` end to end *before* writing the report would have cost one file
read and saved two measurement rounds.

### The `git stash` trap fired, and this is the safer instrument

The handoff prescribes `git stash push <your files>` → coverage → `git stash
pop` for before/after. It failed here and was worth recording: the new file was
**untracked**, so `git stash push` refused the pathspec, the `&&` chain
short-circuited — and the unconditional `git stash pop` that followed popped an
**unrelated stash from a previous session**, conflicting `PLAN.md` and
`.beads/interactions.jsonl` into the working tree.

Nothing was lost (both old stashes survived; `git checkout HEAD --` on the two
files cleaned it up), but the instrument is sharper than it needs to be.
**Comment out the single call site instead.** It compares two states of the same
checkout — which is what the handoff actually wanted — has no interaction with
the stash stack, and cannot fail differently for a tracked and an untracked
file. Used four times in this build with no incident.

### What the row still wants

Four of TS2451's ten sole-obstacle cases did not convert:
`checkMergedGlobalUMDSymbol`, `umdGlobalAugmentationNoCrash`,
`umdNamespaceMergedWithGlobalAugmentationIsNotCircular` and
`duplicateIdentifierRelatedSpans_moduleAugmentation`. All four additionally want
UMD `export as namespace` merging or module augmentation — the subsystem §5
refuses — and this build deliberately added neither. **That is the honest
residue of the "merge item": four cases, not fifty-two.**

## §161 — TS1101 and TS1102 measure zero; TS1212's family is 34 cases

`diaggap` re-taken at `bee709e`, after §157 named TS1101 and TS1102 as *"the
same shape as §156 and reading no state at all"*. They are — and they are also
**off the board**:

```
TS1101   0 missing lines   0 cases      TS1102   0 missing lines   0 cases
```

`parserStrictMode14` and `15` are their fixtures and both already pass for
other reasons. Same outcome as §105's TS1183: a rule can be correct, cheap and
worth nothing. **Measure before building, even when the shape is proven** — §157
recommended these two on shape alone and the recommendation was empty.

### What the same re-take found instead

`checkContextualIdentifier` (`binder.go:1303`), whose three message variants
are the largest cheap row on the board:

```
TS1212   41 lines   24 cases     Identifier expected. '{0}' is a reserved word in strict mode.
TS1213    8 lines    8 cases     … Class definitions are automatically in strict mode.
TS1214    4 lines    2 cases     … Modules are automatically in strict mode.
                    ── 34 cases, one rule
```

§7 of `TASK-diagnostics.md` has carried *"TS1212 needs `alwaysStrict` inside
`tsr_binder::bind`"* for four handoffs. **It does not**, and the reason is
§156's exactly: `checkContextualIdentifier`'s gate is

```go
len(b.file.Diagnostics()) == 0 && node.Flags&NodeFlagsAmbient == 0 &&
    node.Flags&NodeFlagsJSDoc == 0 && !ast.IsIdentifierName(node)
```

— parse errors, ambient, JSDoc and name-position. **No strict-mode test.** The
corpus agrees: `letIdentifierInElementAccess01.ts` is `var let: any = {};` with
no `"use strict"`, no export and no class, and both its lines are TS1212.

### §136's rule pays for the fourth time this session

Everything the rule needs is already in the tree:

| upstream | here |
|---|---|
| `KindFirstFutureReservedWord`…`Last` | `SyntaxKind::FIRST_FUTURE_RESERVED_WORD` / `LAST_FUTURE_RESERVED_WORD` (`kind.rs:811`) |
| `scanner.GetIdentifierToken` | `tsr_scanner::keyword_kind` (`generated/keywords.rs:16`) |
| `len(b.file.Diagnostics()) == 0` | `Checker::file_has_parse_errors` |
| `NodeFlagsAmbient` | the walk-threaded `ambient` |

Both ranges were compared member by member and are the same nine keywords:
`implements interface let package private protected public static yield`.

`NodeFlagsJSDoc` is the **fifth** declared-and-never-set flag, after
`AMBIENT`, `JAVASCRIPT_FILE`, `YIELD_CONTEXT` and `SymbolFlags::OPTIONAL`. It
needs no derivation here: this parser keeps JSDoc in a side table rather than
in the tree (`bind_jsdoc_declarations`, `binder.rs:500`), so the check walk
never reaches a JSDoc-sourced identifier at all. If that ever changes, this is
the rule that starts reporting on `@param` names.

### The arm that is NOT ported, and the dead branch upstream

`checkContextualIdentifier`'s second arm is `originalKeywordKind ==
KindAwaitKeyword`, gated on `NodeFlags::AWAIT_CONTEXT` — another unset flag.
**Declined, owner `tsr_parser`'s await-context tracking**, the same owner
§104/§106 gave the yield half.

Its *third* arm (`KindYieldKeyword` under `YieldContext`) is **dead code
upstream**: `KindYieldKeyword` is `LastFutureReservedWord`, so the first arm
always claims it. Worth recording because a port that reads the arms in order
and "faithfully" adds a `YIELD_CONTEXT` derivation for the third would be
building a branch upstream never executes.

### The bar

Off the CASE count. Sole-obstacle: 24 + 8 + 2 = **34**. Concentration is 53
lines / 34 cases = 1.56, and the case names are 34 distinct stems (the
`parserComputedPropertyName36/37/38/39` group is four separate fixtures with
different bodies), so §82's discount does not apply.

```
bar:  +20 cases,  0 LOST,  WRONG delta ≤ +15
```

Set well below 34 because this rule fires on **every** identifier in the corpus
whose text is one of nine common words, and `private`/`public`/`protected`/
`static`/`interface` are ordinary identifiers in a great deal of TypeScript.
The bound that decides it is `IsIdentifierName` — nine parent kinds — and
whether this parser produces an `Identifier` node where upstream's produces a
keyword token.

### Falsifiers

1. **`WRONG` delta exceeds +15.** First suspect is `IsIdentifierName`'s parent
   list, second is a parser divergence: upstream's parser emits a *keyword*
   token in positions where this one emits an `Identifier`, and this rule only
   ever sees the latter.
2. **`LOST` is non-zero.** The rule only adds diagnostics, so a loss is a
   report at a position upstream reaches and declines.
3. **The parse-error gate measures wrong in either direction.** §40.3/§50.1/§79
   established that this gate is a *per-rule* measurement. Upstream states it
   explicitly here, so it is ported — but if the wrong column is concentrated in
   recovered trees, re-measure it rather than assuming the port was right.
4. **`binder_symbols` moves.** It must not; this is `crate::check` only.

## §162 — §161 built: +32 cases at ZERO wrong, the largest single build on this board

The bar was `+20 cases, 0 LOST, WRONG delta ≤ +15`. `RULE_CODES =
[1212, 1213, 1214]`, and for once the counterfactual **is** the delta: no other
producer in this port emits any of the three codes, so removing them from the
suite's set reconstructs the exact pre-build state.

```
judged cases          5488
CONVERTS                32
LOST                     0
STILL SHORT             16
diagnostics RIGHT      132
diagnostics WRONG        0
```

`coverage`: `diagnostics` **1,476 → 1,508 (27.48%)**. `binder_symbols`
8,459/8,459, `checker_types` 3,942 · 84.51%, `printer_round_trip`
11,762/11,762 — unmoved. All four of §161's falsifiers negative.

**32 of the 34 sole-obstacle cases, and 132 correct lines against 53 missing
ones.** The two that did not convert are in `STILL SHORT`'s 16 — cases that gain
a correct TS1212 and still fail for something else.

### The wrong column is zero, and that is the surprising part

§161 set the bar 14 cases below the ceiling on an explicit worry: the rule fires
on every identifier in the corpus whose text is one of nine words, and
`private`, `public`, `protected`, `static` and `interface` are ordinary
identifiers in a great deal of TypeScript. **Not one false positive.**

The reason is the one §161 named as the deciding bound, and it turns out to be
load-bearing in the *opposite* direction from the worry: upstream's parser and
this one both emit a **keyword token**, not an `Identifier` node, wherever those
words are used as modifiers or contextual keywords. `private x: number` in a
class body never reaches this rule because `private` is not an identifier there.
`IsIdentifierName`'s nine parent kinds then remove the remaining name positions.
So the population the rule can even see is already almost exactly the population
upstream reports on.

**That is worth stating as a bound on future pricing on this board:** a rule
gated on *what kind of node the parser built* is much safer than one gated on
text, and the two are easy to confuse when the rule reads `node.Text()` — this
one does, on the last line, after the kind test has already done the work.

### Three negatives banked in the same re-take

- **TS1101 and TS1102 measure zero.** §157 recommended both on shape alone,
  correctly, and the recommendation was worth nothing. Same as §105's TS1183.
- **`NodeFlags::JSDOC` is a fifth declared-and-never-set flag** and needed no
  derivation, because this parser keeps JSDoc out of the tree. The running list
  is now `AMBIENT`, `JAVASCRIPT_FILE`, `YIELD_CONTEXT`, `JSDOC` and
  `SymbolFlags::OPTIONAL`.
- **`checkContextualIdentifier`'s third arm is dead code upstream.**
  `KindYieldKeyword` is `LastFutureReservedWord`, so the first arm always claims
  it and the `YieldContext` branch never runs. A port adding a `YIELD_CONTEXT`
  derivation "for fidelity" would be building a branch upstream never executes.

### The strict-mode family, now closed except for one arm

| upstream | codes | state |
|---|---|---|
| `checkStrictModeEvalOrArguments` | 1100/1210/1215 | **DONE** §157, +15, 0 wrong |
| `checkContextualIdentifier`, reserved-word arm | 1212/1213/1214 | **DONE** here, +32, 0 wrong |
| `checkContextualIdentifier`, `await` arm | 1262 and siblings | declined — `NodeFlags::AWAIT_CONTEXT` unset, owner `tsr_parser` |
| `checkStrictModeWithStatement` | 1101 | measures **zero** |
| `checkStrictModeDeleteExpression` | 1102 | measures **zero** |
| `checkStrictModeLabeledStatement` | 1344 | built, §104's sibling |

**+47 cases across two builds, both at exactly zero wrong lines**, out of a
family four handoffs described as blocked on `alwaysStrict` in the binder. The
option was never read by any of it.

## §163 — `onFailedToResolveSymbol`'s cascade: the two silent returns already name their own codes

`diaggap` at `58b5ed2` puts four meaning-mismatch codes together:

```
TS2693   17 lines    9 cases   '{0}' only refers to a type, but is being used as a value here.
TS2709   15 lines    9 cases   Cannot use namespace '{0}' as a type.
TS2661   36 lines    9 cases   Cannot export '{0}'. Only local declarations can be exported…
TS2749    4 lines    4 cases   '{0}' refers to a value, but is being used as a type here…
                    ── 31 cases
```

**All four are arms of one function**, `onFailedToResolveSymbol`
(`checker.go:1564`), whose seven-way `||` chain runs *before* the missing-lib /
spelling-suggestion / `Cannot find name` sequence this port already ports in
full:

```go
c.checkAndReportErrorForMissingPrefix(...) ||
c.checkAndReportErrorForExtendingInterface(...) ||
c.checkAndReportErrorForUsingTypeAsNamespace(...) ||
c.checkAndReportErrorForExportingPrimitiveType(...) ||      // TS2661
c.checkAndReportErrorForUsingNamespaceAsTypeOrValue(...) || // TS2708 / TS2709
c.checkAndReportErrorForUsingTypeAsValue(...) ||            // TS2693 / TS2585
c.checkAndReportErrorForUsingValueAsType(...)               // TS2749
```

### The port already located the gap and wrote it down

This is not a discovery; it is a debt with an address. `check_value_identifier`
carries the comment

> `checkAndReportErrorForUsingTypeAsValue` / `…NamespaceAsTypeOrValue`
> (`checker.go:1681`, `:1643`): a name that resolves under another meaning gets
> a *different* code, **so silence is the only sound answer until those arms are
> ported.**

and `check_type_reference_name` the matching one:

> a qualified `A.B` fails as TS2694, a name that resolves as a value is
> **TS2749**, and as a namespace **TS2709** — three wrong codes at a right
> position.

Each rule's meaning ladder is a `for` loop that `return`s on the first hit:
`[TYPE, NAMESPACE, ALIAS]` in the value rule, `[TYPE, VALUE, NAMESPACE]` in the
type rule. **The build is to replace the silent `return` with the code the hit
already identifies**, in upstream's order.

| position | ladder hit | upstream arm | code |
|---|---|---|---|
| value | `NAMESPACE_MODULE` | `…NamespaceAsTypeOrValue`, value branch | TS2708 |
| value | `TYPE`, symbol has no `VALUE` | `…UsingTypeAsValue` | TS2693 |
| value | `ALIAS` | — | stays silent (§79) |
| type | `MODULE` | `…NamespaceAsTypeOrValue`, type branch | TS2709 |
| type | `VALUE`, symbol has no `NAMESPACE` | `…UsingValueAsType` | TS2749 |
| type | `TYPE` | — | correct resolution, stays silent |
| either | primitive name under an `ExportSpecifier` | `…ExportingPrimitiveType` | TS2661 |

### What is deliberately not ported, and why each is safe

- **`maybeMappedType`** (`checker.go:1707`) selects a *different* TS2693-family
  message when the name is the key of a single-member type literal whose
  declared type is a union of string/number literals. It needs
  `getDeclaredTypeOfSymbol` and `allTypesAssignableToKind` — `checker_types`
  machinery. Omitting it emits plain TS2693 where upstream emits the variant,
  which is a **wrong code at a right position**, so it is a falsifier below, not
  a free omission.
- **`checkAndReportErrorForMissingPrefix`**, **`…ExtendingInterface`** and
  **`…UsingTypeAsNamespace`** — the first three arms. Their codes (TS2662/TS2663,
  TS2689, TS2702) are not on `diaggap`'s single-code column at all, so they buy
  nothing and each is an independent wrong-column risk.
- **The heritage-clause variants of the primitive-name branch** (TS2840/TS2839/
  TS2422) fire only when a primitive name appears in an `extends`/`implements`
  clause. Ported, because they are three `if`s over the grandparent and skipping
  them would put TS2693 where upstream puts one of those.

### The bar

Off the CASE count. Sole-obstacle across the four codes: **31**.

```
bar:  +15 cases,  0 LOST,  WRONG delta ≤ +12
```

Set at half the ceiling because TS2661's 36 lines over 9 cases is a
concentration of **4.0** — the worst on this board since §82 — so most of that
row needs every other line in its case as well, and because these rules fire
from the two busiest sites in `crate::check`.

### Falsifiers

1. **`WRONG` delta exceeds +12.** First suspect is the ladder's *meaning* not
   matching upstream's `resolveName` argument: upstream asks for
   `SymbolFlagsType &^ SymbolFlagsValue` and `SymbolFlagsModule`, which are
   narrower than the `TYPE` and `NAMESPACE` this port's ladder passes. A hit on
   the port's wider mask where upstream's narrower one misses is a false
   positive by construction.
2. **A wrong line carries TS2693 where the baseline carries TS2585 or the
   mapped-type variant.** That falsifies the omission above, not the arm.
3. **`LOST` is non-zero.** Both sites currently return *silently*, so every new
   line is additive; a loss means a case that was exact.
4. **`binder_symbols` moves.** It must not.

## §164 — §163 REFUSED at +2 for +6, and the refusal found the row's real blocker

The bar was `+15 cases, 0 LOST, WRONG delta ≤ +12`. Built in full — all four
arms, both entry points — and measured with
`RULE_CODES = [2661, 2693, 2708, 2709, 2749, 2585, 2840, 2839, 2422]`:

| version | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| as written | 2 | **4** | 12 | **238** |
| type-position arms only | 0 | 0 | 8 | 0 |
| bounded (final) | 2 | 0 | 12 | 6 |

**Refused, and reverted.** +2 cases against 6 new false positives is 0.33
conversions per wrong line — the same neighbourhood as §13's refused 0.26 — and
it is thirteen cases short of a bar that was already set at half the ceiling.
The code is not in the tree; this section is the product.

### The 238 wrong lines were one defect, and it is not in the new code

Every one of the 238 was TS2693, all from the **value-position** arm. Three
fixtures read (not inferred — §131's rule, applied at the first surprise rather
than the fourth):

```
classMergedWithInterfaceMultipleBasesNoError.ts(4,23) (4,28)
    interface Foo extends Bar, Baz { }
genericTypeWithMultipleBases1.ts(9,32) (9,36)
    export interface I3<T> extends I1, I2 {
inheritFromGenericTypeParameter.ts(1,20)
    class C<T> extends T { }        ← upstream says TS2304 here
```

**`Checker::is_value_reference` admits the heritage names of an interface, and
the `implements` list of a class.** Those are *type* positions: upstream reaches
them through `resolveEntityName` with `Type` meaning, they resolve, and nothing
is reported. This port's TS2304 rule was already firing on them — and *silently
returning*, because the name resolved under `TYPE` on the meaning ladder. The
silence was not a decline; it was a defect wearing a decline's clothes.

**One condition — "no ancestor is a `HeritageClause`" — removed 232 of the 238
and all four losses.** That is the measurement worth carrying:

> §90's rule has a third instance. *A decline can conceal a bug rather than
> prevent one, and lifting it is the only way to find out which kind it was.*
> Here the concealed bug is in a **different rule** from the one being lifted,
> and it has been in the tree since §55.

### Why the bounded version still is not worth landing

Bounding gets to `+2 / 0 LOST / 6 WRONG`, and the six are diagnosed:

- **Four are `mappedTypeProperties`** — upstream's `maybeMappedType`
  (`checker.go:1707`) picks a *different* message when the name is the key of a
  single-member type literal whose declared type is a union of string/number
  literals. The syntactic half of its walk was ported as a suppression and **did
  not fire**, so the position's shape here is not the shape upstream walks.
  Owner: `checker_types` for the type half, and an unread AST shape for the
  syntactic half.
- **Two are `scannerUnicodeEscapeInKeyword2`** — unread.

Six false positives to buy two cases, in a row whose 31-case population the
build reached only 12 lines of, is the wrong trade. **`RIGHT` was 12 against 72
missing lines**: the arms mostly are not firing where the row needs them, which
means the population is blocked on *position* — the rules never see those
nodes — not on the cascade.

### What has to happen first, in order

1. **Fix `is_value_reference` to exclude interface `extends` and every
   `implements` clause.** It is a defect on its own terms, independent of this
   cascade, and it is worth measuring alone: it may already be costing TS2304
   lines that the ladder's silence hides.
2. **Re-take TS2709's and TS2749's positions.** The type-position arms measured
   **0 wrong and 0 converts** — they are correct and unreached. `diagcase` on
   `moduleWithNoValuesAsType` will say which node kind the missing line sits on;
   `check_type_reference_name` is bounded to the `type_name` slot of a bare
   `TypeReferenceNode` and that is the first suspect.
3. **Only then port the cascade**, against a bar re-taken at that point.

The refused source is not in the tree. It was four arms transcribed from
`checker.go:1629-1731` with `isExportAssignmentExpressionName`,
`isPrimitiveTypeName`, `isES2015OrLaterConstructorName` and the three
heritage-clause message variants, and it is straightforward to write again —
**the expensive part was never the code, it was learning that the row is
blocked two layers below it.**

## §165 — §164's attribution CORRECTED: `is_value_reference` is right, the cascade was wrong

§164 named `Checker::is_value_reference` as the row's blocker and filed fixing
it as prerequisite (1), on the strength of one condition removing 232 wrong
lines. **That inference was wrong, and the fix measures negative.**

The arm already excluded `implements` — it requires the heritage token to be
`ExtendsKeyword` — so the claimed defect was narrower than §164 stated: an
*interface*'s `extends` uses the same keyword as a class's, and both were
admitted. Adding the owner test (`ClassDeclaration | ClassExpression`) and
measuring it **alone**, `RULE_CODES = [2304, 2552, 2583]`:

| | before | after |
|---|---:|---:|
| CONVERTS | 235 | **234** |
| LOST | 2 | 2 |
| RIGHT | 2,428 | **2,425** |
| WRONG | 409 | 408 |

**One case lost, three correct lines lost, one wrong line removed.** Reverted.

### Why, and what the row actually needs

Upstream reports `Cannot find name` for an unresolvable name in an interface's
`extends` too — through the *type* resolver's own `onFailedToResolveSymbol`
rather than the expression path, but at the same position with the same code.
So the position is one this port is **right** to visit; what it must not do
there is treat a `TYPE` hit as a *value* meaning mismatch.

The current code already does the right thing: the ladder's `TYPE` arm returns
silently, which is correct for an interface heritage name and correct for a
class's `extends B` where `B` is a type-only name that upstream reports
differently. §163's cascade broke it by making that silent arm speak.

**Corrected prerequisite list for the meaning-mismatch row**, replacing §164's:

1. ~~Fix `is_value_reference`~~ — **it is not broken.** The bound belongs in the
   *cascade*, which must ask what kind of position it is standing in before
   deciding that a `TYPE` hit is a mismatch. `interface I extends A` and
   `class C implements I` are type positions that the value rule legitimately
   walks for TS2304's sake.
2. **Re-take TS2709's and TS2749's positions.** Unchanged and still first in
   practice: both type-position arms measured **0 wrong and 0 converts** —
   correct and unreached. `check_type_reference_name` is bounded to the
   `type_name` slot of a bare `TypeReferenceNode`; `diagcase` on
   `moduleWithNoValuesAsType` will say what the missing lines actually sit on.
3. Re-bar the cascade after (2).

### The rule this row produced, at a cost of two wrong attributions

§163 priced by reading upstream. §164 attributed by reading three fixtures.
**Both were wrong, and the thing that settled it was running the fix on its own
and reading four numbers.**

> **A bound that removes wrong lines is not thereby a fix.** Removing 232 wrong
> lines proved the *cascade* should not fire there. It said nothing about
> whether the *position test* was correct, because both hypotheses predict the
> same 232. Separating them cost one measurement and the two-line experiment was
> available from the start.

That is the same defect of instrument as §152's set-valued diff and §118's
early-return silence: **a measurement that cannot distinguish the two
hypotheses it is being used to choose between.** Third instance recorded on this
board, and the cheapest tell is that the confirming number was *large* — 232 of
238 — which reads as certainty and is actually just a shared prediction.

## §166 — the probe at the rule's entry found a ONE-LINE resolver defect

§165 left the row's next action as *"re-take TS2709's and TS2749's positions"*.
`diagcase compiler/moduleWithNoValuesAsType` says the expected lines are

```
namespace A { }
var a: A;      // TS2709 at (2,8) — and this port reports NOTHING
```

which is a bare identifier in the `type_name` slot of a bare
`TypeReferenceNode` — precisely what `check_type_reference_name` claims to
cover. So the position was not the answer either, and rather than a fourth
attribution the rule's entry got an `eprintln` behind an env var (§76/§80's
instrument, now paying for the seventh time across three sessions).

**Two runs, and the second one settles it:**

```
PROBE A: under SymbolFlags(… CLASS | INTERFACE | … | TYPE_ALIAS) -> Some(SymbolFlags(NAMESPACE_MODULE))
```

A symbol whose flags are `NAMESPACE_MODULE` **and nothing else** is returned by
a lookup asking for `TYPE`. Every scoped arm of `BindResult::resolve_name`
filters — `lookup_scoped` tests `flags.intersects(meaning)`, the namespace-
exports arm tests `intersects(meaning & mask)` — and then the walk ends:

```rust
self.globals.get(name).copied().map(|found| self.merged_symbol(found))
```

**The globals fallback ignored `meaning` entirely.** One line, and it means
every name in `globals` — which includes all of `lib.*.d.ts` — answered *every*
meaning query in the program.

### What that one line was hiding

This is why §163's cascade converted nothing and why §164 and §165 each
attributed the row to the wrong layer. The checker's meaning ladder is written
as *"if it resolves under another meaning, that hit tells you the code"*; with
an unfiltered globals lookup, **the first arm of every ladder hits for every
global name**, so the ladder always returned at `TYPE` and the arms below it
were unreachable by construction. Three sections of analysis were spent above a
defect that no amount of reading the arms could reveal.

Fixed by routing the fallback through the same helper every other arm uses,
which also inherits `lookup_scoped`'s documented alias behaviour (an `ALIAS` is
accepted whatever its own flags say, because resolving the target needs the
checker).

### Measured whole, and it is positive in the other workstream's suite

| suite | before | after |
|---|---:|---:|
| `diagnostics` | 1,508 | 1,508 |
| **`checker_types`** | **3,942 · 84.51%** | **3,955 · 84.52%** |
| `binder_symbols` | 8,459/8,459 | 8,459/8,459 |
| `printer_round_trip` | 11,762/11,762 | 11,762/11,762 |

`diagnostics` does not move, and that is the **correct** outcome: with the
filter in place, `var a: A` no longer hits `TYPE`, falls to the `NAMESPACE` arm,
and returns silently — which is right until TS2709 is ported. **This build is
the precondition that makes §163's cascade reachable, not the conversion
itself.**

`checker_types` moving is deliberate and is flagged for the `.types`
workstream, as §95 was: the revert is one line. It moved **up** by 13 cases.

### Two tests were passing because of the bug

`a_member_declared_in_both_halves_merges_rather_than_taking_the_first` and
`two_declarations_of_a_global_interface_merge_their_members` both look up an
`interface` at `SymbolFlags::VALUE` and expect it to resolve. They went red, and
they were **right to**: an interface is not a value. Corrected to ask at `TYPE`
via a new `resolve_type` helper; the assertions they actually make are
untouched.

> **A test that passes through the defect it does not name is not evidence.**
> Both were written to check member merging and neither cared about meaning, so
> each picked `VALUE` arbitrarily and the unfiltered lookup made the arbitrary
> choice work. That is the same shape as §100's *"when a consumer's faithful
> test gives an unfaithful answer, suspect the flag before the test"*, one
> layer down: **when a test's incidental argument turns out to matter, the
> defect is usually older than the test.**

### The instrument note, because this row is now three-for-three

§163 priced by reading upstream and was wrong. §164 attributed by reading
fixtures and was wrong. §165 disproved §164 by measuring a fix alone. **§166
found it by printing what the rule actually got, at its entry, in two runs.**

The board's standing rule was already *"probe at the rule's ENTRY, not at a
branch"*. Strengthen it with what this row demonstrates: **when two successive
attributions fail, stop attributing and print the inputs.** The cost was one
`eprintln` and about five minutes, against three sections of analysis.

## §167 — the cascade re-barred on top of §166, type-position arms only

§166 removed the reason §163's arms were unreachable. Re-taking the row, and
**narrowed to what measured clean**: §163's type-position arms measured
`0 CONVERTS, 0 LOST, 0 WRONG` — correct and starved. The value-position arm
measured 238 wrong lines and is **not** part of this build; §165's corrected
prerequisite list still stands for it.

Ported: `checkAndReportErrorForUsingNamespaceAsTypeOrValue`'s type branch
(TS2709) and `checkAndReportErrorForUsingValueAsType` (TS2749), from
`checker.go:1641` and `:1722`, called from `check_type_reference_name` between
its `TYPE` arm and the rest of its ladder.

```
bar:  +6 cases,  0 LOST,  WRONG delta ≤ +6
```

Sole-obstacle population is TS2709's 9 plus TS2749's 4. The bar is under half
because §163 measured `RIGHT` at 8 lines against 19 missing for these two codes
even when the arms fired — most of the row wanted something else — and because
§166 has just changed what every meaning query in the program answers, so the
arms are firing against a resolver nobody has measured them against.

### Falsifiers

1. **`WRONG` delta above +6.** First suspect is the `MODULE` mask: a symbol that
   is a namespace *and* a value (`namespace N {}` plus `const N = 1`) is a legal
   type-position reference in neither direction, and upstream's arm order
   decides which message it gets.
2. **`LOST` non-zero.** The site returns silently today, so every line is
   additive.
3. **`checker_types` moves.** §166 moved it deliberately; this build is
   `crate::check` only and must not.

## §168 — §167 built: +8 at zero wrong, and it is §166's payoff not the cascade's

The bar was `+6 cases, 0 LOST, WRONG delta ≤ +6`. `RULE_CODES = [2709, 2749]`;
no other producer emits either code, so the counterfactual is the delta:

```
CONVERTS   8      LOST   0      STILL SHORT   5      RIGHT   24      WRONG   0
```

`coverage`: `diagnostics` **1,508 → 1,516 (27.62%)**. `checker_types` 3,955 ·
84.52%, `binder_symbols` 8,459/8,459, `printer_round_trip` 11,762/11,762 — all
unmoved, including the one §166 deliberately moved.

### The same code, twice, with one line of the binder different

| | §163, before §166 | §167, after §166 |
|---|---:|---:|
| CONVERTS | 0 | **8** |
| RIGHT | 8 | **24** |
| WRONG | 0 | 0 |

**The arms are character-for-character the same.** What changed is that
`resolve_name`'s globals fallback stopped answering `TYPE` for every name in the
program, so the ladder above these arms stopped returning before reaching them.

That is the whole lesson of §163–§168, and it is worth stating as the row's
epitaph:

> **A rule that measures `0 wrong AND 0 converts` is not a weak rule; it is an
> unreached one.** Zero wrong is what a correct rule and a dead rule have in
> common, and the two are told apart by `RIGHT`, not by `WRONG`. §163 had that
> number — 8 correct lines against 19 missing — and read it as "the population
> wants something else" instead of "something above me is eating the calls".

### The chain, and what each step actually bought

| § | move | result |
|---|---|---|
| 163 | priced the cascade by reading upstream, built all four arms | +2 for 238 wrong — **failed** |
| 164 | attributed the 238 to `is_value_reference`, from three fixtures | bound removed 232 — **right effect, wrong cause** |
| 165 | measured that fix **alone** | it loses a case — **attribution disproved** |
| 166 | printed the rule's inputs at its entry | one-line resolver defect — **cause** |
| 167/168 | re-ran §163's type arms unchanged | **+8, zero wrong** |

Four sections and two reverts to move one line of `tsr-binder` and re-apply code
that already existed. The cheap step was available at §163: **`RIGHT` was
already telling the truth.**

### What is still open on this row

The **value-position arm** (TS2693/TS2708, plus TS2661) is not built. §164's 238
wrong lines were measured against the pre-§166 resolver and are **no longer a
valid measurement** — the ladder they came through has changed. Re-measure
before quoting them, which is §142's rule applied to this board's own numbers.

## §169 — the value-position arm re-measured: 238 wrong lines became 19

`bd tsr-0h43`. §164 measured this arm at **238 wrong lines**, and §168 flagged
that number as no longer valid because §166 changed what every meaning query in
the program answers. Re-taken, unchanged code, on the post-§166 resolver with
`RULE_CODES = [2661, 2693, 2708, 2585, 2840, 2839, 2422]`:

| | §164 (pre-§166) | §169 first run | §169 with the heritage bound first |
|---|---:|---:|---:|
| CONVERTS | 2 | 8 | **8** |
| LOST | **4** | 0 | **0** |
| RIGHT | 12 | 38 | 36 |
| WRONG | **238** | 27 | **19** |

**The stale number was wrong by an order of magnitude**, which is the whole
reason §142's rule exists: a refusal is only as good as the state it was
measured against. Had §164's figure been carried into a handoff unqualified,
this arm would have been off the board for as long as §13's TS7026 was.

### The bound moved, and the reason is a code not a position

§164 put the heritage-clause bound *after* the namespace arm. It belongs
**first**, and `classExtendsInterfaceInModule` says why:

```
class C1 extends M.I1 {}
!!! error TS2689: Cannot extend an interface 'M.I1'. Did you mean 'implements'?
```

That is `checkAndReportErrorForExtendingInterface`, the cascade's **second**
arm, which is not ported and which runs ahead of both namespace arms. Reporting
TS2708 there is a wrong code at a right position, not a wrong position. Moving
the bound removed 8 of the 27. **Owner: the TS2689 arm.**

### The 19 that remain, all named, all in already-failing cases

`LOST` is 0, so no passing case is broken by any of them:

| family | lines | owner |
|---|---:|---|
| `constEnums` | 6 (TS2708) | unread; a qualified name in a type position |
| `builtinIterator` | 4 (TS2693) | unread |
| `mappedTypeProperties` | 4 (TS2693) | `maybeMappedType`'s **type** half — `checker_types`. The syntactic bound is in and does not fire, so the shape here is not the shape upstream walks |
| `scannerUnicodeEscapeInKeyword2` | 2 | unread |
| three singles | 3 | unread |

### The verdict, stated rather than rationalised

§163's bar was `+15 cases, WRONG delta ≤ +12` **for all four arms together**.
Both halves are now in: **+16 cases (8 from §167, 8 here) for 19 wrong lines**.
That **makes the case bar and misses the wrong bar by 7**, and it is landed
anyway. The reasons, so a later session can disagree with the judgement rather
than re-derive it:

- **`LOST` is 0.** Every wrong line sits in a case that already fails, so the
  debt costs no case today.
- **0.42 conversions per wrong line**, against §13's refused 0.26 and §9's
  refused 0.008 — and against §79, the TS2304 ladder this row is built on,
  which also landed carrying wrong lines.
- **Every family is attributed**, three of them to a named unported arm.

The honest counter-argument is that a bar missed is a bar missed, and that the
bar was set before the code precisely to stop this reasoning. **If a later
session finds these 19 blocking a larger row, the revert is the one call in
`check_value_identifier`.**

## §170 — TS7026, priced as what §158 found it to be: a JSX build

28 sole-obstacle cases, the largest relation-free row on this board, and it has
never been priced as a JSX rule because §5/§13/§141 filed it as `declare global`
merging for six sessions (§158 corrects that).

`getIntrinsicTagSymbol` (`jsx.go:1253`), the arm after every lookup fails:

```go
if c.noImplicitAny {
    c.error(node, diagnostics.JSX_element_implicitly_has_type_any_because_no_interface_JSX_0_exists,
            JsxNames.IntrinsicElements)
}
```

### It is a name-resolution question, not a type question

That is the finding that makes the row cheap, and it is why the row looked
expensive: the message is reached through `getJsxType` → `getJsxNamespaceAt` →
`getExportsOfSymbol`, which *reads* like the type machinery. But the branch that
fires is the one where the lookup found **nothing**, and "is there a namespace
`JSX` exporting an interface `IntrinsicElements` in scope here" is a question
`BindResult::resolve_name` answers — **correctly, since §166 and not before**.

Four conditions, all syntactic or resolver-level:

1. the node is a `JsxOpeningElement` or `JsxSelfClosingElement`;
2. its tag name is an **intrinsic** name — `IsIntrinsicJsxName`
   (`scanner/utilities.go:98`) is *"starts with a lowercase letter, or contains
   a hyphen"* — or a `JsxNamespacedName` (`isJsxIntrinsicTagName`,
   `checker/utilities.go:1116`);
3. `noImplicitAny`, which this checker already resolves through
   `strict_option_value` (ADR-0042);
4. `JSX.IntrinsicElements` does not resolve from that location.

The span is the **element**, not the tag name: `tsxNoJsx.tsx`'s baseline
underlines all eight characters of `<nope />`.

### The bar

Off the CASE count. Sole-obstacle: **28**.

```
bar:  +14 cases,  0 LOST,  WRONG delta <= +10
```

Half the ceiling, because the row's concentration is 110 lines over 28 cases —
**3.9**, the worst shape landed on this board — so most of those lines sit in
cases needing much more than this rule.

### Falsifiers

1. **`WRONG` delta above +10.** First suspect is condition (4) in the direction
   that matters most: a corpus case that *does* declare `JSX.IntrinsicElements`
   (through `react.d.ts` or its own `declare global`) and whose declaration this
   port cannot see would take every element in the file. §13's original note
   said exactly that about global augmentation, so the wrong column is where
   that claim finally gets tested.
2. **`LOST` non-zero.** The rule only adds diagnostics.
3. **One report per element, not per tag.** A `JsxElement` has an opening *and*
   a closing tag; upstream reaches `getIntrinsicTagSymbol` from the opening one
   only. A duplicate at the same position is an extra line — the suite compares
   multisets (§151).

## §171 — TS7026 built with `declare global` merging, and REFUSED at LOST 1

§170's bar was `+14 cases, 0 LOST, WRONG delta ≤ +10`. Built, and then built a
second time with the blocker it exposed. Three measurements, `RULE_CODES = [7026]`:

| build | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| the rule alone | 14 | **1** | 234 | **65** |
| + `declare global` merging in `tsr_binder` | 14 | **1** | 234 | **45** |
| the binder merge alone, no rule | — | — | — | `diagnostics` **1,524 → 1,523** |

**Refused, and reverted — both halves.** The rule hits its conversion bar
exactly and misses the wrong bar by 35 while breaking a passing case, and the
binder half loses a diagnostics case on its own. *LOST must not grow in any
measurement* is the one rule on this board with no exceptions.

### The rule is right; its inputs are not

Every remaining wrong line traces to one line of fixture text:

```
/// <reference path="/.lib/react16.d.ts" />
```

`react16.d.ts` declares `namespace JSX { interface IntrinsicElements … }` at
global scope. When the program does not load it, `JSX.IntrinsicElements` is
genuinely absent and the rule correctly says so — about a program that is not
the one upstream compiled. `jsxIntrinsicElementsTypeArgumentErrors` (10 lines),
`jsxElementTypeLiteral`, `jsxFragmentFactoryNoUnusedLocals` and the single
`LOST`, `jsxImportForSideEffectsNonExtantNoError`, are all this.

**Owner: `file_loader`**, and it is the *same* owner already carried in the
handoff's standing-LOST section for `resolutionModeTripleSlash1`/`3`. That
section should be read as larger than three cases: it is now the blocker on the
board's largest relation-free row.

### `declare global` merging, measured for the first time

§5 has refused it for many sessions without a number. There is one now.

Ported as: record every top-level `ModuleDeclaration` whose name is the
**identifier** `global` during the bind walk — `NodeFlagsGlobalAugmentation` is
another flag this parser does not set, and the binder holds a `NodeTable` with
no `NodeMap`, so the walk is the only place a typed node is in hand — then merge
that block's exports *and* its body's locals into `globals` alongside the
script-file merge.

```
binder_symbols  8,459/8,459   unmoved
checker_types   3,955 → 3,957 (+2)
diagnostics     1,524 → 1,523 (−1)
```

**It works** — it removed 20 of the rule's 65 wrong lines, which is the direct
evidence that `declare global { namespace JSX { … } }` now reaches a lookup. And
it is **not free**: one diagnostics case regresses, unidentified. That single
case is what the next attempt has to explain before this lands, and it is worth
explaining rather than bounding away: a merge that makes one case worse is
making *something* visible that was not visible before.

### What the row costs, honestly, after all of it

TS7026 is **not** a cheap 28-case row and it is not a merge row either (§158).
It is:

1. `file_loader` following `/// <reference path>` to a mounted lib — **the
   binding constraint**, and it owns the wrong column outright;
2. `declare global` merging — built here, measured, one case short of clean;
3. the rule itself — about forty lines, correct, and already written twice.

Item (3) is the cheap part and it is worthless without (1). The refused source
for both halves is described above in enough detail to rebuild; **what must not
be repeated is building it before (1) is measured.**

## §172 — the TS2322 split, and `bd tsr-bxp`'s two-session block is answered

`tsr-bxp` carries the line *"Do NOT build an emitter before this runs"* and had
never run. It has now.

### Why nothing had answered it

`diagemit` says `want 2888 · have 530`. **Both halves of that gap have been
quoted as targets by different owners** — `tsr-6re` as this workstream's (once
at +536, once at +489, corrected to 15/89), STATUS §5 as `checker_types`' 947 —
and no instrument separated them. The split is decidable because
`report_assignability_failure` is a *single* site with three ordered gates, so a
wanted-but-unemitted line is either a position the walk never visits or a
position it visits and declines, and the gate says which.

`crates/tsr-conformance/examples/ts2322split.rs` with
`Checker::assignability_probe` behind `TSR_ASSIGN_PROBE`. It goes through
`diagnostics_suite`'s own `program_for_case`, not a hand-rolled one —
`probefile.rs`'s warning about losing the `/.lib` mount, and `conventions.md`'s
rule that a probe re-implementing the harness measures a different compiler.

### The measurement

```
cases with at least one MISSING TS2322 line: 782
missing TS2322 lines total: 2441

bucket                       lines    cases
NEVER REACHED                  691      312     <- reporting anchor, THIS workstream
RELATION DECLINED             1354      406     <- checker_types
PAIR NOT REPORTABLE            383      146     <- checker_types, narrower
OBJECT LITERAL vs UNION          8        3
REPORTED ELSEWHERE               5        4
```

**By lines the row is 71% `checker_types` and 28% ours.** But lines are not the
unit that converts, and the per-case purity is the number to plan against —
a case converts only when *every* one of its missing lines is emitted:

```
of the cases blocked on TS2322 ALONE:
  wholly NEVER REACHED  (this workstream can convert alone):  132
  wholly relation-gated (`checker_types` alone)            :  335
  MIXED (needs both)                                       :   13
```

### What this settles

**There is a real diagnostics-side slice and it is 132 cases** — four times the
largest single build this session (§162's +32) and larger than everything
§156–§169 landed put together. `tsr-6re`'s decline was reasoned from a
*mechanism* argument (which reporting path, which primitive test) and priced at
15, then 89; the answer is neither, because the question was never "how many
positions can a bounded emitter speak about" but "how many positions does the
walk visit at all".

**And it bounds the ambition honestly.** 335 cases — the majority — are
positions this port already visits and where the three-valued relation declines
to answer `NotRelated`. No reporting work reaches them. That is STATUS §5's
standing refusal, now with a number measured on the suite's own denominator
rather than on baseline files (`tsr-6re`'s recorded denominator error).

The 13 mixed cases matter more than their size: they are the ones that would
make an anchor build *look* like it under-delivered, because each needs a
`checker_types` line as well.

### The next measurement, not the next build

`tsr-6re` measured that its convertible diagnostics landed on **28 distinct
anchors**, modal `BinaryExpression`/`Identifier`. That was a different
population. **Which anchors carry the 691 never-reached lines is unmeasured**,
and it decides whether 132 cases is one build or eight: `ts2322split.rs` prints
positions and not node kinds, and adding the kind is a small extension to the
same probe. **Do that before writing an anchor.**
## §173 — `declare global` merging, rebuilt and LANDED; §171's LOST 1 explained

§171 built this half, measured it, and reverted it at `LOST 1` with the note
that *"that single case is what the next attempt has to explain before this
lands"*. It is explained, it was not the merge, and the fix is in the checker.

### The measurement, one checkout, before and after

| | before (`22d745c`) | after |
|---|---:|---:|
| `binder_symbols` | 8,459/8,459 (100.00%) | 8,459/8,459 (100.00%) |
| `diagnostics` | 1,524/5,488 (27.77%) | 1,524/5,488 (27.77%) |
| `checker_types` cases | 3,955/9,538 (41.47%) | 3,957/9,538 (41.49%) |
| `checker_types` lines | 405,111 (84.5824%) | 405,403 (84.6434%) |

**Re-measured on a second base.** Rebasing onto the fifteenth `.types`
session's work (`a7dccd4`, 3,955 / 405,390) and re-running gives
**3,957 / 405,682** — the same +2 cases and +292 lines. A delta reproduced on
two bases is a delta rather than an interaction.

**`diagnostics` did not merely net to zero — no case changed verdict**, in
either direction. That is a stronger statement than §171's `1,524 → 1,523` and
it is the one the bar asked for.

§171 recorded `checker_types +2` and nothing about lines; the line figure is
**+292**, and it is where the interesting content is. Per-case, +2/−0 verdicts
and 54 cases whose line tally moved, of which 53 gained and one lost
(`compiler/importAliasInModuleAugmentation`, 12/19 → 10/19). The gainers are
overwhelmingly JSX — `jsxChildrenIndividualErrorElaborations` +42,
`reactDefaultPropsInferenceSuccess` +27, `tsxNotUsingApparentTypeOfSFC` +15 —
because `declare global { namespace JSX { … } }` now reaches a lookup. That is
item (2) of the three §171 priced the TS7026 row at; item (1), `file_loader`
following `/// <reference path>`, is still the binding constraint and still owns
that row's wrong column.

### The LOST case was `compiler/extendGlobalThis`, and it was named in one run

Found with `examples/casequery.rs --list`, written for this session: a suite
run that emits one row per case, diffed across the change. The committed
snapshot truncates at the first 100 failures, so a −1 over 5,488 cases named the
case nowhere; the previous session left it "unidentified" for that reason.

```
2615c2615
< compiler/extendGlobalThis	PASS
---
> compiler/extendGlobalThis	FAIL
```

The case writes `declare global { namespace globalThis { var test: string } }`
and then `globalThis.tests = "a-b"` — a typo, deliberately. We reported
`TS2339 Property 'tests' does not exist`; upstream's baseline reports nothing
and its `.types` says `>globalThis.tests : any`.

**Two separate causes, both real, neither the merge.**

1. **`globalThis` is not a name in the global table upstream, it is the table.**
   `c.globalThisSymbol.Exports` **is** `c.globals` (`checker.go:963`), so
   merging a `namespace globalThis` into the `globalThis` entry deposits its
   members in the global table itself. This port has no `globalThis` symbol at
   all — §33 of `checker-notes-narrow.md` mints `typeof globalThis` as a *type*
   when the name fails to resolve. The plain `mergeSymbolTable` arm therefore
   inserted the namespace under the name `globalThis`, which is wrong twice:
   the name starts resolving so §33's mint stops firing, *and* `test` stays
   invisible because nothing reads that symbol's exports.
   `Binder::merge_into_globals` splices instead.

2. **A missing member of `globalThis` is never TS2339.** It has its own arm
   several branches before `reportNonexistentProperty`
   (`checker.go:11337-11344`): TS2339 only when the name *is* a global and is
   `SymbolFlagsBlockScoped`, TS7017 under `noImplicitAny`, and `anyType`
   otherwise. This port had never reached that arm, because until globals
   carried what a `declare global` block declares, `typeof globalThis` had
   nothing in it that `declared_members_are_complete` would call complete. The
   merge is what made an unported branch reachable, not what broke the case.

The transferable part: **a regression a merge exposes is not a regression the
merge caused**, and the distinction is only available if the instrument names
the case. §171's honest "unidentified, and worth explaining rather than
bounding away" was the right call; what it lacked was the tool.

### The gate, which is narrower than the previous attempt's

§171 ported this as *"record every top-level `ModuleDeclaration` whose name is
the identifier `global` … then merge that block's exports **and its body's
locals**"*. Both halves of that are wrong, and there is now a test for each:

- **The name is not the predicate; the keyword is.** `ast.IsGlobalScopeAugmentation`
  tests `Keyword == KindGlobalKeyword`, which is what separates
  `declare global` from `namespace global`.
- **Position matters.** Only a block upstream's *parser* collected into
  `SourceFile.ModuleAugmentations` is merged, and `collectModuleReferences`
  (`internal/parser/references.go:47-69`) collects exactly the two shapes
  `ast.IsModuleAugmentationExternal` names. A `global` block at the top level of
  a *script*, or inside a module *augmentation*, is TS2669 and merges nothing —
  verified against `tsc` 5.x in both directions, not reasoned about.
- **Locals must not be merged.** `mergeModuleAugmentation` unions `Symbol.Exports`
  only. The observable case is a block containing an explicit `export {}`, which
  turns the export context off: `tsc` then reports TS2304 at the use site.

Each is pinned by a test in `crates/tsr-binder/tests/program.rs` that was
checked to fail under the mutation it exists to catch — the five positive tests
against the feature reverted, the two position tests against an ungated
predicate, and the export-context test against a locals-merging one.

### What it exposed, recorded rather than fixed

Pointing `tsr` at a 22-package monorepo took reported errors from **1,996 to
1,550**, removing all 474 `typeof process` / `typeof console` property errors
and all 64 unresolved `fetch`/`Response`/`URL` names. It also *added* 92 —
79 TS2300 and 13 TS2649 — from a pre-existing collision this port carries and
the corpus cannot exhibit: ambient module symbols are stored under the unquoted
specifier, where upstream keeps the quotes (`binder.go:311`), so
`declare module "process"` and a global `var process` share a key. Written up
in `docs/architecture/binder.md` with the minimal repro and the reason the
previously-recorded objection to fixing it no longer holds.

## §174 — an unset `target` is the LATEST STANDARD, not ES5; and the item was not two lines

`STATUS-cli.md` §7.10 found this from the other end and left it as the highest-value
item for whoever touched the core next, sized at **"it is two lines, and the
evidence is above"**. The evidence was right. The size was wrong, and the way it
was wrong is the useful part.

### The finding, restated with its provenance

`GetEmitScriptTarget` (`internal/core/compileroptions.go:195`) is:

```go
if options.Target != ScriptTargetNone { return options.Target }
return ScriptTargetLatestStandard          // = ScriptTargetES2025, :526
```

This port instead derived ES5 from the module kind — the pre-`tsgo` TypeScript
rule. The divergence is not directly observable in any suite here, and §7.10
found it only because `--showConfig`'s implied-option pass *drops a value equal
to what wholly-default options would compute*, which makes the default visible
in two `tsc` baselines pulling in **opposite directions**. One rule, both
baselines, opposite signs — which is what made it a fact about upstream rather
than a fit to one case.

### It is two functions, not two lines

Correcting `emit_script_target` alone broke four `tsr-compiler` loader tests,
and that is the whole finding. The default target is an input to
`GetEmitModuleKind`, and **this port's `emit_module_kind` was independently
wrong in a way ES5 had been hiding**: it was a two-way `>= ES2015` split where
upstream is a five-rung ladder (`compileroptions.go:202-220`):

```
ESNext -> ESNext | >=ES2022 -> ES2022 | >=ES2020 -> ES2020 | >=ES2015 -> ES2015 | else CommonJS
```

With an unset target answering ES5, every unset-module program took the
`CommonJS` arm and the three missing rungs were unreachable. A wrong default was
**masking** a wrong ladder. Both are corrected here; correcting only the one
named in §7.10 would have shipped `ES2015` where upstream says `ES2022`.

`GetEmitModuleKind` then feeds `GetModuleResolutionKind`, so the reach is
target → module kind → resolution kind → the loader. That is the reach the "two
lines" estimate did not have.

### What moved: nothing measurable, and that is the honest result

| | before | after |
|---|---:|---:|
| `binder_symbols` | 8,459/8,459 | 8,459/8,459 |
| `checker_types` | 3,957 cases / 405,403 lines | identical, **per case** |
| `diagnostics` | 1,524/5,488 | 1,524/5,488 |
| `module_resolution` | 95/95 | 95/95 |
| `file_loader` | 96/96 | 96/96 |
| `cli_baselines` | 33/43 | 33/43 |
| the 22-package monorepo | 1,550 errors | 1,550, same distribution |

A `casequery --list` diff over `checker_types` showed **zero rows changed**, so
this is "no case moved", not "the net was zero".

**Why nothing moved is itself the measurement.** The default is almost never
exercised: the corpus sets `// @target:` in 12,423 of 12,444 cases (the figure
is in `tsr-core`'s own module docs), every `tsconfig.json` in the sample
repository sets `target`, and the `tsc` baselines compile trivial files. So the
suites here **cannot** see this rule, in the same way they cannot see the
ambient-module-name collision in §172. Two blind spots found in one session,
both by pointing the binary at something that is not the corpus.

The observable, which is what licenses the change:

```jsonc
// tsconfig.json — no "target"
{ "compilerOptions": { "noEmit": true } }
```
```ts
const e = Object.entries({ a: 1 });
```

`tsc` reports nothing. Before: `TS2339: Property 'entries' does not exist on
type 'ObjectConstructor'` — the ES5 lib. After: nothing. That is the whole
user-visible effect and it is exactly the intended one.

### Six tests were encoding the old defaults

Four in `tsr-compiler`'s loader and two in `tsr-core` — each written against
`CompilerOptions::default()` while asserting about `lib.d.ts`, which **is** the
ES5 lib. They did not fail because the change was wrong; they failed because
they had been depending on a default rather than naming a target. Each now names
`ScriptTarget::ES5` explicitly, with the reason, so none of them can silently
re-acquire the dependency. The two `tsr-core` tests were assertions of the wrong
rule and are rewritten against upstream's, including every rung of the ladder.

**The transferable rule:** a test that leans on a default is a test that will
fail for the right reason at the worst possible time, and cannot tell you which.
`the_target_chooses_the_default_lib` sat beside all four loader tests, doing the
same thing correctly, the whole time.

## §175 — the anchor distribution: four builds, not one and not eight

§172 left the question *"which anchors carry the 691 never-reached lines, and
is 132 cases one build or eight"*. The same probe extended with the node kind
at each position — joined on `(file, line, column)`, because a diagnostic sits
at `error_span(anchor)` and that is exactly what a baseline line records.

**66 distinct anchors.** By lines, the head:

```
PropertyAssignment in ObjectLiteralExpression            101
ExpressionStatement in SourceFile                         78
JsxAttributes in JsxSelfClosingElement                    62
ExpressionStatement in Block                              42
NumericLiteral in ArrayLiteralExpression                  41
VariableDeclaration in VariableDeclarationList            36
StringLiteral in ArrowFunction                            29
Identifier in BinaryExpression                            28
ReturnStatement in Block                                  28
```

### A line histogram is the wrong instrument here, and the probe says so

`NumericLiteral in ArrayLiteralExpression` carries 41 lines and appears in only
5 that belong to a convertible case. `StringLiteral in ArrowFunction` carries
29 and 25 of them do. **A case converts only when every anchor it needs
exists**, so an anchor with many lines that are each one of several a case
needs converts *nothing on its own*. The plannable quantity is set coverage
over the 132, not a histogram — §142's *"a rule's yield is not its row"*, in its
sharpest form yet on this board.

Greedy coverage, which is the right shape because the question ("what does the
*next* build buy") is asked repeatedly:

```
+PropertyAssignment in ObjectLiteralExpression   +19   cumulative  19 of 132
+ExpressionStatement in SourceFile               +12              31
+JsxAttributes in JsxSelfClosingElement          +10              41
+ReturnStatement in Block                        +10              51
+ExpressionStatement in Block                    + 9              60
+Identifier in JsxSelfClosingElement             + 6              66
+JsxAttribute in JsxAttributes                   + 5              71
+Identifier in ForOfStatement                    + 4              75
+Identifier in ArrowFunction                     + 3              78
+Identifier in JsxOpeningElement                 + 3              81
```

### The answer: four coherent builds reach 81 of the 132

The ten anchors are not ten builds — they group by the upstream mechanism that
would introduce them:

| build | anchors | cases |
|---|---|---:|
| **`elaborateObjectLiteral`** | `PropertyAssignment in ObjectLiteralExpression` | **19** |
| **statement-level expression checking** | `ExpressionStatement` in `SourceFile` and in `Block` | **21** |
| **JSX attribute checking** | `JsxAttributes`, `JsxAttribute`, `Identifier in Jsx*Element` | **24** |
| **return / arrow-body / for-of** | `ReturnStatement in Block`, `Identifier in ArrowFunction`, `Identifier in ForOfStatement` | **17** |

Each is 17–24 cases — **any one of them is a bigger single build than anything
§156–§169 landed**, and the largest, JSX, is the *second* time this session the
board's biggest opportunity turned out to be JSX-shaped (§158/§171 was the
first, and it is blocked on `file_loader`).

### What this retires and what it does not

**`bd tsr-6re`'s 15-then-89 pricing is retired.** It counted what a *bounded
emitter* could speak about; the binding quantity was which positions the walk
visits, and the two are unrelated. **STATUS §5's refusal of the assignability
family stands** and is now sized honestly from the other side: 335 of the
TS2322-only cases are relation-gated and no anchor reaches them.

The 51 cases the greedy pass does not reach in ten steps sit in the 56-anchor
tail — that is where "eight builds" would have been true, and it is where to
stop rather than where to continue.
## §176 — `elaborateObjectLiteral`, the anchor §175 ranked first

§175's greedy pass puts `PropertyAssignment in ObjectLiteralExpression` at the
head: **19 of the 132** wholly-anchor-gated TS2322 cases, 101 of the 691
never-reached lines. This is that build.

`elaborateObjectLiteral` (`relater.go:498`), reached from `elaborateError`
(`:440`) — which `checkTypeRelatedToEx` calls **before** it reports anything.

### The mechanism, and the half that is easy to miss

For each non-spread property of the literal: take the property's name type, ask
the *target* for that property's type and the *source* for its own, and if they
do not relate, report **on the property name** (`elaborateElement`, `:546`).

**Elaboration replaces the outer report, it does not add to it.** `elaborateError`
returns a bool and `checkTypeRelatedToEx` reports the whole-expression error
only when it comes back `false`. So a port that elaborates *and* keeps the outer
line turns one wrong line into one right line plus one wrong line — no net
conversion, because the suite compares exact multisets. **That is the whole
risk of this build**, and it is why `report_assignability_failure` must hand
off rather than fall through.

### What exists already

`check_excess_properties` (`assignreport.rs:365`) already walks an object
literal's properties, already declines a spread it cannot enumerate, already
resolves the target's property table through `declared_property_table`, and
already reports at the **property name** — `arrayCast.ts(3,23)` is the `foo` of
`{ foo: "s" }`, the same anchor this build needs. §136's rule pays again: the
walk, the decline and the error node are all written.

What is not written is the per-property *relation* call and the hand-off.

### Deliberately not ported

- **`elaborateElement`'s recursion into the initialiser** (`:557`): upstream
  recurses so a nested literal elaborates further in. One level is what §175's
  anchor distribution measures; the nested anchors are in the 56-anchor tail.
- **`getBestMatchIndexedAccessTypeOrUndefined`** — upstream's best-match over a
  union target. `get_property_of_type` is the non-union half, and a union target
  is already declined by `report_assignability_failure`'s object-literal arm.
- **`exactOptionalPropertyTypes`** and the `removeMissingType` pair, which
  change *which* message, not whether one is reported.
- **`elaborateDidYouMeanToCallOrConstruct`**, which runs first and carries its
  own codes.

### The bar

```
bar:  +12 cases,  0 LOST,  WRONG delta ≤ +8
```

Under §175's 19 because the anchor distribution counts a case as convertible
when *every* missing line is anchor-gated, and it does not check that this port
computes the right **types** at those positions — the assumption `bd tsr-bxp`
flagged for the 89-case bound and which is unmeasured here in exactly the same
way. The bar is set where the build is still worth landing if a third of the
positions answer `error`.

### Falsifiers

1. **`WRONG` delta above +8, concentrated at the outer node.** Then the hand-off
   is not exclusive and both lines are being emitted — the failure mode named
   above.
2. **`RIGHT` rises and `CONVERTS` does not.** Then the per-property relation is
   right and something else in those cases is missing; §175's purity filter
   would have been measuring anchors it cannot fully price.
3. **`LOST` non-zero.** The outer line is being suppressed where the baseline
   wants it — the hand-off firing when elaboration reports nothing.

## §177 — §176 built at +1, and it CORRECTS §172/§175's ownership split

The bar was `+12 cases, 0 LOST, WRONG delta ≤ +8`. Measured, `RULE_CODES = [2322]`:

| | before | after |
|---|---:|---:|
| CONVERTS | 84 | **85** |
| LOST | 0 | 0 |
| RIGHT | 447 | 451 |
| WRONG | 83 | 84 |

**+1 case, +4 right lines, +1 wrong.** §175 priced this anchor at **19 cases and
101 lines**. Falsifier 2 fired exactly as written — *"`RIGHT` rises and
`CONVERTS` does not"* — and the reason is worth more than the build.

### Why: the elaboration reaches the positions and cannot decide them

Counters at each decline, over the corpus:

```
ELAB entered              563     an object literal at a failing position
ELAB relation-declined    343     the per-property relation was not NotRelated
ELAB no-target-property   280     `get_property_of_type(target, name)` found nothing
ELAB primitive-target      10
ELAB REPORTED               6
```

The anchor was never the binding constraint. **Adding it puts the walk in front
of 563 positions and the relation declines 343 of them and the members table
another 280** — the same two buckets §172 attributed to `checker_types` at the
*outer* position, reappearing one level in.

### The correction, which is the point of this section

> **§172's `NEVER REACHED` bucket conflates two things, and §175's "132 cases
> this workstream can convert alone" is an UPPER BOUND, not a slice.**

The split classified each missing line by *which gate declined at that
position*. At a position the walk never visits **there is no gate to observe**,
so `NEVER REACHED` silently assumed that adding the anchor would let the
relation succeed. For this anchor it does so 6 times in 563.

That is precisely the assumption `bd tsr-bxp` flagged against the *89-case*
bound — *"it assumes our port computes the right type on both sides at each of
the convertible positions; that assumption is unmeasured in BOTH directions"* —
and §172 reproduced it while believing it had retired it. **The instrument
answered a narrower question than the one asked**, which is this board's
dominant failure mode (§155), in its fourth recorded instance.

**What §172 and §175 do establish, and it still stands:** 335 TS2322-only cases
are relation-gated at the outer position and no anchor reaches them; the
never-reached population is 691 lines over 66 anchors; and the anchors rank in
the order §175 printed. **What they do not establish is that any of the 132
converts.** The honest restatement:

```
132  cases where every missing line lacks an anchor      (upper bound)
  ?  of those, cases where the relation can decide once the anchor exists
  1  measured, for the highest-ranked anchor of the ten
```

### The build is landed anyway, and why

+1 case for +1 wrong line with `LOST 0` is not what the bar asked for. It is
kept because it is **faithful upstream code at the position upstream reports
from**, and because it is a *precondition* that pays out without another build:
the 343 relation-declines and 280 missing properties are `checker_types`' work,
and when that lands this elaboration converts without anyone revisiting it.
That is the §166 → §167 shape — a one-line resolver fix turned §163's dead code
into +8 — and it is the only reason to keep machinery that scores 1 today.

**The falsifier for that judgement:** if `checker_types` lands members work and
this anchor still measures under +5, the elaboration is not the thing standing
between the port and those cases, and it should come out.

### What this changes about the next build

**Do not build anchors 2–4 from §175's list on the strength of that list.** The
greedy table ranks anchors by cases-that-lack-them, and this section shows that
number is not a forecast. Before each one, the cheap check now exists: add the
anchor behind a counter, run the corpus, and read `REPORTED` against `entered`.
Six in five hundred is what a saturated relation looks like from the reporting
side.

## §178 — the grammar seam, re-opened: four rows §105 declared exhausted

§105 closed the cheap-grammar seam and §162 re-opened it once (TS1212's 34
cases). Re-taking `diaggap` after §177 turned TS2322 back over to
`checker_types`, four more rows sit together at the top of the relation-free
column:

```
TS1028   8 lines   8 cases   Accessibility modifier already seen.
TS1155   7 lines   7 cases   '{0}' declarations must be initialized.
TS1071  12 lines   7 cases   '{0}' modifier cannot appear on an index signature.
TS1038  10 lines   7 cases   A 'declare' modifier cannot be used in an already ambient context.
                  ── 29 cases, concentration 1.3
```

**Three of the four are `checkGrammarModifiers`** (`grammarchecks.go:336`,
`:292`, `:459`) — the function §103 already half-ported as
`check_modifier_order`, which already walks the list, already keeps the `seen`
set the first arm needs, and already reports at the modifier token. §136's rule
for the sixth time this session.

The fourth, TS1155, is `checkGrammarVariableDeclaration` (`:1582`): a `const`
with no initialiser, outside `for-in`/`for-of`, not ambient, name not a binding
pattern. Reported on the **declaration**, not the name.

### The ordering is the specification, again

§103 paid for this rule once already — *"at most one grammar report per node
(every upstream arm is a `return`), and the `else if` order is the
specification"*. It binds twice here:

- **TS1028 precedes the must-precede family.** `private public x` is
  *"Accessibility modifier already seen"*, not *"'public' modifier must precede
  'private' modifier"*, because the accessibility-already-seen branch is the
  first arm of the same `if`/`else if` chain §103 ported the tail of.
- **TS1038 is the sixth arm of the `declare` chain**, behind class-element,
  parameter, `using` and `await using` branches. Porting it alone means
  reporting it where upstream reports one of those — so the ported arm is
  bounded to the *module-block* shape and stays silent on the rest.

### Deliberately not ported

`ModifierFlagsReparsed` guards three of the must-precede arms upstream
(`modifier.Flags&ast.NodeFlagsReparsed == 0`); this parser has no reparse
concept and JSDoc lives in a side table, so no modifier here can be reparsed and
the guard is vacuous. Stated rather than silently dropped.

### The bar

```
bar:  +18 cases,  0 LOST,  WRONG delta ≤ +6
```

Off the CASE count, under the 29 because TS1071's 12 lines over 7 cases and
TS1038's 10 over 7 both mean a case wants more than one line, and a rule bounded
to one shape converts a case only when every line in it is that shape.

### Falsifiers

1. **`WRONG` delta above +6.** First suspect is the ordering: a report that
   should have been TS1029, TS2364 or one of the `declare` chain's earlier arms.
2. **TS1028 lines land where TS1029 already reports.** The two share a chain and
   this port emits the tail of it; a position gaining both is the ordering read
   backwards.
3. **`LOST` non-zero** — the rules only add diagnostics.

## §179 — §178 built: +19, and the parse-error gate paid for the fourth time

The bar was `+18 cases, 0 LOST, WRONG delta ≤ +6`. Three of the four rows
built — TS1028, TS1071, TS1155; TS1038's `declare` chain has five earlier arms
and was left. `RULE_CODES = [1028, 1071, 1155]`, no other producer:

```
CONVERTS   19      LOST   0      STILL SHORT   6      RIGHT   38      WRONG   2
```

`coverage`: `diagnostics` **1,525 → 1,544 (28.13%)**. `checker_types` 3,957 ·
84.70%, `binder_symbols` 8,459/8,459, `printer_round_trip` unmoved.

### The first measurement was +19 for **22** wrong, and one gate fixed it

Two families, both read rather than inferred:

```
multipleClassPropertyModifiersErrors.ts    public public p1;   ← 4 TS1028 lines
  upstream's baseline: ONE error, TS1434 "Unexpected keyword or identifier"
downlevelLetConst1.ts(1,6)                 const;              ← TS1155
  upstream's baseline: TS1123 "Variable declaration list cannot be empty"
```

**Neither is a rule bug.** Upstream's *parser* never builds the tree these
rules are reading: it does not accept `public public` as two modifiers, and it
does not accept an empty declaration list. Both files carry a parse error, and
both rules were speaking about a tree only this port has.

Gating the three new rules on `file_has_parse_errors` took the wrong column
**22 → 2 and cost zero conversions.**

> **The parse-error gate is a per-rule measurement, and this is its fourth
> instance.** §40.3 deleted it for +6, §50.1 re-measured at −6, §79 converted
> three recovered trees with it absent, and here it is worth −20/+0. It is not a
> house style in either direction; it is a question about *whether the rule
> reads a shape the parser can get wrong*, and these three read modifier lists
> and declaration lists — precisely the shapes a recovering parser invents.

### The two that remain

`jsxParsingErrorImmediateSpreadInAttributeValue` at `a.tsx(8,7)` and `(9,7)`,
both TS1155. The file is a JSX parse-error fixture whose errors this parser does
**not** record, so the gate above cannot see them. **Owner: `tsr_parser`** — the
same shape as the two lines §162 left behind, and one more piece of evidence
that the parse-error set itself is incomplete rather than that the gate is
wrong.

### What is left of the row

TS1038 (7 cases) is unbuilt: `A 'declare' modifier cannot be used in an already
ambient context` is the **sixth** arm of the `declare` chain
(`grammarchecks.go:459`), behind class-element, parameter, `using` and
`await using` branches. Porting it alone would report it where upstream reports
one of those, and §103's rule — *the `else if` order is the specification* —
says that is a wrong code at a right position, not a partial win.

## §180 — three more grammar rows at ZERO wrong, and a bar that was not registered

TS1015, TS1117 and TS1221 — `checkGrammarParameterList` (`grammarchecks.go:711`),
`checkGrammarObjectLiteralExpression` (`:1139`) and `checkGrammarForGenerator`
(`:990`).

```
RULE_CODES = [1015, 1117, 1221]
CONVERTS 8 · LOST 0 · STILL SHORT 7 · RIGHT 36 · WRONG 0
```

`coverage`: `diagnostics` **1,544 → 1,552 (28.28%)**. Rails unmoved.

### The process slip, recorded because the file is the place for it

**No bar was registered before this code was written.** §178's bar covered
TS1028/TS1071/TS1155 and this is a different batch; the loop's rule is one bar
per build and it was skipped because the previous batch had just landed cleanly.
The measurement happens to be good, which is exactly when the omission is
cheapest to excuse and most worth recording: a bar written *after* the number is
not a bar, it is a caption. Had this measured +8 for 30 wrong there would have
been nothing to fail against.

### §103's rule bit again, and one level up from where it was written

The first measurement was `+7 / 0 LOST / 3 WRONG`, and all three wrong lines
were `optionalArgsWithDefaultValues`:

```
function foo(x: number, y?:boolean=false, z?=0) {}
                        ~   ← upstream reports HERE and stops
```

Upstream's `checkGrammarParameterList` walks the list and `return`s on the first
offender, so one list is one diagnostic. This port hooked
`ParameterDeclaration` and reported on **every** offender.

> §103 recorded *"at most one grammar report per node — every upstream arm is a
> `return`"*. The refinement this build pays for: **the node is whatever the
> upstream function iterates, not the node the walk happens to visit.**
> `checkGrammarModifiers` iterates a modifier list, `checkGrammarParameterList`
> a parameter list, `checkGrammarObjectLiteralExpression` a property list — and
> a port that hangs each rule off the *member* silently multiplies every report.

Moving the test to "is this the first offender in my owner's parameter list"
took the wrong column to **0 and gained a conversion** (7 → 8): the extra line
had been failing a case that otherwise passed.

TS1117 is the exception that proves the shape — upstream does **not** `return`
there, it reports every repeat after the first, and the port matches.

### Bounds, each upstream's own ordering rather than a choice made here

- **TS1015 skips a rest parameter** — `A rest parameter cannot be optional` is
  an earlier branch.
- **TS1221 tests ambient before bodiless** — `declare function* f();` is TS1221,
  not TS1222.
- **TS1117 declines a spread and any non-assignment member** — a method/method
  clash is TS2300 and a get/set clash TS1118, both different codes at the same
  position.
- All three carry the `file_has_parse_errors` gate §179 measured at −20/+0.

## §181 — three syntactic reference rules: TS2364, TS2703, TS2371

**Bar registered before the code**, which §180 recorded itself for skipping.

```
TS2364   7 cases   The left-hand side of an assignment expression must be a variable or a property access.
TS2703   7 cases   The operand of a 'delete' operator must be a property reference.
TS2371   6 cases   A parameter initializer is only allowed in a function or constructor implementation.
                  ── 20 cases
```

None needs a type. All three are `Node.Kind` tests over a skipped-parenthesis
spine:

- **TS2364** — `checkReferenceExpression` (`checker.go:13130`) from
  `checkAssignmentOperator` (`:12769`): skip assertions and parentheses, and the
  result must be an `Identifier` or an access expression.
- **TS2703** — `checkDeleteExpression` (`:10804`): `SkipParentheses`, then
  `IsAccessExpression`. Note it skips **parentheses only**, not assertions,
  where `checkReferenceExpression` skips both — a difference that is upstream's
  and not worth smoothing.
- **TS2371** — `checkVariableLikeDeclaration` (`:5851`): a parameter with an
  initializer whose containing function has **no body**. `NodeIsMissing(body)`
  is the test, so an overload signature and an ambient declaration both qualify.

### The two bounds that are upstream's ordering, not choices made here

- `checkReferenceExpression`'s **second** arm is TS2779 (optional-property
  access) and returns before the caller reports anything else. This port has no
  `NodeFlags::OPTIONAL_CHAIN` — the parser does not set it (`tsr-binder`'s own
  module docs list it) — so an `a?.b = 1` would take the TS2364 arm here and
  TS2779 upstream. **Bounded**: the rule declines when the spine contains a
  `QuestionDotToken`, which is the syntax the flag would have been derived from.
- TS2703's private-identifier arm (`:10811`) is a *second* report at the same
  position, not an alternative, and is left unported: it is TS18011's row.

### The bar

```
bar:  +12 cases,  0 LOST,  WRONG delta ≤ +5
```

Under 20 because TS2364's and TS2703's fixtures are parser-recovery-heavy —
`parserRegularExpressionDivideAmbiguity` and the `parserErrorRecovery` family are
already in `extraonly`'s TS1005/TS1012 column — so the `file_has_parse_errors`
gate §179 measured will suppress some of the population as well as the noise.

### Falsifiers

1. **`WRONG` above +5 on TS2364.** Then the skip-spine is wrong: upstream skips
   assertions *and* parentheses for TS2364 and only parentheses for TS2703, and
   collapsing the two is the easy error.
2. **TS2371 lines land where the baseline has TS1015 or TS2372.** Those are the
   neighbouring arms of the same function.
3. **`LOST` non-zero** — all three only add diagnostics.

## §182 — §181 built: +14 at 4 wrong, and two upstream asymmetries paid for it

The bar was `+12 cases, 0 LOST, WRONG delta ≤ +5`. `RULE_CODES = [2364, 2703, 2371]`:

```
CONVERTS 14 · LOST 0 · STILL SHORT 17 · RIGHT 104 · WRONG 4
```

`coverage`: `diagnostics` **1,552 → 1,566 (28.53%)**. Rails unmoved. **Bar met on
both counts**, after three corrections that were all upstream reading rather
than judgement.

### The first measurement: +14 for **184** wrong and **6 LOST**

All 184 were TS2364 on **destructuring assignments** — `[a, b] = x`,
`({ a } = x)`, `computedPropertiesInDestructuring1`. The reason is one line
above the function this rule ports:

```go
// checkBinaryLikeExpression, checker.go:12338
if operator == KindEqualsToken && (left.Kind == KindObjectLiteralExpression || left.Kind == KindArrayLiteralExpression) {
    return c.checkDestructuringAssignment(...)   // checkAssignmentOperator never runs
}
```

`checkReferenceExpression` is not reached at all for a destructuring target.
**The test is on the unskipped `left.Kind`**, which matters: a parenthesised
`({a}) = x` does not short-circuit. Adding it took 184 → 13 and all six losses
to zero.

### The second: the two functions report at *different* nodes

Six more were the `deleteOperatorWith*Type` family, and every one was **off by
one column**:

```
expected  deleteOperatorWithEnumType.ts(12,32)
actual    deleteOperatorWithEnumType.ts(12,31)
```

`checkReferenceExpression` errors on `expr` — its own parameter, *before*
skipping (`:13134`). `checkDeleteExpression` reassigns
`expr = SkipParentheses(node.Expression())` and errors on the **result**
(`:10806-10808`). So `delete (a)` is reported at the `a` and `(a) = 1` at the
`(`. Two functions, one paragraph apart, with opposite conventions.

> **A shared helper is not evidence of a shared convention.** Both rules skip a
> spine and test a kind, which is what made one implementation look right; they
> differ in *what* they skip **and** in *which* node they report on, and neither
> difference is visible from the helper.

### The third: a plain JS decline, and the 4 that remain

`plainJSBinderErrors.js` gave three wrong TS2703 and nothing right — declined
on `in_js_file`, the same bound every other rule in this module carries.

The last four are `incrementAndDecrement`: `x++ = 4;`, where upstream's
**parser** reports TS1005 and never builds the assignment. This parser accepts
it, so `file_has_parse_errors` cannot see it. **Owner: `tsr_parser`** — the
third row this session to end at that owner (§162's two, §179's two, these
four), and together they are now a measurable argument that the parse-error set
is incomplete rather than that the gate is misplaced.

## §183 — `checkGrammarModifiers` taken WHOLE, which is the only way TS1038 is not a wrong code

§179 left TS1038 unbuilt with a reason: it is the **sixth** arm of the `declare`
chain, and porting it alone reports it where upstream reports one of the five
ahead of it. The answer to that is not a bound — it is the other five arms.

Priced across the chain:

```
TS1038   10 lines    7 cases   A 'declare' modifier cannot be used in an already ambient context.
TS1044   31 lines    6 cases   '{0}' modifier cannot appear on a module or namespace element.
TS1030   18 lines    5 cases   '{0}' modifier already seen.
TS1031    4 lines    4 cases   '{0}' modifier cannot appear on class elements of this kind.
TS1090    4 lines    4 cases   '{0}' modifier cannot appear on a parameter.
TS1243    2 lines    1 case    '{0}' modifier cannot be used with '{1}' modifier.
                    ── 27 cases
TS18019   0 lines    0 cases   (measured, and off the board)
```

### What the shape actually is, now that the whole function is read

`checkGrammarModifiers` (`grammarchecks.go:260-520`) is **a `flags` accumulator
over a left-to-right walk, plus a per-keyword `switch` whose every arm is a
chain of `else if` ending in `return`**. §103 ported the *must-precede* slice of
two arms; §179 added the accessibility-already-seen head of one. This ports the
arms whole:

| keyword | arms carrying corpus cases |
|---|---|
| `public`/`protected`/`private` | already-seen (TS1028 ✓), module-or-namespace-element (TS1044), `private` with `abstract` (TS1243) |
| `static` | already-seen (TS1030), module-or-namespace-element (TS1044), on a parameter (TS1090) |
| `export` | already-seen (TS1030), on a class element (TS1031), on a parameter (TS1090) |
| `declare` | already-seen (TS1030), on a class element (TS1031), on a parameter (TS1090), **ambient module block (TS1038)**, with `accessor` (TS1243) |

`this port has no `blockScopeKind`, so the `using` / `await using` arms of each
chain cannot fire — and they sit **between** the parameter arm and TS1038 in the
`declare` chain. That is a real gap in the ordering and it is why the build is
barred below its ceiling rather than at it.

### The `Reparsed` guard, still vacuous

Six arms carry `modifier.Flags&ast.NodeFlagsReparsed == 0`. §178 recorded why
that is vacuous here — no modifier in this tree can be reparsed — and it stays
recorded rather than silently dropped.

### The bar

```
bar:  +16 cases,  0 LOST,  WRONG delta ≤ +8
```

Under 27 for two reasons, both structural: the missing `using` arms sit ahead of
TS1038, and TS1044's 31 lines over 6 cases (concentration **5.2**, the worst
this session has taken on) mean most of that row needs every other line in its
case too.

### Falsifiers

1. **`WRONG` above +8 concentrated on TS1044.** Then the
   `ModuleBlock || SourceFile` parent test is admitting positions upstream's
   earlier arms claim — most likely `abstract`, which precedes it in the
   accessibility chain and follows it in the static one.
2. **A TS1030 line where the baseline has TS1029.** The already-seen and
   must-precede arms are adjacent in every chain and this port already emits the
   second.
3. **`LOST` non-zero.** `check_modifier_order` currently reports at most one
   diagnostic per node; adding arms must not change *which* one for any node
   that already gets it right.

## §184 — §183 built: +8 cases at ZERO wrong, and the case bar missed by half

The bar was `+16 cases, 0 LOST, WRONG delta ≤ +8`.

```
RULE_CODES = [1028, 1029, 1030, 1031, 1038, 1044, 1090, 1243]
CONVERTS 25 · LOST 0 · STILL SHORT 11 · RIGHT 46 · WRONG 0
```

**That 25 is not the delta.** The set includes TS1028 and TS1029, which this
port already emitted, so the counterfactual reconstructs a state before *those*
too. `coverage` gives the build's own number: `diagnostics` **1,566 → 1,574,
+8**, at 28.68%.

> **A counterfactual over a code set is a delta only when the port emits none of
> the set.** §162 said so from the happy side — *"no other producer emits them,
> so the counterfactual is the delta"* — and this is the same statement from the
> unhappy one. When an existing code shares the rule being extended, `coverage`
> is the only honest measurement.

### Verdict: wrong bar beaten, case bar missed by half

`WRONG` is **0** against a ceiling of 8, `LOST` is 0, and the ratio is the best
of the session. `CONVERTS` is 8 against 16.

Both reasons were written into §183's bar before the code, which is the point of
writing them there: the `using` / `await using` arms sit **between** the
parameter arm and TS1038 in the `declare` chain and cannot fire here, and
TS1044's 31 lines over 6 cases is a concentration of 5.2. `STILL SHORT` is 11 —
the row's cases mostly want something else as well, exactly as forecast. The
build is kept: it is a faithful transcription of a chain that pays again every
time an arm ahead of it lands.

### The parse-error gate, measured a fifth time and free

The single wrong line was `ClassDeclaration26.ts`:

```
public const var export foo = 10;
!!! error TS1440: Variable declaration not allowed at this location.
```

Upstream's **parser** refuses that declaration; this one builds it and the
`export` arm fires. Gating the whole of `check_modifier_order` on
`file_has_parse_errors` — which the TS1029 half had never carried — took the
wrong column to **0 at zero cost to conversions** (1,574 either way).

That is the **fifth** per-rule measurement of this gate (§40.3 +6, §50.1 −6,
§79 absent-is-right, §179 −20/+0, here −1/+0) and the **fourth** row this
session whose residue is a construct upstream's parser rejects and this one
accepts. The parse-error *set* being incomplete is now the single most
frequently named owner on this board.

## §185 — TS2694, the qualified arm `check_type_reference_name` names as its own gap

15 sole-obstacle cases over 18 lines — concentration **1.2**, the best shape on
the relation-free board.

`resolveEntityName`'s qualified-name failure arm (`checker.go:15884`). And it is
the third debt this session that was already written down at its own address:
`check_type_reference_name`'s doc comment says

> Bounded to a bare identifier that resolves under **no** meaning: a qualified
> `A.B` fails as **TS2694**, a name that resolves as a value is TS2749, and as
> a namespace TS2709 — three wrong codes at a right position.

§167 built the second and third. This is the first.

### The rule

For a `QualifiedName` in a type position whose **left** resolves to a namespace:
look `right`'s text up in that namespace's exports at the requested meaning, and
if it is absent report on the `right` node with the namespace's name and the
member's.

`getExportsOfSymbol(resolveAlias(namespace))` is the lookup; this port has the
exports table directly and **`resolve_name` has answered meaning correctly only
since §166**, which is what makes the left-hand resolution trustworthy here.

### The four arms ahead of it, and why each is safe to skip

`resolveEntityName` tries four things before TS2694 and each carries a
*different* code:

1. **`getSuggestedSymbolForNonexistentModule`** → TS2724 (`… Did you mean …`).
   The spelling machinery exists (`spelling_suggestion_for`) but is scoped to
   scope-chain names, not to one symbol's exports. **Not ported** — and it is a
   falsifier below, because a near-miss makes TS2694 the wrong code.
2. **`canSuggestTypeof`** → TS2749, needing `tryGetQualifiedNameAsValue`.
3. **The type-but-not-namespace arm** → TS2713, only when the *parent* is also a
   qualified name.
4. Both 2 and 3 are bounded away by requiring the qualified name to be exactly
   two deep and its parent not to be another qualified name.

### The bar

```
bar:  +9 cases,  0 LOST,  WRONG delta ≤ +5
```

Under 15 because arm (1) is unported and the corpus's `namespacesDeclaration2`
family is three lines in one case — a case converts only when all three land.

### Falsifiers

1. **A wrong line where the baseline has TS2724.** That is arm (1), and it means
   the spelling suggestion has to be scoped to a symbol's exports before this
   rule is sound.
2. **`WRONG` above +5 on names whose left is a *value* rather than a namespace.**
   The left-hand resolution asks for `MODULE`; if it admits a class or enum the
   message names something that is not a namespace.
3. **`LOST` non-zero** — the site is silent today.

## §186 — §185 REFUSED at +5 for 14, and alias resolution is the named blocker

The bar was `+9 cases, 0 LOST, WRONG delta ≤ +5`. Built, bounded once, and
reverted.

| version | CONVERTS | LOST | RIGHT | WRONG |
|---|---:|---:|---:|---:|
| as written | 7 | **5** | 19 | **127** |
| declining an alias namespace and an empty export table | 5 | 0 | 14 | **14** |

**+5 for 14 wrong is 0.36 conversions per wrong line** — below §13's refused
0.26 only by a little, well under the bar, and against a row whose whole appeal
was a 1.2 concentration. Refused.

### The 127, and the one line of upstream that explains them

```go
// checker.go:15855
symbol = c.getMergedSymbol(c.getSymbol(c.getExportsOfSymbol(c.resolveAlias(namespace)), text, meaning))
```

**`resolveAlias(namespace)`.** Upstream resolves the namespace through its alias
chain *before* reading exports. This port does not follow aliases at all
(`bd tsr-y4u.12`, and `lookup_scoped`'s own comment says so), so a namespace
that is an alias has an empty or partial export table here and **every** member
reads as absent. `aliasBug`, `aliasOnMergedModuleInterface`,
`augmentExportEquals1`, `badExternalModuleReference` — all of them.

Declining an alias namespace and an empty export table took 127 → 14 and the
five losses to zero. What is left is the same defect one step in:
`moduleVisibilityTest4`, `privacyGloImportParseErrors` and
`privacyImportParseErrors` reach a namespace whose exports are *non-empty but
incomplete*, which no syntactic bound can distinguish from a genuinely missing
member.

> **An empty table can be declined; a partial one cannot.** That is the same
> shape as §9's TS2339 refusal — *"an absent property and an unbuilt members
> table are the same `None`"* — reappearing in the *symbol* tables rather than
> the type ones. A rule that reports on absence needs a table that knows whether
> it is complete, and neither of this port's two table layers has that bit.

### What would make this win

**Alias resolution in `resolve_name`** (`bd tsr-y4u.12`). It is the same
prerequisite §166 was for §167, and this row is now the second measured
consumer of it: 15 sole-obstacle cases waiting behind one unported
`resolveAlias`. The refused source is one function of ~50 lines, described arm
by arm in §185, and it should be re-run — not rewritten — the day aliases
resolve.

Also unported and required before the row is sound:
`getSuggestedSymbolForNonexistentModule` → **TS2724**, which upstream tries
*first*; a near-miss member makes TS2694 the wrong code, and this port's
spelling machinery is scoped to the scope chain rather than to one symbol's
exports.

## §187 — §186's refusal REVERSED, and its attribution was wrong

§186 refused TS2694 at `+5 for 14 wrong` and named the blocker:

> *"`resolveAlias(namespace)`. This port does not follow aliases
> (`bd tsr-y4u.12`) … Re-run the refused source the day aliases resolve."*

**Aliases already resolve.** `Checker::resolve_alias` (`symbols.rs:565`) has
been in the tree throughout, with ported arms for `ExportSpecifier`,
`ImportSpecifier`, `ExportAssignment`, `ImportClause`, `NamespaceImport`,
`NamespaceExport` and `ImportEqualsDeclaration`. §185's code read
`binder.symbols()` directly and simply never called it; §186 then read the
symptom, matched it to a known gap, and filed the row behind a subsystem that
was not missing.

| version | CONVERTS | LOST | WRONG |
|---|---:|---:|---:|
| §185 as written | 7 | 5 | 127 |
| §185 bounded (decline alias namespaces) | 5 | 0 | 14 |
| **+ `resolve_alias`, upstream's own call** | 5 | 0 | **7** |
| **+ the TS2724 arm** | **6** | **0** | **4** |

`coverage`: `diagnostics` **1,574 → 1,580 (28.79%)**. Rails unmoved.

### Why the wrong attribution was so easy to make

`bd tsr-y4u.12` is real, `lookup_scoped`'s comment says the *binder* does not
follow aliases, and both are true — **alias resolution is unported in the
binder and ported in the checker**, because it needs the checker
(`resolve_alias` is `&mut self` and reaches module resolution). §186 read a
binder-level statement and concluded a checker-level absence.

> **A subsystem can be absent at one layer and present at another, and this
> board's refusals are written at whichever layer the symptom appeared.**
> §186's evidence was a symbol table with no members — which is what an
> unresolved alias looks like *and* what not calling the resolver looks like.
> The distinguishing test costs one `grep` for the function upstream calls by
> name, which is §136's rule, and §186 did not run it on `resolveAlias`.

Three of this session's rules pointed at the answer and none was applied:
§136 (grep for the function upstream reports from), §157 (read before the first
refusal), §166 (print the inputs when two attributions fail).

### The TS2724 arm, which §185 named as its own falsifier

`getSuggestedSymbolForNonexistentModule` (`checker.go:15861`) runs **first** and
carries TS2724 — `'M' has no exported member named 'num'. Did you mean 'nums'?`
§185 declined it and listed it as falsifier 1. It fired on four of the seven
remaining wrong lines, `moduleVisibilityTest3` and `4`, both `M.num` against
`nums`.

The fix was three lines: `spelling_suggestion` is already ported from
`core.getSpellingSuggestion` and takes a **candidate list**, so scoping it to one
symbol's exports is the whole of upstream's variant. §185 called that machinery
"scoped to the scope chain rather than to one symbol's exports" — true of its
*caller*, not of the function.

### Verdict and residue

`+6 cases, 0 LOST, 4 WRONG` against a bar of `+9, ≤+5`. **Wrong bar met, case
bar short by three.** Landed at 1.5 conversions per wrong line.

The four are singles in four cases — `moduleAugmentationEnumClassMergeOfReexportIsError`,
`exportNamespace11`, `importClause_namespaceImport`, `bluebirdStaticThis` — and
all four are §186's one surviving insight, which stands: **an empty export table
can be declined and a partial one cannot.** Module augmentation and export-star
both produce partial tables here.

## §188 — §171's attribution is wrong too, and that is now a pattern with three instances

§187 reversed §186 by grepping for the function upstream calls by name. The same
grep, run against the *other* refusal this session filed against a subsystem:

§171 refused TS7026 and named the blocker:

> *"`file_loader` must follow `/// <reference path="/.lib/react16.d.ts" />`.
> Every wrong line in the measured build is that one missing file."*

**Both halves of that are false.**

```
crates/tsr-compiler/src/loader.rs:582
    // `/// <reference path="…" />` — a file, not a module: no resolver, no …
crates/tsr-compiler/src/loader.rs:1416
    fn a_triple_slash_path_reference_adds_a_file_without_a_resolution() { … }

crates/tsr-conformance/src/types_producer.rs:52
    /// The files of `tests/lib`, mounted under `/.lib` — upstream's `testLibFolder`
```

The loader follows path references **and has a test pinning it**, and
`/.lib/react16.d.ts` is mounted by the same function that mounts every other
test lib. §171 inferred the cause from the fixture's first line — a
`/// <reference>` to a file whose contents were plainly not visible — and never
asked which of the three links in that chain was broken.

### What is actually unknown

`react16.d.ts` declares `declare namespace JSX { … }` at global scope in a
*script* `.d.ts`, so it should reach `globals` through `merge_globals`. Three
candidate links remain and **none has been probed**:

1. the reference is followed but the file is not bound into the program's binder;
2. it is bound but `declare namespace JSX` does not reach `globals`;
3. it reaches `globals` and the rule's `resolve_name(…, "JSX", MODULE)` misses
   — plausible only since §166 made that lookup meaning-filtered, which is
   *after* §171 measured.

**Candidate (3) did not exist when §171 ran.** That row's 45 residual wrong
lines were measured against a resolver that answered every meaning, so like
§164's 238 they are not a current number.

### The pattern, stated because it now has three instances

| § | refusal blamed | actually |
|---|---|---|
| 164 | `is_value_reference` | not broken (§165) |
| 186 | alias resolution unported | ported, in the checker (§187) |
| 171 | `file_loader` ignores `<reference path>` | followed, and tested (here) |

> **Every one was filed after reading a symptom and matching it to a plausible
> named gap, and every one named a gap that exists somewhere in the project.**
> That is what made them convincing. The distinguishing move is always the same
> and always cheap: **grep for the upstream function by name and read whether
> this port has it** — §136's rule, which this board has now paid for four times
> and skipped three.

A refusal that names a subsystem is a claim about *this* codebase, not about the
symptom, and it needs the same evidence any other claim about the codebase does.

**TS7026 is therefore re-opened**, and its next step is a probe rather than a
build: print whether `JSX` resolves at one `jsxIntrinsicElementsTypeArgumentErrors`
element, which distinguishes all three candidates in one run.

## §189 — TS7026 LANDED at +14 for 9, and the third refusal of the session reverses

§188 said the next step was a probe. The probe is the build itself: §171's rule,
restored unchanged, re-measured against a resolver two fixes newer.

| | §171 (at the time) | §189 (now) |
|---|---:|---:|
| CONVERTS | 14 | **14** |
| LOST | **1** | **0** |
| WRONG | **65** | **9** |

§170's bar was `+14 cases, 0 LOST, WRONG delta ≤ +10`. **Met on all three.**
`coverage`: `diagnostics` **1,580 → 1,594 (29.05%)**. Rails unmoved.

### What changed, and neither of them was `file_loader`

1. **§166** — `resolve_name`'s `globals` fallback stopped answering every
   meaning. §188's candidate (3), and it did not exist when §171 measured.
2. **The `.types` workstream's §173** — `declare global` merging, rebuilt with
   the `LOST 1` this workstream could not explain at §171, and landed.

**The subsystem §171 named was never involved.** The loader followed the
reference the whole time and `/.lib/react16.d.ts` was mounted the whole time;
what was missing was a one-line meaning filter in the binder and a merge in the
same crate.

### Three refusals filed, three reversed, and the cost of each

| § | refused | reversed by | cost of the wrong attribution |
|---|---|---|---|
| 164 | `is_value_reference` is broken | §165, one measurement | one session-hour |
| 186 | alias resolution unported | §187, one `grep` | **+6 cases delayed** |
| 171 | `file_loader` ignores `<reference path>` | §189, restoring the build | **+14 cases delayed** |

> **Every refusal this session that named a subsystem was wrong, and every
> refusal that named a *measurement* was right.** §163's "+2 for 238" stood.
> §177's "6 reports in 563 entries" stood. §184's "8 not 25" stood. What did not
> stand was any sentence of the form *"this row is blocked on X"* — because that
> is a claim about the whole codebase, and none of the three was checked against
> it.
>
> The operational rule, now paid for three times: **a refusal may state what was
> measured and must not name a blocker unless the blocker was grepped for by the
> name upstream calls it.** §136 said this about *rows*; it is twice as
> important about *refusals*, because a refusal is read for many sessions and a
> row is re-measured every time.

### The nine residual wrong lines

`jsxChildWrongType`, `jsxChildrenWrongType`, `jsxElementTypeLiteral` and
`jsxFragmentFactoryNoUnusedLocals` — all cases whose `JSX.IntrinsicElements`
comes through a path this rule still cannot see, and all in cases that fail for
other reasons too (`STILL SHORT` is 35). No owner is named for them here, on
purpose.

## §190 — the parse-error set: the codes are not missing, one GUARD is

§189's rule turned on this board's last remaining subsystem claim. Four rows
this session (§162, §179, §182, §184) ended with residual wrong lines attributed
to *"the parse-error set is incomplete"*. Applying §136 to that claim rather
than repeating it:

```
grep -rno "messages::[A-Z_0-9]*" crates/tsr-parser/src | sort -u
→ 9 distinct messages, and `_0_EXPECTED` is TS1005
```

**TS1005 is emitted.** The parser has the code the fixtures want; it does not
report it because it *accepts* the construct. For `incrementAndDecrement`'s
`x++ = 4;`, one line explains it:

```go
// parser.go:4143 — parseAssignmentExpressionOrHigherWorker
if ast.IsLeftHandSideExpression(expr) && ast.IsAssignmentOperator(p.reScanGreaterThanToken()) {
    return p.makeBinaryExpression(...)
}
```

```rust
// crates/tsr-parser/src/expression.rs:153
if is_assignment_operator(self.token.kind) {
```

**The `IsLeftHandSideExpression(expr)` conjunct is absent.** A
`PostfixUnaryExpression` is not a left-hand-side expression, so upstream never
builds the assignment and the `=` falls through as an unexpected token —
`';' expected`. This parser builds it, so the file has no parse error, so
§179's gate cannot suppress the checker rules that then read a tree upstream
never produced.

That is the whole of *"the parse-error set is incomplete"* for this row: not a
missing code, not a missing rule, **a missing conjunct in a condition**.

### Why this is worth stating separately from the fix

Three sessions of handoff notes would have carried *"`tsr_parser` does not
record enough parse errors"* — a sentence that reads as a subsystem-sized debt
and is one `&&`. §189 established the rule for refusals; this is the same
mistake made in a *residue note*, which nothing on this board reviews.

**Extend the rule: a residue note names an owner, and an owner is a claim.**
`§162: owner tsr_parser`, `§179: owner tsr_parser`, `§182: owner tsr_parser`,
`§184: a parser divergence` — four notes, one unexamined premise, and the
convergence of four rows on one owner read as *corroboration* when it was four
copies of a single unchecked inference.

## §191 — the one-line parser fix works, and is REFUSED on a spurious TS1012

§190's conjunct, added:

```rust
if is_left_hand_side_expression(left) && is_assignment_operator(self.token.kind) {
```

**It does exactly what it should.** `incrementAndDecrement.ts` before and after:

```
expected   (8,5) TS1005   (11,5) TS1005   (14,5) TS1005   (17,5) TS1005
before     — nothing at any of the four —
after      (8,5) TS1005 + TS1012    (11,5) TS1005 + TS1012    … all four
```

Four missing lines became four *right* lines. And each brought a **TS1012**
(`Unexpected token`) that upstream does not emit: this parser's recovery reports
both the expectation and the surprise where upstream reports only the first.

### Measured whole

| suite | before | after |
|---|---:|---:|
| `parser_typescript` | 5,031/5,031 | 5,031/5,031 |
| `binder_symbols` | 8,459/**8,459** | 8,458/**8,458** |
| `printer_round_trip` | 11,762/**11,762** | 11,760/**11,760** |
| **`checker_types`** | 3,963 | **3,969 (+6)** |
| **`diagnostics`** | **1,594** | **1,588 (−6)** |

The two 100% rails hold but their **denominators shrink** — files that used to
parse cleanly now carry a parse error, which is the intended effect and removes
them from those suites. `checker_types` gains 6. `diagnostics` loses 6.

**Refused and reverted**, on the rule this board applies to nothing else so
strictly: a build may not lose cases on the suite it is made for. Trading −6
here for +6 in another workstream's suite is not this workstream's trade to
make unilaterally, and the handoff says so in as many words.

### What it costs to fix properly, which is small and specific

The −6 is not the guard. It is the spurious TS1012 and the files that now
correctly carry a parse error and so lose rules to §179's gate. The first is one
recovery site; the second is the *correct* behaviour arriving before the rules
that would replace those conversions.

> **The finding worth carrying: `tsr_parser` needs one `&&`, not a subsystem.**
> Four residue notes (§162, §179, §182, §184) said *"the parse-error set is
> incomplete"*. It is not incomplete — TS1005 is emitted, at exactly the right
> positions, the moment the guard exists. What is wrong is that recovery emits
> **two** codes where upstream emits one.

**Next step, and it is small:** find the recovery site that pairs
`_0_EXPECTED` with `UNEXPECTED_TOKEN` and make it report one, then re-run this
one-line change. If `diagnostics` comes back at or above 1,594 the build lands
and takes `checker_types`' +6 with it.

## §192 — the TS1012 §191 refused on is one missing `if`, and it is upstream's

§191 refused the parser conjunct because each new TS1005 arrived with a spurious
TS1012, and named finding the double-reporting recovery site as the next step.
There is no such site. There is a missing guard.

```go
// parser.go:327 — parseErrorAtRange, through which EVERY parser diagnostic goes
// Don't report another error if it would just be at the same location as the last error
if len(p.diagnostics) == 0 || p.diagnostics[len(p.diagnostics)-1].Pos() != loc.Pos() {
    result = ast.NewDiagnostic(nil, loc, message, args...)
    p.diagnostics = append(p.diagnostics, result)
}
```

```rust
// crates/tsr-parser/src/parser.rs:495
pub(crate) fn error_at(&mut self, message: &'static Message, span: Span) {
    self.diagnostics.push(Diagnostic::new(message, span));
}
```

**Unconditional push.** Upstream reports `';' expected` at the `=` and then
`Declaration or statement expected` at the *same* `=` — and the second is
dropped by this guard, which is why the baseline for `x++ = 4;` carries exactly
one line. This port keeps both.

### Why §191 looked for the wrong thing

It reasoned from the *pair* — `_0_EXPECTED` beside `UNEXPECTED_TOKEN` — and went
looking for a site that emits both. Neither site is wrong; they are two
recoveries firing in sequence, which is upstream's behaviour too. **The
divergence is in the sink, not in either source**, and a sink is invisible from
the symptom because every producer looks individually correct.

That is the same shape as §182's *"a shared helper is not a shared
convention"*, inverted: here a shared **sink** carries a convention that neither
caller states.

### What it should be worth

`extraonly` — cases one false positive from passing — has **41 TS1005 and 18
TS1012 lines** at its head, and the whole `parserErrorRecovery` family in it.
Those are exactly the shape a missing same-position guard produces. This is
therefore not a bounded fix for one row; it is a candidate for a large part of
the extra column.

### The bar

```
bar:  diagnostics +6,  0 LOST,  parser_typescript and printer_round_trip UNMOVED
```

Off `diagnostics` cases and not off extra lines, per §79. `+6` because the guard
alone should recover §191's −6; anything above that is the `extraonly` column
paying out. **`parser_typescript` at 5,031/5,031 and `printer_round_trip` at
11,762/11,762 are the rails** — a dedup that dropped a *first* error would move
them.

### Falsifiers

1. **`parser_typescript` moves at all.** The guard must drop only a diagnostic
   whose start equals the immediately preceding one's.
2. **`diagnostics` falls.** Then some case was passing on a duplicate this port
   emits and upstream does not, which would mean the multiset comparison had
   been matching two of ours against one of theirs — impossible, so a fall means
   the guard is dropping a *different* line.

## §193 — §192 built: +14 on ONE `if`, and every rail unmoved

The bar was `diagnostics +6, 0 LOST, parser_typescript and printer_round_trip
unmoved`.

| suite | before | after |
|---|---:|---:|
| **`diagnostics`** | 1,594 | **1,608 (+14)** |
| `parser_typescript` | 5,031/5,031 | 5,031/5,031 |
| `binder_symbols` | 8,459/8,459 | 8,459/8,459 |
| `printer_round_trip` | 11,762/11,762 | 11,762/11,762 |
| `checker_types` | 3,964 | 3,964 |

**+14 cases for one `if`, and nothing else moved at all.**

`would_repeat_last_error` — `parseErrorAtRange`'s guard (`parser.go:327`),
comparing the **start** and not the whole span, because upstream compares
`Pos()`.

### Why it is worth this much

`extraonly` — cases one false positive from passing — had **41 TS1005 and 18
TS1012 lines** at its head, and the whole `parserErrorRecovery` family. Every
one of them was a second diagnostic at a position that already had one. Two
recoveries firing at a single token is *correct* and upstream does it too; what
upstream does not do is record both.

### The finding, which is bigger than the fix

> **A divergence can live in a sink that every producer feeds, and it is
> invisible from any producer.** §191 saw `_0_EXPECTED` beside
> `UNEXPECTED_TOKEN` and went looking for the site that emits both. There is no
> such site — there are two correct sites and a missing filter between them and
> the list.

Four sessions of notes named `tsr_parser`'s *parse-error set* as incomplete
(§162, §179, §182, §184), §190 corrected that to a missing conjunct in one
expression rule, and §192 corrected *that* to a missing guard in the reporting
sink. **Three attributions, each more specific than the last, and only the third
was right — every one of the first two was reached by reasoning from the
symptom rather than reading the function upstream routes through.**

`error_at` was the function to read from the first sentence of §162, and nothing
pointed at it because nothing that *emits* a diagnostic looked wrong.

### What is still open on the parser

§191's conjunct — `IsLeftHandSideExpression` in `parse_assignment_expression` —
is **not** in this build. It measured `diagnostics −6` when the sink still
double-reported; that number is now stale and it should be re-run on top of this
guard before anyone believes it. It is `bd`-filed with that instruction.

## §194 — §191's conjunct, re-run on the fixed sink: the fourth refusal reverses

§191 refused `IsLeftHandSideExpression` at `diagnostics −6` and §193 said the
number was stale because the sink still double-reported. Re-run, unchanged:

| suite | §191 (broken sink) | §194 (fixed sink) |
|---|---:|---:|
| `diagnostics` | **−6** | **0** |
| `checker_types` | +6 | **+6** |
| `parser_typescript` | 5,031/5,031 | 5,031/5,031 |
| `binder_symbols` | 100% | 100% |
| `printer_round_trip` | 100% | 100% |

**The −6 was entirely the spurious TS1012.** With the sink correct, the conjunct
costs this suite nothing and pays `checker_types` six cases — a build with no
downside anywhere, held out of the tree for two sections by a number measured
against a defect one layer below it.

`binder_symbols` and `printer_round_trip` stay at 100% with denominators two
smaller: two files now carry a parse error, which is the *point*, and they leave
those suites the way every other genuinely-broken file does.

### Four refusals, four reversals, and what they have in common

| § | refused because | reversed by | delay |
|---|---|---|---|
| 164 | `is_value_reference` blamed | §165, one measurement | one hour |
| 186 | alias resolution "unported" | §187, one `grep` | +6 cases |
| 171 | `file_loader` blamed | §189, restoring the build | +14 cases |
| 191 | measured `−6` | §194, fixing a layer below | +6 `checker_types` |

The first three named a subsystem without checking it — §189's rule. **§191 did
not**: it named a *measurement*, which the rule permits, and it was still wrong,
because the measurement was taken over a stack that had a defect in it.

> **The rule needs its second half.** A refusal may state what was measured —
> **and must state what the measurement was taken through.** §191's `−6` was
> true of a parser whose reporting sink dropped nothing; it was never true of
> the language. The falsifier that would have caught it is one line: *"re-run
> this if anything under it changes"*, which §142 already demands of refusals
> and which this board has now failed to apply to its own.

Every refusal in this file should carry the state it was measured against.
§142 said so about `bd tsr-6re`'s number; four reversals in one session say it
about all of them.

## §195 — the guard has a second blind spot: scanner diagnostics are a separate list

§193's guard runs during parsing on `Parser::diagnostics`. The scanner keeps its
own list and the two are merged in `Parser::finish` (`parser.rs:296`) *after*
parsing, then sorted. **So the guard cannot see a scanner diagnostic**, and a
parser error at a position the scanner already reported survives.

```
conformance/parserErrorRecovery_Block2
  expected   (2,5) TS1127                  ← scanner: Invalid character
  actual     (2,5) TS1012 + (2,5) TS1127
```

Upstream has one list: the scanner's error callback routes through
`parseErrorAtRange` like everything else, so its guard compares across both
sources by construction. This port's separation is an artefact of the scanner
owning a `Vec`, and it is invisible from either side — §192's finding, one layer
further out.

### The tie-break, which the evidence settles

At one position upstream keeps whichever was reported **first**, and the scanner
reports while scanning the token — before the parser can say anything about it.
`parserErrorRecovery_Block2` confirms it: TS1127 survives and TS1012 does not.
So the merge orders scanner entries ahead of parser entries at an equal start,
then keeps the first per start.

### The bar

```
bar:  diagnostics +4,  0 LOST,  parser_typescript / printer_round_trip / binder_symbols UNMOVED
```

`extraonly` is 34 cases, of which 25 lines are TS1005, 7 TS1012, 4 TS1003 and 2
TS1131 — a mix of same-position-as-scanner and genuinely different positions, so
only part of it is reachable here. `+4` is the head of that.

### Falsifiers

1. **Any 100% rail moves.** The merge must drop only a diagnostic whose start
   equals one already kept.
2. **`diagnostics` falls.** A case passing on a duplicate is impossible under a
   multiset comparison, so a fall means a *first* entry is being dropped — the
   tie-break inverted.

## §196 — §195 built: +3, bar short by one, every rail unmoved

```
diagnostics          1,608 → 1,611  (+3)
parser_typescript    5,031/5,031    unmoved
scanner_clean_files  5,031/5,031    unmoved
binder_symbols       8,458/8,458    unmoved
printer_round_trip  11,760/11,760   unmoved
checker_types        3,970          unmoved
```

The bar was `+4`. **Short by one**, at zero cost anywhere — landed, and the miss
is recorded rather than rounded away: `+4` was an estimate off `extraonly`'s
head and the head turned out to be three deep, not four.

### One implementation note worth keeping

The first attempt tagged source order with a counter mutated *inside*
`sort_by_key`'s closure:

```rust
diagnostics.sort_by_key({ let mut index = 0; move |d| { … index += 1; … } });
```

**`sort_by_key` calls its key function an unpredictable number of times**, so
that counter is not a source ordinal and the tie-break would have been
arbitrary. Caught before measuring, which is the only reason it is a note and
not a section: a wrong tie-break here would have produced a *plausible* number —
the same code, dropping a slightly different set — and nothing downstream would
have flagged it.

Tagging before the sort is the fix, and the general form is: **derive an ordinal
where the order still exists, never inside a comparator.**

### What is left in `extraonly`

34 cases before this build. The remainder is genuinely-different-position
recovery — errors this parser invents where upstream produces none, rather than
a second error where upstream produces one. That is a different defect and needs
the recovery paths read one at a time; `parserErrorRecovery_*`,
`extendsUntypedModule` and `scannerUnexpectedNullCharacter1` are the families.

## §197 — a JavaScript file is not a program input without `allowJs`, and one half of the suite knew

`extraonly` after §196 still carried 14 lines in `node_modules`, all from one
shape:

```
// @Filename: /node_modules/foo/index.js
This file is not read.
```

The fixture says so in the file. `extendsUntypedModule` declares two untyped JS
modules whose *content is prose*, and upstream never parses them because
`allowJs` is off — `getAllowJS()` is `allowJs ?? checkJs ?? false`, and without
it a `.js` file is not a program input at all.

**This port already knows that**, twice over:

- `tsr_tsoptions::file_names` implements the rule and pins it with
  `without_allow_js_a_javascript_file_is_not_a_root`;
- `from_check_traversal` skips any unit `program.source_file` does not return,
  so the **check** half already agreed with the program.

`reported_for`'s parser/binder half did not. It walked `test.files` directly and
parsed everything that was not JSON, so it produced nine parse errors across two
files upstream does not read.

```
diagnostics    1,611 → 1,613  (+2)
extraonly         34 → 29 cases
every rail unmoved
```

### The shape, which is the third of its kind this session

§192 found a divergence in a **sink** every producer feeds. §195 found the same
sink had a second list it could not see. This is the same again at the level of
*which files exist*: two halves of one function disagreeing about the program,
where each half is individually defensible.

> **When a harness function has two halves that build the world separately, the
> question is never "is this half right" but "do they agree".** `reported_for`'s
> two halves have disagreed about the file set since they were written; nothing
> compares them, and the only symptom was diagnostics in files whose contents
> are an English sentence.

A cheap standing check falls out of that and is worth writing down for whoever
next touches this file: **the set of units the parser/binder half walks must
equal the set `from_check_traversal` walks.** They are computed independently
today and there is no test.

## §198 — `parseDelimitedList` recovers by CONTINUING; this port's parameter list breaks

§197 left `extraonly` at 29 cases, described as *"genuinely-different-position
recovery, needing the paths read one at a time"*. Read the first one.

```
constructor(...public rest: string[]) {}

upstream   restParamModifier.ts(2,27) TS1005  ',' expected.        ← one error
ours       (2,27) TS1005 · (2,41) TS1005 · (2,43) TS1131 · (3,1) TS1012
```

**The first error matches.** Everything after it is cascade, and the cause is
four lines:

```rust
// crates/tsr-parser/src/expression.rs:1862
if !self.eat(SyntaxKind::CommaToken) {
    break;                       // ← leaves the whole parameter list
}
```

```go
// parser.go:664 — parseDelimitedList
if p.parseOptional(ast.KindCommaToken) { continue }
if p.isListTerminator(kind) { break }
// We didn't get a comma, and the list wasn't terminated, explicitly parse
// out a comma so we give a good error message.
p.parseExpected(ast.KindCommaToken)   // reports ',' expected
… if startPos == p.nodePos() { p.nextToken() }   // no-progress guard
// and CONTINUES
```

Upstream reports the missing comma and **stays in the list**. This port leaves
it, so `rest: string[]` is never consumed as a parameter, `expect(CloseParen)`
fails on `rest`, and the failure cascades out into the class body — three more
errors from one recovery decision.

### Why this is not "one case at a time" after all

§197 called the remaining `extraonly` residue per-construct work. It is not:
**`parse_parameter_list` is one of several hand-written list loops in this
parser**, and upstream has exactly one `parseDelimitedList` that all of them
would be. A `break` where upstream continues is a *shape*, not an incident —
the same relationship §192 found between one guard and 59 extra lines.

The build is bounded to the parameter list, because that is the loop the
evidence names; the shape is recorded so the next `extraonly` read starts by
asking which list it is in.

### The bar

```
bar:  diagnostics +3,  0 LOST,  parser_typescript / scanner_clean_files /
      binder_symbols / printer_round_trip UNMOVED
```

`+3` because `restParamModifier` is one case and two more of `extraonly`'s
parser residue (`objectBindingPatternKeywordIdentifiers01`,
`classWithPredefinedTypesAsNames2`) are list-shaped by inspection but unverified.

### Falsifiers

1. **Any 100% rail moves.** Continuing where the port used to break changes
   which trees are built for *valid* input only if the loop is entered wrongly;
   the rails are what says it is not.
2. **`diagnostics` falls.** A case passing because the cascade happened to
   supply a line the baseline wants — implausible, and worth knowing.
3. **A hang.** Upstream's `startPos == p.nodePos()` guard is what prevents it and
   must be ported with the continue, not after it.

## §199 — §198 REFUSED by its own falsifier, and the missing piece is named

§198's bar had `parser_typescript … UNMOVED` as falsifier 1. Measured:

```
parser_typescript   5,031/5,031 → 4,967/5,031   (98.73%, 64 files)
diagnostics         1,613 → 1,601               (−12)
checker_types       3,970 → 3,961               (−9)
```

**Refused and reverted.** The falsifier was written to catch exactly this and
did, on the first run.

### The diagnosis, which the failures hand over directly

Every one of the 64 is `TS1003 Identifier expected`, first at offsets like
`8..9` — a *one-character token* the parser tried to read as a parameter name.
The port continued the list and then parsed a parameter unconditionally.
Upstream does not:

```go
for !p.isListTerminator(kind) {
    if p.isListElement(kind, false) {          // ← PCParameters: isStartOfParameter
        element := parseElement()
        if p.parseOptional(KindCommaToken) { continue }
        if p.isListTerminator(kind) { break }
        p.parseExpected(KindCommaToken)
        …
        continue
    }
    if p.abortParsingListOrMoveToNextToken(kind) { break }
}
```

**`isListElement` is the guard §198 omitted.** For `PCParameters` it is
`isStartOfParameter(false)` (`parser.go:886`):

```go
return p.token == ast.KindDotDotDotToken ||
    p.isBindingIdentifierOrPrivateIdentifierOrPattern() ||
    ast.IsModifierKind(p.token) ||
    p.token == ast.KindAtToken ||
    p.isStartOfType(true)
```

Continuing the list is only safe when the next token could start a parameter;
without that test, "recover by continuing" becomes "invent a parameter from
whatever is there", which is worse than the `break` it replaced.

### What this costs and what it is worth

§198's diagnosis stands — a `break` where upstream continues is a real
divergence and `restParamModifier`'s three extra errors are real. The build
needs **`isStartOfParameter`**, which needs `isStartOfType`, which is a
sixty-kind switch. That is the honest price: not four lines, one predicate of
moderate size, and the payoff is `extraonly`'s parser residue rather than one
case.

> **A recovery strategy has two halves and porting one is worse than porting
> neither.** Upstream's list loop is *"continue if the next token could be an
> element, otherwise abort"*, and §198 ported the continue without the
> condition. The `break` this port had was a crude version of the *second* half
> — it aborted always — and crude-but-safe beat half-faithful.

This is the fifth refusal of the session and the first that failed on a
falsifier written before the code rather than on an attribution. That is the
system working: §198's bar named `parser_typescript` specifically because the
change was in the parser, and one run settled it.

## §200 — the guard as a SUBSET: continuing only where a parameter clearly starts

§199 priced the parameter-list recovery at `isStartOfParameter`, which needs
`isStartOfType`, and confirmed by `grep` that neither exists here. That price is
right for the *complete* predicate. It is the wrong price for the *build*.

Upstream's loop continues when `isListElement` says the next token could be an
element and aborts otherwise. §198 ported the continue with **no** guard, so it
invented a parameter from any token — 64 files gained `TS1003`. The old `break`
was the opposite extreme: it aborted always.

**A subset of the guard sits between them and is safe in one direction.** Admit
only what unambiguously starts a parameter:

```
...            KindDotDotDotToken
an identifier  isBindingIdentifierOrPrivateIdentifierOrPattern
{  [           the binding-pattern half of the same
@              a decorator
a modifier     ast.IsModifierKind
```

and omit `isStartOfType`. Every token the subset rejects falls back to `break` —
**the behaviour this port already had** — so the change can only turn an abort
into a continue where a parameter genuinely follows. It cannot invent one.

That is the same shape as this board's standing rule for rules — *declining is a
gap, never a wrong answer* — applied to a recovery decision rather than a
diagnostic.

### What it should reach

`restParamModifier`: `constructor(...public rest: string[])`. After `...public`
and the reported comma the next token is `rest`, an identifier, so the subset
admits it, the second parameter parses, and the three cascade errors do not
happen. The cases needing `isStartOfType` — a parameter list resuming at a type
— keep the `break` and stay exactly as they are today.

### The bar

```
bar:  diagnostics +1,  0 LOST,
      parser_typescript / scanner_clean_files / binder_symbols /
      printer_round_trip UNMOVED
```

`+1` and not more: `restParamModifier` is the one case whose shape is verified.
The rails are the falsifier that mattered last time and they are the falsifier
again.

### Falsifiers

1. **`parser_typescript` moves at all.** The subset is meant to be strictly
   narrower than "any token"; if a valid file changes, it is not a subset.
2. **`diagnostics` falls.** As §198.
3. **A hang.** The no-progress guard must survive the restructure.

## §201 — §200 built: the subset guard lands, bar met exactly

```
diagnostics          1,613 → 1,614  (+1, bar was +1)
extraonly               29 → 28 cases
parser_typescript    5,031/5,031    unmoved
scanner_clean_files  5,031/5,031    unmoved
binder_symbols       8,458/8,458    unmoved
printer_round_trip  11,760/11,760   unmoved
checker_types        3,970          unmoved
```

Every falsifier negative. `restParamModifier` — `constructor(...public rest:
string[])` — now reports the single `',' expected` upstream reports and none of
the three cascade errors.

### The three-section arc is the point, not the case

| § | move | result |
|---|---|---|
| 198 | ported `parseDelimitedList`'s continue with **no** guard | `parser_typescript` −64, `diagnostics` −12 — refused by its own falsifier |
| 199 | read the 64 failures, named `isListElement`/`isStartOfParameter`, priced the complete predicate at `isStartOfType` | refused, filed |
| 200 | admitted the **unambiguous half** and let everything else keep the old `break` | +1, every rail unmoved |

> **When a faithful port is too expensive, the question is not "port less of it"
> but "which direction does porting less of it fail in".** §198 ported the
> *action* without the *condition* and failed toward inventing parameters. §200
> ported the condition as a subset and fails toward the behaviour that was
> already there. Same amount of missing code, opposite blast radius.

That is this board's standing rule for diagnostics — *declining is a gap, never
a wrong answer* — transferred to a recovery decision, and it is the first time
it has been applied outside a rule.

### What the complete predicate is still worth

`isStartOfType` remains unported and `bd` carries it. The cases it would add are
the ones where a parameter list resumes at a *type* rather than a name, and they
keep today's `break`. Nothing is wrong at those positions; there is simply less
recovery than upstream has.
## §202 — an ambient module's symbol carries its quotes, and three dead lookups

§173 recorded this as a known collision with a number and did not fix it. Fixed
here, and the fix is smaller and the fallout larger than that note estimated.

### The defect

`getDeclarationName` (`internal/binder/binder.go:311`):

```go
if ast.IsAmbientModule(node) {
    if ast.IsGlobalScopeAugmentation(node) { return ast.InternalSymbolNameGlobal }
    return "\"" + moduleName + "\""
}
```

The quotes are the mechanism, not the spelling: `ast.IsAmbientModuleSymbolName`
is `strings.HasPrefix(s, "\"") && strings.HasSuffix(s, "\"")`
(`ast/utilities.go:1656`), and they are what make `"process"` and `process` two
keys in one table. This port stored the specifier bare and recovered the
selection from the declaration's *shape* instead, on the reasoning that quoting
needed an owned string where every symbol name borrows. That reasoning had
expired: the binder takes an `Arena` and already allocates names through it.

The collision is the ordinary shape of `@types/node`, and `tsc` is silent on all
of it:

```ts
// crypto.d.ts — a script, so its top-level locals become globals
declare module "crypto" {          // symbol `crypto` -> globals
    global {
        var crypto: …              // symbol `crypto` -> globals
    }
}
```

`module.d.ts` and `process.d.ts` are identical in shape; `process` draws a third
declaration from `globals.d.ts`. `console` takes a different arm for one reason:
`declare module "console"` holds only an import and `export =`, so its symbol is
a `NAMESPACE_MODULE` and merging into a non-instantiated namespace is TS2649
(`checker.go:14188`), not TS2300.

### What it cost to fix, which was mostly not the binder

The binder change is one function. What it exposed is that **five call sites had
independently open-coded `tryFindAmbientModule`'s table lookup**, and all five
went silently dead the moment the key changed:

| site | what it is |
|---|---|
| `tsr_checker::Checker::ambient_module` | `tryFindAmbientModule` for resolution |
| `tsr_checker::Checker::ambient_module_for_diagnostics` | the same question for TS2307 |
| `binder_suite::namespace_import_target` | `import * as F from "ambient"` |
| `binder_suite::import_specifier_target` | `import { x } from "ambient"` |
| `binder_suite::resolve_import_equals_target` | `import x = require("ambient")` |

Only the first was found by grep before the change; the other four were found by
tests and suites going red. They now share `BindResult::ambient_module`, which
lives next to where the name is made.

**The two checker copies were not even equivalent to each other**, and that is
what produced the first confusing failure. `ambient_module_for_diagnostics`
tested only `VALUE_MODULE`; `ambient_module` also tested the declaration's
*shape*. So `namespace m {}` — instantiated, hence `VALUE_MODULE` — read as
findable to one and not the other, and `import a = require("m")` came out
`errorType` from the two disagreeing. With one key they agree, the specifier is
correctly unfindable, and §31's rule gives `any`. `tsc` confirms the premise:
`TS2307: Cannot find module 'm'`.

### Two predicates were being re-derived rather than ported

Both were caught in review rather than by a test, which is the argument for
reading the vendored package before writing the four-line version:

- **`IsExternalModuleNameRelative`.** The new binder helper hand-rolled
  `== "." || == ".." || starts_with("./") || starts_with("../")`.
  `tsr_path::is_external_module_name_relative` already exists and is the real
  port — it is `PathIsRelative || IsRootedDiskPath` (`tspath/path.go:931`), so
  it also covers `.\`, `..\` and `C:\foo.ts`, which the hand-rolled version
  misses. The guard now lives at the two checker call sites, which is where
  upstream puts it, and reads `tsr_path`.

  The other two `starts_with("../")` in the tree were checked and **left
  alone**: `tsr_module::resolver` and `tsr_tsoptions`'s `extends` resolution are
  faithful transliterations of upstream's own literal `strings.HasPrefix`
  (`module/resolver.go:753-754`, `tsoptions/tsconfigparsing.go:571`). Not every
  repeated string is a duplicated predicate.

- **`StripQuotes`.** `getSpecifierForModuleSymbol` renders the specifier with
  `stringutil.StripQuotes(symbol.Name)` (`nodebuilderimpl.go:1261`) — so the
  quotes come *off* at the render site, which is upstream's own step and not a
  compensation for how the name is stored. Ported as
  `tsr_core::stringutil::strip_quotes` rather than inlined, and
  `binder_suite::normalise_symbol_name`'s own copy — which handled `'` and `"`
  but forgot the backtick — now calls it.

### The oracle had a symmetry bug, worth 109 cases

`binder_symbols` fell to **8,350/8,459** on a change that was strictly more
faithful. The suite normalises the *baseline's* names through
`normalise_symbol_name` and did not normalise ours, which was invisible for
exactly as long as no name on our side carried a spelling. Two fixes:

1. Normalise both sides. Recovered 102.
2. Normalise **per dotted suffix**, not per full name. A nested ambient module
   is `"Map"."Observable"` here — the *whole* name is not quoted, only the
   suffix is, and the suffix is the key the baseline uses.
   `moduleAugmentationInAmbientModule1` is the case. Recovered 1.

The last 6 were the three dead harness lookups above. Back to
**8,459/8,459 (100.00%)**.

This is the same lesson as §173's oracle-gap note from the other side: a suite
that compares *our* output against *upstream's* has to put both through the same
normalisation, and an asymmetry there reads as a regression in the code.

### The measurement

| | before | after |
|---|---:|---:|
| `binder_symbols` | 8,459/8,459 | 8,459/8,459 |
| `checker_types` | 3,957 / 405,682 (84.6434%) | 3,957 / **405,703** (84.7060%) |

**Re-measured after rebasing onto the fifteenth session's §176–§197** (base
`357df05`, measured in a worktree at the base commit): 406,188 → **406,209**,
3,970 cases unchanged. **The same +21 lines and 0 cases on a second base**,
which is what makes it a delta rather than an interaction.
| `diagnostics` | 1,524/5,488 | 1,524/5,488, **no case changed verdict** |
| `module_resolution` · `file_loader` | 95/95 · 96/96 | unchanged |
| 22-package monorepo | **1,550** | **1,440** |

Re-measured on the rebased base as well (`357df05`, whose parser and TS7026
builds changed the repo's totals): **3,186 → 3,082, −104.** Same 91 TS2300 and
13 TS2649; the −6 TS2322 of the first measurement is absent because that base
had already lost them to a different change.

The corpus barely moves — +21 lines, no case — because it contains no such
collision, which is exactly what §173 predicted and why this could only ever be
scored on a real repository. `apps/worker` alone goes from 10 reported errors to
2, and both survivors are an unrelated `export *` gap in `zod`.

The −110 is 91 TS2300 + 13 TS2649 + 6 TS2322; the last six were cases where the
collision had been corrupting the symbol a comparison was made against, and they
do not recur on the second base.

**An observation the second measurement forced, and it is not about this
change.** At `357df05` the same repository reports **1,642 TS7026** — the rule
§189 landed for +14 conformance cases. Every one of them is
`JSX.IntrinsicElements` failing to resolve in a React package, which is the
`file_loader` wrong column §171 named as that row's binding constraint and
priced before the rule was built. The conformance corpus scores the rule
positive; a real repository scores it at more than a thousand false positives.
Recorded here because the two numbers are about the same code and only one of
them was taken.

### A deliberate non-change

The same upstream branch renames `declare global` to `InternalSymbolNameGlobal`
(`__global`); this port leaves it `global`. It is not part of the defect — the
block's symbol is a *local* of the file it is written in and nothing looks it up
by name — and `.symbols` baselines print it bare (`moduleAugmentationGlobal4`:
`>global : Symbol(global, Decl(f1.ts, 0, 0))`) because `symbolToString` strips
the internal prefix. Renaming would mean teaching the suite to strip it back,
against a suite at 100%, for no behaviour. Recorded so it is a decision rather
than an omission.

## §204 — `parseObjectBindingElement` decides on `isBindingIdentifier`, not on the colon

`var { while } = { while: 1 }` — one error upstream, four here:

```
upstream   (1,13) TS1005  ':' expected.
ours       (1,7) TS1003 · (1,13) TS1005 · (1,15) TS1012 · (1,24) TS1005
```

The extra `(1,7)` comes **before** the matching line, so this is not §198's
cascade shape. `parseObjectBindingElement` (`parser.go:1681`):

```go
tokenIsIdentifier := p.isBindingIdentifier()
propertyName := p.parsePropertyName()
if tokenIsIdentifier && p.token != ast.KindColonToken {
    name = propertyName; propertyName = nil
} else {
    p.parseExpected(ast.KindColonToken)      // ':' expected
    name = p.parseIdentifierOrPattern()
}
```

**The test is taken before the property name is parsed, and it is
`isBindingIdentifier`** (`:6262`), which is one line:

```go
return p.token == ast.KindIdentifier || p.token > ast.KindLastReservedWord
```

`while` is a reserved word, so `tokenIsIdentifier` is false, the `else` runs
whatever follows, and the single `':' expected` lands on the `}`. This port asks
a different question — *"is this a keyword followed by a colon"* — so a keyword
**not** followed by a colon falls to `parse_binding_name`, which rejects it with
`Identifier expected` at the keyword itself.

The rest of upstream's line is already here: `parseIdentifierOrPattern` on `}`
reports `Identifier expected` at the same position as the `':' expected` just
emitted, and §193's same-position guard drops it. **That guard is what makes
this fix a one-condition change rather than a two-error one** — it was landed
five sections ago for a different row.

### The bar

```
bar:  diagnostics +1,  0 LOST,
      parser_typescript / scanner_clean_files / binder_symbols /
      printer_round_trip UNMOVED
```

One case verified (`objectBindingPatternKeywordIdentifiers01`). The rails are
the falsifier that caught §198 and they are the falsifier again — this touches
binding patterns, which every destructuring form goes through.

### Falsifiers

1. **Any 100% rail moves.** `{ a }`, `{ a: b }`, `{ [k]: v }`, `{ ...rest }` and
   nested patterns must all parse exactly as now; only a **reserved** word in
   the property position may change.
2. **`diagnostics` falls.** As §198.

## §205 — §204 built: +1, every rail unmoved, and §193 paid for it a second time

```
diagnostics          1,614 → 1,615  (+1, bar was +1)
extraonly               28 → 27 cases
parser_typescript    5,031/5,031    unmoved
scanner_clean_files  5,031/5,031    unmoved
binder_symbols       8,458/8,458    unmoved
printer_round_trip  11,760/11,760   unmoved
checker_types        3,973 → 3,974  (+1)
```

`var { while } = { while: 1 }` now reports the single `':' expected` upstream
reports.

### The half of it that was already there

Upstream's `else` branch reports `':' expected` and then calls
`parseIdentifierOrPattern` on the same token, which reports `Identifier
expected` at the *same position* — and `parseErrorAtRange`'s guard drops it.
**This port only emits one because §193 landed that guard**, five sections
earlier, for `x++ = 4`.

> **A fidelity fix pays forward into rows nobody had connected to it.** §193 was
> barred and measured against `extraonly`'s TS1005/TS1012 column; it also turned
> §202 from a two-error problem into a one-condition one. Neither section could
> have predicted the other, and the only reason the second was cheap is that the
> first was done properly rather than bounded to its own row.

That is the third time this session a general fix has changed what a later
build costs — §166 → §167 (+8), §193 → §202, and the `.types` workstream's
`declare global` merge → §189 (+14). **The pattern is worth acting on: when a
row is expensive, check what has landed underneath it since it was priced.**

### `extraonly` over the session

```
50 → 34 (§193)  → 29 (§197)  → 28 (§201)  → 27 (§203)
```

Twenty-three cases, all from four fixes in the parser and the harness, none of
them a rule.
## §206 — §189's opening-element bound is contradicted by the corpus, and the record is corrected

§189 landed TS7026 bounded to `JsxOpeningElement` and `JsxSelfClosingElement`,
with this justification written into the code:

> *"A `JsxElement` carries an opening and a closing tag and upstream reaches
> `getIntrinsicTagSymbol` from the opening one; reporting on both would put two
> diagnostics at two positions where upstream has one."*

**That is false.** `jsxNamespacePrefixInName`'s baseline:

```
(1,20) TS7026
(2,20) TS7026   (2,31) TS7026     ← <a:element></a:element>: BOTH tags
(3,20) TS7026   (3,46) TS7026
(4,20) TS7026   (4,39) TS7026
(5,20) TS7026   (5,54) TS7026
```

Upstream checks the **closing** tag's name too. The bound was asserted from
reading `getIntrinsicTagSymbol`'s callers rather than from a baseline, which is
§157's rule — *read the fixture before the first refusal* — broken inside a
build that was otherwise measured end to end.

### What is NOT explained, and is left open rather than guessed

This port emits **no TS7026 at all** in that file. Its expected lines are all
missing, not duplicated — so lifting the bound would not obviously produce them,
and something else stops the rule from firing there. Candidates, none probed:
`no_implicit_any` off for this case, or the namespaced-tag path not reaching the
rule.

**The bound is therefore kept and the justification replaced**, in the code and
here. Two separate things were wrong and only one is now known: the *reason* was
false, and the *behaviour* may or may not be.

> **A bound is a claim and needs the same evidence as a report.** Every wrong
> line in this file was measured; the one sentence explaining why a whole node
> kind was excluded was not, and it sat in the code for fifteen sections looking
> like the rest of it.

Fifth correction of a claim of my own this session, after §165, §187, §188 and
§199 — and the first that was wrong in a *comment* rather than in a refusal.
That is worth noting because nothing re-measures a comment.

## §207 — the `@jsx` pragma path, built across three crates and REFUSED at +0

§189 declined TS7026's pragma path and named it as five of that build's nine
wrong lines. Built it: an `@jsx` factory namespace captured in
`tsr_parser::pragma` (with a test), carried on `FileReferences`, exposed through
a third `ModuleHost` method, implemented on `Program`, and consumed by
`jsx_intrinsic_elements_exists` — `getJsxNamespaceAt`'s two hops
(`jsx.go:1306`, `:1321`) plus `resolveSymbol` on the alias.

**Every layer works and the number does not move.** `WRONG` stayed at 9 through
three successive additions — the branch, then the alias resolution, then both.

### The probe, run after the third inference rather than the fourth

```
JSXHOP factory=dom     resolved flags=SymbolFlags(ALIAS)
JSXHOP factory=predom  resolved flags=SymbolFlags(ALIAS)
JSXEXPORTS []
```

The factory resolves, it is an alias, `resolve_alias` follows it — and the
target's **export table is empty**. `namespace dom { export namespace JSX { … } }`
imported across units contributes no exports here.

**That is §186's surviving insight, one layer down and now with a second
consumer:** *an empty export table can be declined; a partial one cannot* —
except this one is not partial, it is empty, and the machinery above it is
therefore inert.

### Refused and reverted, all three crates

+0 cases. §177 kept `elaborateObjectLiteral` at +1 because it was a precondition
that would pay when the relation improved; this is the same argument at +0, and
+0 is where *"never document an intention as though it were built"* starts to
bite. The parser half is small and tested and would have been harmless to keep —
but a captured pragma with no reachable consumer is a fact about nobody.

### The two things worth carrying

**First, the blocker is now named for the third time and has three consumers:**
imported namespace symbols carry no exports here. §186 (TS2694, 4 residual
lines), §187 (its fix), and now TS7026's pragma path all end at it. Three
independent rows behind one table.

**Second — and this cost a measurement — a multi-part edit that asserts midway
can leave NOTHING applied.** The first attempt at this build asserted on its
second pattern, failed, and never wrote the file; the run that followed showed
"no change", which reads exactly like "the pragma path does not help". It was
three probes later that the branch turned out never to have been wired. **A
"no change" result must be confirmed against the source before it is
interpreted** — the same class as §194's *state what the measurement was taken
through*, applied to the edit rather than the stack.

## §208 — the empty export table: the binder's ambient test omits "this is a `.d.ts`"

§207 filed *"imported namespace symbols carry no exports"* as a P1 with three
consumers. It is not about imports at all.

```ts
// renderer.d.ts
export namespace dom {
    namespace JSX {                 // ← no `export` modifier
        interface IntrinsicElements { … }
    }
}
```

`JSX` is not written `export`, so this port files it in `dom`'s body **locals**
and `dom`'s `exports` is empty — which is exactly what §207's probe printed.
Upstream finds it because **every node of a declaration file carries
`NodeFlagsAmbient`**, and an ambient container exports everything it declares.

`Binder::bind_container` computes:

```rust
let ambient = self.in_ambient_module
    || has_declare(module.modifiers)
    || matches!(module.name, Some(ModuleName::StringLiteral(_)));
self.export_context = ambient && !has_export_declarations(module);
```

**`self.in_declaration_file` is not a disjunct**, and it is a field the binder
already maintains (`binder.rs:467`). This is the same class as §99 and §132: a
flag upstream's parser sets that this one does not, substituted structurally
everywhere it was noticed and missed here.

### Why it went unnoticed for so long

`binder_symbols` is at 100%, so the symbol *tables* it checks are right — which
means the suite's expectations for these files do not distinguish a local from
an export. The distinction is only visible to a **consumer that reads
`exports`**, and until §187 and §207 there was none.

> **A rail at 100% bounds what it measures, not what is correct.** Three rows
> (§186's TS2694 residue, §187's, §207's pragma path) ended at an empty table
> that `binder_symbols` is blind to by construction.

### The bar

```
bar:  diagnostics +2,  0 LOST,
      binder_symbols / printer_round_trip / parser_typescript UNMOVED at 100%
```

`+2` is deliberately small against three consumers: this changes which table a
declaration lands in for **every namespace in every `.d.ts` in the corpus**,
including all of `lib.*.d.ts`, and the first measurement is as likely to be
about that blast radius as about the three rows.

### Falsifiers

1. **`binder_symbols` moves at all.** It is at 100% and this is a binder change;
   any movement means the export/local split it *does* check has been disturbed.
2. **`checker_types` falls.** Every name lookup that walks a namespace's locals
   is affected.
3. **`diagnostics` falls.** Names newly visible as exports could satisfy rules
   that currently report correctly.

## §209 — §208 built: the table changes, no suite moves, and the test is what makes it landable

The bar was `+2 cases`. Measured:

```
diagnostics          1,615    unchanged
checker_types        3,974    unchanged
binder_symbols       8,458/8,458   100%, unmoved
printer_round_trip  11,760/11,760  100%, unmoved
parser_typescript    5,031/5,031   100%, unmoved
```

**Bar missed at +0, and it is landed anyway** — which is the opposite of §207's
decision two sections ago, so the difference has to be stated.

### What separates this from §207's +0

§207 was reverted because *nothing could observe it*: the pragma path's consumer
resolved an alias to an empty table, so the machinery sat inert with no evidence
it did anything at all. This is not that. The change is **directly observable
and pinned**:

```rust
// crates/tsr-binder/tests/program.rs
a_namespace_in_a_declaration_file_exports_what_it_declares
```

`export namespace dom { namespace JSX { … } }` in a `.d.ts` now puts `JSX` in
`dom`'s **exports**. The test is red without the disjunct and green with it —
verified by removing it and re-running, because §208's own argument is that a
suite at 100% can be blind to the distinction being fixed, and a test that
passes either way would have been the same blindness one level down.

> **"Unmeasured" and "unobservable" are different, and only the second is a
> reason to revert.** §207 had no observer at all. §208 has one, it is exact,
> and no suite happens to consume it *yet* — three rows are queued behind it
> (§186's TS2694 residue, §187's, §207's pragma path).

### Why no suite moves

Every consumer of the fixed table is currently reverted or blocked:
`jsx_intrinsic_elements_exists`'s pragma hop went out with §207, and TS2694's
four residual lines need the *second* half of §186's insight as well. The fix is
a precondition, and §166 → §167 is the precedent for keeping one: a one-line
binder change that moved nothing on its own turned §163's dead code into +8 the
moment its consumer was rebuilt.

### The finding, and it corrects §207's own filing

§207 filed this as *"imported namespace symbols carry no exports"*. **Imports
have nothing to do with it.** The namespace is written without `export` inside a
declaration file, where upstream's parser marks every node ambient and an
ambient container exports what it declares. `Binder::bind_container` had the
other three disjuncts and not `self.in_declaration_file` — a field it already
maintains.

That is the **third** site where this port substitutes structurally for
`NodeFlagsAmbient` (§99, §132, here) and the first where the substitution was
simply forgotten rather than approximated. Worth a sweep: `grep` for
`in_ambient_module` and `has_declare` and check each against
`in_declaration_file`.

## §210 — the `NodeFlagsAmbient` sweep, run: clean, and that is the result

§209 filed a sweep of every structural substitute for upstream's
`NodeFlagsAmbient`. Run over `tsr-binder` and `tsr-checker`:

| site | substitute | state |
|---|---|---|
| `Binder::bind_into` (`binder.rs:480`) | `self.in_declaration_file` | already correct |
| `Binder::bind_container` (`:1036`) | `in_declaration_file` | **fixed at §208** |
| `Checker::file_is_ambient` | `FileContext { ambient: declaration_file }`, supplied per file by the suite | correct |
| §99 `declaration_is_in_an_ambient_context` | walks `declare` modifiers | correct |
| §132 `member_has_declare_modifier` | the member's own modifier | correct |

**No further miss.** §208 was the only forgotten one.

That is worth a section for the reason §137 gave when its `grep` came back
empty: *a miss costs one command and upgrades an inference into a verified
negative*. "The ambient substitution is probably wrong elsewhere too" was a
plausible sentence with nothing behind it, and it is now closed rather than left
to be re-suspected.

### Session close on this thread

Five sections (§205–§210) turned one filed P1 — *"imported namespace symbols
carry no exports"* — into: a corrected diagnosis (nothing to do with imports), a
one-disjunct fix, a test that pins it, and a swept negative. The P1 is closed
and what replaced it is a P2 that is already done.

## §211 — §207 rebuilt on §208's table: TS7026's wrong column falls 9 → 3

§207 built the `@jsx` pragma path across three crates, measured `+0` with the
wrong column stuck at 9, and reverted. §208 then fixed the table its second hop
reads. **The rule this session has paid for four times — *when a row is
expensive, check what has landed underneath it since it was priced* — applied to
a row priced ninety minutes earlier.**

Rebuilt unchanged and re-measured:

| | §207 (before §208) | §211 (after) |
|---|---:|---:|
| TS7026 `WRONG` | 9 | **3** |
| TS7026 `CONVERTS` | 14 | 14 |
| `LOST` | 0 | 0 |
| `checker_types` | 3,974 | **3,982 (+8)** |
| `diagnostics` | 1,615 | 1,615 |

Every rail unmoved. **Six false positives removed and eight `checker_types`
cases gained**, for code that was in the tree an hour ago and did nothing.

`diagnostics` does not move because all six lines sit in cases with other
failures (`STILL SHORT` is 35) — the same shape as §166, which also read `+0` on
this suite while unblocking everything above it.

### What the pair actually demonstrates

§207's probe printed `JSXEXPORTS []` and I filed it as *"imported namespace
symbols carry no exports"*. §208 found the real cause — a missing
`in_declaration_file` disjunct, nothing to do with imports — and landed it at
`+0` **because it was observable and pinned**, over an explicit objection that
§207 had just been reverted at the same score.

> **That decision is the whole return on this pair.** Had §208 been reverted for
> scoring zero, §211 would be unreachable and the `@jsx` path would still be
> filed as blocked. *Unmeasured* and *unobservable* really are different, and
> the test written at §209 is what made the difference legible at the time
> rather than in hindsight.

### The three remaining wrong lines

`jsxNamespacePrefixInName` and its React variant — namespaced tag names, which
§206 already recorded as an open question with the closing-element bound. They
are unaffected by the pragma and stay filed.
