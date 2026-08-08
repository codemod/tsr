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
