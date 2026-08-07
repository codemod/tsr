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
