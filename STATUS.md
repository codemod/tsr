# Project status

**The live dashboard. Updated at the end of every session, by whoever ran it.**

[PLAN.md](PLAN.md) is the roadmap — scope, phases, architecture, decisions, and it
changes rarely. **This file is the state**: what is ported, what the numbers are,
what is next, and what has already been refused and must not be re-derived. If the
two disagree about status, this file wins and `PLAN.md` needs a correction.

Rules for keeping it honest, which are the same rules the rest of the project runs on:

- **Every number carries the commit it was measured at.** A number without one is
  a number from an unknown compiler.
- **A refused item stays on this page with the number that refused it.** Deleting
  it invites the next session to spend a cycle rediscovering the same negative.
- **Correct in place and say it was corrected.** Silent edits destroy trust in
  every other number here.
- **Do not quote a row's population as work.** A population is a ceiling; the
  conversion is a different and usually much smaller number.

---

## 1. Where the port stands

Measured whole at the merge of `grind/non-checker-conformance` (PR #2) into
main (`185877d`), 2026-08-08, by one release coverage run on the merge commit.
The two workstreams COMPOSE: `checker_types` reads 3,926 against the driver
session's 3,914 and `diagnostics` 1,406 against 1,405 — the branch's parser
fixes ride into the checker rows.

> **The previous table stood corrected, and the correction was large.** What
> stood here was labelled "measured at the §42-v2 landing, 2026-08-07" and was
> stale by several sessions for every row the checker and declaration-emit
> workstreams touch. **A number here without a fresh run behind it is worse
> than no number**, and four sessions of readers were told declaration emit
> was at 47%.

| suite | passed | rate | note |
|---|---:|---:|---|
| `corpus_ingest` | 12,444/12,444 | 100% | |
| `baseline_resolution` | 12,444/12,444 | 100% | |
| `scanner_termination` | 12,444/12,444 | 100% | |
| `scanner_clean_files` | 5,031/5,031 | 100% | |
| `parser_typescript` | 5,031/5,031 | 100% | |
| `module_resolution` | 95/95 | 100% | |
| `file_loader` | 96/96 | 100% | |
| **`printer_round_trip`** | **11,776/11,776** | **100%** | **COMPLETE** — was 99.80%; see the non-checker branch's row in §7 |
| **`binder_symbols`** | **8,459/8,459** | **100%** | **COMPLETE** — 98.06% → 100% across the binder campaign; the mechanisms are in the fourteenth-session commits (tsr-1 canonical numeric names with the arena-threaded bind API, tsr-2 JSDoc declarations, tsr-3 export-star/augmentation bridges, scanner escape fidelity, and a checker-display layer anchored line-by-line to symbolToString) |
| `isolated_declarations` | 13/15 | 86.67% | |
| **`dts_emit`** | **333/374** | **89.04%** | 327 → 333 |
| `dts_shape` | 860/1,008 | 85.32% | the `!!!!`-marker fix moved one case in, and it passes |
| `parser_reachable_target` | 5,031/10,570 | 47.60% | |
| `dts_reachable_target` | 492/1,162 | 42.34% | |
| **`checker_types`** | **3,955/9,538** | **41.47%** | measured at `88d0e63` (gradient **84.63%**, right 405,390 — the fifteenth session's §89–§92 block: degenerate-union parses, instantiated-alias members, conditional-alias evaluation, and the shape property road); previous row (3,927 at `2c3de45`, gradient 84.34): **gradient 84.40% at build 138** (this row's case count was taken at build ~133 by the diagnostics session; the .types session's builds 128–135 landed §75–§77.3 on top — see §7's newest row). **+21 of these are [ADR-0042](docs/adr/0042-checker-options-come-from-compiler-options.md) with no checker change**, counterfactualled twice from different bases (3,842 → 3,863 and 3,864 → 3,885) for the same delta; the §77 base underneath was not separately counterfactualled, so 3,914 is a measurement and the +21 inside it is an attribution |
| **`checker_types`** (superseding the row above) | **3,957/9,538** | **41.49%** | **gradient 405,682/478,954 = 84.70%**, measured at the `declare global` landing on top of `a7dccd4`, one release run. The row above is kept because its attributions are still the record; only the totals are superseded. **The delta is +2 cases and +292 lines, and it was measured twice on two different bases** — once against `22d745c` (405,111 → 405,403) and again after rebasing onto the fifteenth `.types` session (405,390 → 405,682). Same delta both times, which is the evidence that the two workstreams compose rather than overlap. Also: **the committed snapshot was stale by 280 lines at `22d745c`** — it read 84.52% where a fresh run read 84.5824%, i.e. `454a559`'s +280 landed in the commit message and not in the snapshot |
| **`checker_types`** (lines only, superseding the gradient above) | *cases not re-taken* | — | **gradient 405,698/478,954 = 84.70%**, measured at `45436ae` (`45436ae^..45436ae`) by the scorepair pair over a baseline freshly accepted on clean `4965add` (405,682). **§93's true delta is +16 right G→R (fatarrowfunctions 11, fatarrowfunctionsOptionalArgs 5) against 4 G→W — 4:1, zero R→W/R→G.** The commit's own message claims "+311, 8.6:1, +300 G→R"; **that score is WRONG** — it was taken against a baseline stale by several landings, and the +300 (jsxChildren 42, reactDefaultProps 27, jsx arity 17 among them) was drift already on main, proven by the stash/clean-run counterfactual reading 405,682 with every one of those transitions present. The correction is in `checker-notes-narrow.md` §93 — carried inside `45436ae` itself, because the committing session swept the other session's in-tree correction into its commit (conventions' fourth sweep form, benign this once). Case count deliberately not quoted: no coverage run was taken at this commit |
| **`checker_types`** (lines only, §94) | *cases not re-taken* | — | **gradient 405,805/478,954 = 84.73%**, measured at `c9b7f93` (`c9b7f93^..c9b7f93`) by the full scorepair pair over the §93-corrected baseline (405,698). **§94: +93 G→R / +14 W→R against 1 G→W — 107:1, zero R→W/R→G.** `has_no_contextual_type` became the nil-ladder of upstream's `getContextualType` dispatch (`checker.go:29343`); the bar (`67a472e`, committed before the code) predicted +60–110 in the head case and measured 81 there. The one adverse is the strict-optional `| undefined` print residue, now priced twice (§93 0:457, §94 parserParameterList11) and named the seam's next candidate. checker-2 session |
| `diagnostics` | **2,104/5,488** | **38.34%** | measured at HEAD, 2026-08-10, via `cargo xtask measure`. **Fourteenth session (`diagnostics`) §156–§566: +653 by this workstream over one hundred and seventy builds, 0 lost.** Running total 80 → 2,104, **26.3×**. Also **+11 to `checker_types`** from shared producer inputs (§533–§537). **Forty-five refusals; twenty-two reversed or corrected in-session; twenty-seven builds reverted whole; four +0 builds kept for fidelity; ten repairs of the shared tree.** **`extraonly` 54 cases blocked by an extra alone.** `parser_typescript`, `binder_symbols`, `printer_round_trip`, `module_resolution`, `file_loader` all **100%** |
| **`checker_types`** (lines only, §202) | *cases not re-taken* | — | **gradient 406,209/478,954 = 84.81%**, measured at the ambient-module-quoting landing against `357df05` (406,188) in a worktree at the base commit. **+21 lines, 0 cases.** The corpus barely moves because it holds no ambient-module/global name collision; the change is scored on a real repository instead, where it is worth 110 diagnostics |

### `checker_types`, the number the project is steered by

```
393,553 / 478,954 assertion lines = 82.18%      (measured at the §42-v2 landing)
  right 393,553 | gap 51,496 | wrong 23,866        right+gap+wrong = 468,915 exactly
```

**The continuation's two builds** (verdictdump pairs at `490f56b` and
`f345084`): 356,653 + 63 (the loop fixpoint LANDED — §12.5's refusal
superseded by §12.6's trace verdict: the arms were innocent, the poison was
per-node cache writes during transient back-edge passes; +117 gained / 54
adverse, two cases regressed and stated) + 694 (§12.7: definite
assignment targets return the DECLARED type, `checker.go:11109` — **zero
adverse lines**, both regressed cases recovered) = 357,410; then + 77 (§21: overload
failure answers the intersection of candidate returns, `checker.go:9620` —
falsifier (a) fired on non-strict `undefined` arguments and was honoured by
narrowing; the twenty-second stand-in fixture came due) + 0 (§12.8:
empty-so-far loop re-entry restarts — faithful port, measured byte-identical,
prediction recorded as WRONG) + 89 (§13: past-last-assignment closure
narrowing, `checker.go:11139` + `flow.go:2698` — two fired legs honoured
in-build: max extended position, export exclusion) = 357,576; then + 10,000 (§14: the
too-large evolving-array bail — TS2563's observable, `largeControlFlowGraph`
whole, **the largest single build in the project's history**, zero adverse)
= 367,576; then + 1,144 net (§15: compound
assignments do not narrow, `flow.go:229`'s skip at the literal's base —
+1,147 gained / 5 adverse, of which 3 are the documented `noImplicitAny`
plumbing gap `bd tsr-4sc.11`; the fired leg taught that catch variables are
not auto) = 368,720; then + 526 (arrays §5: `T | T` is `T` by
identity, so the named-union decline never fires on identical inputs —
`enumLiteralsSubtypeReduction` whole) = 369,246; then + 108 (§16: the switch-clause
arm — typeof witnesses and identifier discriminants, two fired legs
honoured, 27 more wrongs converted to honest gaps) = 369,354; then + 72 (§17: property access
falls back to the string index signature; `@noUncheckedIndexedAccess`
plumbed as the third per-case option) = 369,426; then + 114 (§22 callres: optional
call chains through the property-access chain's own strip/mark functions;
the twenty-third stand-in came due) = 369,540; then + 133 net (§18: the enum
member's fresh→regular back-link — the relater was comparing interned
lookalikes; enum equality narrowing and assignment reduction now decide) =
369,673; then + 370 net (§19: an un-annotated rest
parameter is `any[]` — one arm, +32 cases; the 6 adverse are `tsr-5o2`'s
node-reuse row) = 370,043; then + 103 (§20: nullable initializers widen to
`any` with `strictNullChecks` off — the fired leg taught the strict gate) =
370,146; then + 27 net (§21: strict catch variables are `unknown`, the
fourth per-case option) = 370,173; then + 24 net (§22 narrow: predicate
narrowing at call conditions, `getNarrowedTypeWorker`'s exact four-rung
ladder after three fired legs) + 0 (§23: call-signature function facts,
measured zero, recorded per the §12.8 precedent) = 370,197; then + 1,342
(§24: template expressions — spans, the all-literal fold, `string`; a
string-operand arithmetic variant measured and REFUSED at −562; two more
stand-ins came due; +79 cases) = 371,539; then + 438 (§25: `void` is
`undefined`, `delete` is `boolean` — +26 cases) = 371,977; then + 167 (§26: non-null
assertions as the non-nullable remainder; the empty-remainder refinement
measured and refused at +102/147) = 372,144; then + 85 (arrays §6: spreads
contribute `Array<T>`'s element type; the twenty-sixth stand-in came due)
= 372,229; then + 155 (§27: readonly
assignment targets answer upstream's `any` — the §14 boundary argument's
second application; the constructor exception was the fired leg) = 372,384; then + 0 (§28: the numeric
element-access fallback — the bar's premise was wrong, the corpus answers
through the lib; landed as the lib-less fallback) + 2,107 (§29:
self-referential aliases serve their NAME inside their own cycle — the
2,000-line `BigUnion` mountain whole, upstream's member-type laziness at
the one seam print-at-creation permits; the degenerate-cycle trade stated)
= 374,491; then + 6,072 net (§31: unresolved
free names answer upstream's TS2304 `any` — five gate sets measured, the
third mountain range converted, +194 cases) = 380,563; then + 4,263 net (§32: member
and element access through minted unresolved receivers answer upstream's
`any` — **the port crossed 80%**) = 384,826; then + 1,424 (callres §23:
untyped calls through §31/§32-provenance receivers, zero RIGHT losses,
+25 cases) = 386,250; then + 848 net (callres §24:
unresolved-identifier callees and `super()` → `void`, +106 cases) =
387,098; then + 680 (callres §25: `new`
through unresolveds, near clean) = 387,778; then + 377 (callres §26: the
unique-symbol mint — the twenty-eighth stand-in came due) = 388,155; then + 378 (§33: `globalThis`
mints its type, members read the merged globals) = 388,533; then + 0 (§34: property-miss-on-
complete-tables — measured zero, REVERTED: the population is callee-side
only, and the arm touched the ADR-0038 boundary for no payoff) + 149 (§36:
uninferred type parameters fall back to `unknown`, `inference.go:1406`) =
388,682; then + 235 net (§37: tuples
instantiate — arm 6 over `tuple_element_lists`) = 388,917; then + 81 (§38: written type-
argument tails fill from defaults, zero adverse) = 388,998; then + 43 net (arrays §7: the
contextual re-open's first slice — literal-free element unions are
context-independent; the head case's TRUE blocker relocated to the
relater's class-pair decline) = 389,041; then + 784 (assign §17
UN-REFUSED: the nominal class arm works — the refusal's diagnosis was
wrong, corrected loudly; the true terminus was the array arm's
`count <= 1` conservatism, relaxed to decidable-is-the-answer;
`generatedContextualTyping` converted 779/900) = 389,825; then + 1,182 (callres §27: the
WRITTEN `unique symbol` type node mints per declaration, +47 cases) =
391,007; then + 343 (callres §28:
interface `this` in type position, second variant — the class arm and
call gate measured and dropped) = 391,350; then + 672 (§39: `this` in plain
functions is `any` in every mode — TS2683 is a diagnostic, +9 cases) =
392,022; then + 525 (§40: variadic tuple
prints with concrete-rest splicing — the fired falsifier named the split)
= 392,547; then + 1,002 net (§41: qualified
type references carry their target's members — three standing rows
converted at once, 264 of the double-refused `underscoreTest1` among
them) = 393,549; then + 4 with 18 W→G (§42 v2:
generic qualified references — v1 refused at +4/352 for unqualified
prints, the named design landed clean) = 393,553; then + 4 with 3 G→W (§43: `new`
fills from class defaults — small; the typed-array road relocated to the
constructor-interface signature) + 4 (§44: all-defaulted construct
signatures instantiate; the row's remaining decline queued for a
per-overload trace) = 393,561; then + 274 (§45:
`Record<string, V>` answers its reads — the one mapped alias
special-cased) = 393,835; then + 891 (§44 build 70: skip-
with-agreement over constructor overloads — the queued trace's one-line
answer, typed arrays whole) = 394,726; then + 46 (§46: generic alias
references carry their body's members — the chain case's bulk needs
member-signature inference, named) = 394,770; then + 66 (callres §29:
`this`-minted call results answer the receiver — `getThisTypeArgument`'s
rule; the derived-through-base polymorphic half recorded) = 394,836; then + 149 (§48: plain binding-
pattern parameters render — the token-kind trap fired a second time and
the pair caught it; +11 cases) = 394,985; then + 734 (§49: union property
projection — the dependent-flow family's prerequisite, its 268 residual
lines now waiting on `tsr-pqnh`'s narrowing half) = 395,719; then + 52 (§50: dependent
destructured narrowing — the pseudo-reference walk, ZERO adverse; a
§16-era comparable bug fixed en route) = 395,771; then + 26 (§50.1: the
switch form, ZERO adverse) = 395,797; then + 6 (§50.2: the projection
re-enters the ordinary walk, ZERO adverse) = 395,803; then + 279 (§30:
`new` consults the untyped-call gate, two narrowings fired and contained,
ZERO adverse) = 396,082; then + 390 (§31-callres: unresolvable require()
aliases read `any`, position disjunct dropped by the fired leg, ZERO
adverse) = 396,472; then + 28 (§50.3: the tuple-parameter dependent case,
ZERO adverse) = 396,500; then + 53 (§32-callres: index-signature prints
admit written non-union keys, ZERO adverse) = 396,553; then + 134 (§33:
`const` type parameters print, three legs contained, ZERO adverse) =
396,687; then + 7 (§6.3: tuple minting for tuple-spread literals,
context-split by two fired legs, ZERO adverse; §6.2's union-only variant
priced at +2/11 and reverted) = 396,694; then + 121 (§34-callres: deferred indexed-access prints, alias
objects decline by the fired leg, 2 accepted wrongs at 60:1) = 396,815; then + 324 (§35-callres: deferred keyof prints, two legs
contained, 22 accepted wrongs at 15:1) = 397,139; then + 453 (§36-callres: template-literal types print as
written, three positional narrowings — the annotation-reuse vs
alias-evaluation split — net wrong −219; §35.1's concrete keyof refused
at +3/32 en route) = 397,592; then + 12 (§36.1 v2: template-alias names reuse in signature
prints — the written_text seam after v1's type-level mint measured +18/52
and reverted) = 397,604; then + 636 (§37-callres: `arguments` binds
IArguments through the mounted libs, arrows/class-field contexts decline
by the fired leg, 29 adverse at 22:1) = 398,240; then + 224 (§38-callres: for-of bindings
take the iterated element, both empty-array spellings declined by the
fired legs, ZERO adverse) = 398,464; then + 566 (§38.1: for-in bindings are string,
6 adverse at 94:1) = 399,030; corrected −6 by the §39 study's revert
wash (399,024 measured at build 91's HEAD); then + 188 (§51: switch on a
discriminant property through §50.1's filter, 5 adverse at 48:1; §39's
union-origin text hack REFUSED at ~2:1 after three variants en route) =
399,212; then + 194 (§51.1: equality on a discriminant property through
the shared filter, 4 adverse at 49:1) = 399,406; then + 12 (§51.2: optional-chain
containment at strict equality, ZERO adverse) = 399,418; then + 13 (§51.3: truthiness on a discriminant
property — the discriminant family's fourth member, ZERO adverse) =
399,431; then + 72 (§51.4: chain containment's whole table replacing
§51.2's quadrant, ZERO adverse — build 96 was the scorepair/TSR_FILTER
tooling, 41s→0.36s inner loop) = 399,503; then + 15 (§51.5: containment composes with the discriminant —
the fallthrough control shape, ZERO adverse) = 399,518; then + 40 (§52: equality's comparable-filter half — reentrancy
guard, operand memo after compiler/con* hung the corpus, alias-named
declines; the §52.1 member-set index measured +39/11 and reverted to the
§39 reshape's account; 1 adverse at 41:1) = 399,558; then + 237 (§53: ORIGIN-CARRYING UNIONS — the §39 reshape
built and landed with four falsifier-driven refinements, superseding the
§39 refusal for enum/plain entry shapes; 19 adverse at 12.5:1 owned by
&&'s unported non-strict arm, queued) = 399,795; then + 104 (§54: &&'s non-strict falsy source — one line,
92 pre-existing wrongs converted, ZERO adverse) = 399,899; then + 64 (§55: ENUM MEMBER VALUES — the bd tsr-8pz constant
folder's first slice, value-keyed literal interning, three model
corrections each priced by one counterexample, ZERO adverse, +13 cases)
= 399,963; then + 210 (§55.1: the single-member enum SPELLING SPLIT —
divergent fresh/regular twins plus the access-road swap, 2 adverse at
106:1, +14 cases, the right count crosses 400,000) = 400,173; then + 52 (§56: literal retention under unit contextual
members — THE CONTEXTUAL ARC'S FIRST LANDED SLICE, const holders only,
both falsifiers fired and contained, ZERO adverse) = 400,225; then §57 (the element-access write seam, ONE LINE) aligned
1,704 previously-unreproducible lines — the denominator itself grew to
470,619 and right to 401,521 (+1,296 gross, +10 cases, 83.83%) — builds
25–105; then + 10 (§58: join-position member-set identity, ZERO adverse;
the let re-admission re-deferred at 3.6:1 with the anonymous-object
union-print diagnosis) = 401,531; then + 10 (§56.1: return-position retention, ZERO adverse;
'x'.length enum folding refused at −16 en route — interning collision)
= 401,541; then + 33 (§58.1: the anonymous-object union-print seam
closed by a one-line TSR_JOIN_DEBUG diagnosis, §56's let gate lifted, 6
adverse at 5.5:1 owned by the switch(true) road; §31.1 ES-import
findability refused at 2.7:1 and §56.2 parameter defaults at zero en
route) = 401,574; then + 65 (§59: switch(true) clause expressions, 4 honest
declines at 33:1) = 401,639; then + 91 (§56.3: argument-position
retention, ZERO adverse — temporal's first 79 lines move) = 401,730;
then + 53 (§60: the qualified heritage base at 6:1 — with the
inside-namespace refusal re-measured at 1.4:1 and the §33 const gate
proved load-bearing at 850 R→G, both priced) = 401,783; then + 58 (§62: unique symbols are per-declaration, 2
parameter-split residue at 30:1; §61's auto-var counter refused — it
cannot reach the cap without upstream's recursion — and §56.5's generic
un-gate measured redundant, both priced) = 401,841; then + 55 (§63: plain literals under tuple contexts, ZERO
adverse, +4 cases) = 401,896; then + 32 (§63.1: the assignment-target tuple context, ZERO
adverse) = 401,928; then + 388 (§64: non-strict nullable widening at return
inference — the §20 rule's twin, ZERO adverse, +19 cases) and + 5
(§63.2: empty tuples) = 402,316; then + 9 (§65 un-refused with three
keys — the noImplicitAny axis plumbed, variables-not-properties,
identifier-not-derived) = 402,325; then + 24 (§68: the return-statement contextual arm — the
dispatch's fourth, ZERO adverse; §65.1/§66/§67 refused en route with the
decline point finally traced to annotation-reuse and contextual-return
positions) = 402,349; then + 16 (§68.1: the parenthesized recursion) and + 4
(§68.2: the array-element arm) and + 1 (§68.3: the concise-arrow-body
arm) — all ZERO adverse = 402,370; then + 279 (§69: this in static members is the static side,
22 adverse at 13:1, +12 cases) = 402,649 exactly — builds 25–121; then
+ 68 (§70: overload-agreement contextual argument — the id-walk mention
test after a text test collided rebound `<T>` names, ZERO adverse)
= 402,717; then + 85 (producer: binding-element property names print
`any` UNCONDITIONALLY — the IsTypeAny precondition REVERSED on
measurement and its pinned test flipped, ZERO adverse) = 402,802; then
+ 61 (§71: renamed binding elements render verbatim) and + 52 (§71.1:
element initializers drop, `{}` spelling) = 402,915; then + 39 (§72:
function-type aliases print their name — the three-arm
getAliasForTypeNode rule was missing only on signature-bearing nodes,
ZERO adverse) = **402,954 exactly — builds 25–126 (84.13%)**; then
+ 30 (§74 construct inference) + 49 (§75 generic contextual
signatures) + 204 (§76 destructuring tuple contexts, slot-walk) + 5
(§76.2) + 89 (§71.2 nested patterns) + 370 (§77 single-quote-gated
written reuse, ZERO adverse) + 53 (§77.1) + 54 (§77.3) plus the
parallel session's +117 arriving under two rebases (§88 trap, both
caught by stash-and-remeasure) = **403,865 — builds 25–135
(84.33%)**; then + 14 (§78: exactOptionalPropertyTypes — missingType
minted, the write-position removal, the long-dormant `isProperty`
parameter finally read) + 184 (§79/§79.1: optional-element tuples
with alias names — three measured gates: generic→structural,
rest→variadic-road, empty→`[]`) + 71 (§80: labeled tuple members) +
the parallel sessions' arrivals under rebases = **404,219 — builds
25–138 (84.40%)** against a population grown to 470,657. Then + 43 (§82 aliased conditions, gated
to ZERO adverse) + 126 (§83 instanceof TRUE-branch at 18:1 — the
false branch is evidence-split between two baselines and awaits an
upstream trace) + 6 (§84 sibling truthiness; a redundant §84 equality
arm measured zero against §50's existing road and was REMOVED) =
**404,377 — builds 25–141 (84.44%)**; then + 42 (§85: the `T & {}`
family — adjusted facts for type variables, three measured
iterations ending in the union JOIN reduction) + 12 (§85.1: truthy
spells `NonNullable<T>` — LANDED AFTER A FALSE REFUSAL: the pricing
adverse was the parallel session's rows against a pre-rebase
baseline, the §88 trap's first WRONG-REFUSAL firing; rule sharpened
to re-run scorepair on the clean tree before pricing any adverse) =
**404,423 — builds 25–143 (84.45%)**; then + 33 (§86: rest-tuple
contextual parameters expand positionally, ZERO adverse — and §50's
destructured-discriminant narrowing composes with it for free) =
**404,456 — builds 25–144 (84.46%)**; then + 3 (§87: variadic tails
consumable lazily; the `unicodeEscapesInJsxtags` alignment
NONDETERMINISM found and recorded as an instrument caveat) + 28 (§88:
rest parameters over plain tuples expand, written reuse winning) +
22 (§88.1: the variadic half) + 26 (§86.1: the function's own
trailing rest) = **404,520 — builds 25–148 (84.48%)** against
470,651; then + 7 (§86.2: own-rest tuple slices through the interned
`create_tuple_type` road) = **404,527 — builds 25–149 (84.49%)**. §81 (blunt
qualified names) REFUSED at 114:6,769 — the qualifier is decided by
the VIEWER's position; per-site printing context now owns THREE heads
(import-spelling, qualified names, `temporal`'s 400). §77.2 (union written order) refused TWICE — 35:249, then
0:20 with the owner located (optionality unions, not the annotation
mint). §73
(JS-wide unresolved-prints-error) REFUSED at 199:1,743 — upstream
prints BOTH `any` and `error` in ONE file; the discriminator is finer
than file kind. The
contextual dispatch holds SEVEN arms. THE GRADIENT
CROSSED 84% and the CASE COUNT crossed 40% at build 115. The
right+gap+wrong identity now sums against 470,619.
Build 96 (scorepair) and the probefile tool are the loop's new
instruments; the full run is 21s since the §52 memo. §35 records a FINDING: tsgo prints
` : error` in JS chains across 123 baseline files — ADR-0038's premise
refined, the §31 JS trade re-grounded, and any future gate stays
source-side (no oracle peeking).

**CORRECTED, and said so:** this chain sat at 369,673 for five builds while
the table above moved to 77.29% — the very "figure that appears twice will
disagree with itself" failure this section warns about, caused by
non-asserting edit scripts (the §21 build's lesson applied to STATUS itself).

**The ninth session's chain, every figure from one `verdictdump` pair per
build:** 352,727 + 246 (empty literal non-strict) + 377 (alias rename, split
filter) + 176 (async `Promise<void>`) + 63 (generator declarations) + 15
(async primitive returns) + 46 (the same arms for class methods) +
245 (`await`) + 157 (the conditional's identity/any branches) +
815 (`||`/`??`, reduction-agnostic pairs) + 112 (decidability-gated
`removeSubtypes`) + 84 (the return aggregate, behind a JAVASCRIPT_FILE flag
that is now actually set) + 2 (the yield aggregate — §15.1's exact leftovers) + 33
(two-object array literals, uncontextual positions) + 24 (`readonly` orders
the strict-subtype relation) + 18 (optionality, both directions) + 0
(privacy — the crutch-deleting zero) + 151 (the `_1` rename, enclosing-scope
half) + 22 (the string escaper's short forms — a RECORDED
bar-less process miss) + 1,016 (the non-strict auto initial + the unary
flag-test fix) + 6 (the compound-assignment target) + 42 (the outer
auto reference — the §9 family's last studied arm) + 122 (heritage clauses
instantiate their written arguments — **past 3,000 cases**) + 58
(non-identifier enum members spell as indexed accesses) + 96 (the
export-assignment name records its declared type) = 356,653 exactly — no
alignment drift this time.

**Re-taken from one `verdictdump.rs` run at `9fe8056`**, and the arithmetic
check against the sixth session's triple names the session's six checker
builds: 347,530 + 128 + 293 + 297 + 58 + 83 + 1,153 + 11 + 1,500 + 1,674 = 352,726 (+1 alignment drift, re-taken whole from the run above).

**Re-taken, not carried.** This block read `347,384 / 81,977 / 39,554` for two
builds after those numbers stopped being true — the namespace deletion (+32) and
the untyped-call arm (+114) had both landed while §1's *table* was updated and
this block was not. Caught by a consistency pass rather than by a reader, which
is luck; the two numbers sat 20 lines apart in the same section. **A figure that
appears twice in one file will disagree with itself unless something re-derives
both**, and the arithmetic check that catches it is free: 347,384 + 32 + 114 =
347,530.

**All three figures are now ONE probe's, which is what this section has been
asking for.** `examples/verdictdump.rs` (new, this session) emits a verdict for
every aligned line in a single pass, so `right + gap + wrong = 468,915` is an
identity rather than a subtraction across instruments. The design-P pair read
`344,411 / 81,977 / 42,527` before and `347,384 / 81,977 / 39,554` after, from
the same probe at both ends — and its `wrong` before-figure of **42,527**
reproduces the qualified-naming build's `wrongdelta` total exactly, which is the
first time two instruments here have agreed on that number.

The remaining **10,039** lines of the 478,954 denominator are **unaligned**: the
baseline and this port disagree about the *expression*, so no comparison of
answers is meaningful and the line is in no bucket.

**This SETTLES the "9,145 short" discrepancy** that this section opened the
session with, and it is now a reconciliation rather than a hypothesis: the whole
triple is one probe's, taken at one commit, and it sums to 468,915 exactly.
`478,954 − 468,915 = 10,039`. The earlier 9,145 was the same phenomenon measured
with three instruments that do not share a denominator, which is why it was
neither stable nor equal to this figure.

> The session opened by recording that gap as *"worth a future session's first
> hour"*. It cost about ten minutes, and only because a build had already needed
> `verdictdump.rs` for an unrelated leg. **The instrument that answers a standing
> question is usually built for something else** — the cheap move is to notice
> when one arrives, not to schedule the question.

**Corrected in place: §1 carried `340,719` and the snapshot at `df13a69` read
`340,727`.** Eight lines, from commits landed after the `tsr-g30h` merge the
figure was taken at. The gap figure is `depend.rs`'s own walk at `a4e3991`, not
a subtraction.

**An open discrepancy, recorded rather than reconciled away — and now largely
answered.** `wrongdelta.rs`'s raw dump read **42,443 lines** at `a4e3991`, and
`right + gap + wrong` summed to 469,809 against a denominator of 478,954 — 9,145
short. The three instruments did not share a denominator (`depend` attributes gap
lines it can reach; `wrongdelta` dumps per-case). **The suggested fix — "one probe
reconciling the three denominators is worth a future session's first hour" — was
built this session** and is `verdictdump.rs`; see the paragraph above for what it
reads and for what it does *not* yet claim. The old warning still stands: **do not
substitute a figure from one instrument into another's series.**

**The wrong bucket re-split post-naming, seventh session (38,433 at
`ffb77fe`, coarse shapes over the verdictdump):** 12,325 want `any`
(ADR-0038/39 territory), 10,000 are `largeControlFlowGraph` alone, the naming
families are down to **1,235** (874 dotted-qualifier + 361 `import()`-form),
and the remainder is structural — unions 4,128, signature bodies 3,017,
generics 2,785, unsplit 14,943. The morning's dominant family is now the
smallest named one; what is left is ceiling and subsystems, which is §4.4's
conclusion measured from the wrong side.

**The wrong figure is carried forward by measured deltas, not re-derived.**
It was once quoted 4,000 lines stale, which nearly failed a bar by 20 lines: a
cross-instrument, cross-session subtraction is not a measurement. Re-run
`wrongflip.rs` at both ends of a pair if the number matters.

**The gate is whole-baseline and positional; the gradient is per-line. They are
nearly orthogonal** — a change can add 2,733 lines and flip zero cases. Say which
you are quoting.

### `diagnostics` — the traversal exists now, and the suite is a long tail

**Corrected, eighth session.** This section read *"`tsr-checker` emits no
diagnostics at all … nothing in the checker's type answers will move this
suite."* Both sentences were true and the *conclusion* the project drew from
them — recorded in two handoffs as "diagnostics is structurally blocked" — was a
statement about **why the number was flat, not a sizing of the work**. Nobody
had measured what the blocked cases were blocked *on*.

`examples/diaggap.rs` (new) does. Over the suite's own 5,488 judged cases it
splits the difference sets and ranks the codes by the only column that is a
forecast — **cases missing exactly one distinct code and reporting nothing
extra**, the bucket where one rule finishes a case on its own. At `7299a14`:
**3,258 of 5,488 were blocked on exactly one code**, across 469 codes. That is
59% of the suite behind single rules, not one wall.

ADR-0040's decisions (1) and (2) are built: `Checker::check_source_file`
(`crates/tsr-checker/src/check.rs`) is a second entry point beside the query
road, with the diagnostic collection on the `Checker` and drained by the
consumer. Three rules on top of it, each bar-scored:

| commit | rule | net cases |
|---|---|---:|
| `bc8045b` | TS2307 / TS2882 — module specifiers (`resolveExternalModule`) | +50 |
| `413174c` | TS2564 — `strictPropertyInitialization` | +133 |
| `7fab616` | TS2304 — `Cannot find name` | +115 |
| `1591c55` | TS2454 — used before being assigned | +233 |
| `12533e9` | TS2369 + TS2695 — two **syntactic** rules | +40 for **0** wrong |
| `60aae99` | `checkGrammarBreakOrContinueStatement` — TS1104/5/7, TS1115/6 | +25 for **0** wrong |
| `04a394b` | `checkGrammarStatementInAmbientContext` — TS1036, TS1183 | +24 for **0** wrong |
| this commit | `checkFunctionOrConstructorSymbol` — TS2390/1/2/3, implementation-expected arms only | +17 for 11 wrong |

`docs/architecture/checker-notes-diag2.md` carries the reasoning, the gates per
rule and the refusals. **`checker_types` is byte-identical across all of them**: the traversal is a second road and no query-path call site invokes it.

The suite's board is now `diaggap.rs`'s single-code column, re-run after every
rule because rows *grow* as rules land (a case blocked on two codes becomes a
case blocked on one). At **611** passing: TS2322 433, TS2339 133, TS2345 85,
TS2304 81, TS6133 80, TS2454 60, TS2564 35 — ~~**TS2339 is refused with its
number**, §5.~~

> **CORRECTED, fifteenth session.** That refusal was **retired** by the
> completeness walk (see the `~` row further down: *"TS2339 — §9's refusal
> retired"*, +12), and this sentence was left behind. It matters because it was
> read as current and acted on: a real-repository run reported 5,396 TS2339s and
> this line was cited as the explanation — *the rule over-reports, as recorded*.
> It does not. Every one of those diagnostics was correct; the namespace type
> handed to it was empty because `export *` did not populate module exports
> (`Checker::get_export_from_star`, fifteenth session). **A stale refusal is
> worse than no note: it supplies a ready explanation and stops the search.**

**The metric a diagnostic rule is scored on is not its conversions.** A case
carrying a spurious diagnostic can never pass however many rules land later, so
the figure is `passing + single-code reachable`. TS2454 read **+28** on it — and
**−210** before its last refusal landed, which is the number that shows why the
declines are the design rather than the polish.

**And the ordering rule the sixth build bought:** a rule that reports on a
*syntactic* fact has no incompleteness to leak. TS2369 and TS2695 converted 40
cases for **zero** wrong lines and needed no tightening pass at all, where every
one of the four semantic rules needed two or three. The small `1xxx` and
syntactic `2xxx` rows — TS1212 22, TS1036 19, TS2391 17, TS1029 12, TS1107 12
and a long tail, ~150 cases — are therefore worth roughly their row at roughly
no risk, and rank **above** TS2322's 543, which needs a members subsystem before
it is worth anything at all (§9).

### `diagnostics`, the tenth session — +176, and a goal that was measured rather than met

Measured at `HEAD`, tenth session. **717 → 893 of 5,488 = 16.27%.** Seven builds,
**zero cases lost across all seven**, `checker_types` untouched by every one of
them (the parallel `.types` workstream's 3,043 / 74.46% is theirs).

| build | rule | net cases | wrong lines |
|---|---|---:|---:|
| `46cb49e` | `checkUnusedIdentifiers` — TS6133/6138/6192/6196/6198/6199/6205 | **+115** | 11 |
| `8ec40c5` | TS2322, assignment and variable-declaration anchors | +29 | 28 |
| `ed2ed2a` | `@ts-ignore` / `@ts-expect-error` + TS2578 — the **program-level** filter | +2 | — |
| `9effd7b` | TS2322, return-statement anchor | +5 | 27 |
| `a071849` | `noImplicitAny` parameters — TS7006 / TS7019 | +7 | **0** |
| `8ba72f9` | TS2322 property/parameter anchors; directive position corrected | +0 | 24 |
| `50bc406` | TS2314 / TS2707 — type-argument arity | +12 | **1** |
| `50bc406` | TS2554 / TS2555 — call arity | +6 | **0** |
| `~` | **TS2339 — §9's refusal retired** on the completeness walk | +12 | 11 |
| `~` | TS2741 — built on the same walk, **REFUSED** at 1 for 8 | 0 | — |
| `~` | TS2353 — excess properties | +6 | **0** |
| `~` | TS2403 — built, **REFUSED** at 6 losses (`isTypeIdenticalTo` unported) | 0 | — |
| `~` | **TS2322 re-gated on `relate_ternary`; §16's primitives gate deleted** | +7 | 50 |
| `~` | TS2345 — argument assignability on the sole-signature gate | +11 | 8 |
| `~` | TS2415 / TS2420 / TS2430 — heritage conformance | +19 | 21 |
| `~` | TS2416 — per-property overrides, shipped at a measured **zero** | 0 | **0** |
| `~` | TS2411 — the index constraint, found by walking the `extends` chain | +2 | 1 |
| `~` | **an object source is never a primitive** — a definite negative the relater declines to give, asked in the rule | +12 | 10 |
| `~` | the object-literal member anchor | 0 | 2 |
| `~` | **`extragap.rs`** (new) splits the extras into *displaced* and *invented* | — | — |
| `~` | TS2583 + TS2301 substitutions in TS2304's residual | +3 | −18 |
| `~` | TS2454's guarded-reference decline | 0 | **−59** |
| `~` | **TS1160 / TS1002 reported at the scanner's position, not the token's** | **+13** | — |
| `~` | **every `declare` path records the name node** — TS2300's position | **+11** | — |
| `HEAD` | TS1125 / TS1124 — the same `s.pos` correction at five more sites | **+5** | — |

**The session was asked for 50%. It is not reachable from here, and the session
measured that three separate ways rather than asserting it once.** `diaggap.rs`'s single-code column — the
only forecastable one — sums to **3,217 cases** at the session's open. 50% of the
suite is 2,744 passing, so reaching it means converting essentially *every*
single-code row plus a share of the multi-code tail. The column's head is
TS2322 at 510 and TS2339 at 143, and both need the members-table subsystem §5
refuses; TS2345/2741/2353/2411/2430/2416/2420 (≈370 more) need the same. The
honest ceiling for a session that does not build that subsystem is the long tail,
and the long tail is what this session spent itself on.


### `diagnostics`, the eleventh session — **+113 to 22.43%**, and the instrument that reframed the board

Measured at `HEAD`. **1,118 → 1,301 of 5,488 = 23.71%.** Thirty-four builds,
**zero cases lost in any of them**, `checker_types` unmoved by every one (the
parallel `.types` workstream took it 3,645 → 3,683 over the same hours; those are
theirs).

**The single most useful thing this session produced is a number, not a rule.**
`examples/diagreach.rs` (§54) counts the cases that report **nothing extra** and
whose every missing diagnostic carries a code some rule here **already emits** —
cases needing no new rule at all, only completeness:

```
cases reachable by deepening existing rules: 1,283   (against 1,239 passing)
TS2322 549 · TS2345 137 · TS2339 128 · TS2741 80 · TS2454 56 · TS2353 52
TS2304  40 · TS2554  36 · TS2564  36 · TS2411 34 · TS2352 32 · TS7006 28
```

It does not contradict §5's TS2322 refusal — it **prices** it: the assignability
family alone is worth more cases than the whole suite currently passes. And 786
of the 1,334 do not involve TS2322 at all. **Run it first from now on.**
`diaggap.rs`'s single-code column drops to what it always was, an ordering over
*new* rules; §55 was chosen off `diagreach` and is the session's largest build at
**+43**.

**And the split that says how much of the suite is this workstream's** (§75,
`diagreach.rs` carries it):

```
cases reachable by deepening existing rules : 1,305   (against 1,301 passing)
  wants only relation-bound codes           :   937
  wants a mix                               :    52
  wants NO relation-bound code              :   316
```

Relation-bound is TS2322, TS2345, TS2339, TS2741, TS2353, TS2352, TS2416,
TS2430, TS2420, TS2415, TS2403, TS2411 — §5's refused subsystem. **937 of the
reachable set is the assignability family**, `checker_types`' structural
relation and members table arriving through a second door, and it converts with
**no diagnostics work at all** once that lands. **316 need none of it**, about
the size of the eleventh session's whole harvest, and they are the cheaper
quarter: every attempt to reach into the relation from this side has been
refused or bounded (§16, §24, §49). That is the measured answer to *"is it time
to wait on the checker"* — **not yet**, and re-take the split when relation work
lands.

`diagreach` also corrects an arithmetic trap the session fell into once and
recorded: a **converted** case leaves the reachable set by definition, so
`before − after` over-counts the damage from new false positives by exactly the
conversions. §55's real cost was 10 cases pushed out of reach against 43 banked —
**count what a wrong line costs in cases, not in lines.**

| build | rule | net cases | wrong lines |
|---|---|---:|---:|
| §42 | TS2454's `isOuterVariable && !isNeverInitialized` disjunct | +1 | 30, from 29 |
| §42.1 | the **named-union printing guard**, removed for a non-printing consumer | **+7** | +1 |
| §43 | TS2564's private-identifier and computed name kinds; `is_error` for `== errorType` | **+14** | 6 → **4** |
| §44 | the same error test at `pair_is_reportable` | 0 | 0 (a measured zero, kept) |
| §45 | **TS2367**, the comparison overlap | **+15** | **0** |
| §46 | TS2352 ported to `getBaseTypeOfLiteralType` instead of its proxy | 0 | 0 (kept) |
| §47 | **TS2872 / TS2873**, `checkTruthinessOfType` | **+21** | 4 → **0** |
| §48 | `GetErrorRangeForNode`, centrally over 36 report sites | 0 | −4, **0 lost** |
| §49 | TS2365 — `+` and the relational operands | +3 | 58 → **0** |
| §50 | TS18050 is chosen by the **node**, not the type | +5 | 96 → **0** |
| §51 | `checkNonNullType`'s other five messages | +1 | 2 |
| §52 | TS2464 — the computed property name's type | +3 | **0** |
| §53 | TS2540 — the read-only assignment target | +1 | **0** (71 right lines) |
| §54 | **`diagreach.rs`** — the reachable-by-deepening count | — | — |
| §55 | **TS2304 in *type* positions** | **+43** | +51 |
| §56 | TS2554 for constructors — optional parameters and the base's | +1 | 31 → **6** |
| §57 | **`names_in_scope` takes a meaning** — §55's refusal retired | **+7** | 495 → **456** |
| §58 | `x.constructor === C` joins the unported-narrowing list | +2 | 30 → **15** |
| §59 | TS2345 reports the **first** failing argument, not every one | +2 | 12 → **10** |
| §60 | a named function expression's own name is in scope inside it | +1 | 456 → **451** |
| §61 | `this[key]` with a literal-union index marks what it reaches | +1 | 17 → **15** |
| §62 | TS2411's literal member names, and `isNumericLiteralName` as a round-trip | +3 | 1 → **1** |
| §63 | **TS2389** — the arm §14 declined rather than ported | **+13** | 11 → **11** |
| §64 | **a `Constructor` has no symbol here** — TS2390 and three siblings | **+10** | 11 → **12** |
| §65 | **`diagemit.rs`**, then **TS2362 / TS2363** — the arithmetic operands | **+13** | **4** for 866 right |
| §66 | TS2356 at the `++` / `--` operand | +5 | 31 → **0** |
| §67 | TS2341 — a private property outside its class | +7 | 13 → **0** |
| §68 | TS2445 — the `protected` sibling | 0 | 31 → **0**, +44 right |
| §69 | TS2374 — a duplicate index signature | 0 | **0**, +19 right |
| §70 | an inaccessible property's access answers `errorType` | +2 | 104 → **100** |
| §71 | a **class's** own name is in scope inside it — §60 at a second kind | +1 | 451 → **444** |
| §72 | §59's standing loss: the **elaboration** reports instead of the outer code | +1 | 10 → **6**, LOST 1 → **0** |
| §73 | the same rule at the five assignment anchors | +1 | 112 → **106** |

**The last two are zeros on the suite and are kept on a different number.**
`diagreach.rs` reads **1,304** against 1,296 passing, up from 1,283 at §64: 63
right lines and no wrong ones moved eight cases from *needs a new rule* to
*needs an existing one to finish*. **A rule that converts zero and emits no
wrong line is not the same as one that converts zero and does** — §44 was the
first kind, §61.1 the second, and `diagreach.rs` is what tells them apart.

**The session's finding, and it recurred five times in seven builds: the thing
between a rule at +2 and the same rule at +15 was a sentence about *why*, not a
threshold.** In order:

1. **A printing guard applied where nothing prints** (§42.1). `union_type_worker`
   answers `errorType` for a union with a named constituent so `E | undefined`
   does not print `E.a | E.b | undefined`. Correct — about printing. TS2454
   compares `(file, line, column, code)`, and the guard was silencing it on
   **every enum-typed declaration in the corpus**.
2. **A comment describing three cases above code handling one** (§43).
   `checkPropertyInitialization` accepts identifier, private identifier and
   computed name; the port quoted that line and matched `Identifier` alone.
3. **`== self.intrinsics.error` where `Checker::is_error` was meant** (§43, §44).
   This port has two error types — the intrinsic, and the `Named` an unresolved
   type reference mints — and upstream has one, `TypeFlagsAny`-carrying, so
   `t.flags&AnyOrUnknown` **is** the error test. The narrow spelling removed two
   wrong lines that predated the build it was found in. **31 more identity tests
   against `intrinsics.error` remain in the checker and each is the same
   question.**
4. **A decline copied with its conclusion and not its cause** (§45). §31's
   `same_primitive_family` reads as *"comparability reduces literals to their
   base primitive"*; it is really a stand-in for `getBaseTypeOfLiteralType`,
   which `checkAssertionDeferred` applies **at assertion sites only**. Inheriting
   it into TS2367 declined the whole `stringLiteralsWithEqualityChecks` family and
   read 2 conversions instead of 15.
5. **A literal's text compared against a literal's value** (§47). Upstream tests
   `node.Text() == "0"`, and a `NumericLiteral`'s `Text` is the scanner's
   *normalised* value — `0.0` reads `"0"`. This port keeps the source spelling,
   so `if (0.0)` reported as always truthy.

**The eighth build is the one with no number and the longest reach.** §48 ported
`scanner.GetErrorRangeForNode` — the mapping from *the node a rule names* to
*the span it is reported at*, which upstream runs on **every** diagnostic and
this port did not have at all. Thirty-six report sites across fourteen modules
were routed through it in one edit and **not one case regressed**, which is the
evidence that no rule here was passing a declaration node where upstream passes
the name node. It converts nothing today; it stops §30's build (+11, found one
call site at a time) from having to be repeated per rule.

**Two instruments, and the first one steered four of the builds.**
`examples/diagmissing.rs` prints the **missing** half of one code — every
baseline line the port does not emit, restricted to the cases that code alone
blocks, so each case printed is exactly one conversion. It is the twin
`extragap.rs` never had: `extragap` splits the *extra* column, `diaggap` ranks
the codes, and neither says **which lines are absent**. §42.1, §43, §45 and §47
were all found by reading its output rather than by re-deriving a board.
`examples/diagcase.rs` prints one case's expected and actual side by side.

### The three measurements that price 50%, and the second is the surprising one

1. **The column's arithmetic.** `diaggap.rs`'s single-code column — the only
   forecastable one — sums to **3,217 cases**. 50% of the suite is 2,744
   passing. Reaching it means converting essentially every single-code row, and
   the head is TS2322 511 + TS2339 131 + TS2345 114 + TS2741 41, all behind the
   relation and the members table.

2. **A row's population is a ceiling for the *code*, not for a *rule*, and the
   gap is wider than this project had assumed.** The session built the members
   subsystem the ninth session's handoff named as the prerequisite
   (`crate::member_completeness`) and then ran three rules on it. TS2339's row
   was 143 cases and the rule converted **12**; TS2741's was 40 and it converted
   **1**; TS2353's was 37 and it converted **6**. The `STILL SHORT` column says
   why — 22, 15 and 2 cases respectively gain a *correct* diagnostic of that code
   and still fail, because a case that gets object types wrong usually does so
   several times and in several ways. **The single-code column measures which
   code a case is blocked on, not how many of that code it needs.**

3. **Two more rows are behind a function that does not exist.** TS2403 was built
   and refused at six losses because `isTypeIdenticalTo` is unported and
   `TypeId` equality is not a stand-in for it (§24). TS2320, TS2717 and TS2394
   sit on the same predicate — ~70 cases behind one relation *mode*.

So the honest forecast is not "50% with more rules". It is: the relation and the
members table together are worth on the order of 800–1,000 cases, which lands the
suite near **30%**, and the remainder is a long tail of rows worth 10–30 cases
each. That is a multi-session number and it is now a costed one.

**And the third measurement is the one to act on, now taken twice.** Across
§25–§29 the TS2322 family emits **392 correct diagnostics and finishes 63
cases** — roughly **six right lines per case**. The board says 496 cases are
blocked on TS2322 *alone*; at six lines each that row is ~3,000 correct
diagnostics away, which is not a set of anchors, it is complete assignability.

> **`diaggap.rs`'s single-code column names the code a case is blocked on. It
> does not say how many of that code the case needs, and for the assignability
> family the answer is about six.** Every forecast this project has made off that
> column has been high by that factor. The column is still the right *ordering*;
> it is not a case count. That correction is the tenth session's most portable
> finding after the `relate_ternary` one.

**And the second instrument the session should have had from the start.**
`examples/extragap.rs` (new) splits the *extra* column into **displaced** — the
same code is missing elsewhere in the same file, so the port found the defect and
put it in the wrong place — and **invented**, and adds the only forecastable
number on that side: cases where one code, got right in *both* directions, would
finish the case.

```
  code   displaced   invented   sole obstacle
TS2322          42         23           489
TS2339           0         11           132
TS2345           0          8           104
TS2304           1         85            88
TS2454           3        111            66
TS2300          47         47            38
TS1160          12          0            12
```

It paid immediately and three times. **TS2304 and TS2454 are *invented*, not missing** —
two rules that had been read as incomplete for two sessions are over-reporting,
and the fix is subtraction (−18 and −59 wrong lines). And **TS1160 read
`12 displaced, 0 invented, 12 sole obstacle`**, which can only mean one
diagnostic in the wrong place: `s.error` (`scanner.go:413`) reports at the
scanner's position with length zero and this port reported from the token's
start. **One argument, twelve cases**, every parser and scanner suite
byte-identical. `diaggap.rs` had been printing "TS1160 12 cases" in its
false-positive list for two sessions with no way to tell those apart. **TS2300
was the same shape**: `declare` reaches `declare_into` through eight call sites
and only one recorded the declaration's name node, so a duplicate `class` inside
a `namespace` reported on the keyword and the same class at file scope reported
on its name. Eleven cases, and `binder_symbols` is unchanged at 98.03% —
positions are not part of that suite's comparison, which is how the defect
survived two sessions.

**+28 cases for three arguments and one moved statement**, and a measured negative
beside them: the same correction applied to the unterminated *regex* and *JSX
string* sites cost 2 cases and was reverted. The rule is not "the scanner always
reports at `pos`"; it is "report where upstream's call reports".

**CORRECTED, same session, and the correction is the session's most useful
result.** This block read: *"The first TS2322 build measured 947 right against
988 wrong — the relation disagreeing with upstream almost exactly half the time.
That is `checker_types`' 26% non-gradient arriving as diagnostics […] an
incomplete relation does not report less, it reports wrongly."*

**The number was right and the diagnosis was wrong.** `crate::relater` is
**three-valued**: `Ternary::NotRelated` means *"does not hold, and this port is
entitled to say so"*, `Ternary::Unknown` means *"cannot decide"*. The rule was
calling `is_type_assignable_to`, the **binary projection**, which collapses
`Unknown` into `false`. The 988 was not disagreement — it was **every undecidable
pair being reported as an error**, and the primitives-only gate built on top of it
was a bound drawn around a defect rather than around the relation.

Switching to `relate_ternary` and **deleting that gate entirely**
(`checker-notes-diag2.md` §25) took TS2322 from 35 conversions / 24 wrong to
**42 / 50 with 0 lost**, and then unlocked three rows that had never been
attempted, because a conformance check is the purest consumer of a negative:
TS2345 **+11**, TS2415/2420/2430 **+19**. **+37 cases from one corrected function
call.**

> This is the third time this project has drawn a bound around the wrong thing
> and measured it honestly: `checker-notes-selectable.md`'s `SELECTABLE` flag set
> (*"the blocker was the gate, not the relation"*), §9's `members: Some(_)`, and
> now this. In all three the numbers were right and the premise under them was
> not. **The tell is the same each time: a bound that has to model a subsystem's
> incompleteness is a bound in the wrong place, because the subsystem already
> knows.**

**Three rules reported zero or one wrong line**, and all three report on a
*syntactic* fact — §14's ordering rule holding for a third and fourth kind of
rule. TS2314 and TS2554 look like type questions and are not:
`getMinTypeArgumentCount` is *"the index of the first type parameter with a
default"* and `getMinArgumentCount` is *"the index of the first optional
parameter"*, both readable off the declaration.

**The closing board**, `diaggap.rs` at `HEAD` (re-run; rows grow as rules land):
TS2322 510, TS2339 143 (refused, §5), TS2345 114, TS2304 81, TS2454 62,
TS2564 40, TS2741 40, TS2353 37, TS2403 29, TS2411 29, TS18050 29, TS2430 28,
TS2300 27, TS7026 27 (refused, §5). **TS6133 and TS6196 have left the board
entirely.** 36 cases are blocked by an *extra* diagnostic alone and 586 by both —
the false-positive column is now the smaller half of the problem for the first
time.

---

## 2. The ceiling — 100% is not reachable and never was

[ADR-0038](docs/adr/0038-errortype-prints-error-and-the-corpus-has-a-ceiling.md).
Upstream renders `errorType` as **`any`**; this port prints **`error`** so that a
gap (we could not compute this) stays distinguishable from a wrong answer (we
computed something else). Every line where upstream's answer *is* an `errorType`
is unmatchable however good the checker gets.

| | lines | points |
|---|---:|---:|
| firmly unreachable (`examples/ceiling.rs` at `0fe102a`) | **2,202** | 0.46 |
| ~~unreachable, best estimate~~ | ~~26,000~~ | ~~5.4~~ |
| ~~firm upper bound~~ | ~~35,508~~ | ~~7.41~~ |

**Corrected a second time, 2026-08-06 (third session), from ~26,000 down to
2,202 — and the previous correction had itself quadrupled the estimate the
other way.** The ceiling's premise carried an unstated assumption: that a line
where upstream's answer is a rendered `errorType` (`any`) can never be matched
because this port refuses to print `any` for a gap. The `tsr-4qx` build showed
the assumption false: `Array<any>`'s instantiated index signature *honestly
computes* `any` on 10,000 `largeControlFlowGraph` element accesses, and the
computed answer coincides with upstream's bail-out. A ceiling built from
"upstream's answer is errorType" is an upper bound only on lines this port
*also* fails to compute — which is not a stable population. Treat `ceiling.rs`'s
attributed count as the only firm figure and expect it to move.

**Consequences for any target:**

```
reachable denominator   476,752 of 478,954
today                    333,651 / 476,752 = 69.98% of reachable   (carries the 1,539
                         right-lines-in-the-unreachable-set offset the 332,570 figure carried)
80% of the full          383,163 lines  =  80.37% of the reachable
gap to 80%               +47,184 lines (full-denominator terms: 383,163 − 335,979)
gap to 70%               CROSSED (70.003%; the threshold was 335,268)
```

---

## 3. What is ported

Per-crate, by what the conformance suites actually assert — not by what exists.

| subsystem | state | evidence |
|---|---|---|
| scanner | **done** | 100% termination and clean-files |
| parser | **done for TypeScript** | 99.38%; `parser_reachable_target` is a wider target set |
| binder | **near done** | 99.15% at `8dcdc71`; `getMergedSymbol` redirect landed 2026-08-06 |
| module resolution | **done** | 95/95, `file_loader` 96/96, [ADR-0041](docs/adr/0041-the-checker-asks-its-program-for-a-module.md) |
| printer | **done** | 100% round-trip at `8dcdc71` |
| declaration emit | **partial** | `dts_shape` 85.32%, `dts_emit` 89.04% at `8dcdc71` |
| **checker** | **72.56% of lines** | the mountain; §4 and §5 |
| transformers | **not started** | |
| **compiler driver / CLI** | **seam only** | three pieces, no binary. `tsr_vfs::OsFileSystem` (the real disk, `internal/vfs/osvfs`), `Checker::apply_compiler_options` ([ADR-0042](docs/adr/0042-checker-options-come-from-compiler-options.md)), and `tsr_diagnostics::format` (the plain `a.ts(1,1): error TS2304:` line and the `Found N errors` summary, byte-exact). **No command-line parser, no `tsc` binary, no emit, no pretty output** — see §4 |
| diagnostics | **started — 6.89%** | the check traversal (ADR-0040 (1) and (2)) plus three rules; §1 and `docs/architecture/checker-notes-diag2.md` |
| language service / LSP | **not started** | |

### Inside the checker — what has an arm

Landed across the three sessions to date, newest first:

| commit | what | net |
|---|---|---|
| `1b02597` | **the single-signature literal collapse, with written carriage at three slots** (`bd tsr-d4li`, §10.15–16) — first built without carriage, **fired its leg at 112 lost, reverted whole** (`dbc1ae9`), rebuilt to the revert's own spec; three measurement residuals each named the next carriage slot, converging 112 → 55 → 25 → **7** with conversions untouched. Δwrong **−1,666**, +48 cases | **+1,668** |
| `ffb77fe` | **the composite-print twin** — `signature_to_string_at`, every rendered slot of a single-signature type through the site-aware naming stack (`checker-notes-modobj.md` §10.13–14, `bd tsr-2ghn`). **Forecast delivered to the line: 1,500 forecast, 1,500 net**, RIGHT→WRONG 2 = the counterfactual's at-risk count, Δwrong −1,500 — because the probe's `compose()` and the build are the same function, proven by the self-check leg before either ran. The enabler (`0796633`, the Union/Intersection symbol arm in `qualified_name_at`, +11) is what raised the forecast from 674 to 1,500 | **+1,500** |
| `9fe8056` | **a JSX element expression has the `JSX.Element` type** (`checkJsxElement`, `jsx.go:72`) — buildable only after the `/.lib` mount; the fifth session's "46% cannot resolve JSX" premise predated it and is corrected in `checker-notes-jsx.md`. 152% of the 759 forecast via the arrow-function-return cascade; own new wrong **4**, lost **4** (baselines that record `error`), residual = the composite-signature-print boundary (285, design P's baked-text seam) | **+1,153** |
| `cc8c422` | **`import d from "m"` resolves the real `default` export** (`getTargetOfModuleDefault`, plain half; synthetic default declined) — plus the rendering refusal that a symbol named `default` **never prints as a name**, which converted 40 would-be wrongs into gaps and cleaned 67 pre-existing ones. Δwrong **−38** (§10.12) | **+83** |
| `67949ee` | **a chain segment prints under its best name** — `React.Component`, never `__React.Component`. One edit on `symbol_chain`'s parent segment; measured WRONG→RIGHT 58 with **no other transition of any kind** (`checker-notes-modobj.md` §10.10) | **+58** |
| `952b328` | **`export = X` resolves through the assignment, and the rename prints the alias** — `resolve_alias`'s `ExportAssignment` arm (`checker.go:14889`), `import a = require` following `resolveExternalModuleSymbol` (`:15556`), and `Checker::best_name` (the innermost-table walk of `getAccessibleSymbolChain`). Sized at 302 seed converts / 23 would-wrong (§10.8). **Leg 4 FIRED at 130 lost on the first measurement** — a same-file `import a = b` alias renamed `privacyGloImport`'s namespaces, and the counterfactual's alias reduction was blind to that form, a false 0 at-risk — fixed with upstream's own `useOnlyExternalAliasing` flag (`symbolaccessibility.go:568`) and re-measured at **RIGHT→WRONG 0** (§10.9). The React/tsx `typeof React` family converts | **+297** |
| `897abdd` | **a chain through a module container qualifies** — `symbol_chain`'s three new arms in upstream's order: `trySymbolTable`'s direct-alias guard (an in-scope alias naming the symbol itself stops the chain), the container-alias arm via the tri-state `module_alias_at` (ambiguity declines outright), and the ambient `import("x").` branch (`nodebuilderimpl.go:1260`). Sized at 104 strict converts (§10.6), landed **283%** of that through composite prints; RIGHT→WRONG was **exactly the 3 at-risk lines the counterfactual named in advance** (`checker-notes-modobj.md` §10.7) | **+293** |
| `f62582e` | **`tryFindAmbientModule`** — `declare module "x"` is a resolution target, consulted before the host (`checker.go:15533`, `:15154`), selected by declaration shape because this binder stores ambient names unquoted. The type-creation refusal for bodied ambient modules moved to the rendering path (the thirteenth unported-stand-in fixture came due). Mechanism-own new wrong ~15; 328 more GAP→WRONG were correct types wearing unqualified names — attributed by arithmetic over two `verdictdump` runs and converted by `897abdd` the same session | **+128** |
| `93b540a` | **the `/.lib` test-library folder mounts** (`harnessutil.go:39`, `:141`) — measured at **zero** and shipped with the zero stated: react.d.ts loads and `"react"` is in globals, but the tsx corpus's `typeof React` head is blocked one mechanism later, on `export = __React` (the export= arm, `tsr-e2u` family) plus alias-preferred naming | 0, stated |
| design **P** | **`getSymbolChain` — a name declared in a namespace prints its qualifier** (`checker-notes-qualname.md` §11). The *printing* half of qualified naming, refused for four cycles on "lost 3,202 lines"; re-sized post-W at **2,990 converts / 14 at risk** and built to its registered bar. **All four legs passed**, the conversion column landed on **2,990 exactly** (100.0% of forecast), and `Δgap` was **0** — the leg that could only be written because P is a renaming and cannot make a gap line computable. `getMergedSymbol` measured ON against OFF: **13 losses removed for 0 conversions**. 64.1% of the gain is `compiler/temporal`, and leg 3's `cases gaining ≥ 60` (read 174) is what proves it is not a one-case build | **+2,973** |
| `8e28971` | **qualified type names reprint what was written** — `resolve_entity_name` plus a written-text reprint in `declared.rs`, with the *inside-the-namespace* positional refusal kept at 1.2:1 and an *enum-root* one **declined on principle** (upstream prints `Choice.Yes` verbatim). **Its bar's third leg fired and is overridden loudly** in `checker-notes-qualname.md` §9.6, by a third party, on arithmetic over the bar's own stated rule | **+3,590** |
| `d8590ff` | **the overload gate asks about the PAIR** — `relater.rs` made three-valued (`Related`/`NotRelated`/`Unknown`, Kleene composition) and `calls.rs`'s `SELECTABLE` **flag set deleted**, plus an `any`-parameter positional refusal priced at 40/26 → 33/2. The ternary itself converts **zero**; it is what makes removing the gate safe (`checker-notes-assign.md` §5–§6) | +94 |
| `tsr-g30h` | **structural type-argument inference** — the candidate walk for `T[]`, `(x: T) => U` and friends; leg 2 fired **twice** and both were build defects a +311 net had hidden (`checker-notes-infer2.md`) | +372 |
| `tsr-4sa` | **a `Named` callee reaches signature lookup** — call and construct signatures resolved off an interface's members, with `unique symbol` (291) and namespace-qualified naming (75) **refused positionally**; the naive design measured 646 converts against 377 wrong and those two refusals cost **zero** conversions (`checker-notes-namedcallee.md`) | +1,018 |
| `tsr-84iz` | **an array pattern implies a tuple over its literal** — the construct `tsr-o00` refused; a shared `create_tuple_type` keeps an inferred tuple interned with a written one (`checker-notes-patctx.md`) | +206 |
| type predicates | **`x is T` in return position** — `getTypeFromTypeNode`'s `KindTypePredicate` arm plus the node builder's return slot, and a parser ASI fix (`next_is_is_keyword` lacked `!hasPrecedingLineBreak`) that moved `parser_typescript` and `binder_symbols` **up** (`checker-notes-typepred.md`) | +1,041 |
| `tsr-jril` | **constructor type nodes** — `new (x: T) => U` and `abstract new`; a `SignatureKind` on `Signature` so one renderer serves both spellings (`checker-notes-ctortype.md`) | +1,358 |
| `tsr-rppd` | **`getApparentType` reads a type parameter through its `extends` constraint** — its bar fired at net 0 first and the diagnosis was a missing symbol route (`checker-notes-apparent.md`) | +156 |
| `tsr-0opd` | **private names** — `this.#x`; the binder already filed `#x` members, only the access-side name extraction was missing (`checker-notes-privname.md`) | +590 |
| `tsr-tgov` | **`new C<T>()` instantiates from written type arguments** — the call side already substituted them and `check_new_expression` refused at its first line; plus upstream's one name-independent quoting rule, a **method** named `new` (`checker-notes-callres.md` §14–15) | +686 |
| `4b81458` | **binding elements** — the plain destructuring leg (`tsr-o00`): object patterns via the `a["b"]` lookup pair, array patterns by position through the tuple reverse index; the parser records array-binding holes; six refused legs each with a number (`checker-notes-destructure.md`) | +1,081 |
| `acdeed5` | the `in` guard narrows by property presence; its bar's leg 2 caught the OPTIONAL-flag bug pre-ship (`checker-notes-narrow.md` §6.1) | +43 |
| `e7a65fb` | `typeof` guard narrowing — subtype relations, sixteen facts bits, three flow arms (`checker-notes-narrow.md` §6) | +745 |
| `cf33aee` | nullable receivers strip, optional chains propagate `undefined` (`checker-notes-nnaccess.md`) | +590 |
| `9eaa2f1` | `t[0]` — a tuple's numeric-literal property is its element (`checker-notes-tuple.md` §8) | +103 |
| `0d56467` | the tuple arm of `compare_types` (`tsr-5ll`) — 64 wrong lines fixed; three bar legs fired and are overridden loudly, `checker-notes-tuple.md` §7 | +58 |
| `ff49871` | `typeof x` in type position (`tsr-4sc.10`), plus written-node reuse for `typeof` annotations in signature prints | +1,958 |
| `b00738d` | object-literal method members | +420 |
| `bf5681b` | plain tuple type nodes (74.9% of printed tuples; modifiers still refuse) | +1,227 |
| `c72ebf2` | narrowing for property/element references (`tsr-6ka`) | +58 |
| `2642e7b` | equality narrowing against `null`/`undefined` | +30 |
| `385fb60` | calls through instantiated members, default type arguments (`tsr-1uz`) | +405 |
| `856972a` | signature-typed members instantiate, `strictNullChecks` plumbed (`tsr-0hc`) | +2,266 |
| `0fe102a` | instantiated generic members (`tsr-4qx`) — property access, element access, index signatures and the relater all through one seam | +12,357 |
| `40970d7` | instantiation depth/count guard (`checker.go:22111`) | 0, by design |
| `d356450` | an unresolved type reference prints the written name (`tsr-eep`) | +4,645 |
| `3b7fa44` | namespace exports resolve (`tsr-56r`) | +4,319 |
| `5290e1a` | the `&&` arm of `checkBinaryLikeExpression` | +958 |
| earlier | cross-file aliases, export markers, `getApparentType`, `autoArrayType`, unit-return widening, `this` parameter, `super`, object spread, `getMergedSymbol`, union parenthesisation and ordering | +11,000 approx |

Deliberately **not** ported, each with a reason on record: the evolving-array
`x.push(e)` widening (53 lines, all already wrong); `hadErrorBaseline`
([ADR-0039](docs/adr/0039-the-any-that-upstream-prints-is-the-baseline-writers-decision.md));
rendering `any` for `errorType` (ADR-0038).

---

## 4. What is next — the scored board

Measured at **`b00738d`** by `examples/depend.rs`, re-confirmed unchanged by a
fresh run at `7cecc02` (fourth session; every row within noise). Two lists, because the
project's ordering rule has two halves: **rank by the conversion, and where the
conversion is unknown, rank by how cheap it is to find out.**

### 4.-1 The driver, newly on the board — and the tracker it is not filed in

> **The full plan now lives in [STATUS-cli.md](STATUS-cli.md)**, which is this
> file's discipline applied to the CLI workstream: the inventory, the baseline
> oracle (194 `tsc` + 41 `tscWatch` + 80 `config` cases, asserting exact stdout
> and exit status), the measured ceiling without emit (**50 of 194**), nine
> phases each with a gate, and the refusals. The four items below are its head;
> that file supersedes them on any detail.

> **`bd` does not work in a fresh clone of this repository, and these four items
> could not be filed.** `.beads/` carries config, hooks and `metadata.json`
> (`"backend": "dolt"`, `"dolt_mode": "embedded"`) but the database itself is
> gitignored (`dolt/`, `embeddeddolt/`) and **the remote has no `refs/dolt/*`** —
> `git ls-remote origin 'refs/dolt/*'` is empty, and there is no
> `.beads/issues.jsonl` either. So the 183 `bd tsr-*` ids cited across `docs/` (651 citations)
> are unreachable from a clean checkout, and `cargo run -p xtask -- issue-ids`
> prints `SKIPPED` rather than checking any of them.
>
> Running `bd init` would make this **worse, loudly**: `known_ids()`
> (`xtask/src/issue_ids.rs:195`) treats a successful `bd list` as authoritative,
> so a fresh database holding four new issues would fail all 651 of those
> citations at once. The items are therefore recorded here, where the project's
> own rule says current state belongs. **Whoever has the Dolt database should
> push `refs/dolt/data` or export `.beads/issues.jsonl`**; until then this
> section is the tracker.

Ranked by what unblocks the most, not by size. None is a checker item — this is
the first workstream in the project that is not.

1. **The command-line parser** (`internal/tsoptions/commandlineparser.go`, 403
   LOC). Table-driven off `tsr-tsoptions`' declarations, **not** `clap` — the
   surface has `@responsefile` expansion, booleans with an *optional* following
   value (`--strict false` consumes, `--strict --noEmit` does not), at most two
   leading dashes so `-target` == `--target`, a second *watch* option table
   consulted on a miss, TS-coded errors with did-you-mean suggestions, and a
   separate `--build` option set. A derive macro would also reintroduce the
   declaration/assignment split `declarations.rs` exists to avoid.
2. **The option table's coverage: 48 of upstream's ~130 declarations.** Was 38
   before this session's ten checker options. Everything else reports as unknown.
   Fine for a checking-only driver; a hard blocker for CLI baselines.
3. **A `tsr check` binary.** The workspace has exactly one binary today
   (`tsr-conformance`'s `coverage`), so nothing can be pointed at a real
   repository. The seam is now complete enough to write it: real host + program
   assembly + checker configuration + diagnostic rendering. Needs a
   `ResolutionHost` impl pairing `OsFileSystem` with a current directory —
   every such impl in the tree is still in tests or examples.
4. **Pretty diagnostic output** (`FormatDiagnosticWithColorAndContext` and
   `writeCodeSnippet`, `diagnosticwriter.go:134-252`). The source frame with
   squiggles, gutter and colour. Deliberately **not** approximated — see the
   `tsr-diagnostics::format` module docs for why a nearly-right frame is worse
   than none.

Two more, smaller, discovered while doing the above:

5. **`apply_test_directives` is a hand-written duplicate of the declaration
   table** (`crates/tsr-conformance/src/trace_case.rs`). Upstream's harness
   applies *every* directive whose name matches a declaration
   (`harnessutil.go:266`); this one is a struct literal listing them by hand, so
   a directive is honoured only if someone remembered to add a line. That is how
   `isolatedModules` came to be silently dropped for 80 cases. Routing it through
   `tsr_tsoptions::declarations::find` would delete the duplication — with its
   own conformance risk, hence separate.
6. **`Diagnostic` has no message chain and no related information.** Upstream's
   has both, and the "Type 'A' is not assignable to type 'B'. / Property 'x' is
   missing…" cascade is unrepresentable until it does. The formatter's recursion
   is written and guarded by a `chain()` that returns nothing.

### 4.0 The gap-root board, re-measured at `138fb45` (seventh session, closing)

`examples/depend.rs`, fresh. **Gap 80,315** (was 81,769 at the session's first
board run — the naming family's conversions came mostly from the *wrong*
bucket, which this board does not walk). The head rows are unchanged in kind
and all owned: `PropertyAccessExpression / dependency` 10,210 (the §4.3
symptom), unresolved VALUE identifiers 7,678 (74% want-any),
`CallExpression` 6,610 (call family, refused legs), member name 4,786,
`ArrowFunction` 3,836 at 0.9% want-any (contextual typing, refused),
`NewExpression` 3,318, FunctionDeclaration-cycle 2,943, ObjectLiteral 2,383
(contextual), BindingElement-cycle 2,363, ArrayType 2,120 (94.3% one case),
TemplateExpression 1,890 (refused). ~~**No unowned row remains above 1,000.**~~

> **CORRECTED, eighth session, and the correction is about this file rather
> than about the compiler.** The board was re-run at `7299a14`: **gap 80,315,
> every row unchanged**. But *"no unowned row remains above 1,000"* was true
> only of the rows `depend.rs` **attributes**. Two of its five endings attribute
> nothing — `cycle` 5,394 and `depth cap` 1,330, 8.4% of the gap — and the kind
> printed for those is an arbitrary member of a loop, not a cause. `cyclegap.rs`
> (new) measured them; `docs/architecture/checker-notes-cyclegap.md`:
>
> - **The `cycle` ending is not a cycle.** 5,319 of 5,394 have loop length
>   **one** — the node's step is itself — and all 5,319 are declarations with
>   **neither an annotation nor an initialiser**, so `step` falls through its
>   declaration arm into its reference arm and returns the node it started from.
>   Both rows are `NO STEP ARM` wearing the `cycle` label. The genuinely
>   recursive shape `depend.rs`'s own C2 describes is **75 lines**, not 5,394.
>   **`FunctionDeclaration`-cycle is therefore UNOWNED**: 2,943 lines, want-any
>   **1.8%** (net 2,889), 611 cases, top-1 3.7%, and **1,894 of them want a
>   signature** — an un-annotated function declaration whose own name gaps. It
>   is the most diffuse, lowest-want-any row above 2,000 on the board.
>   `BindingElement`-cycle (2,369, want-any 32.5%) belongs to `tsr-84iz` /
>   `tsr-pqnh`.
> - **`ArrayType` 2,120 is 99.7% propagation, not a root** — 2,114 of it has a
>   gapping *child* type node. Across all type-node kinds `step` has no arm for,
>   **4,696 of 8,499 lines are propagating** and belong inward.
> - **`depth cap` is not an item**: all 1,330 lines are two pathological cases.
>
> Two `depend.rs` fixes are named and deliberately not made, so the board stays
> comparable: relabel a length-1 cycle, and add `step` arms for
> `ArrayType`/`TupleType`/`UnionType`/`IntersectionType`.

### 4.0b The gap by what upstream's ANSWER LOOKS LIKE — new, eighth session

`cyclegap.rs` at `7299a14`, over the same 80,315 lines — and **re-run unchanged
at `0d0971a`**, after the parallel session's check traversal landed (it touches
`types_producer.rs`, so the re-take was not optional): every figure on this page
and in §4.0's correction is byte-identical across that merge. The first whole-gap view
that is not a node-kind histogram, and it re-frames the board:

```
any (ADR-0038) 19,970 (24.9%) | primitive 14,652 (18.2%) | signature/arrow 12,606 (15.7%)
bare name 6,411 | anonymous object 5,799 | array 5,152 | generic ref 4,177 | union 4,112
typeof query 2,109 | qualified name 1,425 | tuple 1,414 | string literal 1,406 | rest 1,082
```

**A quarter of the remaining gap wants `any`.** That is *not* §2's firm ceiling
of 2,202 — it is the unstable population §2 warns about (lines this port fails
to compute *and* whose baseline answer is `any`). Neither figure supersedes the
other and neither may be substituted for the other.

**"Signature / arrow" at 12,606 lines is the largest shape that is neither
ceiling nor primitive, and it matches no single board row** — `ArrowFunction`
3,328, the mislabelled `FunctionDeclaration`-cycle 1,894,
`PropertyAccessExpression` 908, `FunctionExpression` 832, `FunctionDeclaration /
dependency` 664, long tail. This is §4.4's *structured signature types*
capability measured from the answer side for the first time, and it is the
largest single thing the board has ever shown.

### 4.0c The ninth session's closing board, at `440d0ce`

`depend.rs`, fresh. **Gap 78,430** (was 80,315 at the session's open — −1,885
on the board's own denominator; the wrong bucket fell 36,767 → 36,094
beside it). What moved on the board itself: **`BinaryExpression / no further
dependency` fell 1,815 → 979 and its want-any share doubled to 54.2%** — the
`||`/`??` build harvested the computable half and left the ADR-0038 share
concentrated; `FunctionDeclaration`-cycle fell 2,943 → 2,531 (the signature
arms); `ArrayLiteral` 1,776 → 1,726. Every head row keeps its owner from
§4.0's correction; no new unowned row appeared.

### 4.0a The board it replaces, re-measured at `d9a730b` (sixth session)

`examples/depend.rs`, run fresh so this section's numbers are this session's.
**Gap 86,642 → 82,871 across the session** (−3,771). The single most useful
thing in it is a row that is *no longer there*:

```
  10,342  12.5%  PropertyAccessExpression   dependency types    want-any 47.9%   top-1 14.9%
   7,685   9.3%  Identifier, no VALUE       no further dep      want-any 73.9%   top-1 14.9%
   6,692   8.1%  CallExpression             dependency types    want-any 12.5%   top-1  4.2%
   4,827   5.8%  Identifier, member name    dependency types    want-any 43.7%   top-1 13.9%
   3,917   4.7%  ArrowFunction              NO STEP ARM         want-any  0.8%
   3,337   4.0%  NewExpression              dependency types    want-any 15.7%
   2,402   2.9%  Identifier, no value decl  no further dep      — the refused import-alias row
   1,563   1.9%  TypeReference              dependency types    (was 1,939)
       —         TypeReference              NO FURTHER DEP      — GONE. Was 4,010 and ranked 5th
```

**`TypeReference / no further dependency` was 4,010 lines at the start of this
session and does not appear at all now.** That is independent confirmation of the
qualified-naming build from a different instrument than the one that scored it:
`typerefgap.rs` said the row was 99.8% one mechanism, and removing that mechanism
removed the row.

Two head rows are *known* not to be items and must not be re-scored as such:
`PropertyAccessExpression / dependency types` is §4.3's downstream symptom
(`bd tsr-mcd`), and `Identifier / no value declaration` (2,402) is the import-alias
family refused on naming grounds (`bd tsr-4jk`, §5).

### 4.1 How the score is built, and what it is not

```
score = (reachable / effort) x feasibility
```

- **reachable** is *measured*: the root's gap lines minus its `want-any` share.
  It is a **ceiling, never a forecast** — this file's fourth rule. Observed
  conversion over the seven builds of the third session ran **15% to 57%** of
  the sized population (tuples 57%, property references 35%, member
  instantiation 18% ex-windfall, object methods 15%), so read a score as an
  *ordering*, not as a line count. The fourth session's `typeof` build then
  converted **122%** of its sized row (+1,958 against 1,600), because its
  mechanism — written-node reuse in signature prints — reached lines whose
  `depend.rs` root was not the `TypeQuery` node: a population is a ceiling
  *for the row it was measured on*, and a mechanism can turn out wider.
- **effort** is 1–5, anchored to builds that actually happened rather than to
  intuition: **1** = one arm on machinery that exists (tuples, object methods);
  **2** = a few arms plus new data (the six narrowing facts bits); **3** = a new
  side table or a reshape (signature instantiation); **4** = a subsystem with a
  partial already in place (contextual typing); **5** = a subsystem from
  scratch (overload resolution).
- **feasibility** is 0–1: are the prerequisites ported, and has the item been
  refused before *with a number*? This is the only judgement column, and it is
  the one to argue with.

### 4.2 The scored list

| score | item | reachable | eff | feas | file |
|---:|---|---:|---:|---:|---|
| ~~**~2,976**~~ | **LANDED, design P — `getSymbolChain`, +2,973. The board's top item, and the second refusal retired by re-measurement in two sessions** (`checker-notes-qualname.md` §10–§11). Sized post-W at **2,990 converts / 14 at risk / 213:1**, against a refusal that had stood four cycles on "lost 3,202". Bar registered at `90c4e70` **before any code**, four legs, all four PASSED — net 2,973, lost 17, cases regressed 0, cases gaining 174, and **gap→wrong exactly 0**. Conversion **100.0%** of forecast, because §10.5 registered in advance that this forecast was a point estimate rather than W's floor. Two facts that must travel with the number: **64.1% of the gain is `compiler/temporal`** (outside it, 1,072 lines), and 17 lines were lost — 9 to `bd tsr-4jk`'s alias chain, 4 to `getContainersOfSymbol`'s unported multi-parent ordering | ~~2,990~~ landed | 3 | — | `checker.rs` |
| ~~**~1,530**~~ | **LANDED at `8e28971`, +3,590 — 211% of the forecast, the session's largest build.** ~~qualified naming, design W — the written entity name. THE TOP OF THE BOARD, and it arrived by overturning a refusal rather than by finding a new row** (`checker-notes-qualname.md`). Reprint the written qualified name for a type reference whose leftmost identifier resolves as a namespace, declining when the reference site is **inside** the namespace it qualifies — a positional refusal upstream's own `needsQualification` licenses, worth 68 conversions to remove 79 wrong lines. Forecast **1,702 converts / 20 would-be-wrong / 0 at risk = 85× **, and the forecast is a **floor, not a ceiling**: 1,302 lines are excluded as unscorable rather than declining. Bar registered in §8 of that page **before any code** — `lost == 0`, `new wrong ≤ 40`, `gained ≥ 900` — with four falsifiers named. Effort 2: `resolve_entity_name` plus a written-text reprint, on a node the parser already records Residue, each with an owner: alias naming 20 (the missing `alias_symbol_for_type_node` call on the type-reference arm), enum narrowing 6, design **P** — the symbol chain on the *outer* name — 13 and now **created** by W's conversions, so §8's "W first, then re-measure P" is load-bearing rather than tidy | ~~1,702~~ landed | 2 | — | `declared.rs` |
| ~~**~1,400**~~ **REFUSED** | **type-argument inference's REMAINING LEGS — sized and refused, sixth session** (`checker-notes-infer2.md` §7). The 2,056 was a **row**: the mechanism's own population is 1,231, of which **77.3% is two things that are not the lattice** — 494 lines where no candidate is found at all and 458 downstream of a gapping argument. Both remaining legs measured as deltas: contravariant bucket 6 converts / 1 wrong, priority lattice 11 / 1, **~17 conversions together**, every leg's at-risk column **0** over 792 admitted right lines. The lattice **alone** is worse (11 / 4), so sequencing it first to dodge `strictFunctionTypes` does not rescue it. `bd tsr-g30h` closes. ~~old row:~~ **type-argument INFERENCE — the largest gate in the call funnel by 4x.** Re-measured at `a57a04b` (`callgate.rs`, sixth session): of 7,303 admitted call lines, **2,056 stop at "inference gapped"** at only **1.6% want-any**, against 497 more at "a generic candidate in the set". `bd tsr-g30h` landed the first slice (+372, structural candidate walk); the priority lattice and contravariant tracking are the remainder. **The contravariant bucket alone is refused at 6 own-node lines (§5) — that refusal is about the BUCKET, not about this row**, and re-reading it as a refusal of inference would be the "population identified by the shape of the answer" error | ~~2,056~~ 17 | 5 | — | refused |
| ~~869~~ ~~**~78**~~ **~41** | **call resolution — overload sets. RE-SCORED DOWN A SECOND TIME, sixth session**, on a funnel re-run after `SELECTABLE` was deleted (`d8590ff`). The gates overload selection actually owns now sum to **658 lines**, not 1,095: `any` parameter 324, undecidable pair 163, ambiguous return 81, nothing-assignable 67, arity 17, this/rest 5, spread 1. Want-any across them is ~200, so reachable is ~460. **Two of those gates are this session's own refusals and are working as designed**, and `undecidable_split.rs` shows the undecidable 163 is 92 signature-bearing + 41 no-members-table + 36 unported-flag + 16 absent-property. Three of the relation's six `Unknown` sites essentially never fire | 658 | 5 | 0.45 | `calls.rs`, `relater.rs` |
| ~~869~~ ~~**~78**~~ | ~~**call resolution — overload sets. RE-SCORED DOWN, fifth session.**~~ `bd tsr-klm` is answered (`callgate.rs`, `checker-notes-callres.md` §13): the 9,660 was **the row, not the mechanism**. Summed from the gates overload selection actually owns — generic candidate 490, parameter 473, argument 28, ambiguous 50, arity 9, nothing-assignable 39, this/rest 5, spread 1 — its own population is **1,095 lines, ~880 net**, an **8× smaller** item, and third of the three the split found | 1,095 | 5 | 0.45 | `calls.rs`, `relater.rs` |
| ~~360~~ | **type-argument inference — FIRST SLICE LANDED, +372** (`bd tsr-g30h`). The counterfactual forecast 131 own-node conversions and it converted 372 (2.8x, cascade). **`bd tsr-g30h` stays open**: the contravariant bucket / priority lattice owns both the 3 lost lines and the largest share of the 33 new wrong, which is the argument for it being the next leg. Tuples (42), object-type members and intersections refused with numbers in §5.4 of that page. ~~old row:~~ The largest gate in the corrected split: a single candidate resolves and *inference* is what stops, **2,324 lines, want-any 28**. `inference.rs`'s own doc measures the cliff — 53% of generic calls have no type parameter written bare, so the candidate must be dug out structurally. A subsystem (`inference.go:53`: priority lattice, contravariant tracking), which is why the effort is 5 and not 3 | 2,324 | 5 | 0.75 | `inference.rs` |
| ~~340~~ | **a `Named` callee — LANDED, +1,018.** The counterfactual computed the at-risk column in the same pass as the target one, exactly as `docs/conventions.md` requires, and it earned its keep: the naive arm read **646 converts against 377 wrong**. Four measured refusals cut that to **8 new wrong** — `unique symbol` and qualified naming each cost **zero** conversions. Remainder, each refused with a returning condition: 926 generic candidates (inference), 274 class instance types, 291 `unique symbol`, 75 qualified naming, 48 disagreeing overload sets, 27 heritage. ~~old row:~~ 2,791 lines across the call and `new` halves — lib constructor *interfaces* (`DateConstructor`, `MapConstructor`) and interface-typed callees. Needs `getSignaturesOfType` → `resolveStructuredTypeMembers`. **Carries a measured wrong-manufacturing risk**: §5 of `checker-notes-callres.md` records 264 `unique symbol` lines that would print `symbol`, and half the row wants a generic instantiation this port has no members for | 2,791 | 4 | 0.50 | `signatures.rs` |
| ~~649~~ | **destructuring / binding patterns — LANDED at `4b81458`** (`tsr-o00`). Sized by the new `bindgap.rs` at 1,228 buildable of 2,632 classified (~1,111 net); converted **+1,081 = 97% of the sized net**, above the band again via downstream unblocks. The row's residue belongs to its owners: contextual pattern parameters 604 (the re-armed contextual refusal), pattern-implied tuple inference ~250+155 (`tsr-84iz`), flow-of-destructuring (`tsr-pqnh`), rest 172, computed 86, no-source 113 | — | — | — | `destructure.rs` |
| ~~374~~ | **element access remainder — decomposed to shards, none an arm.** After the fourth session's builds the row is **634** (`elemgap.rs` at `935a221`): 177 string-literal misses (want-any 27%; head cases are `noImplicitAnyStringIndexerOnObject` — option modelling — and `mappedTypeRelationships` — mapped types, unported), 118 numeric-literal misses (lib-array receivers under literal indexes), 108 enum/named indexes (enum machinery), 86 other. Each shard belongs to an unported subsystem, not to `indexed.rs`; the row stops being a board item and its shards go to their owners. | — | — | — | split complete |
| **RE-OPENED, seventh session** | **JSX — the fifth session's blocker measurement is STALE**: `jsxfeas.rs`'s "46% cannot resolve the JSX namespace" was measured before the `/.lib` mount (`93b540a`), and react.d.ts declares a *plain global* `namespace JSX` that now loads. First-order size at `cc8c422`: **1,955 gap + 96 wrong** lines want `Component<…>`/`JSX.*` shapes over 220 cases; the whole tsx/jsx non-right population is 4,374. `bd tsr-fpti`; re-run `jsxfeas.rs` before costing. ~~old row:~~ **JSX — re-scored DOWN at the fifth session's `jsxfeas.rs`** (`checker-notes-jsx.md`): of 1,199 element gap lines, **46% cannot resolve the `JSX` namespace at all** (it sits behind `declare global` augmentation, unported in the binder) and **29% resolve but print bare `Element`** — the refused qualified-naming family (2.7 wrong/right). Feasibility ~0.25; blocked on global augmentation, then namespace-qualified naming | 1,199 | 4 | 0.25 | blocked — prerequisites named |
| ~~165~~ | **template literal types / `TemplateExpression` — REFUSED with a ratio, fifth session** (`checker-notes-tmplexpr.md`). The old grounds ("the cheap leg is not separable") were re-tested on the fresh 1,890 row and are now measured: the cheap leg alone converts 446 against 590 new wrong (**0.76 per wrong**), the folder-plus-fallback design 515 against 521 (**0.99**). Both are worse than every refusal on this page. The legs **interleave** — 151 lines want `string` exactly where the folder fires | 1,036 | 3 | 0.15 | refused |
| ~~165~~ | ~~**template literal types.**~~ Refused once: the cheap leg is **not separable**, because upstream's `evaluate` is a syntactic folder consulting no types. Kept on the list because the row survived the session unchanged. | 1,237 | 3 | 0.40 | `declared.rs` |

### 4.2a What is actually left, after a session that emptied most of §4.2

Six of the rows above are struck through and §4.3 lost two more, so the scored
list no longer reads as a board. Stated plainly, at `a57a04b`:

**Live and scoreable**

| lines | item | note |
|---:|---|---|
| 285+ / **1,437 loose-converts ceiling** | **the composite-print seam** (`bd tsr-2ghn`) — baked text inside signature prints cannot take site-aware naming. Fresh loose columns at `b228524`: **1,437 converts / 724 at-risk** (the pre-W 4,801 is stale), and the at-risk head is the *inside-the-container* family design P's shipped stop conditions already refuse — the loose model just lacks them. Refine the probe with the shipped predicates before building; the temporal-case concentration caveat travels with the converts | new, P1 |
| 658 / ~460 reachable | overload selection's own gates | score ~41; effort 5. Two of its largest gates are this session's own refusals working as designed |
| 2,276 / ~1,833 | the **callee-type** family — `callee type is not an object type` 1,398 + `identifier: symbol types as a non-object` 878 | **NOT PROBED. Start here.** The largest unexplored gate in the call funnel, and the only live row whose population has never been split by mechanism. A probe was dispatched this session and produced nothing; no partial instrument was kept, so there is no half-number to inherit. **The open question is whether the two gates are one mechanism or two** — plausibly both are "the callee does not type as something with signatures", but that is a hypothesis and testing it is the probe. Note `tsr-4sa`'s four positional refusals are prior art here, and **one of them (qualified naming) is no longer refused** — both halves shipped this session — so any bucket blocked on it must be re-examined rather than carried |
| ~~494~~ | ~~inference finds **no candidate at all**~~ **SIZED BY RE-INSPECTION, seventh session** — a fresh `infergen.rs` run at `cc8c422` shows the row's own sub-buckets are the *already-refused* legs of `tsr-g30h` §5.4 plus three small unported sub-mechanisms, each with its head named: rest parameters 56 (`getSpreadArgumentType`), spread arguments 39, `null`/`undefined` candidates 32 (93.8% one case, needs `getWidenedType`), return-not-rebuildable 88 (45.5% one case). The remaining ~279 are the structural candidate-walk shapes (object members, intersections) the `tsr-g30h` bar refused with numbers. **Not a board item** |
| **1,094** | **`getSpecifierForModuleSymbol` — the symbol chain stops at a module container** | **NEW, and it is the named prerequisite of a refusal.** `qualnamep.rs`'s *"why a chain was NOT built"* table: 44,982 need no qualifier, **1,094 stop because the container is an external module**, 229 have no container at all. `checker.rs:654` returns `None` there by design, because upstream names such a container with `getSpecifierForModuleSymbol` (`nodebuilderimpl.go:667`) and this port has no port of it. **Effort CORRECTED and the item SPLITS IN TWO**, from reading `nodebuilderimpl.go:1249` rather than from the row: the general path calls `modulespecifiers.GetModuleSpecifiers`, a whole package taking compiler options, a host, import-specifier preferences, resolution modes, `baseUrl` and a per-symbol cache — **effort 4–5**, not the 3 first filed. But `:1260` has a cheap **exact** branch: for an **ambient** module (`declare module "x"`) the specifier is the name with quotes stripped, and `IsAmbientModuleSymbolName` (`ast/utilities.go:1656`) is just *starts and ends with a quote* — **effort 1**. **The sizing question is therefore not "1,094"**; it is how many of the 1,094 are ambient containers versus real file modules, and that split decides whether this is a small effort-1 item or a large effort-5 one. **A corpus-level PRIOR, offered as a prior and not as the measurement**: an ambient module declaration appears in only **276 of 18,876 cases (1.5%)**, and `compiler/temporal` — a head case of this very family — contains **zero**. That is a count over *cases*, not over the 1,094 *lines*, so it cannot settle the split; it does say the cheap half is likely small, which would leave an effort-4–5 item that does not clear this board. **Take the real split before building either half.** `bd tsr-xpb8` |
| **810 / 779 net** | **the un-annotated `function f` types as its own signature** — `fnsiggap.rs`, eighth session | **NEW, sized, and small.** `cyclegap.rs` found the 2,943-line row (§4.0's correction); the split refuses **72.5% of it as downstream** — 1,016 a parameter gaps, 985 a returned expression gaps, 132 both — leaving 810 lines in **four** mechanisms: plain 366, `async` 228 (wants `Promise<T>`), generator 173, async generator 43. 548 of the 810 take no parameters. Half the row (1,468 of 2,943) is reached only through the cascade. **≈0.16 points at 100% conversion**, and nothing converts at 100%. Buildable and honest about its size; `docs/architecture/checker-notes-cyclegap.md` §7 |
| 399 | `arguments` | §4.3 — three mechanisms wearing one spelling, needs the split first |

**Found by `calleegap.rs`, sixth session** — the callee-type family is **not** an
item, but two things fell out of splitting it, both effort 1:

| lines | item | note |
|---:|---|---|
| ~~25~~ **32** | ~~delete the stale namespace refusal~~ **LANDED at `a662de1`** | predicted stale by the probe's header *before* it ran; all four legs passed |
| ~~213~~ **LANDED, +114** | ~~**`resolveUntypedCall`**~~ `a1cbd93` | `checker.go:9935` → `:9902` → `anySignature`. 16.4 gained per wrong **after** one positional refusal (decline when the `any` originates in an **unannotated parameter** — that is contextual typing, §5's 2,082-line refusal arriving through a new door, and refusing it removes 64 of 77 misses for 36 conversions). **Two risks, both stated: 47% is one case** (`compiler/duplicateLocalVariable1`, 107 of 121), **and it needs an ADR-0038 argument written, not assumed** — `anySignature`'s return is `anyType`, an honest computation, not a rendered `errorType`, the same argument §2 used when `Array<any>` collapsed the ceiling |

**Small, owned, and honest about being small**: accessors 78 (`bd tsr-32y`),
computed names 219, design P's outer symbol chain 13, discriminated-union
narrowing on `switch` (`bd tsr-5kii`, **SIZED, ninth session: ≤104 reachable lines** — `switchgap.rs`, `checker-notes-narrow.md` §8.1; an effort-3 mechanism that does not clear the board), the value-name convertible bucket 70 (a *lower* bound).

**The ninth session's builds point the next session at ONE re-measure:**
`removeSubtypes` (`tsr-eak`, refused at 1.03 gained-per-lost) was priced
before the ternary relation existed. The relation now answers
`Related / NotRelated / Unknown` (`tsr-kmzf`), so a reduction gated on
*every pairwise relation decidable* — declining the moment any pair reads
`Unknown` — is a design neither of `tsr-eak`'s numbers priced. It is the
named owner of the `||`/`??` residue, the conditional's non-safe pairs, the
two-object array literals, the multi-distinct return aggregates and the
`1 | 2` yield unions — five doors onto one mechanism, all found this session.
Counterfactual first, as always.

**Refused, each with its number** — §5. Do not re-derive: inference's remaining
legs (~17), the ternary as an item (0 marginal), template literals (0.76 / 0.99),
`removeSubtypes` (1.03), contextual typing (86% entangled, reproduced a **third**
time this session *after* three call-resolution builds), JSX (blocked on
`declare global` augmentation).

> **The honest read: no single item on this list is worth more than a few hundred
> lines, and §4.4's conclusion has now been paid for twice over.** The two builds
> that moved this session (+3,590, +2,973) did not come from this table at all —
> they came from re-measuring a refusal that had stood for four cycles on a cost
> belonging to a different design. **The highest-expected-value move available is
> not the top row of §4.2; it is re-measuring §5.** Two of the entries there are
> older than four builds, and the last three sessions have retired more gradient
> by re-reading refusals than by ranking rows.

### 4.3 Measurement first — cheap probes that unlock a score

None of these can be scored yet, and each is one probe. **Quoting any of these
populations as work would break this file's fourth rule.**

| population | why it cannot be scored | the probe |
|---:|---|---|
| ~~6,233~~ | **`BinaryExpression` roots — DECOMPOSED at `edaf0e4`** by the new step arm in `depend.rs`: own-root is **1,712 lines at 37.6% want-any (~1,068 net)**, top case `logicalOrOperatorWithEveryType` — i.e. mostly the `\|\|`/`??` family §5 already refused on `UnionReductionSubtype`; 738 more are one pathological depth-cap case; the remaining ~5,000 of the old row propagate to operand roots and were never this row's | measured — nothing left to probe |
| ~~3,855~~ | **`TypeReference` — MEASURED AND DISSOLVED, sixth session** by the new `typerefgap.rs`. The row is **4,010** at `a4e3991`, and **4,002 of it (99.8%) is one mechanism: namespace-qualified naming**, `resolveEntityName` refused. Another **471** arrives through the `dependency types` ending, so the family owns **4,473** of this root. The "~1,867 of unknown cause" the row was carried on does not exist — the cause is known and it is already refused (§5). All six controls read their expected 0 | measured — nothing left to probe |
| 3,739 | **property access, "the property has no type"** — a *downstream symptom*: the property's own declaration gaps elsewhere. `bd tsr-mcd` established this and it is not an item | follow to the type-node roots, which is how tuples were found |
| 399 | **`arguments` — the largest single NAMED mechanism `valgap.rs` found**, and the only one of its buckets that is neither want-`any` nor already refused. Top-1 case 36.6%, and the name is literally `arguments` on all 399. **Do not cost it as "synthesise `IArguments`" without splitting it first**: across the corpus's `.types` baselines the name resolves to `IArguments` 132 times, `any` 120, and to ordinary user declarations that merely share the name (`number` 78, `any[]` 46, `string` 37, `"arguments"` 26) — three different mechanisms wearing one spelling. This is `docs/conventions.md`'s *"a population identified by the shape of the answer is not thereby attributed to a mechanism"*, caught before anything was costed | split the 399 by whether the name resolves to a real declaration in scope, a synthesised function-scope `arguments`, or nothing |
| 2,223 | **object-literal remainder** — accessors (`bd tsr-32y`) and computed names both fall into the catch-all, in unknown proportion. Accessors are **not** a copy of the method arm: upstream prints an accessor as a *property* | split the catch-all by member kind |
| ~~1,425~~ **7,685** | **unresolved value names — RE-SIZED AND SPLIT, sixth session** by the new `valgap.rs`, and **the published 1,425 was a different cell**: `depend.rs` at `a4e3991` reads this root at **7,685 lines, 73.9% want-any**. Split by what the baseline wants, the bucket that could convert — *a candidate exists, types, and prints **exactly** what is wanted* — is **70 lines**, in three scope families (another file 27, out of scope 28, namespace body 15). **Two of its controls fired and are reported rather than tuned**: C3 (31 depth-0 lines declared nowhere yet wanting a real type) and C4 (the mirror reproduces 85.2% against a ≥95% bar, because the identifier arm does more than read the symbol — which makes the 70 a **lower bound**, not an upper one) | measured; the 70 needs its floor re-taken against a counterfactual that models the producer's arm, not `get_type_of_symbol` |
| ~~4,088~~ | **`FunctionDeclaration` rows — DECOMPOSED fifth session** by `retgap.rs` (`checker-notes-callres.md` §12): 879 return-annotation gaps + 507 parameter-annotation gaps belong to unported type nodes (template literal types, variadic tuples, `const` type-parameter modifiers), 797 are downstream return-expression gaps, 466 async/generator, 257 annotated-everything-types unsplit, **81 multi-distinct aggregate refused with its number (§5)**. Return-type inference itself is ported; the row is its inputs | split complete — shards to their owners |

### 4.3a The call row cannot reach 10% of the gradient, at any conversion

Asked out loud this session and worth a line, because the figure has been
carried informally. Measured, not estimated (`callgate.rs`, §13 of
`checker-notes-callres.md`):

```
admitted call+new lines (callee already typed)   8,398  =  1.75% of 478,954
the widest call-shaped population ever measured  20,721 =  4.33%   (18,294 carrying + 2,427 cascade)
observed conversion band                         15–57%
```

So **every call-shaped line in the corpus, converted at 100%, is 4.3 points**,
and the band puts a realistic ceiling for the whole family near **+0.6 to +2.5
points**. Call resolution is the largest *family* on the board and it is not a
double-digit item. The three mechanisms it decomposes into are scored above.

### 4.3b ~~Structural assignability is unported~~ — **CORRECTED, same session**

> **This section was WRONG when first written, hours earlier in this same
> session, and it is corrected rather than edited away.** It claimed
> *"`relater.rs` compares object types only to themselves"*. Structural
> comparison of object types landed at **`e24b7ca`**, *387 commits before this
> section was written* and an ancestor of the commit that wrote it:
> `properties_related_to` / `property_names_of` / `collect_property_names` walk
> every property of the target, own and inherited over base symbols, and six of
> `tests/relater.rs`'s eighteen tests assert it. Verified against the code, not
> taken on report.
>
> **The source of the error was `crates/tsr-checker/src/lib.rs`'s crate doc**,
> frozen at the day `checker_types` read 0% and still saying "Object types
> relate only to themselves — structural comparison is not ported", under a
> heading "Why `checker_types` still reads 0%". It was quoted here in good
> faith. `bd tsr-7wkn`; the file now carries a STALE banner naming this page as
> the authority.

**The numbers survive; the diagnosis does not.** The seven dependents are real
and still blocked — `selectable.rs`'s 303 object-parameter lines and 44 union
lines, `namedcallee.rs`'s 48 disagreeing overload sets, `||`/`??`'s 358 + 96,
`removeSubtypes`, the destructuring defaults, the `ArrayLiteral` row. What
blocks them is **not a missing relation**. It is that the relation cannot say
*"I could not tell"*.

`checker-notes-assign.md` §2 names six sites that answer `false` without
knowing — an unfollowable base, an absent property whose counterpart may be
optional, either side lacking a members table (function and index-signature
types never reach the structural arm at all), the depth cap, generic member
types, and signature-bearing types. The last is the load-bearing one: for
signature-bearing types the relation is unsound **in both directions at once**,
so *no per-type flag predicate can separate the trustworthy pairs from the
rest*. **Decidability is a property of the pair, not of either type** — which
is precisely why widening `SELECTABLE` with more flags cannot work, and
`checker-notes-selectable.md`'s refusal stands on better grounds than the ones
it was written with.

~~So the real item is a **three-valued relation**~~ — **MEASURED AND BUILT,
sixth session, and the diagnosis in that sentence was wrong.** `bd tsr-kmzf` is
closed; `checker-notes-assign.md` §5–§6 carries it.

Leg 1 forecast **33** conversions against a floor of 150 — the falsifier the
registration flagged as likely. **Control C3 fired harder and is the finding:
all 33 are conversions the EXISTING BINARY relation already makes.** The
cross-tab it forced carries no `[NEEDS the ternary]` row anywhere, and
`stringLiteralTypesOverloads01` settles why: its overloads take
`"boolean" | "string"` parameters, which the binary relation decides without
difficulty and which `SELECTABLE` — a **flag set** containing `STRING_LITERAL`
but not `UNION` — never *asked* about.

> **The blocker was the gate, not the relation.** This section's own premise
> conflated the two, for the second time on this item: §4.3b was wrong about
> whether the relation existed, and its replacement was wrong about what the
> relation lacked. Both times the numbers were right.

Three-valuedness is worth **zero conversions** and is load-bearing for the
*safety* of deleting the gate — the 52 `UNDECIDED` lines are precisely what a
naive removal decides wrongly. Both landed together: **+94 net, 0 lost, 6 cases
finished, 0 regressed, Δwrong +3**, against a fresh bar registered before the
code (§6.1).

**What this does to §4.4.** Assignability is first of the five capabilities that
four cycles of ranking said the remaining gradient hides behind. On the
population it was named to unblock it is now measured at **33 lines**, and the
whole build — reaching wider than its counterfactual, through property-access
callees the probe never classified — was **+94**. One of the five is answered,
and the answer is that it was not where the mass is.

### 4.4 What the scores say about 80%

- Distance to **80%** is **+47,184 lines**; **70% is crossed** (70.15% at
  the `tsr-tgov` build, measured on the coverage instrument).
- The scored list's *reachable* column sums to ~22,000. At the observed 15–57%
  conversion that is **+3,300 to +12,500** — so 70% is reachable from this
  board, and **80% is not**, even if every item on it lands.
- The rest is behind the five capabilities named repeatedly by four cycles of
  ranking: **assignability, call resolution, qualified naming, contextual
  typing, structured signature types.** Two of those five moved this session.

So the standing conclusion holds and is now quantified: **80% is reachable and
it is not reachable by ranking rows.** A session has to take one capability as
its whole deliverable and accept that it converts nothing until finished.

---

## 5. Refused, with the number that refused it

### New, this session, TS1016 and the true-positive audit

- **TS1016 was missing `parameter.Initializer == nil`** (§554). A *defaulted*
  parameter after an optional one was an error — the ordinary shape of a React
  hook signature. It is **not** the same test as the arm above it: §288 had
  correctly established that `seenOptionalParameter` is set by a `?` alone, and
  the initialiser exclusion lives on a different arm and was never added. Repo
  62 → 51, both snapshots byte-identical.

- **Every rule touched this session was deleted outright to find which test
  catches it** (§554). A fix that removes diagnostics can always be faked by
  removing the rule, so each of the eight — TS1016, TS1192, TS1361, TS2304,
  TS2339, TS2345, TS2686, TS7026 — now has a named true positive that fails
  when the rule is gone. **Four did not exist and were written for the audit.**

- **One true positive could not be written, and it is recorded rather than
  dropped** (§554). `globalThis.blockScoped` over a script's `let` must report
  TS2339 upstream; this port reports nothing, and **measurement shows §173's
  guard is not the cause** — §33's minted `typeof globalThis` has no members
  table, so the completeness gate declines first. The guard's block-scoped
  branch is therefore unreachable code today. Kept, because it is upstream's
  rule and becomes live when §33's mint grows members; pinned by a divergence
  test whose assertion flips when it closes.

### New, this session, optional parameters

- **An optional parameter's type includes `undefined`** (§541).
  `getTypeOfParameter` (`checker.go:17042`) adds optionality for a `?` **or** an
  initializer; the argument check took the written annotation alone, so every
  `T | undefined` argument at such a position drew TS2345 — 22 on a 22-package
  repository, upstream's answer for a *required* parameter and nobody's for an
  optional one. Both call arms had it, `call` and `new`.

  **Both conformance snapshots are byte-identical**, which is the notable part
  for a change that *widens* a type: the risk was a lost case, and there was
  none. The corpus's TS2345 fixtures use required parameters, because that is
  what a test for TS2345 looks like. Sixth defect this session only a real
  repository exhibits.

### New, this session, the synthetic default

- **TS1192 reported on essentially every default import in a React codebase**
  (§531) — 78 on a 22-package repository, 12 in one package. Its report is the
  third of three conjuncts (`checker.go:14566`) and only the first was ported;
  `canHaveSyntheticDefault` is what makes `import React from "react"` legal
  against `export = React`, which is how every `@types` package ships.

  **Both conformance snapshots are byte-identical** before and after: the corpus
  has no `.d.ts` written `export =` and imported as a default. Fifth defect this
  session the suites cannot see, fifth found on a real repository.

- **`IsDeclarationFile` is a host question here** (§531), for
  [ADR-0016](docs/adr/0016-file-info-not-a-file-name.md)'s reason: no file name
  on the AST, and `FileContext` describes only the file being checked. Defaults
  to `false`, which sends a module down the stricter `export =` arm — reporting
  where upstream might not, rather than the reverse.

### New, this session, heritage positions

- **A shared predicate is not a shared decision** (§502). An interface's
  `extends` is a *type* position — `isIdentifierInNonEmittingHeritageClause`
  (`ast/utilities.go:3132`) — and this port keyed the distinction on the
  `extends` **keyword**, which an interface's clause also uses. Worth **19
  TS1361 and 1 TS2686** on a 22-package repository, all false, all on
  `interface P extends VariantProps<…>` over an `import type`.

  Answering it once in `is_value_reference` silenced three rules and lost
  `compiler/protoAssignment` — upstream **does** report TS2304 for an
  unresolved interface-heritage name, through `resolveEntityName` at type
  meaning. Three rules, three different upstream gates: TS1361 has
  `IsValidTypeOnlyAliasUseSite`, TS2686 has `meaning&Value == Value`, TS2304 has
  none. The suite showed the loss only as `1,994 → 1,993`.

- **`core.Every` read as `any`** (§502). TS2686's declaration test. A UMD `.d.ts`
  merges a `ModuleDeclaration` and a `NamespaceExportDeclaration` into one
  symbol, so `any` made every reference to `React` inside `@types/react` an
  error. Upstream's second disjunct is not ported — it needs per-file global
  exports this binder does not keep — which makes the test stricter and the rule
  report less, the safe direction.

- **Residual, recorded not groomed**: three TS1361 remain on that repository,
  all at one `new QueueEvents(...)` behind a **plain** import through `bullmq`'s
  all-`export *` barrel. A different mechanism from the above and pre-existing.

### New, this session, `symbolIsValue`

- **A stale "not ported" note outlived the reason that wrote it, and cost 1,384
  diagnostics** (§400). `symbolIsValueEx` (`checker.go:22095`) has two
  disjuncts; this port had the first. An alias's own flags carry no `VALUE`
  bit — which is *why* upstream has a second — so every module export written
  as a **specifier** rather than a declaration answered "no property", and
  `nonexistent_property` reported TS2339 on it. Every `index.parts.d.ts`-style
  barrel: `import * as P from "…"; P.Root`, **1,384 against `tsc`'s zero** on a
  22-package repository.

  The note said *"nothing follows aliases yet"*. `get_symbol_flags` had followed
  them for sessions, and `get_type_of_alias` three functions away already took
  its own `VALUE` test over it and explained why. **A decline is only as good as
  the fact that justified it, and nothing re-checks those facts.**

- **The suite's summary hid a regression the tool named in one run.** Landing
  the alias half without upstream's `excludeTypeOnlyMeanings` guard read
  `1,898 → 1,896`; `examples/casequery.rs` named
  `conformance/exportNamespace3` and `conformance/importEquals2`, both of which
  test that a **type-only** re-export must not become a value. The transitive
  walk to answer it already existed as §121's
  `type_only_alias_declaration` — a syntax-only copy was written first and
  discarded, and it would have been wrong in exactly the way §120 records.

### New, this session, TS2353 through a type assertion, 33 cases

Refused by **§306**, after §303–§305 published **three wrong attributions** and
an instrumented probe replaced them with a measured one.

The arm was **firing the whole time**. `diagcase` on `arrayCast`: the wanted line
at (3,23) **and an extra at (6,23)** — the fixture's own control, `[{ foo: "s" },
{}]`, where a second element widens the array literal's type to `{}[]` so nothing
is excess. ***The `+0` was a right line and a wrong line cancelling, and all
three earlier attributions assumed it was a decline.***

**Owner: excess-property checking belongs inside the relation**, where the array
literal's own widened type is known. An element-by-element hop is right for a
single-element array and wrong the moment a second element changes the array's
type.

### New, this session, the index-signature sequence, 7 cases

Refused by **§292**, after three measurements (`+4/WRONG 22`, `+4/WRONG 21`,
`+0`). Owner: **`isValidIndexKeyType` and `isGenericType`**
(`grammarchecks.go:826`–`:832`).

**The sequence is *ordered* and its two type-reading guards sit in the middle.**
Without them every later guard reports at a position upstream never reaches —
`arraySigChecking` wants TS1268 at (11,17) and got TS1021 at (11,16).
***An ordered guard sequence cannot be ported in fragments.***

The third measurement is the one worth keeping: **the syntactic guards alone
move zero cases** — and **§293 corrected why**. It is not that this port covers
them with other codes (the `occupied` column shows those positions empty). It is
that **their codes have no blocked cases at all**: TS1017/1018/1019/1020/1022/1096
are absent from all 442 rows. *Before porting a rule, check that **its code** is
in the gap — not its function, not its neighbours.*

### New, this session, TS7005 — variable implicitly has an 'any' type, 7 cases

Refused by **§286**, after two measured bounds (`−35`, then `−4`). The
declaration arm (`checker.go:18347`) is real and two fixtures are its shape, but
the row's mass is the **evolving-`auto`** path (`:11186`), which reports at a
**use** and pairs with TS7034 at the declaration. Reporting at the declaration is
a *wrong position*, which costs a case exactly as much as a missing line.

**Owner: the evolving-`auto` path** — `autoType`/`autoArrayType`, the flow type
at each reference, and TS7034 as partner; `is_evolving_array_operation_target`
and `convert_auto_to_any` both unported. The declaration arm cannot ship alone
because every variable it would report on is one that path claims first.

**This sharpened `diagslice`'s limit (§273):** it counts *lines per case* and
does not know **where** they are. `diagcolumn` knows about positions, and only
for codes this port already emits. Nothing connects the two.

### New, this session, TS2783 — spread overwrites a property, 9 cases

Refused by **§251**. `checkSpreadPropOverrides` (`checker.go:13371`) tests
`right.Flags&ast.SymbolFlagsOptional == 0`, and **`SymbolFlags::OPTIONAL` is
declared here, read in three places, and set in none.** Without it every
property of a spread type looks required and the rule over-reports on exactly
the optional ones.

**Owner: `SymbolFlags::OPTIONAL`, unset since the binder was written.**

### New, this session, TS2323 — duplicate exported names, 8 cases

Refused by **§246**, after three bounds and three measurements. Owner:
**`declareSymbol`'s fresh-symbol-on-conflict behaviour** (upstream
`binder.go:202`), which this port does not reproduce.

On an exclusion conflict upstream gives the offending declaration a **fresh
symbol**; this port's binder **merges** it into the existing one. So upstream's
export entry carries only the declarations that merged cleanly and
`exportedDeclarationsCount` never reaches two — while ours carries all of them,
and every name the binder already complained about looks like a duplicate export.
`duplicateDefaultExport` wants TS2528 alone and `exportInterfaceClassAndValue`
wants TS2451 alone; both got the right diagnostic **and** TS2323 on top.

**This is not local to TS2323.** It is a difference in what a symbol's
`declarations` list *means*, and any rule that **counts** declarations rather
than looking one up will read a list this port assembles differently. TS2323 is
the first. It will not be the last.

### New, this session, TS2303 — circular import alias, 10 cases

Refused by **§237**, after being built and reverted. All ten are
`namespace M { import A = B; import B = A; }` — an **entity-name
`ImportEqualsDeclaration`**, which `resolve_alias` (`symbols.rs:568`) does not
dispatch at all. The diagnostic cannot fire because the resolution that would
cycle never runs.

§236 also claimed the missing cycle guard was a latent hang. **Two mutations
disproved it**: a unit test for two files re-exporting each other passed with the
guard disabled, first without a `ModuleHost` and then with one. A pure re-export
cycle cannot form — `get_export_of_module` answers `None` at the first hop
because neither file declares the name. *"There is no guard" and "a cycle can
happen" are two claims, and only the first was checked.*

**§238 built that arm and §239 measured it: `+0` diagnostics and `−40`
`checker_types`.** TS2303 does not fire even with the arm *and* the guard —
`import A = B` resolves in **one hop** and never re-enters `resolve_alias`,
because `dontResolveAlias` is true there. §237 named an owner and the owner
turned out not to own it.

The 40 lines also give `checker-notes-nameres.md` §14's older refusal of this
arm the number it never had. `checker_types` is the other workstream's rail; 40
of its lines for zero diagnostics is not a trade at any exchange rate.

**Real owner: cycle detection lives in the individual chain-walkers.**
`get_symbol_flags` (`symbols.rs:424`) carries its own local `seen` set and
*silently absorbs* the cycle upstream reports. Reaching TS2303 means moving
detection into a shared resolution stack — a refactor of alias resolution, not
an arm, and bigger than what §237 named.

### New, this session, TS7026

- **A rule scored only against the corpus is a rule scored against one dialect**
  (§221). TS7026 landed at +14 cases (§189) and reports **1,642 diagnostics on a
  22-package repository where `tsc` reports zero** — one per JSX element. It
  ported `getJsxNamespaceAt`'s *third* road and neither of the first two, on a
  rustdoc claim that upstream uses the `@jsx` pragma "when one is set, otherwise
  the global `JSX`". There is no "otherwise": road 2's name is `React` by
  default, and `@types/react` 19 has **no global `JSX`** — `namespace JSX` lives
  inside `declare namespace React`.

  **The two halves are each other's blind spot, and this is the cleanest
  instance of it the project has.** §211 built the pragma half concurrently: it
  moved the corpus wrong column 9 → 3 and moved the repository by **zero**,
  because no real project writes a pragma. This half moves the repository to
  **zero TS7026** and the corpus by **not one line** — four snapshots
  byte-identical. Neither instrument could have found both.

  Third defect this session no suite here can reach, after §200's
  ambient-module key and §202's collision. All three were found by pointing the
  binary at a real repository. **The corpus is a compiler test suite, not a
  sample of how TypeScript is written**, and a rule with no reachable oracle
  gets unit tests instead — `crates/tsr-checker/tests/jsx_namespace.rs`, six
  cases against five mutations.

### New, fifteenth session, `declare global`

- **`declare global` merging is no longer refused — it is BUILT** (§173). §5 had
  carried it for many sessions on §13's "12 conversions for 47 wrong", a number
  measured against TS7026 alone; §171 measured the merge itself for the first
  time and reverted it at `LOST 1`. Rebuilt, and the LOST is explained rather
  than bounded: it was `compiler/extendGlobalThis`, and it was two *checker*
  gaps the merge made reachable, not a defect in the merge. Landed at
  **`binder_symbols` unmoved, `diagnostics` with no case changing verdict in
  either direction, `checker_types` +2 cases and +292 lines.**

- ~~**Ambient module symbols share a key with value globals**~~ **FIXED** (§175).
  Upstream stores an ambient module symbol under the *quoted* specifier
  (`binder.go:311`), so `c.globals` holds `"process"` and `process` as two
  names; this port stored it unquoted, and `@types/node` declares both
  `declare module "process"` and a global `var process`. It was worth **79
  spurious TS2300 and 13 spurious TS2649 across a 22-package monorepo, and zero
  conformance cases** — the corpus contains no such collision, so only a real
  repository could score it. The rename took that repo from 1,550 to **1,440**.

  Two findings outlive the fix, and both are about instruments rather than the
  binder:
  - **Five call sites had open-coded `tryFindAmbientModule`'s lookup** and all
    five went silently dead when the key changed; only one was findable by grep
    beforehand. Two of them were not even equivalent to each other, which is
    what made the first failure confusing rather than obvious.
  - **`binder_symbols` fell to 8,350/8,459 on a strictly more faithful change**,
    because the suite normalised upstream's names and not ours. An oracle that
    compares two sides has to normalise both, and the asymmetry reads as a
    regression in the code.

- **`binder_symbols` at 100% is not a binder that is finished** (§173). The
  suite compares *declaration positions*, never *scope*. ~40 corpus cases use
  `declare global` and passed for six sessions with the feature entirely absent,
  because binding into the wrong table still puts every declaration on the right
  line. Building it moved the suite by **zero**, as predicted before the work
  started. This joins the three same-shape defects already on record (static vs
  instance members, block vs function locals, per-class type parameters), three
  of whose four fixes also moved it by zero. **Closing the gap means a
  resolution suite, not a stricter symbols one.** Written up in
  `docs/architecture/binder.md`, "What the `.symbols` oracle cannot see".

### New, thirteenth session, `diagnostics`

- **The binder-merge item is now the board's LARGEST, at ~52 cases** (§141), and
  its refusal is quoted from a stale number. TS2451's 10 sole-obstacle cases
  turn out to be **global and cross-file merging**, joining TS7026's 28, ~13 of
  TS2454's (§85) and TS1362's last line (§130). **§5 has refused this for
  thirteen sessions on §13's 12-conversions-for-47-wrong — measured against
  TS7026 ALONE.** That is the price of one row inside the subsystem, not of the
  subsystem. **Re-measure it as one build across all four rows before quoting
  the refusal again — and §142 captured the before-state so there is something
  to beat**: `RULE_CODES = [7026, 2451, 2454, 1362, 2300, 2567]` gives
  **CONVERTS 320, LOST 0, STILL SHORT 118, RIGHT 4,354, WRONG 98**. `STILL
  SHORT` 118 says even a perfect merge leaves most of that population needing
  something else, so ~52 is the honest ceiling; `WRONG` 98 says the next attempt
  must report the **delta**, not the total.
- **A refusal is only as good as the state it was measured against, and this
  repo records the refusal without the state** (§142). §13's number was taken
  once, against a corpus 104 cases further back, and quoted for thirteen
  sessions as current. **Capture a baseline beside every refusal.**

- **A RULE CAN BE DELETED BY A MATCH ARM AND THE SUITE WILL NOT NOTICE** (§140).
  A new `Node::InterfaceDeclaration` arm landed ahead of §112's decorator arm
  and silently swallowed it; `rustc` said `unreachable pattern` and **the
  conformance numbers did not move**, because TS1206's interface case has no
  corpus fixture. `check_node`'s dispatch is 40-odd arms — **read the warning;
  the suite is not a backstop for this.**
- **TS2428 landed at +7 cases and 52 right lines** (§140), comparing type
  parameter lists syntactically. **Bar missed at 8 wrong**, a debt: they are the
  *constraint* half, owner `getDeclaredTypeOfSymbol`'s local type parameters,
  target 7 conversions under 3 wrong.

- **TS18013 landed at +11 cases** (§138) — a `#name` is **lexically scoped**, so
  the test is syntactic: walk out and report unless an enclosing class declares
  it. Missing column went **22 lines / 12 cases → 1 line / 1 case**. **Bar missed
  at 10 wrong**, and **§139 paid half of it: `any` alone takes wrong 10 → 7 at
  zero cost.** The broad gate (`any` + `error` + `unknown`) halves the column but
  **costs two conversions** — upstream still reports through an `errorType`
  receiver, because its check asks whether the *class* declares the name and
  reaches that even when the receiver is unresolved. **Try the narrow gate
  before the broad one**; a decline too broad costs as much as one too narrow.
  The seven left need the type's **members** (index signatures) — owner
  `get_index_info_of_type`, `checker_types`' road.
- **"Cheap grammar codes" was too narrow a name.** §105 called that seam closed,
  §112 found TS1206, and §138 is the **third** double-digit row produced by
  re-taking `diaggap` after a run of builds — and it is *scoping*, not grammar.
  **The real class is: a code needs no subsystem if its rule is decidable from
  the tree.** Re-take `diaggap` before believing any seam is closed.

- **TS1100 and TS7026 are CHECKED unbuilt, not believed unbuilt** (§137).
  §136's grep rule run on both: no strict-mode tracking exists anywhere in
  `tsr-binder`, and `GlobalExports` covers only a UMD `export as namespace`
  claim, never `declare global`. §105's and §5's pricings stand. **The rule hit
  three times this session and missed twice — worth the two greps either way,
  since a miss upgrades an inference into a verified negative.**

- **TS2303 is UNREACHABLE, not unported** (§136), and the pricing was already in
  the tree: `resolve_alias`'s rustdoc records that upstream's circularity frame
  **was written here, measured, and removed for cause** — this port's
  `resolve_alias` is not self-recursive, so the frame could not fire. Reaching
  TS2303 needs `resolve_alias` to become **transitive**, a `checker_types`-shaped
  change. Ten cases, off this workstream's critical path.
- **Refusals recorded in a doc comment never reach the board** (§136). `STATUS`
  §5 carries refusals with numbers; nothing carries the ones made at the point
  of code, and `resolve_alias`'s note is a full refusal — measurement, reason,
  falsifier. This workstream carried TS2303 as open for five sessions because of
  it. **Third item this session whose answer was already in the repo** (§134 and
  §135 were both in §81). **When a row will not move, grep the crate for the
  function upstream reports from, before pricing it.**

- **TS1361/TS1362 — LANDED, §121, +5 cases and +22 right lines** by walking the
  alias **chain** with the existing `resolve_alias`; the chain is worth 3 of the
  5 cases over §120's single hop for one extra wrong line. **BAR MISSED at +8
  wrong**, recorded as a debt in §104's shape. **Residual PAID, §123: wrong 8 → 5.** §122
  claimed the owner was unknown and the lines were not this rule's; **both
  claims were wrong.** The row emits eight of its own, and the clause that
  wanted them is `isPartOfPossiblyValidTypeOrAbstractComputedPropertyName`
  (`ast/utilities.go:3143`) — a computed property name on an `abstract` member
  or in an interface/type literal is erased. §121's attribution was right all
  along. **§125 took it to three.** §124 declared the
  `export =` family untouchable on the strength of the fixture's own `// Error`
  comment; `importEquals1.errors.txt` puts TS1361 on the four **consumer** files
  and nothing on the re-exporting one, so those two lines were plain false
  positives and declining cost nothing. **§126 read both**: `computedPropertyName`'s baseline stops at line 24 and ours
  fire at 32 and 36, so they are sites the abstract/`TypeLiteral` clause excuses
  and §123's port does not reach; `mergeSymbolRexportFunction`'s baseline has
  **no TS1362 anywhere**, making ours a pure false positive — suspect the
  merged-symbol hazard §97 declined for. **§127 implemented both named predicates and
  neither removed a line** — six inferences on these three lines, two right.
  **Reading the baseline improved the DESCRIPTION of the residual without
  improving the hit rate on its CAUSE**, because the baseline says what upstream
  reports and these lines are about why *this port* reports something extra,
  which no upstream artefact answers. **§128 ran that probe and it named the defect
  in one run: `getTypeOnlyAliasDeclarationEx(result, SymbolFlagsValue)`
  (`checker.go:1861`) takes a MEANING argument that this port's walk does not
  have**, so the chain follows an alias to a type-only *export* specifier and
  emits TS1362 for a symbol whose only declaration is an `ImportSpecifier`.
  **§129 then inferred what that meaning should DO and was wrong again** —
  measured unchanged at 3, reverted. **Seven inferences on three lines, two
  right, and both of those were cases where an upstream artefact stated the
  answer outright.** Every inference about *this port's internal behaviour* has
  failed. **§130 ran that probe and it answered: the TS1362 line is a MISSING BINDER
  MERGE, not a rule defect.** The chain reaches a type-only re-export; upstream
  reaches it too and stays silent because `Row` also names a **variable** and
  its `result.Flags&Value == 0` conjunct fires. This binder never merges the
  import alias with the variable, so `resolve_name` returns an alias with no
  `VALUE` bit and the conjunct cannot fire — restoring it changed nothing.
  **Owner: `tsr_binder`'s symbol merging**, the subsystem §5 names for TS7026.
  The case has been called `mergeSymbolRexportFunction` the whole time.
- **The probe that works prints every step of the thing you are guessing about**
  (§118/§128/§130): §118 probed below an early return and learned nothing, §128
  probed the symbol and found a missing argument, §130 probed the **loop** and
  found the loop was never the problem. **§131 then explained the last two by READING THE FIXTURE** — they are extras,
  not misplacements, and they are the `abstract` and ambient clauses §123
  skipped (`member_is_abstract` lacks the signature member kinds;
  `NodeFlags::AMBIENT` is never set, so §99's
  `declaration_is_in_an_ambient_context` is the substitute). **Nine sections
  attributed these lines from every artefact except the 19-line test file.
  `diagmissing` prints case names, `diagcase` prints line numbers, `diag2307`
  prints columns — none of them prints the code. Read the fixture before the
  fourth hypothesis.**
- **§132 built §131's two clauses and only the ambient one worked** (wrong
  3 → 2): `declare class H` routes through §99's ambient walk, but the abstract
  member is **neither `MethodSignatureDeclaration` nor
  `PropertySignatureDeclaration`** in this parser. **Reading the fixture
  explained which upstream clause applies — not what this parser produced**, and
  those are different questions. **§133 ran that probe and it overturned §132**:
  `MEMBER kind=MethodDeclaration abstract=true` — the clause was **already
  working**, and "adding signature kinds changed nothing" meant "already
  handled", not "wrong kind". The residual includes an **object-literal**
  computed name that no ported clause covers and upstream would report too, so
  the divergence is in the **symbol** resolved there, not the site. Left
  unattributed: thirteen attributions on this row, same failure each time.
- **Only a probe answers "which code path ran, with what"** — and it has been
  right every time it was run at the correct depth (§128 the symbol, §130 the
  loop, §133 the member). `diaggap` answers *which cases*, the baseline *what
  upstream reports*, upstream's Go *what upstream computes*, the fixture *which
  language rule applies* — **each of those four has produced a confident wrong
  answer on this row.**
- **§134 CLOSED the row: `class G { declare [onInit]: any }`.**
  `declaration_is_in_an_ambient_context` reads `declare` on the kinds that
  *contain* members and never on a member itself, so `declare class H` was
  caught and `declare [onInit]` was not (+1 case, wrong 2 → 1). **§81 recorded
  this exact gap three sessions ago and warned that every rule taking `ambient`
  for a member is one `declare` away from it.** Nine sections rediscovered it.
  **`checker-notes-diag2.md` is 135 sections and is itself an instrument nobody
  reads — grep it for the symptom before probing a fourth layer.**
- **§81's ambient-member hazard is now CLOSED, not outstanding** (§135). All
  four rules that take the walk-threaded `ambient` for a class member were
  checked: TS2564 and TS7010 already read the member's own `declare`, TS1361 was
  fixed by §134, TS2464 was the one gap and measures a **verified zero** (no
  corpus case writes a `declare` member with a computed name). The four-row
  table in §135 is what makes this re-checkable. **`NodeFlags::AMBIENT` would
  collapse all four into one flag read and is still set by nothing — the third
  time this session that flag family has been the root of a bug.** **Six inferences, two right,
  then one probe that cost less than any of them — every wrong guess reasoned
  from artefacts describing *upstream*, while the defect was in what this port
  did with an argument it never had.**
- **FOUR instrument failures this session, all one shape — the artefact decides,
  not the description of it.** §118 a probe below an early return; §122 a `grep`
  over a `head`-truncated listing; §124 **a fixture's `// Error` comment read as
  a baseline**; and §88's wrong column pricing *lines* where `extraonly` prices
  *cases*. **ADR-0006 already says this about `ast.json` versus generated Go.
  It generalises: read the baseline.**
- **`head` and `grep` compose into a SILENT FALSE NEGATIVE** (§123). §122's
  "the wrong column holds no TS1361" came from filtering a `head`-truncated
  listing: the filter matched nothing because the lines were **cut off**, not
  because they were absent, and two conclusions were written into this file on
  that basis. Same failure as §118's probe below an early return, one layer out:
  **a query that can return empty for two different reasons has told you
  nothing.**
- **An attribution is a hypothesis** — three were disproved by measurement this
  session (§118 by §119, §115 partly by §116, §121 by §122). Each was plausible,
  each was cheap to overturn **because it was written down with a number
  attached**. **Isolate the code alone before believing which rule owns a
  line.**
- **(superseded by §121)** TS1361/TS1362 — refused at +2 cases for +7 wrong (§120), and §119's
  hypothesis is now **confirmed with a number**: this port's alias symbols
  answer `VALUE`, so `check_value_identifier` returned at its VALUE early return
  before any type-only test; moving the test there makes the rule fire. The 7
  wrong lines are the missing **alias chain** —
  `getTypeOnlyAliasDeclarationEx` (`checker.go:1861`) follows re-exports to find
  *which* declaration carried the `type`. **Owner: that function.** The 18 cases
  stay on the board, now at a measured price rather than an asserted one.
- **§115's "resolver-owned" pricing survives §116**, and the distinction is
  sharp: **TS2302 needed only that resolution had been ATTEMPTED** (so moving
  the test two lines down was enough, §116); **TS1361 needs what resolution
  FOUND, one hop out** (so no ordering trick helps, §120).

- **§118's conclusion was WRONG and §119 corrects it.** There is **no traversal
  coverage gap**: no unit is skipped and `component.ts` is checked with
  `parse_errors=false`. §118's probe sat at `check_value_identifier`'s meaning
  ladder, *after* the early return taken when a name resolves with `VALUE` — and
  this port's alias symbols answer `VALUE`, so everything returned before
  reaching it. **The silence measured the probe's position, not the rule's
  execution.** Corrected rule: the print must be at the rule's **entry**, not at
  the branch you care about; a probe that can be silent for two reasons has told
  you nothing. Likely real cause, **unmeasured and claimed as such**: upstream
  tests `Alias && !Value` (`checker.go:1860`) and this port's aliases carry
  `VALUE` — a symbol-flags divergence of the §93/§95/§100 family, owner
  `classify`'s `S::ALIAS`. **§102's audit passed that flag as "not a collapse",
  which was true of the flag and silent about the meaning it answers.** The 18
  cases stay on the board, same owner, same size.
- **(superseded, kept for the sequence)** §118 claimed the rule's host never runs.
  `TSR_DEBUG_1361` at `check_value_identifier` prints **zero lines** for
  `conformance/computedPropertyName` — no identifier in the case reaches the
  rule at all. TS1361/TS1362 is **not a rule gap**; it is `crate::check`'s
  traversal not visiting the case's files. **Test `file_has_parse_errors`
  first** — "zero identifiers, not few" is its signature — and note this may be
  worth far more than 18 cases, since a traversal that skips a file skips
  *every* rule on it. **Seventh payment for "print whether the rule runs before
  asking what it decided", immediately after §117 skipped it and paid a build.**
- **(superseded by §118)** TS1361/TS1362 — a byte-identical ZERO (§117). §116's pattern (the site is
  already there, occupied by a silence) pointed at
  `check_value_identifier`'s `ALIAS` arm; reporting there changed **nothing** —
  the rule never fires. Not a wrong predicate: something upstream of it. Three
  candidates recorded, and the first to check is whether the parser sets
  `is_type_only` / `ImportClause::phase_modifier` at all — **a field that exists,
  compiles and is set by nobody is now a known failure mode of this port**
  (`NodeFlags::YIELD_CONTEXT`, §104, was the same).
- **The attempt skipped the workstream's own rule** — *print whether the rule
  runs before asking what it decided* — and paid a build to learn it the
  expensive way. That rule has now paid six times and been skipped once.

- **Every remaining relation-free row on the `diagnostics` board is
  RESOLVER-owned** (§115). TS2302 (9 cases), TS1361/TS1362 (18) and TS2303 (10)
  are three codes in one subsystem — each needs a name to have been *resolved*,
  where everything this workstream built cheaply needed only the tree. **The next
  build is a `tsr_binder` `resolve_name` build and it pays for 37 cases at
  once**, plus TS1100's 12 behind the same crate's strict-mode state. Falsifier
  recorded: two "seam is closed" claims this session were premature, so re-take
  `diaggap` after the next `.types` landing before believing this one.

- ~~TS2302 in the checker — REFUSED at +137 wrong lines and a grown LOST~~
  **(§114) — LANDED by §116 at +8 cases and ZERO new wrong**, by moving the
  identical predicate **two lines down**, to after resolution has failed instead
  of before it. A name that resolves cannot reach the second site. **Before
  concluding a diagnostic needs a subsystem, check whether it needs that
  subsystem's ANSWER or merely its having run** — §114 needed only the first
  half of "did this resolve, and to what", and the checker already had it.
  §115's "the rest is resolver-owned" now rests on two rows and 28 cases, not
  three and 37. Original refusal text follows.
- **(superseded by §116)** TS2302 in the checker — refused at +137 wrong lines
  (§114). The rule belongs **inside the resolver**: upstream tests
  `lastLocation.IsStatic` on the branch where the name has *already resolved to
  the type parameter symbol* (`binder/nameresolver.go:178`), so a checker-side
  version fires on every name that merely spells the same as a type parameter
  while resolving to something else. **Owner: `tsr_binder`'s `resolve_name`**,
  which needs the `lastLocation` parameter upstream's resolver carries. Do not
  re-attempt in the checker; this is its number. 9 cases stay on the board.
- **An idea that generalises is not an idea that transplants** (§114). §108 and
  §109 established that a scope boundary is a property of the **edge**; §114 had
  that same idea right and its **host** wrong — knowing which child you came
  through is useless where the answer is not yet known.

- **A row can be "blocked on exactly one code" and still not convert when that
  code lands** (§113). `diaggap` priced TS1344 at 9 sole-obstacle cases and 3
  converted, with `STILL SHORT` at 6 — because that column is computed against
  the **current** output: a case missing five lines of a code counts once, and
  emitting four of them converts nothing. Take the bar off cases *and* check the
  line-to-case ratio; TS1344's was 15:1.
- **An upstream function's NAME can be a misnomer and the dispatch is the
  authority** (§113). `checkStrictModeLabeledStatement` (`binder.go:1433`) is
  called unconditionally from `bindWorker` (`:639`), not under a strict-mode
  gate. Gating on the name would have produced a rule that fires on none of its
  nine cases. **Read the call site, not the identifier.**

- **A `STILL SHORT` of zero is a row's own completion signal** (§112, the first
  one this workstream has seen). TS1206 converted 8 of its 10 cases and left
  `STILL SHORT` at **0**, which says the remaining lines sit in cases needing
  *other* codes too — so the unported parameter arm is worth nothing until those
  land. That is a stronger statement than "3 lines left" and only that column
  shows it.
- **An anchor naming a real file can still fail** (§112): `dts_emit_suite.rs`
  wrote `vendor/typescript-go/internal/…` where the checker already resolves
  relative to the vendor root, so it looked for the prefix twice. The error
  reads *"no such file upstream"*, which points at the submodule rather than at
  the path.

- **A class-expression contextual decline for TS7006 — REFUSED at −4 right
  lines** (§111). `return class { static m = (arg) => … }` from a function
  annotated `: Foo` **is** contextually typed and §80's allow-list admits it
  because the property carries no annotation — but narrowing that arm on
  **syntax** removes four correct TS7006 along with the four wrong ones, at
  **+0 cases** (1,406 on both sides of a stash). **Owner: the contextual type**,
  because the decline must key on whether a signature actually reaches the
  parameter, not on the class expression's position. Do not re-attempt on syntax
  alone; this is its number.
- **`diagcase` matching a case exactly is NOT evidence a rule is finished**
  (§111). After that change the case matched upstream line-for-line, which is
  what a finished build looks like; the **whole-suite counterfactual** is what
  showed the same predicate silencing four correct lines in *other* cases.
  **A per-case diff cannot see a rule's cost outside the case you are looking
  at** — §88's lesson arriving from the other direction.

- ~~TS2448 / TS2450 — refused three times (§96 176 wrong/6 LOST, §97 21/1,
  §98 21/1)~~ **LANDED, §99, at 1 wrong and 0 LOST.** The four measurements are
  kept below because each isolated a different arm and the sequence is the
  diagnosis. Remaining: one wrong line, `enumUsedBeforeDeclaration(2,24)`, owner
  ~~`getEnclosingBlockScopeContainer` for an enum~~ — **that owner was wrong and
  §100 fixed the real one**: `classify` collapsed `const enum` into
  `REGULAR_ENUM`. The rule now measures **9 converts, 0 LOST, 0 wrong**.
- **(superseded, kept for the sequence)** TS2448 / TS2450 by extending §83 — refused at 176 wrong lines and 6 LOST**
  (§96). `checkResolvedBlockScopedVariable` (`checker.go:1888`) shares
  everything below its message choice, so the two arms looked like thirteen
  cheap cases; measured CONVERTS 6 → 7, **LOST 0 → 6**, WRONG **0 → 176**, and
  reverted whole. §83 bounded its arm to the `extends` clause because that is
  the one position `isBlockScopedNameDeclaredBeforeUse`'s deferral arms cannot
  apply to — **the contrapositive is that outside it the deferrals are the
  majority of the behaviour, not an edge case.** Owner:
  `isBlockScopedNameDeclaredBeforeUse` (`checker.go:1922`), **ported whole and
  first, with the report attached afterwards.** Do not re-attempt the cheap
  version; this is its number.
- **The §96 retry, §97 — refused again at 21 wrong and 1 LOST**, but the number
  moved by a factor of eight from one arm: *a use in a type context is deferred
  regardless of position* (`checker.go:1932`). The multi-declaration decline
  §96's diagnosis predicted was confirmed — it took LOST from 6 to 1. **What
  remains is `GetEnclosingBlockScopeContainer` ported so the walk quits where
  upstream quits**; the baseline for the next attempt is recorded as LOST 0 and
  WRONG under 10.
- **§98 ported the container too — CONVERTS 6 → 8, RIGHT 14 → 31, WRONG 21,
  LOST 1 — and reverted on the LOST alone.** The refusal now rests on **one
  named case**, `conformance/controlFlowNullishCoalesce`, in the
  block-scoped-variable arm (confirmed by removing the enum arm and watching
  LOST stay at 1). Three measurements, three isolated arms: *the sequence is the
  diagnosis*, and what remains is debugging one fixture rather than porting a
  predicate.

- ~~**`ValueModuleExcludes` — REFUSED at +2 cases for +14 wrong lines** (§94)~~
  **REVERSED by §95** at +2 cases and **wrong 79 → 64**, by fixing the owner
  §94 named rather than the mask it measured. Kept below because the refusal's
  reasoning is what produced the fix.
- **(superseded)** `ValueModuleExcludes` — refused at +2/+14 (§94).
  Upstream has two module excludes (`symbolflags.go:66-67`) and this port
  returns `None` for both, so a namespace never collides with a variable
  (`module_augmentExistingVariable`). Supplying the stricter mask over-reports
  on **ambient and augmentation** module declarations, which upstream permits
  because `classify` chooses `NamespaceModule` for them via
  `IsInstantiatedModule`. **Owner: `classify`, not `excludes()` — the fix is to
  choose the flag, not to change what the flag excludes**, and it needs §89's
  `GetModuleInstanceState` moved from `crate::check` to `tsr-ast` where the
  binder can reach it. Do not re-measure; the mask is not what is wrong.
- **The other eleven excludes derivations are now checked against
  `ast/symbolflags.go` rather than assumed** (§94) — the durable half of an
  audit that landed nothing, exactly as §84's option table was.

- **TS2300's 61 false positives are NOT 61 conversions.** `extraonly.rs` lists
  no TS2300 at all — every case carrying one is also missing something else. The
  claim that each cost a case was made in §88 from `diag2307`'s wrong column,
  which prices **lines**, and corrected in §89. Still worth removing; not the
  cheapest thing on the board. Owner: `tsr_binder`'s declaration-merging rules.
- **`getModuleInstanceStateForAliasTarget` (`ast/utilities.go:2400`) declined**
  to `Instantiated` — upstream's own "couldn't locate, assume could refer to a
  value" fallback, so it can remove no report that exists (§89).
- **TS2554's remaining families measured but not taken** (§90): the overload
  question on the **call** arm (`functionOverloads29`/`34`/`37`), rest-parameter
  and initialiser ordering (`genericRestArity`, `requiredInitializedParameter1`),
  and the JS arm — where §74's rule applies, a JS decline does not transfer.
- **`neverReturningFunctions1`, 25 of TS7027's 29 missing lines**, needs
  `isReachableFlowNode` over calls to never-returning functions
  (`checker.go:2466`). Owner: the flow subsystem, and it wants `never` return
  types, so it is `checker_types`-shaped.

### New, twelfth session, `diagnostics`

- **A blanket `const enum` decline for TS7027 — NOT taken** (§82). Three wrong
  lines, all `const enum` under `preserveConstEnums: false`, which upstream's
  `reportError` excludes via `ShouldPreserveConstEnums()`. The option is not
  plumbed into this checker and `reachabilityChecks1` writes it **true**, so an
  unconditional decline would cost right lines. **Owner: the option**, and it is
  the third tristate in three builds — plumb the remaining `CompilerOptions`
  this corpus writes in one build, not one per rule.
- **TS7026 re-checked and still refused** (§13's number stands). Its
  sole-obstacle row has grown to **28 cases**, the largest relation-free row on
  the board, and the blocker is unchanged: the corpus's JSX cases declare
  `namespace JSX` inside `declare global { … }`, and global augmentation is
  unported in this binder.

- **Containing `errorType` inside a type constructor — REFUSED at 22 lines /
  3 cases**, `checker-notes-diag2.md` §77. TS2564's largest residual is
  `class X<T> { p1: () => X }`: upstream's `() => errorType` is an anonymous
  object type and is **not** `AnyOrUnknown`, so the rule fires; this port
  answers the bare `errorType` for the whole annotation, because
  `get_signature_from_declaration` returns `None` whenever a *part* is an error
  and `signature_bearing_type_node` turns that into `errorType`. That
  propagation exists for the same reason §42.1's union guard did — **this port
  computes printed text when a type is created, and there is no text for
  `() => error`**. §76 was cheap because `get_union_type_unprinted` already
  existed and one call site needed rerouting; the signature equivalent would be
  a whole unprinted road through `get_signature_from_declaration`,
  `signature_to_string` and `store.new_anonymous`, all of them on the
  `checker_types` query path. **Owner: `checker_types`** — whoever ports
  print-from-the-store gets it for free, because the reason to propagate
  disappears with print-at-creation. **How this would be shown wrong:** if a
  single non-printing consumer can be given the unprinted road in isolation, the
  way §76 did for unions, with no query-path call site reaching it. Refused on
  cost, not on impossibility.
- **A TS2554 decline for `callWithMissingVoid` — measured and REJECTED**
  (§78). `class X<T> { f(t: T) }` with `X<void>`, `X<void | number>` and
  `X<any>` writes the *same* annotation `t: T` at all three call sites, so any
  test that silences the two false positives silences the true one too. The
  build's arrival in that case is 3 right lines against 2 wrong, and the case
  cannot pass either way. Owner: `parameter_annotation_is_void`, which reads the
  written annotation rather than the instantiated type.
- **TS2345's callee gate deliberately NOT generalised** (§78). TS2345 is
  relation-bound and §75's split says do not build into the assignability
  family, so `sole_signature_parameters` stays on `FunctionDeclaration` while
  `sole_signature_arity` accepts all four signature kinds.

### New, eleventh session, `diagnostics` — the later builds

- **TS2365's four declines**, each with an owner rather than a threshold (§49):
  a **type parameter** on either side (constraint following is unported —
  48 wrong lines), an **ES symbol** operand (upstream reports TS2469 in its
  place), **`+=`** (the assignment-target checks answer `errorType` first, and
  `f += 1` on a class is TS2629 — neither TS2629 nor TS2364 is ported), and a
  union **containing** `undefined` (`checkNonNullType` reports TS18048).
- **The spelling-suggestion arm in TYPE positions — REFUSED at 143 wrong lines
  for 1 conversion** (§55). `getSuggestedSymbolForNonexistentSymbol` searches the
  names in scope **with the requested meaning** and `Binder::names_in_scope` is
  meaning-blind, so a missing *type* is answered with a nearby *variable*.
  `parserRealSource13` alone was 105 wrong TS2552 lines for one missing `AST`.
  **Returns when `names_in_scope` takes a meaning.**
- **A class type parameter named from a static member is TS2302, not TS2304**
  (§55) — upstream's resolver *finds* it and the position is the error. Declined
  here; 39 wrong lines. Same for an `infer T` name inside its conditional type.
- **TS2464's union target, decomposed constituent by constituent — REFUSED at
  2 wrong lines for 0 right** (§52). The decomposition is *sound* for a non-union
  source, and whatever the relater cannot decide about the union it cannot decide
  about the constituents either.

### New, eleventh session, `diagnostics`

- **TS2454's `declared == errorType` decline — REFUSED, and it is upstream's own
  arm** (`checker-notes-diag2.md` §42.2). Eight cases (the `moduleAugmentation*`
  family plus `augmentExportEquals5`) sit behind it, all `let x: SomeImportedType;`
  whose annotation this port cannot resolve. Upstream's `errorType` carries
  `TypeFlagsAny`, so `assumeInitialized`'s `t.flags&AnyOrUnknown` disjunct fires
  and upstream reports **nothing** either. It is also unreachable from here:
  `error | undefined` is `error` again, so there is no initial type to run the
  flow with. Returns when the annotations resolve — a `checker_types` question.
- **TS2564's `file_has_parse_errors` gate — REFUSED on the suite at −1** (§43).
  It removes all four of the rule's remaining wrong lines and they convert
  nothing, while the gate costs a real conversion. Same result as §40.3, which
  *deleted* TS2304's. **The parse-error gate is a per-rule measurement, not a
  house style.**
- **`GetErrorRangeForNode`'s four text-dependent arms — not ported, recorded**
  (§48): `KindSourceFile`, `KindArrowFunction`, the case/default clauses, and
  `return`/`yield`/`constructor`. All need `SkipTrivia` or a scanner over the
  file's text, and the checker holds spans and no text (ADR-0034). A displaced
  `return` diagnostic is the symptom this would produce.

### New this session, `diagnostics`

- **TS2741 — REFUSED at 1 conversion for 8 wrong lines** (`checker-notes-diag2.md`
  §22), 0.125 per wrong against a band of 0.47–1.03. **Not a machinery failure**:
  41 right lines and 15 cases that gain a correct TS2741 and still fail. The
  switch is a named constant rather than a deletion. Four residual owners: the
  binder's numeric-name normalisation, private-identifier keys, an inherited
  modifier, and narrowing.
- **TS2403 — REFUSED at 6 losses** (§24). `isTypeIdenticalTo` is unported and
  **`TypeId` equality is not a stand-in for it**: two `{}` type literals mint two
  anonymous types, so a variable redeclared with the *same* annotation read as a
  different type on six cases that pass. ~70 cases (TS2403 30, TS2320 14, TS2717,
  TS2394 13) sit behind that one relation mode.
- **§55's type-position TS2304 arm in `.js` files — REFUSED at one right line
  for one wrong** (`checker-notes-diag2.md` §74), no case moved either way.
  **Third time this file has assumed a JS decline transfers between arms.** The
  rule is not *"JS files are unreliable"*, it is *"a JSDoc-sourced **annotation**
  is unreliable"* — only the arms that read an annotation pay for it, and a type
  *reference* in a `.js` file is written in the source like any other. Returns
  if JSDoc types land.
- **`SymbolFlags::OPTIONAL` is declared in `tsr-binder` and set by nothing** —
  the third writerless flag this project has stepped on, after
  `NodeFlags::AMBIENT` (eighth session) and `NodeFlags::JAVASCRIPT_FILE` (ninth).
  Optionality is read off the declaration's `?` in `crate::member_completeness`;
  the flag itself needs the binder.

### Retired this session, with the number that retired it

- **TS2339's refusal (§9, "2 conversions for 254 wrong lines") is RETIRED.**
  §9 named its own condition — *"a members table that knows whether it is
  complete"* — and `crate::member_completeness` is it, by asking the **walk**
  rather than the type. **12 conversions for 11 wrong lines**, 1.09 per wrong
  against §9's 0.008. The load-bearing half turned out to be already written:
  `members::base_symbols_of`'s contract, *"any base that cannot be followed makes
  the whole lookup a miss"*, had been documented for two sessions.

- **TS2322 on anything but a primitive-shaped pair — REFUSED at 988 wrong lines
  against 947 right** (`checker-notes-diag2.md` §16, build 0). Gated on nothing
  but the error type, this port's relation disagreed with upstream on
  *half* the assignments it was asked about; `arr_i1 = arr_c1` where
  `C1 implements I1` is assignable upstream and not here, and every structural
  row behaves the same way. **Returns when the members table and the structural
  relation are finished**, not before — and the same number retires it, since it
  is a direct read of `checker_types`' non-gradient.
- **TS2322 for a union-typed reference source — REFUSED at 18 wrong lines.**
  `controlFlowAliasing` (13, aliased conditional expressions) and
  `inferTypePredicates` (5) both write `let t: string = x` after a narrowing this
  port does not perform. Restricting the decline to *references* rather than to
  every union source is worth 7 conversions.
- **Implicit-any and TS2322 in `.js` files — REFUSED at 33 wrong lines**
  (28 for `noImplicitAny`, 5 for TS2322's declaration anchors). JSDoc `@param`
  and `@type` supply the types upstream reads and this port does not parse.
  `typedefOnStatements.js` alone was 15 lines on one line of source. Returns with
  JSDoc types. **The assignment anchor keeps JS**, because declining it there
  costs 4 conversions for 5 wrong lines — an asymmetry that is measured, not
  principled.
- **The auto-to-any divergence, from the diagnostics side.** `let x;` and
  `let x = undefined;` both get upstream's auto type, which `convertAutoToAny`
  makes `any`; this port answers `undefined`. Already refused from the `.types`
  side (`checker-notes-narrow.md` §9.1) — recorded here only so the two sides
  keep **one owner** and the next session does not price it twice.
- **Enum assignability — REFUSED at 11 wrong lines and one loss.**
  `isTypeRelatedTo`'s enum arms and `numberAssignableToEnum`'s numeric widening
  are unported, so enum and enum-literal types are vetoed inside TS2322's gate.

**Do not rebuild these without new evidence. Each cost a measured cycle.**
Rows marked **WITHDRAWN** are kept because the rule is never to delete a
refusal — but their stated grounds have since been contradicted by a
measurement, which is named in the row. A withdrawn refusal is not a licence:
it means the item returns to §4 needing a fresh bar, not that it is now good.

| item | population | why refused |
|---|---:|---|
| **WITHDRAWN** — call resolution | 18,294 | ~~spellability **68.3%** vs 70% bar — 85 lines short~~ **CORRECTED 2026-08-06.** That figure was taken at `058b4a9`; re-run unchanged at `d75cf16` the same expression reads **69.4%, 34 lines short**. R2′'s numerator moves with the compiler, so a bar it crosses by tens of lines decides nothing. Split by row: **CALL 70.6% (+25), `new` 64.8% (−60), `InitCall` 58.9%, `ExprCall` 76.3%.** The refusal no longer stands on its stated grounds and R2′ has stopped discriminating — §4 item 2 |
| contextual typing — **withdrawal itself withdrawn, refusal RE-ARMED** | 2,082 + 1,809 wrong | **86% entangled**, 48.8% behind call resolution. Marked WITHDRAWN at `b00738d` as possibly stale; **re-measured at `0d56467` (fourth session) and it reproduces exactly** — |G| 2,082, every row within 14 lines. **Reproduced a THIRD time at `a4e3991` (sixth session), and this time it means more**: the re-run comes *after* three call-resolution builds (`tsr-4sa`, `tsr-g30h`, and the ternary gate), and |G| is still **2,082** with stands-alone still **291 = 14.0%**. Improving call resolution has not moved the entanglement, which is the one thing that could have. The landed builds changed what a member's type is, not whether an argument position can be typed without resolving its call. `checker-notes-fnexpr.md` §10. Off the scored board until call resolution exists |
| **WITHDRAWN** — qualified naming build | ~~1,318~~ **4,557 classified; design W forecasts 1,702** | 90.7% accurate on target row; counterfactual **lost 3,202 lines, regressed 753 cases**. **The refusal STANDS on that measured cost — but its population was understated**, sixth session, `typerefgap.rs`: the `TypeReference` root alone contributes 4,473 lines of this family (4,002 of the 4,010 `no further dependency` row plus 471 more), and `compiler/temporal` supplied 2 of the 3 wrong lines the ternary gate build minted. A refusal priced against 1,318 has not been priced against 4,473, and the *cost* side (3,202 lost) was measured on a compiler seven builds older. **This is the strongest candidate on the page for a fresh counterfactual**, and it needs a new bar rather than an inherited one. **RUN, same session (`qualname.rs`, `checker-notes-qualname.md`): the refusal was of the WRONG DESIGN.** The item is two halves with different at-risk columns — **W** reprints the *written* entity name (1,770 converts, 99 wrong, **0 at risk**, because it fires only where the line already gaps) and **R** resolves and prints the bare name (384 / 1,025 / 381). **The 3,202-line loss was measured on the printing half and then quoted against the resolution half's population**, which is how a 1,318-line refusal came to block a 4,557-line row. Design **R stays refused on its own fresh number, 0.37 gained per wrong.** Design **W is now §4.2's top item** |
| **WITHDRAWN** — element access | 1,590 | **59.3% want `any`**; refused 3×. **Superseded at `b00738d`:** the `tsr-4qx` build collapsed the row 12,544 → 1,391 and the `want-any` share with it, to **19.2%**. The refusal was true of a population that no longer exists |
| `TemplateExpression` | 1,036 | cheap leg **not separable** — upstream's `evaluate` is a syntactic folder consulting no types |
| **WITHDRAWN** — module object (`tsr-6ph`) | 3,539 | ~~2.1 and 2.5 wrong per right, two designs~~ **RETIRED by re-measurement, seventh session** — both priced designs printed the module's *file path*; the alias-search naming that shipped after the refusal (c91314c, then `f62582e`/`897abdd`) was a third design neither number priced. Landed **+421 for 3 lost**. The remainder is the `export =` family (react/tsx head) and the no-alias file-module `import(…)` forms — see `checker-notes-modobj.md` §10 |
| ALIAS row | 5,207 | convertible set and spellable set are **disjoint** |
| `ArrayLiteral` wrong bucket | 1,773 | 36.7% one case; 42.3% is tuple inference in `contextual.rs` |
| wrong bucket case-flips | 37,709 | **81% symptom**; best actionable row flips 37 cases |
| `hadErrorBaseline` | 40,759 | ADR-0039 |
| **WITHDRAWN** — the export= follow (`tsr-e2u`'s stated blocker) | 218 C9 lines + the react/tsx head | The ALIAS-row refusal's spellability leg (0.0% export=) priced a compiler with no alias naming and no export= arm. **Both shipped, seventh session** (`952b328`): the chain converts 302 seed lines at 23 would-wrong measured, and the ALIAS row itself needs a fresh `nameres.rs` re-take before anything further is claimed about it |
| **`tsr-jle` naming** | 11,008 | **10,000 of 11,004 are one case** — `largeControlFlowGraph`, ADR-0038's ceiling. Real size 1,004 in 566 pairs, head **21 lines** |
| **`BinaryExpression` addition fallthrough** | 297 | **277 (93.3%) want `any`** — ADR-0038/0039 forbid it |
| **`BinaryExpression` arithmetic (bigint mixing)** | 43 | 42 of 43 want `any`, and **97.7% is one case** |
| **`BinaryExpression` destructuring assignment** | 252 | needs destructuring patterns **and** tuples — two unported subsystems |
| **`ArrayLiteral` own-root row** | 604 | 602 are the object-reduction guard; **355 are one case**; needs assignability |
| **`new C()`, the cheap design** | 1,052 | *strip `typeof` from the callee* exact-matches **23 of 1,052 — 2.2%**, and on 712 the callee is not `typeof X` at all |
| **WITHDRAWN in part — `\|\|` and `??`** | 358 + 96 | ~~both need `UnionReductionSubtype`~~ **re-measured, ninth session**: the reduction question is a property of the constituent *pair*, and the reduction-agnostic pairs landed at +815/18 (`checker-notes-assign.md` §8). The refusal's surviving core is the non-reduction-free pairs |
| **`tsr-iiu` — `undefined \| null` prints backwards** | — | **NOT A DEFECT.** Upstream prints `null \| undefined` too — 6+3+2+2… baseline instances, the other order **zero**. I filed a defect against correct code from an expectation I never checked |
| **annotation reuse, naive form (`tsr-a2c`)** | 740 | **6,736 right lines broken against 740 converted — 9.1 lost per gained**, worse than every refusal below. And the 740 is a string coincidence: its head is `string \| undefined` → `string`, i.e. `tsr-e10`'s optionality population, not reuse |
| **strict-gating the optionality arm** (`tsr-e10` as an item) | 256 | **converts ZERO**, by two independent measurements: **244 of 256 lines are `@strict: true` and none is non-strict**, so a rule that only fires when strictness is off cannot reach them; and the union constructor already neutralises the added `undefined` in non-strict cases (`add_type_to_union` drops it, `get_union_type_from_sorted_list` collapses the remainder). `tsr-e10` is re-diagnosed as a **four-mechanism symptom row** — optional chains (26.6%), equality, `in`, `instanceof` — not an item. `docs/architecture/checker-notes-narrow.md` §1–§2 |
| **the multi-distinct return aggregate (`tsr-4sc.9` leg)** | 81 | `retgap.rs`, fifth session: 60+18+3 gap lines across the whole corpus want a subtype-reduced union of return types. The machinery is `removeSubtypes` — refused at `tsr-eak` — for ~12–46 converted lines at the observed band |
| **`TemplateExpression`, both candidate designs** | 1,036 | fifth session, `checker-notes-tmplexpr.md`: cheap leg **0.76 gained per wrong**, folder-plus-fallback **0.99**. Supersedes the "not separable" wording with a number. Only a *complete* constant folder (arithmetic, booleans, const-enum members) changes either ratio |
| **the object-literal remainder as an item** | 2,223 quoted | fifth session, `checker-notes-objgap.md`: **77% is downstream** — every member kind has an arm and a member *value* gaps. The accessor item `bd tsr-32y` is **78 lines**, computed names 219. §4.3's "unknown proportion" warning was right and the proportion is 6% |
| **import aliases in value position — REFUSAL RE-CHECKED AND IT STANDS** | 2,404 row / 840 measured | fifth session, `checker-notes-novaldecl.md`: the largest unopened gap root is 93% import aliases, and `resolve_alias` already refuses them **for a naming reason** — an alias prints its own name, so resolving `import * as ns` would print `typeof <stripped file path>`. `bd tsr-4jk`; same family as qualified naming. **Re-checked at `a57a04b`, sixth session, on the theory that shipping both halves of qualified naming had dissolved its premise. It has not.** The stated cause is a *naming* one — resolving `import * as ns` would print `typeof <stripped file path>` — and the machinery that would name a module container, upstream's `getSpecifierForModuleSymbol` (`nodebuilderimpl.go:667`), is **still unported**: `symbol_chain` (`checker.rs:654`) returns `None` the moment the container is a module symbol, deliberately. The refusal stands **and now names its own unlock**, which is the row directly above in §4.2a at 1,094 lines. Cost of the re-check: one grep |
| **the `this` half of `getApparentType`'s head** | 827 | fifth session, `checker-notes-apparent.md`: **zero** convertible. 522 find the member and gap on its own type; 305 are absent from the class's declared type entirely — filed as a separate members-table question |
| **the two callee-type gates as an item** | 2,276 | sixth session, `checker-notes-calleegap.md`. The two rows are **positionally disjoint** — 1,398 is 100% `new`, 878 is 100% call, and the shared label is **two counters spelled the same**, not one mechanism. **58.8% (1,338 lines) belongs to refusals that already exist**: generic candidate 923 (inference, refused this session at ~17), class instance types 268, `unique symbol` 291 — which reproduces `tsr-4sa` §4.1 **exactly, five builds later** — disagreeing overloads 88, heritage 27, type-parameter return 3. The *"everything handled, something downstream gapped"* bucket reads **0**; this is not the object-literal row. Neither gate is an item. What they are is **a second door onto seven populations that already have owners**, and the two things worth having came out of the split rather than the row: a stale refusal deleted (+32) and `resolveUntypedCall` (+114) |
| **the flag-test form of `isUntypedFunctionCall`** | 328 gained / **248 wrong** | sixth session, `checker-notes-calleegap.md`. Porting `IsTypeAny(funcType)` literally manufactures **248 wrong lines against 328 gained — 1.3 per wrong**, worse than every refusal on this page bar `removeSubtypes`. The predicate is a *faithful* port and it is still unsound here, because **the two compilers disagree about which types are `any`**: upstream reaches it only where the source said `any`. The written-annotation form landed instead at **114 / 0** |
| **the priority LATTICE, the other inference leg** | 11 converts / 1 wrong | sixth session, `checker-notes-infer2.md` §7. Sized because the contravariant refusal below had the obvious objection that it priced one leg of a two-leg mechanism and the lattice was the one `inference.go`'s structure makes look larger. **It is 11 lines**, and it is the *better* of the two — both together are ~17. Measured as a delta over the shipped arm so the legs cannot double-count, with at-risk **0** in the same pass over 792 admitted right lines. Its control C4 under-reports in the safe direction (239 of 792 the probe gaps, 0 it answers differently), so the zero is readable. **The row that carried this at 2,056 was a row: the mechanism's own population is 1,231 and 77.3% of it is elsewhere** — 494 find no candidate at all, 458 are downstream of a gapping argument |
| **the contravariant inference bucket** | 6 own-node / ~36 with cascade | fifth session, `checker-notes-infer2.md` §6. Recommended by me on "the same mechanism owns both the losses and the largest share of the new wrong" — and that was **a ceiling on a row, not a forecast of a mechanism**, this file's fourth rule catching its own author. Re-derived line by line: of the 120 wrong lines in the four contravariant-named cases, **4** are this mechanism; 116 belong to five other items. It also needs `strictFunctionTypes`, an option this port does not model, and guessing a sibling flag wrong once cost 1,221 lines. A tenth of the smallest thing ever refused here |
| **destructuring an array literal without the pattern's contextual type** | ~80 wrong minted | the first `tsr-o00` run answered `var [a, b] = [1, "x"]` elements as `string \| number`; upstream infers the **tuple** through `getTypeFromBindingPattern`'s implied contextual type (`checker.go:16748`). Approximating it passed the bar's ratio leg (6.0×) and was refused anyway — the construct refuses whole until `tsr-84iz` builds the mechanism |
| **`bd tsr-kmzf` — a three-valued relation, as an ITEM** | 33 / 0 | sixth session, `checker-notes-assign.md` §5–§6. Leg 1 forecast **33 against a floor of 150**; control C3 then showed **all 33 are conversions the existing BINARY relation already makes**, so the mechanism's own marginal yield is **0**. Refused as an item and **shipped anyway**, because it is what makes deleting `SELECTABLE` safe — the 52 `UNDECIDED` lines are what a naive removal decides wrongly. Do not re-open as "make the relation three-valued": that is done, and it converts nothing |
| **widening `SELECTABLE` as a flag set** | — | same measurement, and it is a *structural* refusal rather than a numeric one. Decidability is a property of the **pair**: for signature-bearing types the old relation was unsound in **both directions at once**, so no per-type flag predicate separates the trustworthy pairs from the rest. The flag set is now deleted rather than widened |
| **the `any`-parameter overload set** | 7 conversions | sixth session. `any` relates to everything both ways, so such a candidate is trivially applicable and declaration-order selection always stops on it — a **wrong rule, not a bad trade**. Refusing it positionally costs 7 conversions and removes **24** would-be-wrong lines: 40/26 → **33/2** |
| **the overload braces form of the composite-print seam** | 939 ceiling | seventh session, `sigprint.rs` braces mode (self-checked): **14 converts / 30 would-wrong (0.47 per wrong) / 330 of the ceiling unmodelable** — the wrongs are inside-signature defects (Intl/Temporal option unions), not naming. The single-signature twin's +1,500 is the seam's harvest; this shard is not its sibling |
| **TS7026 as a `diagnostics` rule** | 27 cases | **CORRECTED, fourteenth session — the attribution was wrong, not just the number.** This row said TS7026 wants `declare global` merging. It does not: the message is `JSX element implicitly has type 'any' because no interface 'JSX.{0}' exists` and upstream emits it from `jsx.go:1253`. All 110 of its missing lines are in `.tsx` files (`tsxNamespacedTagName1`, `tsxElementResolution5/13/14/16/18`, `reactNamespaceJSXEmit`). **It is a JSX build**, and the twenty-eight case names saying so were printed by `diagmissing` in front of every session that quoted §13. §13's *"12 conversions for 47 wrong"* stands as a measurement of a JSX rule on a corpus 104 cases back, and must not be quoted for binder merging. `checker-notes-diag2.md` §158 |
| **TS2339 as a `diagnostics` rule** | 133 cases | eighth session, `checker-notes-diag2.md` §9. Built to the tightest available bound — fire only where the receiver type carries a resolved members table — and measured at **2 conversions against 254 wrong lines, 0.008 per wrong**, two orders of magnitude below the worst refusal on this page. The diagnosis is structural and is the reusable part: **an absent property and an unbuilt members table are the same `None`**, and `Named { members: Some(_) }` says a table was built, not that it is complete — heritage, mapped, conditional and mixin members are resolved lazily by other arms. Same shape as the `SELECTABLE` refusal, in a second subsystem. Returns when `resolveStructuredTypeMembers` carries an explicit resolved state per type rather than an `Option` that conflates "no members" with "not yet" |
| **WITHDRAWN in part — `removeSubtypes` (`tsr-eak`)** | 5 rows, ~1,100 quoted | **the decidability-gated form landed, ninth session** (+112/0, `checker-notes-assign.md` §9) — the 1.03's cause is the relation's unread modifiers, and that population still declines. The original grounds: **255 right lines broken vs ≤263 changed — 1.03 gained per lost at the ceiling**, worse than the 2.1 / 2.5 / 2.7 that refused three earlier items. And only **500 of 2,146** structured wrong lines are its population; 21,093 of 26,140 union lines carry no structured constituent and are outside it by construction |

---

## 6. Instruments

**`examples/parserextra.rs`** (§220) — per case, every parser diagnostic the baseline has **no** entry for at that position. `extragap` counts the extra column by code and `extraonly` lists cases blocked by an extra *alone*; this is the only view that says **where** the 1,288 invented parser lines are. Its answer is a distribution: 326 of 383 cases invent one to four lines.

Built and maintained; **use them, do not rebuild them.**

| instrument | answers |
|---|---|
| `examples/diagreach.rs` | **`diagnostics`, run this first.** Cases reachable by deepening the rules that already exist — nothing extra reported, every missing code one this port already emits. **1,334** at `2b0f9ab`, ranked by which rule to deepen (`checker-notes-diag2.md` §54) |
| `examples/diagmissing.rs` | the **missing** half of one code, restricted to the cases that code alone blocks, so each case printed is exactly one conversion |
| `examples/diagcase.rs` | one case's expected and actual diagnostics side by side, through the suite's own `reported_for` |
| `examples/diagemit.rs` | **`want` against `have`, per code.** How many diagnostics of each code the baselines record beside how many this port emits. A large `want` with a zero `have` is a rule that is **not running**; a small `have` is one declining. It found TS2362 (863 lines) and TS2363 (768) — the two largest unported rows in the corpus, and nowhere near the top of `diaggap.rs`, because those lines almost always arrive beside a code this port already emits (`checker-notes-diag2.md` §65) |
| `examples/extraonly.rs` | **the cases blocked by an extra diagnostic ALONE** — each is one false positive from passing, so the count *is* a forecast rather than a ceiling. **54** at `b78c4d7`; TS1005 38 and TS1012 15 of the original 58 are the parser's. Three builds forecast off it and three came in exact (`checker-notes-diag2.md` §58–§60) |
| `examples/gaproot.rs` | root/cause split — ranks **causes**, not symptoms |
| `examples/casedelta.rs` | **per-case joinable TSV.** A net hides a change that helps and harms at once. `checker_types` only — it reads `types_suite::compare` directly |
| `examples/casequery.rs` | **the same idea through the `Suite` trait, so it works for all sixteen.** `casequery <suite> <case>` prints one case's verdict, tally and the suite's own reason; `casequery <suite> --list` emits every case as a sorted TSV, and a `diff` across a change *names the case that regressed*. `casequery all <case>` asks every suite at once. Written because the committed snapshot truncates at the first 100 failures, so a suite that moves by −1 over 5,488 cases names the case nowhere — which is how §171 came to leave its single LOST "unidentified". §173 named it in one run. Prints its four tallies to stderr as a control against the snapshot |
| `examples/reconcile.rs` | a probe's denominator against the suite's |
| `examples/ceiling.rs` | the ADR-0038 unreachable bound |
| `examples/rank_board.rs` | the gradient board, `TERMINAL`/propagated split |
| `examples/wrongflip.rs` | the only cause split for **wrong** lines |
| `examples/refmatch.rs` | what a narrowing **matcher** can reach — in-range lines split by guard form and by current verdict, with a strict and a loose bound reported together |
| `examples/verdictdump.rs` | **the transition probe, and the only one that can say where a wrong line CAME FROM.** One verdict per aligned line (`case:file:position`), so two runs give the exact matrix — `WRONG→RIGHT`, `RIGHT→WRONG`, `GAP→WRONG`. `wrongdelta` cannot distinguish a gap→wrong arrival from a right→wrong one and that distinction is what `docs/conventions.md` requires a bar's absolute to be written against; design P's fourth leg is scored on it. Also the first probe whose `right + gap + wrong` is an identity rather than a cross-instrument subtraction — §1 |
| `examples/qualname.rs` · `qualnamep.rs` | the two halves of namespace-qualified naming, priced separately: `qualname` the **resolution** half (designs W and R, which convert *gap* lines), `qualnamep` the **printing** half (design P, which converts *wrong* lines and cannot touch a gap). **`qualname.rs`'s at-risk-P column of 36 is superseded by 23** — it omits `getMergedSymbol` |
| `examples/cyclegap.rs` | **what the gap board does NOT attribute, and what the gap WANTS.** `depend.rs`'s `cycle` and `depth cap` endings resolved (the first is 98.6% a length-1 self-loop — a missing step arm, not a recursive shape), plus the whole-gap **want-shape** histogram, which is the only view of the gap that is not a node-kind histogram. Its C2 is frozen against `depend.rs`'s cycle/depth-cap counts: if they stop reproducing exactly, the copied walk has drifted and nothing it prints is readable |
| `examples/fnsiggap.rs` | the un-annotated `FunctionDeclaration` row split **downstream-first** — parameters and return expressions checked before the line may enter the wiring bucket, so the flattering answer is the one that has to earn it. Its C1 froze against `cyclegap.rs` and **fired on the first run** (1,475 vs 2,943): the probe had tested the line's own node while the row is defined by where the *walk ends*. Half the row is not at its own node — a distinction any forecast on this family has to carry |
| `examples/wrongdelta.rs` | **`casedelta`'s sibling for the wrong bucket** — raw joinable `want`/`got` dump; two runs over a `git stash` attribute every gap→wrong line, which `casedelta` cannot see by construction |
| `examples/diaggap.rs` | **the `diagnostics` board.** Ranks the suite's failures by the code each case is blocked on, and prints the only forecastable column — cases missing **exactly one** distinct code and reporting nothing extra — beside the ceiling column that must not be mistaken for one. Also the false-positive table: codes this port emits where the baseline does not, which is the cost side every diagnostic rule has to be scored against. **Re-run it after every rule**: rows grow as rules land, because a case blocked on two codes becomes a case blocked on one |
| `examples/diag2307.rs` | the per-rule counterfactual for `diagnostics`. Calls `diagnostics_suite::reported_for` — the suite itself — and reconstructs the *before* side by removing the codes under test, so probe and build cannot diverge. `CONVERTS` / `LOST` / `RIGHT` / `WRONG`, with `LOST` the leg that must read 0 |
| `fnexpr` · `nameres` · `evolvearray` · `thisparam` · `receiver_gap` | per-workstream |

Five gates, all green before every commit — **on the toolchain
`rust-toolchain.toml` pins (1.96.0)**, which is the only version their verdict
is defined against. Do not report a lint gate's colour without saying which
rustc read it; one session published a red clippy that was 1.89.0's opinion.

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p xtask -- anchors      # every upstream file:line resolves
cargo run -p xtask -- issue-ids    # every `bd <id>` cited in docs/ exists
```

---

## 7. Session log

Append one row per session. Keep it to what a future session needs.

| date | commit | gradient | cases | net | what moved it |
|---|---|---:|---:|---|---|
| 2026-08-09 | `968271df` | **85.40%** | — | **checker-1 continued: §108.1, §111 (3 slices), §113 re-price, §117 (3 slices) — ~+660 more right** | **The sixteenth session's continuation.** §108.1 void-bearing written unions (+22/0, the admission principle's third confirmation); §111 hasInstance narrowing (3 slices, +38 net — landed inert at user direction, activated by the entry probe finding the ladder's missing any-arm with upstream's Function/Object exception); §113 the §31.1 stale-refusal re-price — RE-CONFIRMED at the era's exact 35:13, blocker precisely priced as the loader/vfs symlink+paths unlock (~240 lines + 428 near-miss cases, PARKED for the user with the scanner family); §117 the lib-member subsystem opened by census (2,795 miss events classified) and three slices landed: the Object/Function member fallback (+348 at 7:1, one measured gate), the synthetic .prototype (+181/0), union-constituents-through-apparent (+76/0) — subsystem total +786. LEDGER INCIDENT resolved by two independent instrument chains agreeing: a suspected 16-line regression dissolved as checker-2's aligner artifact in a GAIN column; the sharpened rule (the artifact test applies to gains too) landed in conventions.md. Numbering collisions ×3 resolved by bar-commit ancestry, the drill now proven |
| 2026-08-09 | `28f41c2`+ | **85.23%** | — | **checker-1's printing-lane arc §95–§110: ~+1,150 right across eight landings, five priced refusals** | **The sixteenth session (checker-1), interleaved throughout with checker-2's lane under the two-session protocol.** LANDED: §95 site-aware reference argument slots (+146/0), §97 alias-named unions join the origin gate (landed at user direction, composed at 13:1), §99 multi-signature composites render at sites (+55/0 clean-main; underscoreTest1's first fall), §100 predicate inference from single-return bodies (+24/0), §102 within-print renames DECODED — the shadow test alone, no byText (+269/0; the §20.1 double-refusal closed by a three-line want-pair study after two 763-R→W refusals of byText forms), §106 the file-module import spelling (+260/14 at 18.6:1 — the "architectural per-file printing" head fell to a one-line probe finding a virtual-root slash; THE CORPUS CROSSED 85% here), §107 nested-signature shadows through the render scope (+36/0 — three probes found three stacked killers, incl. a scoped identity flag for foreign type parameters in print-clones), §108 Array-headed annotations join the §77 reuse gate (+407/0 — the arc's largest; the reuse-admission principle generalized: any spelling the fresh render cannot reproduce), §110 slice 1 JSDoc @template plumbing (+1/0, end-to-end verified by probe). REFUSED with numbers: §102-byText twice (88:763, 763-again), §107 first form (13:4, mis-anchored site test), §110 slice 2 (net −17 — the JSDoc TYPE GRAMMAR is the named prerequisite subsystem). PROCESS: the §85.1 stash-pair rule violated once (§93 scored +311 against a stale baseline, corrected by checker-2 to +16 — the session's one false score, corrected loudly); two attribution disputes resolved by DECISIVE evidence classes now in TASK (verified-recompile revert tests; pre-window checkouts before naming an R→W's owner; bar-commit ancestry for number claims). Cross-lane composition verified numerically at every landing |
| 2026-08-08 | `88d0e63` | **84.63%** | **3,955** | **+796 right (6 builds), zero R→W in five of six; 12 G→W priced in §92** | **The fifteenth session, `.types` — the GENERIC-ALIAS INSTANTIATION subsystem opened and its first four seams landed.** §89 alias-named signature mints survive the composite re-render (+178 at 26:1 — the build-149 open trace closed by one keep-text set); §89.1 degenerate leading-operator unions are REAL NODES (+8/0 — `parser.go:2649` keeps the node, the checker answers a single constituent before the alias attaches, `boolean` exempted from the intersection parenthesiser, and the printer re-emits the leading operator after `printer_round_trip` fell to 99.97% — the bar's falsifier firing and being honoured); §90/§90.1 instantiated alias references keep their body's members (+280/0 — `create_type_reference` mints the body's TypeLiteral symbol, §46's admission one road lower; the `r_1` print-rename landed print-only after the semantic rename measured −182, gated empirically at three conditions with upstream's `typeParameterToName` study still owed to §20.1); §91 CONDITIONAL alias bodies evaluate at instantiation rebuilds (+217/0 — chain3 whole: an env-stack evaluator with keyof/key-intersection arms, `extends never` only, computability as the gate); §92 property reads through evaluated alias bodies (+113 at 9.4:1 — intersection constituents gated to evaluator-produced types after a written intersection measured 134 G→W, `Omit<T, K>` by global arity-2 symbol, alias-body evaluation with TypeLiteral spines refused). **Transferable:** (1) the §89/§90.1 pair says every baked-text consumer needs a keep-text contract with the site re-renderer — two builds hit the same trap from different directions; (2) an ABORTED scripted edit left registration code silently absent and only a TypeId-printing probe found it — assert-and-write-at-end scripts fail atomically, which is good, but the follow-up must re-verify EVERY edit in the batch, not just the one that raised; (3) `global_type_symbol` defaults to arity 1 and answers None for `Omit` — the §45 Record precedent used `binder.global` for the same reason. All 100% suites verified held (parser/binder/printer/scanner); baseline advanced per landing; everything pushed at `88d0e63` |
| 2026-08-08 | binder push (main) | — | — | **`binder_symbols` 8,411/8,473 → 8,422/8,461 = 99.54%; `parser_typescript` and `printer_round_trip` held at 100%** | **The thirteenth non-checker session: PR #2 merged into main (`185877d`), then the binder-symbols push toward 100%, nine commits.** The suite's checker-display layer grew six accommodations, each anchored to what `symbolToString` actually prints: late-bound members under their written bracket text (with a string-quoted expression re-wrapped after escaping), same-valued late-bound members unioning under each spelling (`dynamicNamesErrors`), const-propagated computed names, `export default <identifier>` naming the default symbol, module-file symbols under specifier spellings (`import('./test')`), and namespace-import transparency (`import * as self` self-imports). One genuine pairing bug: a case holding both `utils/index.ts` and `index.ts` judged the section against the wrong unit — exact name now wins. **The session's parser build is the deep one**: `parse_identifier` rejects reserved words exactly as `isIdentifier` (`parser.go:6248`), with `parse_identifier_name` carrying the any-keyword behavior for the positions upstream reads that way (right-of-dot, property names, module export names, type-reference entity heads — `typeof this` — and heritage operands — `extends null`); a `this` parameter and keyword binding-property renames handled where upstream handles them. Twelve error-recovery cases (`reservedWords3`, `parserEnumDeclaration4`…) now diagnose as upstream does and leave the judged populations. `parser_typescript` dipped to 5,026 mid-build and the five failures named the exact positions upstream is permissive (`extends null`, `{ enum: e }`, `typeof this`) — the gate doing its job. Remaining 39 binder failures classified and filed: `bd tsr-2` JSDoc declarations (needs the JSDocTable through `FileInfo`, the `bd tsr-1` arena precedent shows the shape), `bd tsr-3` augmentation-of-reexports (checker-owned merge), `bd tsr-4` late-bound expando — one of which records a **baseline-vs-code-reading disagreement** on `GetPropertyNameForPropertyNameNode`'s signed arm to resolve against the oracle before coding |
| 2026-08-08 | driver seam | **84.30%** | **3,914** | **+21 `checker_types`, +1 `binder_symbols`, `diagnostics` unmoved, 0 lost** | **The fourteenth session, and the first that is not a checker session.** Three gaps between "the APIs exist" and "a `tsc`-like driver can be written", closed faithfully with no invented surface. **(1) The real filesystem** — `tsr_vfs::OsFileSystem`, ported from `osvfs/os.go` + `vfs/internal/internal.go`; the only `impl FileSystem` before it was `InMemoryFileSystem`, so `module_resolution` 95/95 and `file_loader` 96/96 are both *in-memory* results and symlinks, casing and realpath on a real disk were untested. Upstream's case-sensitivity probe (swap-case the executable's own path and stat it) ported including its panics, because every default is silently wrong somewhere. **(2) [ADR-0042](docs/adr/0042-checker-options-come-from-compiler-options.md), checker options come from `CompilerOptions`** — eleven options, previously derived by **two** callers from the raw `@directive` map, which disagreed about `noImplicitAny` and `useUnknownInCatchVariables` for eleven sessions (§80 recorded the disagreement and did not resolve it, because each side had a measurement). Ten `Tristate` fields added to `CompilerOptions`, the three upstream accessors ported, one `apply_compiler_options`. Consolidating `diagnostics_suite` moved **nothing**. Applying the same derivation to `types_producer` **overturned a recorded measurement**: §21 and §65 measured `false` defaults as better and the faithful `true` now reads **+21 cases**. Measured twice from different bases — 3,842 → 3,863 pre-rebase, and 3,864 → 3,885 after rebasing onto §102 — the identical delta, which is stronger evidence than either run alone. The row's 3,914 is a third measurement, on §77's base, and is *not* a third counterfactual. *A default tuned against a partial implementation measures the gap, not the language, and has to be re-measured whenever the gap closes.* Also closes `bd tsr-e7a` — `isolatedModules` is honoured as a directive. **(3) `tsr_diagnostics::format`** — the plain `a.ts(1,1): error TS2304: …` line and the `Found N errors` summary with its tabular breakdown, byte-exact against `diagnosticwriter.go`; pretty output **refused, not approximated**. Its prerequisite, one ECMA line map in `tsr-core` (LF, CR, CRLF, U+2028, U+2029 — the harness's private copy split on `\n` alone), gave `binder_symbols` **+1** when the harness switched to it. **The session's transferable findings.** (1) **A number in §1 without a fresh run behind it is worse than no number**: the whole table was stale by several sessions and told four sessions of readers that declaration emit was at 47% when it was at 87%. Corrected in place. The *checked-in snapshots* were stale too, and by less but in the same direction — `checker_types` read 3,838 against a fresh 3,842 and `diagnostics` 1,374 against 1,375, **before any edit this session**. Both are attributed to whatever landed them, not here: this session's +21 and +1 are before-and-after on one checkout, which §88's rule already demands and which is the only reason the two stale rows did not get claimed as gains. (2) **`bd` cannot run in a fresh clone** — no `refs/dolt/*` on the remote, no JSONL — and `bd init` would flip `xtask issue-ids` from `SKIPPED` to failing all 651 citations across 183 ids, so the four driver items are filed in §4.-1 instead |
| 2026-08-08 | `89cca5a` | **84.13%** | **3,842** | **+305 (5 builds), zero net adverse; 1 refusal at 199:1,743** | **Builds 122–126: the printing block.** §70 overload-agreement contextual argument (+68 — three mention-test designs measured: direct-id +127/49, TEXT +8/0 because names re-bind, id-walk +68/0); the producer's binding-element property name converted to UNCONDITIONAL `any` (+85 — the IsTypeAny precondition REVERSED on measurement, its pinned test flipped: the position's answer is decided by upstream's getTypeOfNode trace, not by what this port computes); §71/§71.1 pattern printing (+113 — renamed elements verbatim, initializers dropped, `{}` spelling; the exposed alias-name residue diagnosed in-flight); §72 function-type aliases print their name (+39 — the three-arm getAliasForTypeNode rule was missing ONLY on signature-bearing nodes, and closing it retired §71's residue at its root). **§73 REFUSED with its number**: JS-wide unresolved-prints-error measured 199:1,743 — upstream prints BOTH `any` and `error` for unresolved names in ONE file, so file kind is not the discriminator. NEW DEFERRED HEAD: `import("...").Name` spelling (128 lines, one case) needs per-file printing context — type texts are minted globally; architectural, not a patch |
| 2026-08-08 | §140 landing | — | **`diagnostics` 1,450 / 5,488 = 26.42%** | **+104, 0 lost, 28 builds** | **The thirteenth session, `diagnostics`.** Opened on a **debug build that could not run**: `cargo run -p tsr-conformance --bin coverage` exited 101 on `span.end - span.start` in the `.types` workstream's template-escape decline, which wraps harmlessly under release's disabled `overflow-checks` — so the build that landed it measured cleanly and no one saw the crash (**§86**; `saturating_sub`, behaviour-preserving by construction and by measurement). **§87 TS2564** (**+16 for ZERO wrong**, the session's largest): §6 declined every class with a constructor because upstream's second disjunct needs a *synthesised* `this.x` node and ADR-0012 forbids one — but **a flow analysis you cannot run still has inputs you can read**, and a constructor body that never mentions `this.<name>` cannot assign it on any path. Every uncertain answer (depth cap, unmapped node, `this[…]`) declines, because that branch is silence and the other is a wrong line. **§88 TS2300 on a duplicate type parameter is the CHECKER's, not the binder's** (+5/0): `checkTypeParameters` (`checker.go:7002`) compares **symbol identity** across one list, because the binder has already merged the duplicates — *a duplicate that merges silently is not a binder that missed the error; it is a binder that left it to the consumer that can position it.* **§89 unreachable code comes in RUNS** (+4, **wrong 3 → 0**): upstream reports once per run of consecutive unreachable statements, so a statement that breaks the run makes the next one report **again** — §82's "any earlier sibling" could only ever report once per list and was one word too wide. Its other half: `isSourceElementUnreachable` asks a **different question per kind**, and a namespace that emits no JavaScript is not unreachable *code*. `preserveConstEnums` plumbed at last — the third option shape, `IsTrue()`, so unset is false. **§90 arity does not need generics** (+2, **wrong 14 → 10**): a generic or overloaded constructor has a perfectly good `(min, max)`, and computing arity and argument types at one gate declined both together. Reading the wrong column then found a defect **the lifted decline had been hiding** — the base-class hop advanced while no class had a constructor *with a body*, walking past `declare class`'s ambient overloads into its base — so fixing it removed four wrong lines that predated the build. **The session's transferable findings.** (1) **A decline can conceal a bug rather than prevent one**, and lifting it is the only way to find out which. (2) **Check `checker_types` byte-identity by `git stash`-and-remeasure on the same checkout, never against a number written down before your last push** — the twelfth session and this one both misread a `.types` build arriving through their own `git pull --rebase` as their own drift (§88). (3) A release-only measurement culture can land code no debug build can run. **§91** closes the session: both arity arms read the minimum as *the first* optional parameter's index, where upstream resets it at every **non-optional** one (`checker.go:19872`) and therefore answers the position after the **last required** parameter — the two agree whenever optionals are trailing, and `function f1(a, b = 0, c)` is the shape where they do not (+1/0). **§92** transplants §90's split to the **call** arm: `getArgumentArityError` (`checker.go:9715`) computes its range across *all* candidates, so an overload set has an arity even where it has no resolution (+3/0). Added as an `or_else` fallback so the single-signature path is bit-for-bit untouched and the counterfactual measures only the new arm. **§93** is the session's largest line yield — **+64 right lines from one argument** — and the cheapest, because *nothing was missing*: `declare_into` **derived** excludes from the declared flags, so a parameter got `FunctionScopedVariableExcludes`, which deliberately does not collide with another function-scoped variable (`var x; var x;` is legal), and `function bar(a, a) {}` reported nothing. Upstream's `declareSymbol` (`binder.go:202`) takes includes and excludes as **two arguments** and `bindParameter` (`:1200`) passes `ParameterExcludes`; its own comment at `:1176` says the distinction exists for exactly this. **A derived value that is right at most call sites is not the same as a parameter.** The `binder_symbols` rail did not move (8,311/8,475 · 98.06% with and without). **§94** then audited the other seven `declare_into` sites the way §84 audited the options: eleven derivations verified against `ast/symbolflags.go`, one real divergence found (`ValueModuleExcludes`), **measured at +2 cases for +14 wrong lines and refused with its owner named** — `classify` maps every `ModuleDeclaration` to `VALUE_MODULE`, so the stricter mask also catches the ambient and augmentation declarations upstream flags `NamespaceModule`. *A derived excludes can only be as right as the flag it is derived from.* **§95 then REVERSED §94's refusal by fixing the owner it named** (+2 cases, **wrong 79 → 64**): `bindModuleDeclaration` (`binder.go:1268`) picks the module flag from `GetModuleInstanceState`, which §89 had ported *into `crate::check`* where the binder cannot reach it — moved to `tsr-ast` and rewritten against the **typed** tree via `push_children`, so it needs no side tables and no `&Checker` and both consumers share it. The 15 removed false positives are **a third of the TS2300 row** §88 found. `binder_symbols` did not move; **`checker_types` did — 3,843 → 3,846 — the only build this session that changed it**, an improvement rather than damage but recorded rather than glossed. Three tests failed and **all three had encoded the divergence**, two by using `namespace N {}` as a fixture: *a green suite is evidence the port agrees with its tests, not that it is faithful.* **§96** closes the session with a refusal that cost nothing but a measurement: the TS2448/TS2450 arms measured **176 wrong lines and 6 LOST** and were reverted whole, leaving the session's zero-loss record intact. §88's byte-identity trap fired **five** times in this one session and was caught every time by the rule §88 had just written. **Two of the session's nine investigations ended in refusals, and one of those refusals (§94) is what produced §95's build** — the argument for pricing a negative with its number rather than deleting it. **§99 then LANDED the item §96/§97/§98 had refused three times** (+2 cases, **wrong 21 → 1**): the last blocker was one under-scoped helper — §83's `declaration_is_in_an_ambient_context` had two kinds because those are the two its `extends` bound could ever see, and a `declare const` puts the modifier on the enclosing **`VariableStatement`** — and it accounted for the LOST *and* twenty of the twenty-one remaining wrong lines. **A predicate written to be sufficient for one caller is a landmine for the second**, and `NodeFlags::AMBIENT`, one of the three flags this parser never sets, would have answered it in one read. **§100 closes the rule at ZERO wrong**: §99's last line was a `const enum` used early, which upstream says nothing about because it is inlined and has no temporal dead zone — `checkResolvedBlockScopedVariable` tests `RegularEnum`, not `Enum` (`checker.go:1908`). Changing the checker's test **did nothing**, because `classify` mapped every `EnumDeclaration` to `REGULAR_ENUM`. **THE SESSION'S PATTERN, FOUND THREE TIMES** — §93 derived `excludes` from the declared flags where upstream passes it; §95 made every module a `ValueModule` where upstream picks with `IsInstantiatedModule`; §100 made every enum a `RegularEnum` where upstream picks with `IsEnumConst`. Two of the three were first attacked at the *consumer* and measured nothing or worse: **when a consumer's faithful test gives an unfaithful answer, suspect the flag before the test.** Unaudited collapse candidates remain: `S::PROPERTY` over five node kinds and `S::ALIAS` over five import/export forms. **§101 closes the session by retiring §83's `extends` bound at a measured +0 cases** — landed anyway because it deletes scaffolding that reads as a deliberate bound (§96 lost part of a measurement to exactly that misreading), and because **a bound removed without cost is a test of what replaced it**: the wrong column staying at zero is independent evidence §97–§100's port is sound. **§103 is the session's cheapest build and its second largest — TS1029 modifier order, +9 cases for ZERO wrong** — and the interesting part is why it sat untouched for thirteen sessions: **it needs no types, no symbols, no flow and no relation**, and every previous session ranked targets with `diagreach`, which measures *cases reachable by deepening rules that EXIST*. A code with no rule at all appears only in `diaggap`'s single-code column. **`diagreach` and `diaggap` answer different questions and the cheap grammar codes live exclusively in the second** — TS1100 (12 cases), TS1109 (11) and TS1163 (10) are the same shape. **§104 took the first of them, TS1163 (`yield` outside a generator), for +10 cases** — 10 missing lines across 10 cases, a concentration of exactly **1.0**. Its test upstream is one flag read, `NodeFlagsYieldContext`, and **this port declares `NodeFlags::YIELD_CONTEXT` and never sets it — so the standing note that THREE flags are declared-and-unset is wrong and is corrected here: there are at least four.** Derived structurally instead (the nearest enclosing function-like must be a generator). **The bar was +8 cases and at most 2 wrong lines; met on cases and MISSED on wrong, at 7.** The wrong column is `tsr_parser`'s: a bare `yield` is an *identifier* outside a generator in non-strict code and this parser builds a `YieldExpression` for it — requiring an operand took 11 wrong lines to 7, and the rest want the same yield-context tracking the unset flag would have carried, **so the flag and the wrong column have one owner between them**. **§107 then paid most of that wrong column WITHOUT touching the parser**: two of the three shapes upstream's lookahead rejects survive into the finished tree — `yield(foo)` leaves a `ParenthesizedExpression` operand (it is a CALL) and `yield * []` leaves an asterisk (it is a MULTIPLICATION outside a generator) — so wrong went **7 → 4** at +0 cases and zero rails touched. *A parse-time decision can leave a post-parse fingerprint, and where it does the consumer can decline without waiting for the parser.* **§108 then closed TS1163's wrong column at ZERO**: a **computed property name is evaluated where the CLASS is**, not where its members are, so `[yield 1]` inside a class inside a generator is in the generator and `= yield 2` is not. The walk now passes through a class member it reached via a `ComputedPropertyName` — which required a loop rather than a `find_map`, because **the boundary test is a property of the EDGE, not of the node**. §104 landed at 7 wrong and recorded a missed bar; §107 and §108 paid the miss off rather than leaving it as furniture. **§105 then priced the rest of that row and two of the three were not the shape §103/§104 advertised**: TS1100 is the **binder's** (seven call sites, `b.inStrictMode`, and a three-way message split of which only one is TS1100), TS1109 is the **parser's** (every line a yield/await recovery position), and TS1183 measures **zero** and is off the board. *The cheapest way to learn none of them was the advertised build was one `diagmissing` run each rather than starting any of them* |
| 2026-08-08 | §84 landing | — | **`diagnostics` 1,346 / 5,488 = 24.53%** | **+44, 0 lost, 8 builds** | **The twelfth session, `diagnostics`** (this row supersedes the §79 one below, which is the same session at its halfway point). §80–§82 continue §75's relation-free 316. **§80 `noImplicitAny` DEFAULTS ON in this corpus and the harness had it off** (+5): `GetStrictOptionValue` (`core/compileroptions.go:294`) answers `Strict != TSFalse` for an unset option, and `diagnostics_suite.rs` read `unwrap_or(false)` **ten lines below a `strictNullChecks` that already read `unwrap_or(true)` for the same reason** — the two disagreed for eleven sessions. Turning it on corpus-wide did NOT go negative: the contextual-typing allow-list held with one arm wrong (a `return` inside a function with a **written return annotation** is contextually typed). **§81 TS7010** (**+10 for ZERO wrong**), the return-type sibling §80 made reachable — `checker.go:3446`, the arm `implicit_any.rs` already quoted and left; needs **no** contextual fence, because a bodiless declaration has no inferred return type to supply. Its one wrong line named both halves of a mis-read `isPrivateWithinAmbient`: a `#name` element is private without the keyword, and `NodeFlagsAmbient` covers the member's **own** `declare`. **§82 TS7027** (+3, wrong 19 → 3) — the binder already records `NodeFacts::UNREACHABLE` per node, so the standing *"options are not plumbed into `tsr_binder::bind`"* blocker did not apply; `reportedUnreachableFlow` modelled as mutable walk state read as a faithful transcription and was **wrong**, because this binder starts a fresh flow for a namespace body and upstream does not — the stateless structural model (no ancestor, no preceding sibling carries the fact) is shorter and right. **The session's two transferable findings.** (1) **Take the bar off `diagreach`'s CASE count, never off `diagmissing`'s line count** — the three line-barred builds read 5, 2 and 3 against 8, 6 and 9, all at a ~15:1 concentration; the two case-barred ones (§79, §81) both MET at +13 and +10. §82 then found the metric's own limit: `reachabilityChecks1`…`11` is one file under eleven option combinations and `diaggap` counts it eleven times. (2) **Three of six builds turned on a compiler option's tristate**, and the notes now recommend plumbing the remaining `CompilerOptions` this corpus writes in ONE build rather than one per rule. **§83 TS2449** closes the session (**+6 for ZERO wrong**): `class A extends B` where `B` comes later, bounded to the `extends` clause because that is the one position `isBlockScopedNameDeclaredBeforeUse`'s eighty lines of *deferral* arms cannot apply to — fourteen right lines from twenty lines of rule, and all three shape-falsifiers were pre-empted by guards the bar named before the code existed. **§84 audits every option default at once** (a **verified** zero — the whole-suite counterfactual is byte-identical on both sides, run precisely because a zero case count cannot tell *"nothing depends on this"* from *"the code does not run"*): five of seven were already right, and the wrong one — **`useUnknownInCatchVariables`, a strict option this harness never set at all**, so every un-annotated `catch (e)` was `any` here and `unknown` upstream — **was the only one no rule had reached for yet**. An option nobody consumes is an option nobody has checked |
| 2026-08-08 | §79 landing (`77e2866`) | — | **`diagnostics` 1,322 / 5,488 = 24.09%** | **+20, 0 lost, 3 builds** | **The twelfth session, `diagnostics`.** Worked §75's 316 relation-free cases throughout; `checker_types` byte-identical across all three. §76 **a union ANNOTATION of named types declares `errorType`** (+5 for **zero** new wrong lines — §42.1's guard one level up: `get_type_from_union_type_node` routes `var c: E \| F` through the *printing* union worker, so every enum-union declaration silenced TS2454 at its first type test; found by one `eprintln!` behind `TSR_DEBUG_2454`). §77 **TS2564's residual is the same phenomenon at SIGNATURE scale — REFUSED with its number**: `() => X` is `errorType` here because `get_signature_from_declaration` answers `None` when a part is, where upstream *contains* the error inside the type constructor; 22 lines / 3 cases against the signature road `checker_types` is steered by. Owner named. §78 **TS2554's callee gate accepted one declaration kind out of four** (+2 — a method, a method signature and `var f = function(){}` were all declined at the same `Node::FunctionDeclaration` match, while `callee_symbol` already resolved all of them; falsifier fired and an *annotated* variable now declines, at zero cost). §79 **`typeof A` is a value position** (**+13**, and the wrong column went **DOWN 9** — the allow-list had no `TypeQueryNode` arm, so eleven cases never reached the rule; two declines found by reading the wrong column, of which `ALIAS` on the meaning-ladder was worth ten lines across the whole corpus). **The session's transferable finding: take the bar off `diagreach`'s CASE count, never off `diagmissing`'s line count** — §76 and §78 were barred off lines and read 5 and 2 against 8 and 6, and §79 was barred off cases and met at +13. The two line-barred builds both measured a 15:1 concentration. Also: TS2304's parse-error gate got its **third** measurement (absent is right — §40.3 +6, §50.1 −6, §79 clean on three recovered trees), and the three standing LOST are diagnosed in the notes, `resolutionModeTripleSlash1`/`3` newly so (`/// <reference types>` + conditional `exports`, owner `file_loader`, three cases behind it) |
| 2026-08-08 | §73 landing | — | **`diagnostics` 1,301 / 5,488 = 23.71%** | **+183, 0 lost, 34 builds** | **The eleventh session, `diagnostics`.** §42–§48 (+57, in the row below), then §49 TS2365 (+3, four declines took its wrong column 58 → 0), §50 TS18050 is chosen by the NODE not the type (+5, wrong 96 → 0), §51 the other five `checkNonNullType` messages (+1), §52 TS2464 (+3), §53 TS2540 (+1 for 71 right lines), **§54 `diagreach.rs`** — 1,334 cases reachable by deepening existing rules, the largest number this workstream has taken — §55 TS2304 in **type** positions (**+43**, the session's largest single build), §56 TS2554 for constructors (+1, wrong 31 → 6), §57 **`names_in_scope` takes a meaning** (+7 — §55's refusal retired by the condition it named, and the *value* arm wanted the filter too), and **§58–§61 off `extraonly.rs`** (+6 — the cases blocked by an extra diagnostic ALONE, exact three times running and then §61.1, which showed the limit: the instrument says a case is one removal from passing, not that the removal is *expressible*), §62 TS2411's literal member names and `isNumericLiteralName` (+3), **§63 TS2389** (+13 — §14 declined it and the decline was *exact*, so the port was the message and its two positions), and **§64 a constructor has no symbol in this binder** (+10 — a one-line trace found `symbol_of` returning `None` for a `Constructor`, which silenced four codes at once), **§65 `diagemit.rs` + TS2362/TS2363** (+13 — `want` against `have` per code found the two largest *unported* rows in the corpus, and 866 right lines came for four wrong), §66 TS2356 at the `++`/`--` operand (+5 for zero wrong), §67 TS2341 (+7 for zero wrong), and **two measured zeros kept on the reachable set** — §68 TS2445 and §69 TS2374, 63 right lines and 0 wrong between them, moving `diagreach.rs` 1,283 → **1,304** — and §70, an inaccessible property's access answering `errorType` so no assignment check follows it (+2) |
| 2026-08-08 | §48 landing (`adfd789`) | — | **`diagnostics` 1,175 / 5,488 = 21.41%** | **+57, 0 lost, 7 builds** | **The eleventh session, `diagnostics`.** §42 the outer-variable disjunct (+1 — `markNodeAssignments` was already ported and the handoff still named it as missing), §42.1 the named-union PRINTING guard silencing a non-printing consumer (+7), §43 TS2564's private and computed name kinds plus `is_error` for `== errorType` (+14), §44 the same correction at `pair_is_reportable` (a measured zero, kept), §45 TS2367 (+15 — §31's inherited decline was a stand-in for `getBaseTypeOfLiteralType`, not for the relation), §46 TS2352 ported to the real widening (zero, kept), §47 `checkTruthinessOfType` (+21, the top of its forecast), §48 `GetErrorRangeForNode` centrally (0 converted, 4 wrong lines removed, **0 lost across 36 report sites**). New instruments: `diagmissing.rs`, `diagcase.rs` |
| 2026-08-07 | §49 landing | **82.62%** | **3,659** | **+734/268** | **Builds 71–74: the reference/member block.** Alias bodies carry members (§46, +46), `this` results answer receivers (§29-callres, +66), plain binding patterns render (§48, +149 — the token-kind trap's second firing caught by the pair), union property projection (§49, +734 — the dependent-flow family's prerequisite laid). The shipped-red protocol now reads: FULL suite before the landing commit |
| 2026-08-07 | build-70 landing | **82.41%** | **3,645** | **+891/147** | **Builds 67–70: the typed-array chase.** Three probes walked the row's decline inward — class defaults (§43, +4), all-defaulted construct signatures (§44, +4), and the real gate: the candidates loop's `?` letting ONE unbuildable overload kill the interface. Skip-with-agreement converted typed arrays and every uniform-return constructor interface; §45's `Record<string, V>` (+274) and its measured-and-reverted option refinement (−6) round out the block |
| 2026-08-07 | §42-v2 landing | **82.18%** | **3,641** | **+4/0, 18 W→G; v1 refused at +4/352** | **Build 66: generic qualified references** — the refusal-names-the-design loop inside one build: v1's unqualified prints fired 352 R→W and were reverted; v2 carries the qualified text and registers the seam |
| 2026-08-07 | §41 landing | **82.17%** | **3,640** | **+1,016/97 — three rows at once** | **Build 65: qualified references carry members.** The mini-namespace probe found the temporal root in one shot: resolution existed, the answer was print-only. The members-carrying qualified mint converted `temporal` (193), the enum-literal families (211), and — the surprise — 264 lines of the DOUBLE-REFUSED `underscoreTest1`, which was never mostly a `_1`-rename problem |
| 2026-08-07 | §40 landing | **81.96%** | **3,639** | **+525/129** | **Build 64: variadic tuples print, concrete rests splice** — the print-only citizen pattern earns a fourth application; a five-session-old refusal's fixture came due with its property intact |
| 2026-08-07 | §39 landing | **81.85%** | **3,638** | **+672/140, +9 cases** | **Build 63: plain-function `this` is `any`** — `tryGetThisTypeAtEx`'s fallthrough; the diagnostic/type boundary again |
| 2026-08-07 | callres-§28 landing | **81.71%** | **3,629** | **+343/122, 0 R-losses** | **Build 62: interface `this`** — the print half at the second measured variant; receiver instantiation stays the priced residue |
| 2026-08-07 | callres-§27 landing | **81.64%** | **3,627** | **+1,182/129, +47 cases** | **Build 61: the written `unique symbol`** — `getTypeFromTypeOperatorNode`'s ESSymbol arm, one mint per node; the §26 pair's other half |
| 2026-08-07 | §17-unrefusal landing | **81.39%** | **3,580** | **+784/7 — the mountain converted** | **Build 60: the un-refusal.** The §17 refusal's diagnosis ("prerequisite: members slice") was WRONG and a lib-less micro-probe proved it in minutes: class instances carry their member symbols; the nominal private-identity arm fires; the terminus was `check_array_literal`'s `count <= 1` gate discarding DECIDABLE multi-survivor reductions. Corrected loudly per the STATUS rules — a refusal's diagnosis is itself a claim the next probe must test |
| 2026-08-07 | arrays-§7 landing | **81.23%** | **3,579** | **+43 net, both columns improve** | **Build 59: the contextual re-open's first slice.** The mechanism-level argument STATUS §5 demanded: `isLiteralOfContextualType` preserves only FRESHABLE literals, so a literal-free element union is context-independent and the §13 admission extends to it. The head case then named the real blocker — sibling classes with private members relate Unknown in `union_with_subtype_reduction` where upstream's check decides; the next slice is a RELATER arm, queued in arrays §7 |
| 2026-08-07 | §38 landing | **81.22%** | **3,575** | **+81/0** | **Build 58: written tails fill** — `fillMissingTypeArguments`' written half, zero adverse |
| 2026-08-07 | §37 landing | **81.20%** | **3,575** | **+236/33** | **Build 57: tuples instantiate** — `tsr-5ll`'s element lists close `instantiate_type`'s oldest decline |
| 2026-08-07 | §36 landing | **81.15%** | **3,575** | **+149/39; §34 zero-reverted; §35 finding** | **Build 56: the `unknown` fallback** (`getInferredType`'s last leg). §34 measured zero and REVERTED (callee-side-only population; the arm touched the ADR boundary for nothing). §35: tsgo prints `error` in JS chains — 123 baseline files carry want-`error` lines, matchable by honest gaps; recorded with the no-oracle-peeking constraint |
| 2026-08-07 | §33 landing | **81.12%** | **3,574** | **+378/24** | **Build 55: `globalThis`** — the §31 exclusion became its own rule; a binder accessor opens the merged globals to member access |
| 2026-08-07 | callres-§26 landing | **81.04%** | **3,571** | **+377/19** | **Build 54: the unique-symbol mint** — one distinct type per valid declaration position; the positional gate's error became the mint it was guarding for |
| 2026-08-07 | callres-§25 landing | **80.96%** | **3,565** | **+680/7** | **Build 53: `new Unresolved()` — the sixth hop, near clean** |
| 2026-08-07 | callres-§24 landing | **80.82%** | **3,559** | **+856/162, +106 cases** | **Build 52: the chain's fifth hop.** §31-provenance IDENTIFIER callees are untyped calls; `super(...)` is `void`. The `f()` stand-in fixtures re-anchored to `satisfies` — their premise became upstream-true |
| 2026-08-07 | callres-§23 landing | **80.64%** | **3,453** | **+1,424/452, 0 R→W** | **Build 51: `any.m()` is an untyped call.** The written-annotation gate learned the §31/§32 provenances; JS-file receivers stay declined (the fired leg) |
| 2026-08-07 | §32 landing | **80.35%** | **3,428** | **+4,263/496 — ACROSS 80%** | **Build 50: one hop further.** Access through `tsr-eep`'s minted unresolved receivers is upstream's `errorType` access — `any` — at both access forms, behind the §31 structural gate. The 496 adverse are the port's own `export default interface` binder miss (invisible to the file gate, recorded as a future rule). Three fixtures updated to the minted-vs-otherwise split |
| 2026-08-07 | §31 landing | **79.46%** | **3,427** | **+6,267/486, +194 cases** | **Build 49: the third mountain range.** `parserRealSource*`'s ~9,000 gap lines were names from `///<reference>` files the corpus deliberately omits — upstream reports TS2304 and answers `errorType`, printed `any` (the §14/§27 boundary argument's third application). FIVE gate sets measured and priced in §31; the landed set excludes any-meaning-resolvable names, `arguments`/`globalThis` (port misses, now their own future rules), and import-machinery files. 182 of the 195 R→W are the standing JS md5 case. Four stand-ins updated; ADR-0038's fixture renamed with its §31 truth |
| 2026-08-07 | §29 landing | **78.19%** | **3,233** | **+2,107/68 (§29); 0 (§28, premise corrected)** | **Builds 47–48: the second mountain.** §28's numeric element-access bar had a WRONG premise (the corpus already answers through the lib's index signature; the arm stays as the lib-less fallback) and its mandated trace found the real row: type-position indexed access, subsystem-bound. §29 then took the trace's other find — 2,000 gap lines in ONE case, a self-referential alias union — with upstream's member-type laziness rebuilt at the single seam print-at-creation permits: an on-stack alias mention answers a memoized NAME placeholder (read-only probe, no failure marking). Degenerate cycles (`type T = T`) now answer their name, a stated trade (`bd tsr-5e7.6` owns the diagnostic) |
| 2026-08-07 | §27 landing | **77.75%** | **3,232** | **+155/0** | **Build 46: readonly assignment targets.** `M.x = 1` on an exported const answers upstream's TS2540 `errorType`, printed `any` — the §14 boundary argument's second application, at both target sites (`checker.go:11096`/`:11377`). Fired leg: the constructor exception (`this.x` readonly assignments in the declaring constructor are legal). Zero adverse after the gate |
| 2026-08-07 | arrays-§6 landing | **77.72%** | **3,228** | **+85/22** | **Build 45: array spreads.** `getSpreadElementType`'s `Array<T>` half; the 22 adverse are the falsifier's own tuple/contextual population, counted and accepted |
| 2026-08-07 | §26 landing | **77.70%** | **3,228** | **+167/82** | **Build 44: `x!`.** The non-nullable remainder; the `null!`-keeps-null refinement was measured BOTH ways and refused (+102/147 vs +167/82). The 82 are chain-interplay and `null!` families, both variants' numbers recorded |
| 2026-08-07 | §25 landing | **77.66%** | **3,228** | **+438/5, +26 cases** | **Build 43: `void` and `delete`.** Two one-line rules off the fresh TERMINAL board (`undefined`/`boolean`); 26 cases finished |
| 2026-08-07 | §24 landing | **77.57%** | **3,202** | **+1,342/422, +79 cases** | **Build 42: template expressions.** The fresh `rank_board`'s top TERMINAL row (961 lines) plus its downstream: `checkTemplateExpression` with the all-literal fold (the evaluator's observable for string/number parts) and three declines. The 422 adverse are four OWNED families (invalid-arithmetic rendering ×216 — its fix was measured and refused at −562 R→G; scanner legacy-octal cooking ×66; the contextual template-literal blind spot ×19; tagged overloads ×10). The twenty-fourth and twenty-fifth stand-in fixtures came due |
| 2026-08-07 | §22-narrow landing | **77.29%** | **3,123** | **+27/3** | **Build 40: `if (isNumber(x))` narrows.** `narrowTypeByCallExpression`'s identifier-predicate half. Three bar legs fired and each taught upstream's exact shape: mutual relations prefer the ASSERTED type (the four-rung ladder, `flow.go:915`), unchanged mappings keep the original named alias, and the false branch keeps constituents the true branch mapped AWAY (`{}` vs `Record`). The 3 residual are relater precision on `Record` instantiations |
| 2026-08-07 | §21 landing | **77.29%** | **3,122** | **+29/2** | **Build 39: strict catch variables are `unknown`.** `useUnknownInCatchVariables` follows the explicit `@strict` directive only — a bare `@strictNullChecks: true` does not imply it. The second fired leg was procedural and transferable: a producer-side plumbing edit silently failed (python replace printing ok without asserting), measured byte-identical, and was caught by the identical TOTAL — plumbing edits assert their anchors now |
| 2026-08-07 | §20 landing | **77.28%** | **3,122** | **+103/0** | **Build 38: nullable initializers widen — non-strict only.** `getWidenedTypeWithContext`'s nullable arm; the bar's first pair fired 69 R→W and named the gate (the widening twins exist only with `strictNullChecks` off; strict `let x = null` keeps `null`). Gated: zero adverse |
| 2026-08-07 | §19 landing | **77.26%** | **3,118** | **+376/6, +32 cases** | **Build 37: `(...args)` is `any[]`.** The `any[] ← any` board row was one rule — the implicit-any fallback's rest arm (`reportImplicitAny`'s `anyArrayType`). Thirty-two cases flipped whole on a five-line arm. The 6 adverse are `tsr-5o2`'s written-annotation node-reuse (upstream prints a written `(...args)` fn-type verbatim as `any` while typing the symbol `any[]` — this port prints the computed type) |
| 2026-08-07 | §18 landing | **77.18%** | **3,086** | **+142/9** | **Build 36: the enum member's regular twin.** `Choice.One → Choice` answered Unknown because `get_regular_type_of_literal_type` interned a LOOKALIKE of the union's constituent — upstream's `freshType`/`regularType` are two pointers on one type object, and the port's equivalent is a back-link recorded at the single member-type creation site. One fired leg: one-constituent unions (the one-member-enum deviation) decline assignment reduction. The fix's reach: enum equality narrowing and reduction decide corpus-wide, not just the 120-line head case |
| 2026-08-07 | §22 landing | **77.16%** | **3,084** | **+114/23** | **Build 35: optional call chains.** `checkCallChain`'s boundary — the callee strips its nullable half via `getOptionalExpressionType`/`checkNonNullType`, resolution runs on the remainder, `propagateOptionalTypeMarker` re-unions. The 23 adverse are named and owned elsewhere: closure callees this port does not flow-narrow (§13 residue — the marker fires where upstream's narrowing already removed `undefined`), and `deleteChain`'s inner-link marker mechanics. A gate-report correction also landed this block: the §17 commit claimed clippy ok while the lib-test target was red — rtk masks exit codes (now in memory + verified by `grep -c` since) |
| 2026-08-07 | §17 landing | **77.13%** | **3,084** | **+72/2, +4 cases** | **Build 34: the string-index fallback.** `checkPropertyAccessExpressionOrQualifiedName`'s `prop == nil` path — a property miss on a receiver with an applicable string index signature answers the index value type. Fired leg: `@noUncheckedIndexedAccess` (adds `\| undefined`) plumbed as a real per-case option |
| 2026-08-07 | §16 landing | **77.12%** | **3,080** | **+108/6, 27 W→G** | **Build 33: SWITCH_CLAUSE.** `getTypeAtSwitchClause`'s two matching arms with Kleene declines. The build's transferable lesson repeated §12.5's: the first pair read +14/17 and ONE trace print named it — the clause node's `kind` token is `CaseKeyword`, not the node kind, so every witness read as default and every case narrowed to `never`. Second leg: JSDoc parenthesized casts don't narrow (paren skip declines in JS files — the JS trap's fourth appearance). Priced residue: typeof-facts granularity for `function`/`object`, type-parameter narrowing, the 1,298-line discriminant-property row |
| 2026-08-07 | arrays-§5 landing | **77.09%** | **3,078** | **+526/2** | **Build 32: `union(E, E)` is `E`.** The `E[] ← error[]` board row (512) was the named-union decline firing on IDENTICAL inputs — `[E.E0, E.E1]` widens both elements to the same named `E`, and `T \| T = T` needs none of upstream's `origin` denormalisation. One identity fast path in `get_union_type`. The 2 adverse are the recorded null-widening intrinsic gap surfacing one step closer (`error`→`null` against widened-`any` wants). Diagnostics 993 → 998 |
| 2026-08-08 | `8dcdc71` | 77.25% | 3,093 | **printer_round_trip 11,755/11,778 → 11,776/11,776 = 100% (COMPLETE); binder_symbols 8,310 → 8,401 (+91); dts_emit 327 → 333; dts_shape 859/1,007 → 860/1,008** | **The twelfth session (non-checker conformance): a suite retired and two others advanced, seven commits.** (1) `36c5918` closed the round trip's last 23: sixteen were JSDoc trivia leaking into the token histogram (the gate's own scope statement — "JSDoc is trivia" — made positional via the JSDoc side table's spans); five were the parser diverging from upstream's class-element grammar (`tryParseConstructorDeclaration` commits on the keyword alone and parses type parameters + return type, `parser.go:1917`; an asterisk commits to a method, `:1944` — `*constructor() {}` is a *method* upstream); two were the printer dropping setter return type annotations upstream's `emitSignature` prints. The parser fix moved `checker_types` +5 and `diagnostics` +1 as riders. Also in it: dts path references rebase against the emitted file's directory (`getReferencedFiles`, `transform.go:464`) — `commonSourceDirectory`. (2) Three dts slices: annotated object-literal accessors keep their shape (pair → both signatures, lone getter → `readonly` property, lone setter → mutable property); an arrow returning an annotated name copies the annotation, gated on certain resolution (own parameter first, file scope only when no scope can intervene, `typeof` annotations never copy — they resolve through upstream's print); JSDoc `@returns`/setter-`@param` type JavaScript accessors. Plus `!!!!` baseline-runner annotations recognized as metadata (`noEmitOnError` — the noCheck emit is the right oracle for a checker-free emitter). (3) `8dcdc71`: the binder suite indexes **alias-transparent spellings** — the `.symbols` baseline is checker-written and prints an aliased symbol under the alias's name with the *target's* declarations; an entity `import x = a.b.c` resolves through `resolve_name` + exports tables and indexes its target (members three deep) under the alias spelling; an alias the binder failed to create adds nothing, which keeps the gate honest. +91 cases, no binder behavior changed. **Filed `bd tsr-1`**: numeric member names should canonicalize (`0b11` binds as `3` upstream, `ast/utilities.go:3160` + scanner value) — needs an arena through `FileInfo` or a value field on `NumericLiteral`; ~6 cases. Residue classified in `TASK-conformance.md` and `CONFORMANCE_TODO.md`: dts_emit's remaining 41 are ~25 checker-owned + ~10 faithful parse skips + comment-preservation and CommonJS-exports families; dts_shape's 148 are dominated by checker-driven import synthesis; binder's 72 are canonicalization, late-bound folding, export= augmentation, `@overload` lists and singles |
| 2026-08-07 | §15 landing | **76.98%** | **3,078** | **+1,147/5 (229×), +3 cases** | **Build 31: compound assignments do not narrow.** The `2,624 number ← any` board row decoded at `flow.go:229`: upstream's assignment arm SKIPS a compound target's effect, answering the antecedent's type at the literal's base — the md5 chains stay `number` through every `a += any` because the plain shift re-anchors and `+=` never injects. `binaryArithmeticControlFlowGraphNotTooLarge` whole (968) plus `controlFlowSelfReferentialLoop`'s remaining 131. Fired leg: catch variables were misclassified auto and the skip exposed it (`useUnknownInCatchVariables01` — `is_auto_typed_declaration` now excludes catch clauses). 3 residual = `noImplicitAny`-off, options unplumbed, `bd tsr-4sc.11`. Diagnostics rode along 964 → 993 (+29 across two builds) |
| 2026-08-07 | §14 landing | **76.75%** | **3,075** | **+10,000/0 — one build, one case, +2.09 points** | **Build 30: the too-large bail.** The wrong board's head was ONE case — `largeControlFlowGraph`, 10,001 lines want-`any`, 29% of the entire wrong column. Mechanism: upstream's DELIBERATE give-up — 10k chained `data[0] = 0` against `const data = []` (autoArrayType) trips the flow depth cap through `getTypeAtFlowArrayMutation`'s per-mutation recursion (`flow.go:118`/`1404`), TS2563 reports once at the declaration, `flowAnalysisDisabled` poisons the containing body (`checkBlock` save/restore, `checker.go:3791`), and every flow reference answers `errorType` — **which upstream prints as `any`**. The port: a mutation-count stand-in for the recursion (≥2,000 same-name element assignments in the container), a per-container disabled set scoped by lexical containment, and the `any` intrinsic as the bail's answer — with the ADR-0038 boundary argued in `checker-notes-narrow.md` §14: `error`-printing is for THIS port's failures; TS2563 is upstream's own, and its observable IS `any`. One leg fired in-build: the declaration keeps its widened `any[]` (the poison is for references, not the symbol). Falsifier (a) held: `binaryArithmeticControlFlowGraphNotTooLarge` (10k nodes, iterative walk upstream, no trip) untouched |
| 2026-08-07 | §13 landing | **74.66%** | **3,074** | **+166 net across three more builds (77+0+89), 0 adverse lines** | **Builds 27–29.** §21: a decided overload failure answers the INTERSECTION of candidate returns (`createUnionOfSignaturesForOverloadFailure`, `checker.go:9620`) — the iteration-errors `foo(x) : never` decoded; its bar's falsifier fired on `fn1(undefined)` under `@strict: false` and was honoured by the non-strict undefined/null guard (ambiguity stays a gap). §12.8: upstream's empty-so-far loop re-entry restart, landed at a measured ZERO with the wrong prediction recorded. §13: past-last-assignment closure narrowing — the `flowContainer` extension loop, the START walk-out through the closure's creation-site flow (one binder `record_flow` arm), and `markNodeAssignments`' single-walk position marking; two legs fired in-build (stack-order vs source-order → maximum extended position; `export let` exclusion) and the final pair read +89/0. Diagnostics rode along +12 (949 → 961) |
| 2026-08-07 | `238261b` | **74.62%** | **3,064** | **+757 net (+63 then +694/0), +21 cases, 2 regressed then both recovered** | **The ninth session's continuation, builds 25–26: the loop fixpoint landed by exonerating it.** §12.5's mandated one-line trace (`TSR_TRACE_LOOP`) proved all eight of `controlFlowIterationErrors`' loop labels compute the upstream-correct union — the eight-cycle "semantic residue" was never in the walk's arms; it was **per-node cache entries stamped during transient back-edge passes** (`foo(x)` resolved against a provisional `string`, cached forever). Upstream's guard is `checkExpressionCachedEx` clearing `flowLoopStack` before caching (`checker.go:7517`); the port's dual is a one-line write gate: `check_expression` persists nothing while `flow_loop_stack` is non-empty — §9.2's "provenance must be a flag", named. That landed +117/54 with two regressed cases, and reading the 53 adverse lines against upstream collapsed two residue families into ONE missing rule: **a variable in a definite assignment-target position returns its DECLARED type, no flow analysis** (`checker.go:11109` — `AA=a : number` but `AA : any`, the target prints declared, auto prints `any`; the "self-referential any bail" hypothesis was WRONG, no bail exists). §12.7 ported the full `GetAssignmentTarget` walk + kind classification (logical assignments corrected to DEFINITE, amending §10): **+694 with zero adverse transitions**, the session's cleanest sweep. Residue priced: overload-failure `never` (a callres question), so-far under-accumulation (~20 lines), and upstream's nested-loop restart semantics (empty so-far falls through, `flow.go:1347`). Interleaved with the diagnostics session's pushes; snapshot regenerated on the merged tree, 947 → 949 |
| 2026-08-07 | `HEAD` | untouched | untouched | **`diagnostics` 717 → 893 (13.06% → 16.27%), +176 cases, 0 lost across all seven builds, 0 regressed** | **The tenth session: seven builds down the long tail, and the session's real product is the number that prices the top of the board.** Builds: `checkUnusedIdentifiers` **+115** (the largest — seven codes, reference marking rebuilt as its own over-approximating pass because upstream's is a side effect of type checking this port does not do); TS2322's four anchors **+34**; the `@ts-ignore`/`@ts-expect-error` **program-level** filter with TS2578 **+2**; `noImplicitAny` parameters **+7 for zero wrong**; type-argument arity **+12 for one wrong**; call arity **+6 for zero wrong**. **The goal was 50% and 50% was measured as unreachable rather than missed**: `diaggap.rs`'s single-code column — the only forecastable one — sums to 3,217 cases, of which TS2322 (510) + TS2339 (143) + the TS2345/2741/2353/2411/2430/2416/2420 family (≈370) all sit behind the members-table subsystem §5 refuses. **Four transferable findings.** (1) *An incomplete relation does not report less, it reports wrongly* — TS2322 gated on nothing but the error type measured **947 right against 988 wrong**, `checker_types`' 26% non-gradient arriving as diagnostics, and every gate in `checker-notes-diag2.md` §16 exists to bound it. (2) *A register site inside a conditional is a precondition, not plumbing* — `registerForUnusedIdentifiersCheck(sourceFile)` sits inside `if IsExternalOrCommonJSModule`, and missing that reported every top-level declaration of every script in the unused corpus: **108 of 158 wrong lines and all three losses**, all of which then converted. (3) *A type-shaped question can still be syntactic* — `getMinTypeArgumentCount` is "the index of the first type parameter with a default" and `getMinArgumentCount` is "the index of the first optional parameter"; both read off the declaration, and both rules landed at 0–1 wrong lines. §14's ordering rule, holding for a third and fourth kind of rule. (4) *The error node can differ by direction* — TS2554 reports on the callee when there are too **few** arguments and on the first excess argument when there are too **many** (`checker.go:9770` vs `:9804`); getting one half right looks like a working rule. **Five refusals with numbers** in §5, and one of them (auto-to-any) is recorded only to keep **one owner** with the `.types` workstream's §9.1 |
| 2026-08-07 | `0f8838f` | **74.13%** | **2,942** | **+3,926 across twenty-four builds and FOUR measured refusals, 0 lost, +202 cases, 0 regressed** — the sixteenth a measured zero shipped for the crutch it deletes (§16.1), the seventeenth the `_1` family's enclosing-scope half at 151/0 with two scope rules learned from the losses (`checker-notes-callres.md` §19.1) | **The ninth session: TASK.md's three items and one found item, every build bar-first.** (1) **Fresh wrong split** (`wrongflip` at `0a609cc`, wrong 35,859) surfaced the `undefined[] → never[]` W2 row and the arm landed same-session (`3bfec72`, +246, WRONG→RIGHT 246 with **zero** other RIGHT traffic, 7 cases — exactly the row's forecast upper bound): `checker.go:8098`'s non-strict branch, skipped since before `set_strict_null_checks` existed. (2) **`tsr-epnz` → the alias-rename filter** (`594973e`, +377): `best_name` widened from external-import-equals-only to upstream's own exclusions (`useOnlyExternalAliasing` is FALSE in the baseline path — only hover sets it), **split by print position** after the bar's named falsifier fired at 130 losses in exactly the four `privacy*` cases — the corpus wants the same-file alias on chain *segments* and the target name on the *whole* print, same file, same symbol pair. Mechanism (b) first broke the enum-union collapse (2 regressed, reverted) and relanded through the existing `enum_member_owners` side table at **+6 exact**. **The sized 26 converted 11; the +377 is 96% segment-path windfall** — recorded loudly in `checker-notes-modobj.md` §10.17. (3) **`tsr-fpti` re-probed: no buildable item** — 960 lines decompose to ceiling (256 want-any element lines are upstream `errorType` at `jsx.go:74`; the fragment escape at `:123` has a measured population of ZERO), `declare global` augmentation (154), pragma factories (`tsr-xpb8`), the heritage shard's downstream argument types. (4i) **the return aggregate reduces, behind a flag that now exists** (+84, +10 cases, 0 own wrong — §10's three-consumer wiring was REFUSED whole (+168 but 44/12, three unrelated owners pooling under one bar, §10.1), the return consumer alone then refused AGAIN on the JS `@overload` falsifier whose exclusion had nothing to read — **`NodeFlags::JAVASCRIPT_FILE` was declared and set by nothing, the `AMBIENT` trap's sibling, caught before it cost a build** — and landed once the program's two parse sites stamp it, §11.2; fixture pairs sixteen and seventeen came due against an intuition comment). (4h) **`removeSubtypes` lands, decidability-gated** (+112, WRONG→RIGHT 45, **zero** other traffic after five measurements and three falsifier-driven narrowings — `tsr-eak`'s 1.03 is now *located* in the relation's unread modifiers rather than diffuse; the modifier-bearing population returns to the board owned by `properties_related_to`; the §9 bar's falsifier fired verbatim on `{ a } | { readonly a }` and the fifteenth stand-in fixture came due, `checker-notes-assign.md` §9.1). (4g) **`||` and `??` land for the reduction-agnostic pairs** (+815, **+29 cases — past 74%**, the session's largest: `bd tsr-5s2`'s refusal re-measured — the reduction question belongs to the constituent PAIR; two legs fired across three measurements and both reshaped the build, the non-strict `Base*Facts` delta placed at the whole-operand question after a global placement lost 2 lines to narrowing leakage; leg 2's 18-vs-15 overridden loudly at 45:1 with the overage owned, `checker-notes-assign.md` §8.1). (4f) **the conditional's identity/`any` branches** (+157 — the first conditional build to clean more *wrong* than gap, WRONG→RIGHT 84; the union of `[t, t]` needs no reduction, `any` absorbs under both, `checker-notes-assign.md` §7). (4e) **`await e` answers** — the identity-and-global-`Promise` slice of `checkAwaitExpression` (`checker.go:10845`, +245, **+22 cases**, own wrong 6, 0 lost — the board's 578-line `AwaitExpression / NO STEP ARM` row had no expression arm at all; the unwrap runs through `type_reference_targets` against the merged global `Promise` symbol and declines every thenable it cannot prove). (4d) **the same two arms reach class methods** (+46, no other transition, +5 cases — a class method takes no contextual return type, and the object-literal exception is exactly what `may_return_never` discriminates, `checker.go:29711`). (4c) **async primitive returns answer `Promise<T>`** (`c3b47e2`, +15 with NO other transition — the §16 residual refused two admitted shapes before shipping: nullable primitives widen to `any` non-strict, and a reachable body end appends `undefined` strict, which gave `block_completes_normally` its `ReturnStatement` arm). (4b, same session, continuing) **generator declarations answer `Generator<Y, void, unknown>`** (`f273c08`, +63, 1 own wrong, **+17 suite cases — past 30% of cases**): §15's bar had leg 2 fire twice (bare `yield;`, `castOfYield`'s next slot, the `1 | 2` literal union, the computed-name walker blind spot — each reshaped the arm) and leg 1's guessed +80 floor miss at +63 **overridden loudly** in §15.1 — the floor priced unsized shares that turned out to be the majority, and the remaining conversions are 63/1 with zero losses and 17 whole cases. (4) **The surprise: async declarations answer `Promise<void>`** (`097d6bc`, +176 = 101% of the 174 forecast, 0 own wrong, +4 suite cases): `checker.go:20175`'s zero-aggregate async arm, gated to declarations because only non-declarations consult the contextual return type. Residue owned: async valued returns 39 (`getAwaitedType`), generators 216, esm string-named export= 6. **Found and left on the table: the `_1` type-parameter disambiguation family, 625 wrong lines in 128 cases** — upstream renames a shadowing type parameter (`T` → `T_1`) per printed type; a baked-text architecture needs a per-print naming context to follow. Unowned, diffuse (top-1 16%), the largest single naming family left |
| 2026-08-07 | `0fab830` | untouched | untouched | **`diagnostics` 80 → 717 (1.46% → 13.06%), +637 cases, 0 regressed, 0 lost** | **The eighth session: the suite two handoffs called "structurally blocked" moves 8.75×, and the unblocking was a measurement rather than a build.** ADR-0040's diagnosis was right about *why* the number was flat and was being quoted as a *sizing*; `diaggap.rs` (new) asked what the blocked cases are blocked **on** — **3,258 of 5,488 on exactly one code**. Then ADR-0040 decisions (1) and (2), a `Checker`-owned collection and a `check_source_file` traversal, and eight builds: **TS2307/2882** +50, **TS2564** +133, **TS2304** +115, **TS2454** +233, **TS2369/2695** +40, **break/continue grammar (5 codes)** +25, **TS1036/1183** +24, **the overload implementation-expected family** +17. Two **refusals with numbers**: TS2339 at 2 converts / 254 wrong and TS7026 at 12 / 47. `checker_types` byte-identical throughout — the traversal is a second entry point and no query call site invokes it. **Four transferable findings.** (1) *The gates are the design*: upstream's `resolveExternalModule` carries fourteen messages and TS2307 is what is left when thirteen decline; ungated 45/0/100/**60**, gated 50/0/101/**21**. (2) *`tsr_ast::NodeFlags::AMBIENT` is declared, documented and set by nothing* — it caused **three** separate residuals (86 wrong lines, 4,781, 2) and a flag with no writer is a landmine with one instance per reader. (3) *A wrong column dominated by one case is one predicate*: `genericDefaults`'s 227 lines were `declare const`. (4) *A syntactic rule has no incompleteness to leak* — the three grammar builds converted 89 cases for **zero** wrong lines and needed no tightening pass, where all four semantic rules needed two or three. Forecast **exact twice** (50, 133) because the counterfactual calls the shipped code. Scored on `passing + single-code reachable`, the only currency that prices a false positive: TS2304 +56, TS2454 +28 — and **−85 / −210** respectively before their last refusals landed |
| 2026-08-07 | `7299a14` | 73.65% | 2,841 | 0 lines, by design | **the eighth session opens by falsifying the seventh's closing sentence.** Board re-run fresh: gap **80,315**, every row unchanged — and *"no unowned row remains above 1,000"* holds only over the endings `depend.rs` **attributes**. `cyclegap.rs` (new) measured the two that attribute nothing (8.4% of the gap): the `cycle` ending is **98.6% a length-1 self-loop**, every one a declaration with neither annotation nor initialiser, i.e. `NO STEP ARM` under a label that says "a real shape here". **`FunctionDeclaration`-cycle is unowned: 2,889 net, want-any 1.8%, 611 cases, 1,894 wanting a signature.** `ArrayType` 2,120 is 99.7% *propagation* (2,114 have a gapping child type node); 4,696 of 8,499 type-node roots are the same. And the first whole-gap **want-shape** histogram: **24.9% wants `any`**, and **signature/arrow is 12,606 lines (15.7%)** spread across six rows — §4.4's structured-signature capability seen from the answer side. Both `depend.rs` fixes named and not made, so the board stays comparable. **Then the row was split the same session (`fnsiggap.rs`) rather than left as a candidate: 72.5% is downstream (a parameter or a returned expression gaps) and the item is 810 lines / 779 net in four mechanisms — plain 366, async 228, generator 173, async generator 43. ≈0.16 points at 100% conversion.** Its C1 fired on the first run (1,475 vs 2,943) and diagnosed the *probe*: it had tested the line's own node, while half the row is reached only after ≥1 step. **So the strongest unowned candidate the board has had since the seventh session closed is worth under two tenths of a point** — §4.4 arriving through another door. `docs/architecture/checker-notes-cyclegap.md`. **Also recorded, then CORRECTED the same session: ~~the clippy gate is RED at `7299a14`~~** — reported as red on `crates/tsr-ast/tests/kind_conformance.rs:83` (`inefficient_to_string`). **It is green.** The reading came from a machine whose default toolchain was **1.89.0**; on the 1.96.0 the workspace now pins, `clippy --workspace --all-targets -- -D warnings` finds **nothing**, and CI's `@stable` had been green on that file since the scaffold commit. The diagnosis in the original note ("toolchain drift, not a regression") was right and the *verdict* was wrong — **an unpinned lint gate is not a gate, because its answer is a property of whoever ran it**. Fixed at the cause: `rust-toolchain.toml` pins 1.96.0 and both workflows pin the same version at the action. **Left standing and unresolved: `Cargo.toml` claims `rust-version = "1.85"` and the workspace uses let-chains, stable in 1.88 — the MSRV claim is false and is NOT what this session pinned; it needs its own measurement** |
| 2026-08-05 | `058b4a9` | 61.09% | 2,173 | — | baseline for the session below |
| 2026-08-07 | `154653b` | **73.30%** | **2,793** | **+2,664 in the session's third act, 6 lost, +26 cases, 0 regressed** | **The JSX element arm and the composite-print seam, each a stale premise re-measured.** JSX (+1,153, `9fe8056`): the "46% cannot resolve" figure predated the `/.lib` mount; the arm is 30 lines and converted 152% of forecast. The seam (`bd tsr-2ghn`, filed, sized and SHIPPED in one session): symbol-exact counterfactual (`sigprint.rs`, self-check leg) → the Union/Intersection naming arm it exposed (+11, `0796633` — a class-typed return qualified while an alias-typed parameter did not, ONE missing match arm) → the site-aware twin `signature_to_string_at` (+1,500 — **forecast delivered to the line**, RIGHT→WRONG exactly the measured 2, because probe and build are the same function). Plus the default-import arm (+83, `cc8c422`) with the `default`-never-prints refusal cleaning 67 pre-existing wrongs |
| 2026-08-07 | `cc8c422` | **72.74%** | **2,767** | **+859 across five checker builds + one harness mount, 5 lost, +17 cases, 0 regressed** | **The seventh session's second half: the whole export= / naming family, each slice with a registered bar.** After the ambient and container-qualifier slices (rows below): the **export= chain** `952b328` (+297 — `ExportAssignment` arm, `import a = require` follows `resolveExternalModuleSymbol`, and `best_name`, the innermost-table walk; **leg 4 fired at 130 lost on the first measurement** and the fix was upstream's own `useOnlyExternalAliasing` flag — the counterfactual's alias reduction was blind to same-file `import a = b`, a recorded probe defect); the **chain-segment rename** `67949ee` (+58, WRONG→RIGHT 58 with *no other transition*); the **default-import arm** `cc8c422` (+83, Δwrong −38 — its first measurement fired leg 2 at 67 and 40 of those printed **`default` as a name**, which upstream never does; the rendering refusal cleaned 67 pre-existing wrongs too). `bd tsr-6ph` and `tsr-6j2` CLOSED; residue filed as `tsr-epnz`, `tsr-wwum`, `tsr-fpti`. Fresh `depend.rs` at `cc8c422`: gap 81,769; every head row is owned (contextual typing, call/inference, property-access symptom, modulespecifiers, JSX instance typing) |
| 2026-08-07 | `93b540a` | **72.65%** | **2,754** | **+421 across three builds, 3 lost, +4 cases, 0 regressed** | **The seventh session: the module-object refusal retired by re-measurement, exactly as the handoff predicted.** `tsr-6ph` stood refused at "2.1 and 2.5 wrong per right, two designs" — both designs printed the module's *file path*, and the alias-search naming that shipped since (c91314c, designs W/P) was never priced against it. Fresh counterfactuals (module_object.rs alias-search columns, qualnamep.rs container-qualifier arms): (1) **`tryFindAmbientModule`** `f62582e` +128 — its leg-2 global fired at +345 and split by arithmetic into ~15 mechanism-own + 328 correct-types-wearing-unqualified-names with named owners; (2) **the container-qualifier arms** `897abdd` +293, RIGHT→WRONG = exactly the 3 pre-named at-risk lines, Δwrong −293; (3) **the `/.lib` mount** `93b540a`, measured 0, shipped with the zero stated — it bought the finding that the tsx `typeof React` family (largest remaining head) sits behind `export = __React` + alias-preferred naming. **Two probe defects caught before quoting**: both ambient tests keyed on upstream's *quoted* symbol name, which this binder never stores (binder.rs:4091) — `tsr-xpb8`'s "ambient = 0" was re-taken on a sound test and survived. `bd tsr-xpb8`'s ambient half is shipped; its file half still needs modulespecifiers for the no-alias cases (107 want `import(…)`) |
| 2026-08-07 | design **P** | **72.53%** | **2,741** | **+2,973, 17 lost, +61 cases, 0 regressed, gap→wrong 0** | **`getSymbolChain` — the printing half of qualified naming, and the second four-cycle refusal retired by re-measurement in two sessions** (`checker-notes-qualname.md` §10–§11). The refusal's cost (3,202 lost) was cycle 20b's; re-taken on a post-W compiler it is **14**. Sized at 2,990/14, bar registered at `90c4e70` **before any code**, **all four legs passed** — and the conversion column landed on **2,990 exactly**, 100.0% of forecast, because §10.5 registered in advance that this forecast was a *point estimate* rather than W's floor and said not to expect another 211%. **Leg 4 is the one worth copying**: P is a renaming, so it cannot make a gap line computable, and `gap→wrong == 0` was registered *by construction* — making a non-zero reading a **diagnosis** ("the build changed resolution too") rather than a trade. That is `docs/conventions.md`'s post-W rule finally written as a leg instead of a caveat, and the instrument that scores it (`verdictdump.rs`) is the reusable part. Two corrections: **`qualname.rs`'s at-risk 36 is wrong and is 23** (it omitted `getMergedSymbol`, `symbolaccessibility.go:696` before `:702`), then measured ON against OFF on the real build at **13 losses removed for 0 conversions**; and the unit fixture written to pin that clause **does not** — checked by deleting the merge and re-running, it still passes, because the binder resolves a single-file merge as it binds and all 13 lines are *cross-file*. Conceded: 17 real losses (9 to `bd tsr-4jk`'s alias chain, 4 to `getContainersOfSymbol`'s unported multi-parent `sortByBestName` ordering, named case `compiler/giant`), and **64.1% of the gain is `compiler/temporal`** — leg 3's `cases gaining ≥ 60`, read 174, is what proves it is not a one-case build |
| 2026-08-07 | `a1cbd93` | **72.56%** | **2,750** | **+114, 0 lost, +9 cases, 0 regressed, gap→wrong 0** | **an untyped call answers `any`** (`resolveUntypedCall`, `checker.go:9902`). The **ADR-0038 objection was answered from upstream before a bar was written**: `anySignature` returns `anyType` (`checker.go:1042`) and `unknownSignature` returns `errorType` (`:1043`) — two adjacent lines, two situations, and upstream's *error* path is the separate `resolveErrorCall`. The ADR was **not amended**; it is immutable and unchanged. **The first design was wrong and its number is the finding**: the naive flag test read **328 gained / 248 manufactured wrong**, leg 4 fired at 248 against 20, and the bar's *second named falsifier* had predicted exactly that check. The residual said why in one line — `want string got any` was 137 of 248. **This port's `any` and upstream's are not the same claim**: upstream reaches it only where the source *said* `any`, this port also reaches it wherever an unported mechanism gives up, so `IsTypeAny` is sound upstream and unsound here. Narrowed to *written in an annotation*: **114 gained, and nothing else moved at all**. All four legs pass; **falsifier 1 did not fire** — it predicted 47% in one case, measured top case is 30 of 114 and net-minus-top is 84 against a threshold of 50, so the item is more diffuse than it was sold as. The narrowing costs **214 conversions to remove 248 wrong lines** and is taken anyway: answering off a premise the two compilers do not share is a wrong rule, not a bad trade. It also **subsumed** the registered positional refusal, whose predicate was deleted rather than left dead |
| 2026-08-07 | `a662de1` | **72.54%** | **2,741** | **+32, 0 lost, 0 regressed, gap→wrong 3** | **the callee-type family split (`calleegap.rs`) — and it is NOT an item.** 2,276 lines, and the two gates are **positionally disjoint**, proved by a cross-tab against the blocking node's kind rather than by impression: `new` owns all 1,398, `call` owns all 878, and the label collision is a `define_counters!` declaration-order shadow (`calls.rs:737` vs `:400`). The "downstream" bucket, put first as the rule requires, reads **0** — no inflation, unlike the object-literal row's 77%. **58.8% is inherited refusals** (`tsr-4sa`'s six families plus inference, refused this session at ~17); the generic-candidate bucket alone is 923 lines and is inference's. Two things fell out, both effort 1: a **stale namespace refusal deleted** (+32, all four legs passed — it had declined 35 lines to avoid **3** wrong ones, right about the phenomenon and an order of magnitude wrong about its size), and **`resolveUntypedCall`** sized at 213 with a named positional refusal. Two controls fired and both diagnose the **probe**: C6's 292 is 291 `SymbolConstructor` reproducing `tsr-4sa`'s 291 **exactly, five builds later**, and C1's 53 is inherited from `callgate.rs` itself. **Process miss recorded: `246f004` shipped with clippy at 8** — the count was printed in the same compound command as the commit, the third instance of this failure here, and the fix is that the gate and the commit must be separate invocations |
| 2026-08-07 | `a57a04b` | **72.53%** | **2,741** | **+2,973, 17 lost, +61 cases, 0 regressed, Δwrong −2,973** | **design P — the symbol chain**, the other half of qualified naming and the second half of a refusal that stood for four cycles. §8 sequenced it (*"W first, then re-measure P against whatever W leaves"*) and that turned out load-bearing rather than tidy: **W's conversions created P's population**. Re-sized from scratch (every prior P number was pre-W), bar registered at `90c4e70` **before any code**, and **all four legs passed** — net 2,973 against a floor of 800, trade 175.9:1, 0 cases regressed, and leg 4's `gap→wrong == 0` reading a structural **0** because P is text-only. **The forecast was exact: 2,990 forecast conversions, 2,990 delivered.** The four-cycle refusal ("lost 3,202, regressed 753") was of the design that qualifies *everything* — measured here at **717 at risk, 5.4:1** — while the design honouring upstream's `needsQualification` stop conditions loses **17**. A control (CP6) fired during sizing and was traced to a missing `getMergedSymbol` in the *earlier* instrument (`symbolaccessibility.go:696` before `:702`); §11.2 then re-measured that clause on the **build**, ON against OFF: 0 conversions cost, 13 wrong lines removed. New `verdictdump.rs` makes `right + gap + wrong` an identity from one pass, and offers 10,039 unaligned lines as the **probable** explanation of §1's "9,145 short" — flagged as a hypothesis, not a reconciliation |
| 2026-08-07 | `8e28971` | **71.91%** | **2,680** | **+3,590, 0 lost, +17 cases, 0 regressed, Δwrong +84** | **qualified type names reprint what was written — and it exists because a REFUSAL was re-examined, not because a new row was found.** §5 had refused this on "lost 3,202 lines, regressed 753 cases"; the fresh counterfactual showed that cost was measured on the **printing** half and then quoted against the **resolution** half's population, so a 1,318-line refusal had been blocking a 4,557-line row. Design **W** (reprint the written entity name) has *no at-risk population at all* — it fires only where the line already gaps — and forecast 1,702/20/0. Built, it converted **3,590 = 211%**, because the counterfactual's own first named falsifier ("the unscored buckets are conversions") fired. **The bar's third leg fired at 84 against 40 and is OVERRIDDEN, loudly**, by a third party who wrote neither bar nor build (`checker-notes-qualname.md` §9.6): hypothesis one was eliminated *by measurement* (the refusal was disabled and re-run), and the bar's own stated rule — "twice the forecast 20" — re-evaluated against the population that turned up gives `2 × 20 × 3590/1702 = 84.4` against a measured **84**. 0 lost, 0 regressed, 0 right→wrong traffic; 47 of the 84 are other mechanisms' defects newly *exposed*, which §9.5 measured cannot be refused away except at 11.8 conversions per wrong line. Two sub-refusals priced: INSIDE kept at 1.2:1, ENUM **declined on principle** — `enumLiteralTypes3.types:9` records `>Yes : Choice.Yes`, so the lead's suggested refusal would have refused a shape the port already gets right. The rule it bought: **an absolute on *global* Δwrong tightens as the build improves**, and belongs against the mechanism's own new wrong |
| 2026-08-07 | `8965426` | 71.16% | 2,663 | 0 lines, by design | **three rows retired by measurement, two teammates in parallel with the build above.** `typerefgap.rs`: the `TypeReference` row is **99.8% one mechanism** — namespace-qualified naming — so §4.3's "~1,867 of unknown cause" does not exist, and the qualified-naming refusal is **re-sized from 1,318 to 4,473** on this root alone. The refusal stands on its cost (lost 3,202, regressed 753) but that cost is seven builds stale, which makes it the page's strongest candidate for a fresh counterfactual. `valgap.rs`: §4.3's 1,425 was **a different cell** — the root is 7,685 — and the convertible bucket is **70 lines**, with **two controls firing** and C4's diagnosis making the 70 a *lower* bound. `fnexpr` reproduced |G| 2,082 / 14.0% a **third** time, now after three call-resolution builds: the one thing that could have moved contextual typing's entanglement did not |
| 2026-08-07 | `a4e3991` | **71.16%** | **2,663** | **+94, 0 lost, +6 cases, 0 regressed, Δwrong +3** | **the sixth session — assignability answered, and it was not where the mass is.** `bd tsr-kmzf`'s bar said run leg 1 before writing checker code and named its own falsifier as likely; both fired. The ternary relation is real (`Related`/`NotRelated`/`Unknown`, Kleene composition, **behaviour-neutral by construction** — `is_type_related_to` is *defined* as `relate_ternary(..) == Related`, and `checker_types` was bit-identical across the refactor, which is what made the forecast readable). Leg 1 read **33 against a floor of 150**, and **control C3 fired harder**: all 33 are conversions the existing **binary** relation already makes. The blocker was `calls.rs`'s `SELECTABLE` **flag set**, not the relation — my own C3 premise had conflated the gate with the relation, the **second** time this item's diagnosis was wrong while its numbers were right. Gate deleted, `choose_overload` now asks about the **pair**; the `any`-parameter positional refusal priced at 40/26 → **33/2** before shipping. The registered falsifier (net > 60) fired and was followed: `objectCreate`/`objectCreate2` supply 18 of the 94 through a **property-access callee the counterfactual never classified**. Residual read despite a passing ratio — 2 of the 5 lines entering the wrong bucket are the already-refused qualified-naming family, 2 were wrong before and are wrong differently, **exactly 1 is new.** A workspace example failing to compile made `cargo test` print **zero** result blocks; `grep -c` caught it where `head` would not have |
| 2026-08-07 | `1c20e57` | **70.85%** | **2,617** | **+3,352 across six builds** | **the fifth session's second half, two teammates in isolated worktrees.** Landed: constructor type nodes **+1,358**, type predicates **+1,041** (with a parser ASI fix that moved `parser_typescript` and `binder_symbols` **up**), private names **+590**, `tsr-84iz`'s pattern-implied tuple **+206**, `getApparentType`'s instantiable head **+156**. **Five rows were retired by measurement rather than converted** — `TemplateExpression` refused with a *ratio* (0.76 and 0.99 gained per wrong, replacing "not separable"), the object-literal row shown **77% downstream** with the accessor item at 78 lines not 2,223, the `FunctionDeclaration` row decomposed to unported type-node inputs, JSX re-scored to 0.25 feasibility, and the largest unopened root (2,404) shown to be **93% import aliases already refused on naming grounds**. Two registered bars fired and both were build bugs, caught by the bar: `tsr-rppd` at net **0** (no symbol route existed) and `tsr-84iz`'s leg 3 vacuous at `0 < 0` |
| 2026-08-06 | `tsr-tgov` | **70.15%** | **2,564** | **+686, 0 lost, +24 cases, 0 regressed, Δwrong −47** | **`new C<T>()` instantiates** — the call side already substituted written type arguments (`inference.rs:153`) while `check_new_expression` refused at its first line; 826 lines sat in that asymmetry. Sized by a **counterfactual** (`newgen.rs` forecasts the printed string against the baseline: 166 exact, 1 miss named in advance) rather than by the row, bar committed first. Conversion **413% of the forecast** — the cascade, named as upside in the registration. Reading the 68 residual instead of banking an 8.4× ratio found upstream's one name-independent quoting rule (a **method** named `new` prints `"new"`, `nodebuilderimpl.go:2384`), worth **+114 and 84 pre-existing wrong lines**. Twelfth stand-in fixture came due |
| 2026-08-06 | probe | — | — | 0 lines, by design | **`bd tsr-klm` answered — and it re-scored the board's top item down 8×** (`callgate.rs`, `checker-notes-callres.md` §13). Control C3 fired on 2,569 of 8,398 lines and the diagnosis was the *instrument*: `check_new_expression` had no counters at all, and it is the more admitted half of the row. Six gates added there, four splitting `single candidate`, `checker_types` unchanged across the change. The result: **overload selection owns 1,095 lines, not 9,660** — that figure was the row, not the mechanism — the largest gate is **inference at 2,324**, and the whole call family cannot reach 10% of the gradient at any conversion (§4.3a) |
| 2026-08-06 | probes | — | — | 0 lines, by design | **two rows leave the board by measurement** (fifth session): `retgap.rs` decomposes the `FunctionDeclaration` row — return-type inference is already ported; the mass is unported type-node *inputs*, and the multi-distinct aggregate is refused at **81** (`checker-notes-callres.md` §12). `jsxfeas.rs` walks `getJsxElementTypeAt`'s path per line — **46% of the JSX row cannot resolve the namespace** (`declare global` augmentation, a binder prerequisite now blocking two items) and **29% is the refused qualified-naming family** (`checker-notes-jsx.md`). The cheap-probe-first ordering closed two items for two probes' cost |
| 2026-08-06 | `tsr-xs0` | **70.003%** | **2,540** | **+37, 0 lost, +3 cases, 0 regressed, Δwrong −37** | **assignment narrowing keeps a fresh boolean literal fresh** (`flow.go:2421`, `checker-notes-narrow.md` §7) — found by `tsr-o00`'s wrongdelta, sized 73+47 from the live wrong dump, bar registered before the four-line fix; every leg passed with zero downside and the mechanism's named case (`literalFreshnessPropagationOnNarrowing`) converted. **This crossed the exact 70% threshold** |
| 2026-08-06 | defaults | **69.998%** | **2,537** | **+66, 0 lost, 0 finished, 0 regressed, Δwrong 0** | **defaults under a typed annotation** (`tsr-o00` §6) — the `IS_UNDEFINED` facts bit (the port's facts comment had already drawn the `UndefinedFacts`/`VoidFacts` line it splits), the `checker.go:17782` strip, sized 69/net 59, converted 112%, every bar leg passed with zero downside. The coverage display now reads 70.00% **by rounding**: the exact threshold is 12 lines away, said so it is not quoted as crossed |
| 2026-08-06 | `4b81458` | **69.98%** | **2,537** | **+1,081, 1 lost, +16 cases, 0 regressed, Δwrong +109 (9.9×)** | **binding elements** (`tsr-o00`, fifth session) — sized by the new `bindgap.rs` (1,228 buildable of 2,632, six refused legs each with a number), bar registered and committed before code (`982bfe4`). The first run read +1,239 at 6.0× and **passed every leg while minting ~80 wrong lines** from approximating pattern-contextual tuple inference — refused whole on the faithfulness rule, not the ratio (`tsr-84iz`). The parser now records array-binding holes as all-nil `BindingElement`s (upstream `parser.go:1663`); skipping them renumbered every element after a hole. The eleventh unported-stand-in fixture came due (`types.rs`). Residuals filed: `tsr-pqnh` (flow-of-destructuring, the family the bar named in advance), `tsr-xs0` (assignment narrowing drops freshness — pre-existing, exposed). 70% now sits **+78 lines** away |
| 2026-08-06 | `acdeed5` | **69.76%** | **2,521** | **+43, 0 lost, 0 regressed, Δwrong −19** | **the `in` guard** (`tsr-q9g` §6.1). The first run measured +45/−13 — numerically passing every leg — and **the 13 losses in one case were a bug the ratio would have priced as a trade**: the presence test read `SymbolFlags::OPTIONAL`, which this binder never writes, collapsing optional-property else-branches to `never`. Fixed to read the declaration's question token; the pair is pinned |
| 2026-08-06 | `e7a65fb` | **69.75%** | **2,521** | **+745 net (767/22, 34.9×), 0 regressed, +12 cases, Δwrong −448** | **`typeof` guard narrowing** (`tsr-q9g`'s typeof form) — `Relation::Subtype`/`StrictSubtype` in the relater, the sixteen typeof facts bits with per-kind aggregates, and the `narrowTypeByTypeof` arm family. Sized twice by probes before building (access lines 27, identifiers **440 with 417 wrong**); the bar's primary leg was `wrongdelta` for the first time, and it read **−448**. Conversion **169%** of the sized row. A third "unported stand-in" fixture came due (`narrowing.rs`'s typeof guard) and was replaced with a comparability pair. Residual 260 new wrong in three owned families: loop fixpoints (unported incomplete-types iteration), further narrows (`tsr-97d`), want-`any` ceiling |
| 2026-08-06 | `cf33aee` | **69.59%** | **2,509** | **+590 lines, 0 lost, 0 cases moved** | **nullable receivers and optional chains** — `checkNonNullType` (diagnostics-less), `getOptionalExpressionType`, `propagateOptionalTypeMarker`, wired at all three access sites. Sized by the new `nnaccess.rs` (704 lines, want-any 3%), bar registered before code; conversion **86%** of the sized population. **The registered falsifier fired exactly as named**: 106 of 129 new wrong lines are `controlFlowOptionalChain` wanting the post-access *narrow* — attributed in advance and filed as `tsr-97d` against the flow matcher. A types.rs fixture asserting "optional chains are unported" came due and was rewritten from `elementAccessChain.types` |
| 2026-08-06 | `9eaa2f1` | **69.47%** | **2,509** | **+103 lines, 0 lost, +1 case, 0 regressed** | **`t[0]` answers the element** — one arm at `get_type_of_property_of_type`, reading `tsr-5ll`'s reverse index; sized by the new `elemgap.rs` (56-line row, converted 184% — the seam serves more consumers than the row). All four bar legs passed (34× on the gap→wrong leg; 3 residuals are narrowing/instantiation). §8's registration guessed out-of-range is a gap and the **baseline corrected it before the code ran**: `>strNumTuple[2] : undefined` — the diagnostic and the type answer are separate channels. §3's "no members" safety argument is deliberately spent, on record |
| 2026-08-06 | probe | — | — | 0 lines, by design | **the contextual-typing withdrawal is itself withdrawn**: `fnexpr` re-run at `0d56467` reproduces the 86%-entangled table exactly (|G| 2,082, every row within 14 lines), so the refusal stands re-armed on a fresh number and the 761-score row leaves §4.2. One probe decided a 5,074-line item's session priority — the cheap-probe-first ordering paying out |
| 2026-08-06 | `0d56467` | **69.45%** | **2,508** | **+58 net (+61/−3), 64 wrong fixed, 6 new wrong, 1 case regressed** | **the tuple arm of `compare_types`** (`tsr-5ll`) — a tuple's text no longer poses as a *name*, and two tuples compare by `compareTupleTypes` (readonly, arity, elementwise). Sized to 34 lines from the live wrong dump; **three bar legs fired and are overridden loudly** (`checker-notes-tuple.md` §7): all 9 bad lines are written annotations in signature prints, `tsr-5o2`'s family, proven by baselines that record an order `CompareTypes` cannot produce. The obvious wider fix — blanket written-union reuse — was built, measured **net-negative** (+323/−270), and reverted; `tsr-5o2` carries the number. The tuples.rs two-tuple expectation was intuition and wrong; the comparator was right |
| 2026-08-06 | `ff49871` | **69.44%** | **2,509** | **+1,958 lines, 0 lost, +57 cases, 0 regressed** | **`typeof x` in type position** (`tsr-4sc.10`, fourth session) — sized by `examples/tquery.rs` (1,728-line row decomposed by mechanism form), bar registered and committed **before** code (`30d1ce8`). **The bar's gap→wrong leg FIRED** (+1,341 vs +1,053) and the diagnosis was a mechanism boundary, not a bad build: upstream reuses the **written** `typeof a` node in signature prints. Ported as `Parameter::written_text`/`Signature::written_return`; an intermediate refuse-parameters narrowing measured +1,084/+1,024 and was removed for the mechanism. Final legs all pass at **4.5×** gained/wrong. Δwrong **+423** (436 new in owned families — accessibility chains `tsr-93f`, module internal names, alias naming, signature-position reuse beyond `typeof`, filed `tsr-5o2`). Conversion **122% of the sized row** — the mechanism reached beyond it (§4.1) |
| 2026-08-06 | `b00738d` | **69.03%** | **2,452** | **+1,647 lines, 0 lost, +25 cases, 0 regressed** | **plain tuple type nodes** (`bf5681b`, +1,227) and **object-literal method members** (`b00738d`, +420). The tuple arm was registered with a bar and passed all four legs, its falsifier not firing; the method arm **was not registered**, the second such miss in two sessions, and its Δwrong/Δright of 0.35 sits just over the 1-in-3 the last three registrations used — recorded in `checker-notes-tuple.md` §6 rather than rounded down. Ten fixtures across nine files had used a tuple as their stand-in for "unported" and all came due at once |
| 2026-08-06 | `c72ebf2` | **68.68%** | **2,427** | **+58 lines, 6 lost, +2 cases, 0 regressed** | **narrowing reaches property and element references** (`tsr-6ka`) — `isMatchingReference` made structural, both access forms wired to the flow walk (the binder had recorded their flow nodes all along), and `containsMatchingReference` added after the corpus named it: five over-narrowed lines in `destructuringControlFlow`, the one direction this module can produce a wrong line rather than a gap. **Sized through the matcher first** with the new `refmatch.rs` — 181 strict, 2,172 loose, delivered 64 gained. **No bar was registered before the build**, recorded as a process miss in `checker-notes-narrow.md` §5 |
| 2026-08-06 | `2642e7b` | **68.67%** | **2,425** | **+30 lines, 3 lost, +1 case, 0 regressed** | **equality narrowing against `null`/`undefined`** (`narrowTypeByEquality`'s nullable half; the other half needs `areTypesComparable`). **Its registered bar fired on the floor — 33 gained against 150 — and is overridden, loudly**, in the commit, the issue, here and `checker-notes-narrow.md` §4: the build is right (six fixtures from two baselines, Δwrong **−18**) and the floor was derived from what upstream's *users* write rather than from what this port can *reach* — `is_matching_reference` is identifier-only, so no property-access guard narrows anything. That constraint is now the board's item 1 (`tsr-6ka`). The session's other product is a **refusal with its number**: strict-gating the optionality arm converts zero |
| 2026-08-06 | `385fb60` | **68.67%** | **2,424** | **+0.09 pts, +405 lines, 0 lost, +15 cases** | **calls through instantiated members** (`tsr-1uz`): signatures resolve from the type's recorded `Vec<Signature>`, plus `fillMissingTypeArguments`' no-candidate default fallback — built as one registered iteration after the first arm read +280 against a 300 floor and the registration's own branch sentence named the missing arm. `p.then(f)` / `p.catch()` / `arr.push(x)` resolve; overload sets stay with call resolution |
| 2026-08-06 | `856972a` | **68.58%** | **2,409** | **+0.47 pts, +2,266 lines, 46 lost, +19 cases, 0 regressed** | **signature-typed members instantiate** (`tsr-0hc`): `Signature` side-table + `instantiateSignature` arm + **`strictNullChecks` plumbed** (union constructor only). First run **failed its leg 4** (+538 wrong vs 381) and the new `wrongdelta.rs` attributed it: instantiated lib signatures rendered under the wrong strict mode; the harness default was then measured off the baselines (strict-ON) after a wrong first guess lost 1,221 lines. Δwrong finished at **−600**. Residual: 264 annotation-reuse lines (`tsr-a2c` note). Calls through instantiated members filed as `tsr-1uz` |
| 2026-08-06 | `0fe102a` | **68.11%** | **2,390** | **+2.58 pts, +12,357 lines, 0 lost, +26 cases** | **instantiated generic members** (`tsr-4qx` steps 3+4, one change) behind the **instantiation depth/count guard** (`40970d7`, corpus-neutral alone, `tsr-el3.2` half). Scored against a bar registered at `2d490b8`; all legs passed, the concentration falsifier fired and is decomposed in `checker-notes-inst.md` — 10,000 of the gain is `largeControlFlowGraph` via `Array<any>` index signatures, which **collapsed ADR-0038's ceiling estimate to 2,202 firm** (§2). Ex that case: +2,357 diffuse over 147 cases. Also corrected §1's stale wrong-bucket figure by re-running `wrongflip` at the pre-build commit in a worktree: 41,286 → 41,391, Δ+105, confirming the registered leg-4 expression exactly |
| 2026-08-06 | `d356450` | **65.53%** | **2,364** | **+0.97 pts, +4,645 lines, 0 lost** | **an unresolved type reference prints the name that was written** (`tsr-eep`) — upstream reports `TS2304 Cannot find name` *and renders the name*; answering `errorType` was the divergence |
| 2026-08-06 | `3b7fa44` | **64.56%** | **2,335** | **+0.90 pts, +4,319 lines, +60 cases** | **namespace exports resolve** (`tsr-56r`) — `resolve_name` never read a namespace's `exports`, and its locals lookup never filtered by meaning, so **exporting a declaration made it unresolvable**. Found by `examples/depend.rs` (`tsr-550`), the first instrument to walk declaration edges rather than span edges |
| 2026-08-06 | `a371ec8` | **63.66%** | **2,275** | **+0.02 pts, +81 lines, −1** | `compareTypeNames` for type references (`tsr-bgz`) — the reshape the issue said it needed was already stored in `type_reference_targets`. The single loss is `bd tsr-a2c`, a different mechanism |
| 2026-08-06 | `39a3853` | **63.64%** | **2,275** | **+0.10 pts, +481 lines, 0 lost** | union-constituent parenthesisation (`tsr-xm9`) +353, and the same predicate fixing a pre-existing defect in `array_element_text` +128. Also this session: `removeSubtypes` sized and **refused**, `tsr-jle` and `tsr-iiu` withdrawn, the call row's bar re-scored with `new` separated |
| 2026-08-06 | `5290e1a` | **63.54%** | **2,275** | **+0.20 pts, +958 lines, +5 cases** | the `&&` arm of `checkBinaryLikeExpression` — the only unblocked arm in the board's top three rows. The session's main product is the **board rewrite**: `tsr-jle` fell 11,008 → 1,004, `ArrayLiteral` and `\|\|`/`??` were shown blocked on assignability, and `new` was sized alone for the first time |
| 2026-08-06 | `3299f53` | **63.34%** | **2,270** | **+2.25 pts, +10,761 lines** | export-marker link (+2,265), `this` parameter (+2,733), `super` (+838), `@lib`/`@noLib` harness fidelity (+431), unit-return widening (+1,188), `getApparentType` (+973), `getMergedSymbol` (+430), `autoArrayType` (+1,005), object spread (+74) |

**Fourth session's process miss:** `180bcb0` shipped with clippy RED — the
compound command printed the count (5) and committed anyway, the same class as
the `head`-piped gate: instrument correct, reading skipped. Fixed and recorded
at `5a6d735`.

**Process failures worth carrying, all now written up in
`docs/conventions.md`:** a gate piped through `head` reported green while a
test failed (`5290e1a`); a registered bar fired and was overridden on
independent evidence (`2642e7b`); two builds shipped with **no bar registered
at all** (`c72ebf2`, `b00738d`); and five test expectations across the sessions
were written from intuition and were wrong — the port was right every time.
`docs/architecture/checker-notes-*.md` hold the per-item reasoning; this table
holds only the numbers.

---
| 2026-08-09 | `e1737fb` | **26.71%** | **1,466** | **+15 cases, 0 lost, 0 wrong** | **TS1100/TS1210/TS1215 — `eval` and `arguments`.** Took a row's *entire* sole-obstacle ceiling for the first time on this board. §105 and §137 had priced it as a subsystem (`b.inStrictMode` plus a three-way split); upstream's `Binder` struct has no such field and `parserStrictMode3-negative.ts` — 24 characters, no prologue, no module — reports TS1100 anyway. `checker-notes-diag2.md` §156–§157 |
| 2026-08-09 | `bda3fde` | **26.90%** | **1,476** | **+10 cases, 0 lost, WRONG delta 0** | **Cross-file merge conflicts** (`mergeSymbol`'s error arms). The binder already merged globals across files and returned silently at the excludes conflict; it now records the pair and the checker reports it, which is upstream's own layering and the only way to get the per-declaration file right. Three unported upstream branches came with it (`SymbolFlagsAssignment`, the plain-JS suppression, the `NamespaceModule`/TS2649 arm) accounting for 26 of the first measurement's 33 new wrong lines. Also **corrects §141's "~52-case merge owner" to ~10**. `checker-notes-diag2.md` §158–§160 |
| 2026-08-09 | `58b5ed2` | **27.48%** | **1,508** | **+32 cases, 0 lost, 0 wrong** | **TS1212/TS1213/TS1214 — reserved words as identifiers**, `checkContextualIdentifier`'s future-reserved-word arm. The largest single build on this board, and the fourth row this session whose stated blocker (`alwaysStrict` in the binder) did not exist. Everything it needed was already in the tree. `checker-notes-diag2.md` §161–§162 |
| 2026-08-09 | `7d5c718` | 27.48% | 1,508 | **REFUSED: +2 for +6, and a self-correction** | The meaning-mismatch cascade (TS2661/2693/2708/2709/2749), built in full and reverted: +2 cases against 238 wrong lines, bounded to 6, thirteen cases short of its bar. §164 then attributed the row to `is_value_reference` and **§165 disproved its own attribution by measuring the fix alone** — it loses a case. The rule banked: *a bound that removes wrong lines is not thereby a fix* when both hypotheses predict the same removal. `checker-notes-diag2.md` §163–§165 |
| 2026-08-09 | `2dfe2a1` | 27.48% | 1,508 | **checker_types 3,942 → 3,955** | **`resolve_name`'s `globals` fallback ignored `meaning`** — one line. Every scoped arm filtered; the fallback did not, so every name in `globals` (including all of `lib.*.d.ts`) answered every meaning query. Found by an `eprintln` at a rule's entry after two attributions failed. `diagnostics` correctly does not move: this is a precondition, not a conversion. Flagged for the `.types` workstream as §95 was. `checker-notes-diag2.md` §166 |
| 2026-08-09 | `6ab4ef3` | **27.62%** | **1,516** | **+8 cases, 0 lost, 0 wrong** | **TS2709/TS2749** — §163's refused code, unchanged, re-run on top of §166. It measured 0 converts before and +8 after. The row's lesson: *a rule that measures 0 wrong AND 0 converts is not weak, it is unreached* — and `RIGHT`, not `WRONG`, is what tells the two apart. `checker-notes-diag2.md` §167–§168 |
| 2026-08-09 | `f8b60df` | **27.77%** | **1,524** | **+8 cases, 0 lost, 19 wrong** | **The value-position meaning-mismatch arm.** §164 had measured it at 238 wrong lines against the pre-§166 resolver; re-measured on the fixed one it is 19. The stale number was wrong by an order of magnitude — §142's rule, demonstrated on this board's own figures. Makes §163's case bar, misses its wrong bar by 7, landed with the counter-argument recorded. `checker-notes-diag2.md` §169 |
| 2026-08-09 | `5d8f0ed` | 27.77% | 1,524 | **REFUSED at LOST 1** | **TS7026 + `declare global` merging**, both built and both reverted. The rule is correct; its wrong column is one cause — `/// <reference path="/.lib/react16.d.ts" />` is not loaded, so `JSX.IntrinsicElements` is genuinely absent from a program that is not the one upstream compiled. **Owner: `file_loader`.** `declare global` merging is measured for the first time (removes 20 of 65 wrong lines; `binder_symbols` unmoved, `checker_types` +2, `diagnostics` −1). `checker-notes-diag2.md` §170–§171 |
| 2026-08-08 | *(this session)* | 27.77% | 1,524 | **`declare global` LANDED: `checker_types` +2 cases / +292 lines, `diagnostics` no case changed verdict, `binder_symbols` unmoved** | §171's refused half, rebuilt with a narrower gate and its LOST explained. The gate is `IsGlobalScopeAugmentation && IsModuleAugmentationExternal` plus the collector's ambient test — a `global` block at the top level of a *script*, or inside a module *augmentation*, is TS2669 and merges nothing; and it is the block's **exports** that merge, never its locals. Both corrections are against the previous attempt and both have a test that fails under it. The LOST was `compiler/extendGlobalThis` and it was two *checker* gaps the merge made reachable: `c.globalThisSymbol.Exports` **is** `c.globals` upstream (`checker.go:963`), and a missing member of `globalThis` is never TS2339 (`checker.go:11337`). Also landed **`examples/casequery.rs`**, which named that case in one run, and the `plugins` compiler option (3 spurious TS5023 in one monorepo, `cli_baselines` 33/43 unchanged). Real-repo control: 1,996 → 1,550 errors over 22 packages, with 92 *new* ones attributed to a pre-existing unquoted-ambient-module-name collision and recorded in §5 rather than fixed. `checker-notes-diag2.md` §172, `docs/architecture/binder.md` |
| 2026-08-08 | *(this session)* | 27.77% | 1,524 | **an unset `target` is the LATEST STANDARD: every suite identical, per case** | `STATUS-cli.md` §7.10's finding, fixed in `tsr-core`. **Its "it is two lines" estimate was wrong by a function**: the default target feeds `GetEmitModuleKind`, and *that* was independently wrong — a two-way `>= ES2015` split where upstream is a five-rung ladder (`compileroptions.go:202`). A wrong default was masking a wrong ladder, and fixing only the named line would have shipped `ES2015` where upstream says `ES2022`. §7.10 also said the function is read by the checker, loader and module resolution; in this port it had **no production caller at all**. Nothing measurable moved — five suites identical per case, `cli_baselines` 33/43, the 22-package repo at 1,550 with the same distribution — because the corpus writes `// @target:` in 12,423 of 12,444 cases. The observable that licenses it: a `tsconfig.json` with no `target` using `Object.entries` reported TS2339 before and nothing after, which is what `tsc` does. Six tests were leaning on the old defaults and now name their target. `checker-notes-diag2.md` §174 |
| 2026-08-09 | `dcaa96a` | 27.77% | 1,524 | **the TS2322 split, measured** | `examples/ts2322split.rs` + `Checker::assignability_probe`. Of 2,441 missing TS2322 lines: 691 lack a reporting anchor, 1,737 are relation- or members-gated. Per case: 132 anchor-gated, 335 relation-gated, 13 mixed. Answers `bd tsr-bxp`, filed two sessions earlier with *"Do NOT build an emitter before this runs"*. `checker-notes-diag2.md` §172, §175 |
| 2026-08-09 | `65a578d` | 27.79% | 1,525 | **+1, and it CORRECTS the split above** | `elaborateObjectLiteral`, the anchor §175 ranked first at 19 cases. It enters 563 positions and reports at **6**: 343 decline on the relation and 280 on a missing target property. **§172's NEVER REACHED bucket conflates "no anchor" with "anchor exists but the relation cannot decide"** — at an unvisited position there is no gate to observe — so the 132 is an upper bound, not a slice. `checker-notes-diag2.md` §176–§177 |
| 2026-08-09 | `dbfa89d` | **28.13%** | **1,544** | **+19 cases, 0 lost, 2 wrong** | **TS1028/TS1071/TS1155**, three grammar rows §105 had declared exhausted. Two land inside `check_modifier_order`, which already had the walk and the `seen` set. First measurement was +19 for **22** wrong; gating on `file_has_parse_errors` took it to 2 at zero cost — the gate's fourth per-rule measurement on this board. `checker-notes-diag2.md` §178–§179 |
| 2026-08-08 | `45436ae` | *(checker_types)* **84.70%** | *lines 405,698* | **§93 LANDED at +16/4 (4:1) — and its commit message's "+311, 8.6:1" is WRONG, corrected in-commit** | The stale-baseline trap's **fifth firing**, first time on a LANDED score: §93 (any-context call arguments type their arrow standalone) was scored at +300 G→R against a baseline stale by several landings; the stash/clean-run counterfactual (checker-2) read 405,682 on clean `4965add` with jsxChildren 42 / reactDefaultProps 27 / arity 17 already present — pure drift. True §93: +16 G→R (fatarrowfunctions 11, fatarrowfunctionsOptionalArgs 5) / 4 G→W, zero R→W. The corrected score paragraph rides inside `45436ae` itself (checker-2's in-tree correction, swept in by checker-1's `git add -A` — both sides tightened: explicit-path adds, worktree isolation, announced measurement windows). builtinIterator 6 / intraExpressionJsx 5 struck from §93's residues — they predate it. `checker-notes-narrow.md` §93. Two-session parallel protocol now in force: checker-1 (alias-instantiation/printing lanes, isolated worktree), checker-2 (contextual dispatch arc, main checkout) |
| 2026-08-09 | `6f537ce` | 28.28% | 1,552 | **+8 cases, 0 lost, 0 wrong** | **TS1015/TS1117/TS1221.** First measurement was +7 for 3 wrong: upstream's `checkGrammarParameterList` returns on the FIRST offending parameter, so one list is one diagnostic. §103's rule refined — **the node is whatever the upstream function ITERATES**, not the node the walk visits. Moving the test to the list took wrong to 0 *and* gained a case. `checker-notes-diag2.md` §180 |
| 2026-08-09 | `b427d42` | **28.53%** | **1,566** | **+14 cases, 0 lost, 4 wrong** | **TS2364/TS2703/TS2371.** First measurement was +14 for **184** wrong and 6 lost: `checkBinaryLikeExpression` short-circuits to `checkDestructuringAssignment` before `checkReferenceExpression` runs, so `[a,b] = x` never reaches it. Then six off-by-one columns — `checkReferenceExpression` reports **before** skipping and `checkDeleteExpression` **after**. Two functions one paragraph apart with opposite conventions. `checker-notes-diag2.md` §181–§182 |
| 2026-08-08 | `c9b7f93` | *(checker_types)* **84.73%** | *lines 405,805* | **§94 LANDED: +93 G→R / +14 W→R / 1 G→W — 107:1, zero R→W** | **Statement position shows contextual absence.** `has_no_contextual_type` knew ONE shape (unannotated var initializer); every arrow in `(arg) => 2;` statement position gapped on unshowable absence. The faithful rule is the nil-ladder of `getContextualType`'s dispatch (`checker.go:29343`): no arm for expression statements; parens/ternary-branches/`&&`-and-comma-rights climb; ternary conditions and non-listed operators (`+`, `instanceof`) answer nil outright; `||`/`??` rights are typed by their left, never showable. Bar committed before code (`67a472e`), predicted +60–110 in the head case, measured 81. First cut (statement-only) converted just 21 — the population sat behind the ternary/binary arms, which is why the whole ladder landed. The +14 W→R are parserArrowFunctionExpression11/16/17, binary-operand arrows previously confidently mistyped. One adverse: the strict-optional `| undefined` print residue, priced twice now (§93 0:457 + parserParameterList11's error-recovery optional rest) — the seam's next candidate. §93's own residue diagnosis ("widen the callee road") was corrected in the §94 section: the gaps were statement-position, not call-argument. `checker-notes-narrow.md` §94. checker-2 session |
| 2026-08-08 | `8f7be7d` | *(checker_types)* **84.76%** | *lines 405,957* | **§96 LANDED at +6/0 — the bar's magnitude missed and the miss is recorded** | **The initializer branch adds optionality.** One line: `get_type_for_variable_like_declaration`'s initializer return wraps in `add_optionality_for_declaration`, upstream's own site (`checker.go:16750`) — `(b? = 0)` is `number | undefined`. Closes §93/§94's twice-priced 0:457 residue; optionalMethods 3, isolatedDeclarationsAddUndefined 1. Zero adverse; the must-not-move leg (no defaulted-only parameter gains `| undefined`) held. Bar predicted +12–25, measured **6**: the exact-insertion sizing probe bucketed by the answer's SPELLING, not the mechanism — the other candidate lines belong to initialized-before-required arity printing (~8) and optional-literal widening (~6), owners now named in the section. `checker-notes-narrow.md` §96. checker-2 session. Between my rows, checker-1 landed §95 (+146, site-aware reference re-render, temporal 112) and §99 (+55, multi-signature composites at the site) with §97 held on its branch pending my §98 — the four-way sequencing is in the sections |
| 2026-08-08 | `bd31a96` | *(checker_types)* **84.82%** | *lines 406,241* | **§98 LANDED at 245:1: +93 W→R / ~152 G→R / 1 R→W** | **Retention's roots widen.** The §56 walk gains three machines, each upstream-anchored: the ASSIGNMENT root (`c.x = { a: "a" }` — the equals arm, `checker.go:29843`), the DISTRIBUTING member step (unions map the lookup over constituents, intersections collect and intersect — `getTypeOfPropertyOfContextualTypeEx`, `checker.go:30555`), and root DISCRIMINATION as upstream's ternary algorithm (`relater.go:1212` — constituents lacking the member SURVIVE; non-matching members eliminate only when something matched). Heads: excessPropertyCheckWithUnions 37, destructuringParameterDeclaration8 12, missingDiscriminants 9, staticFieldWithInterfaceContext 6. Three fired legs in the section — including TWO boolean over-corrections built, measured at R→W 23 and 25, and REVERTED: bare `boolean` retains (`{ hoge: true }`); the `autoIncrement : boolean` widening that motivated them was the discriminated constituent LACKING the member plus intersection-blind discriminator lookup. Residue: one line (attributes2, primitive-constituent union root), priced. `checker-notes-narrow.md` §98. checker-2 session |
| 2026-08-09 | `b89b7c4` | *(checker_types)* **85.23%** | *lines 408,196* | **§109 LANDED at +26/0 — THE READONLY SUBSYSTEM CLOSES** | **The value-spelling carriage.** A single-quoted member value prints single-quoted inside the object type (the §77.3 name-quote precedent applied to values); the §105 gate lifts for exactly the carried shape, and the impossible-not-detectable property held end to end (zero adverse in any column; es2018IntlAPIs — the original blocker note's locale family — among the converts). The subsystem sized as "four machines, do not attempt piecemeal" landed as SIX measured slices; the two that looked hardest each turned out to be ONE ARM once the machinery around them existed. Only §103's inference plumbing remains, entered at §33's decline (calls.rs:481). Second same-day test-behind-its-feature red, both caught by the tee-log grep. `checker-notes-narrow.md` §109. checker-2 session |
| 2026-08-09 | `4933b2b7` | *(checker_types)* — | — | **callres2 REUNION (slices 2+3) REFUSED ×3: 361:424 → 9:73; the triangulation is the spec** | Three measured iterations proved DEFERRAL-AS-EXCLUSION wrong: upstream's SkipContextSensitive is a PRIORITY mechanism — `inferTypeArguments` still collects from a context-sensitive argument's non-contextual parts. The first fire's +361 (temporal 58, parenthesized 59) is the registered falsifier the correct build must reproduce; resumption requires the `inference.go` priority-machinery read FIRST. All three build shapes preserved. The summit's dragon is mapped to the line. checker-2 session |
| 2026-08-09 | `18a318b1` | *(checker_types)* **~85.8%** | *+494 right / −193 wrong / −301 gap NET* | **THE SUMMIT LANDS: the callres2 unit + the materialization conjunct — every steering aggregate improves** | Fifteen-plus trace rounds, three eliminated specifications, two custody rules, and one landing: the two-phase argument order, the constraint-fallback memo, the reentrancy guard, the tsr-0hc TYPE-FIRST READ (the freeze-breaking wire, found in the port's own documented hazard), and arms (a)+(b) gated on CONTEXT MATERIALIZATION (the conjunct that collapsed the standalone-any class 251→193). +272 G→R +396 W→R vs 193/49/15; the registered bar's reading AMENDED on the net-wrong argument (it counted new wrongs without crediting 396 removals — the total split's improvement is the split's own purpose). The ladder test pins the middle rung; the residue is priced per family for the pipeline. inferTypePredicates 29, temporal 60, contextSensitiveReturn 22, overEager 18. checker-2 session |
| 2026-08-09 | `12ee7229` | *(checker_types)* **85.43%** | *lines 409,173* | **callres2 slice 1 LANDED at ~9:1: +674 G→R / +43 W→R / 74 G→W / 4 R→W (+723 net — the largest build since §108)** | **`hasCorrectArity` promoted ahead of the generic decline** (`checker.go:9107`, upstream's own first pass): a single arity-survivor returns as a born-single candidate does; generic survivors flow to check_generic_call. The generic-overload resolution subsystem's first slice, from the callres2 study opened hours earlier with anchored entries and measured ceilings. The 4 R→W are the §21 `never` family, REAL and priced, with a measured-and-rejected refinement recorded beside them (it fixed nothing and cost 25 — the loop's undecidable tail is a slice-3 question). strictBindCallApply1 32, jsDeclarationsGetterSetter 27, underscoreTest1 26, wide spread. checker-2 session |
| 2026-08-09 | `44156979` | *(checker_types)* **85.29%** | *lines 408,450* | **§115 LANDED at +98/0** | **The dispatch's ConditionalExpression arm.** A ternary branch answers the conditional's own context, the condition answers nil (`checker.go:30022`) — the §94 nil-ladder's positive twin, proven ABSENT by §114's round-8 grep. conditionalOperatorWithout/WithIdenticalBCT 52, contextualTypingOfConditionalExpression 18, wide tail; the §114 artifact-position risk did not materialize (the callee arm's guards decline first). One arm, ten minutes, +98 — the nine-round archive is what made it ten minutes. Re-isolated identically over §111 slice 2's +36 (clean composition). checker-2 session |
| 2026-08-09 | `e6a7b2a7` | *(checker_types)* **85.26%** | *lines 408,346* | **§114 family 1 LANDED — CORRECTED to ~+22 true: the claimed +38 included 16 GAIN-side aligner artifacts (two-chain bisect, both lanes agreeing; no regression existed; §108.1's +22 stands whole)** | **Arity selection over disagreeing overloads.** When §70's agreement declines on disagreement, the single candidate whose parameter count equals the call's argument count decides (upstream's first resolution discriminator), same guards. The nine-round trace record in checker-notes-ctx §114 is the project's most complete: two arm shapes measured and reverted, an instrument that printed only one side self-caught, a stale doc inverted, and the adverse run to ground as the §87 ALIGNER-ARTIFACT class with its test now written down (a fixed index whose want text changes between runs is the aligner moving, not the answer). Corpus spillover: callWithMissingVoid 8, typeGuardTypeOfUndefined 8. checker-2 session |
| 2026-08-09 | `a409d56` | *(checker_types)* **85.13%** | *lines 407,727* | **§105 trace 1 RESOLVED: +73 W→R / 6 W→G / zero adverse** | **`is_const_context` learns the member shape.** Asked from the property-assignment NODE, the climb saw parent = ObjectLiteralExpression and no arm covered it — upstream never sees that shape because it always asks from the initializer one level down. The instrumented arm named it in one run ("reached, const_context=false" ×4). One arm added: ts-expect-error whole at 72/72, constAssertions +22, inferFromNestedSameShapeTuple +8. Trace 2 (computedPropertiesNarrowed 4) remains the open next-window entry. `checker-notes-narrow.md` §105. checker-2 session |
| 2026-08-09 | `e78be0d` | *(checker_types)* **85.11%** | *lines 407,654* | **§105 slice 2a LANDED at ~9.6:1: +64 G→R / +90 W→R / 16 G→W / 12 W→G, zero R→W** | **Const-context objects and arrays.** `isConstContext` (`checker.go:13615`) ported into `check_object_literal` AND `check_array_literal`: readonly regular members behind the single-quote value gate (zero wrong-quote lines — the gate is what makes them impossible), spread members inherit readonly, const-context methods print as readonly arrow properties, and the literal's OWN line answers — slice 1's assertion-side mint was the wrong placement, corrected here. Bar +40–120, measured 154 (favorable-direction miss, same destructuring-reach cause as slice 1). One unit test went red because it PINNED THE PRE-SLICE GAP — the failure was the feature arriving; rewritten to the new truth. The rtk-masked gate hid that failure and the tee-log grep caught it — the memory rule's third firing. TWO OPEN TRACES are mandatory next-window entries (ts-expect-error 8 member lines answering boolean; computedPropertiesNarrowed 4). `checker-notes-narrow.md` §105. checker-2 session |
| 2026-08-09 | `8f8eb1f` | *(checker_types)* **84.98%** | *lines 407,148* | **§105 slice 1 LANDED at ~10:1: +112 G→R / +6 W→R / 12 G→W, zero R→W** | **as-const arrays mint readonly tuples.** The tuple machinery already carried the readonly flag (`create_tuple_type`, keyed intern) — the slice was WIRING the as-const array gate into a mint: elements to regular literal types, nested arrays recursing through the assertion arm itself, spreads/holes/objects declining whole. Bar predicted +25–60, measured 118 — a 2× miss in the FAVORABLE direction (destructuring/spread contexts the sizing did not count), recorded per the prediction rules. The 12 G→W are two NAMED next-rule populations (§31-chain precedent): 6 shadow-rename wants (`readonly [T, T_1]`) owned by §102's arm — flagged to checker-1 — and 6 readonly-member wants owned by slice 2, whose trace entry (constAssertions 0:115's holder chain) is recorded. `checker-notes-narrow.md` §105. checker-2 session |
| 2026-08-09 | `230e585` | *(checker_types)* **84.92%** | *lines 406,724* | **§104 slice 0 LANDED at 39:1: +232 G→R / +1 W→R / 6 G→W, zero R→W** | **`as const` reaches its ported arm.** The mandatory §104 trace found a TWO-CONTRACT SEAM: the parser deliberately encodes `as const` as a None-named `TypeReferenceNode` (`types.rs:471`) while `is_const_type_reference` demanded the identifier spelling — every const assertion in the corpus gapped on the mismatch, exactly as assertions.rs's own module doc recorded (blamed on `bd tsr-0ao`; the encoding was later made deliberate and the test never updated). One arm fixed it; objects AND arrays stay gated (readonly minting is the subsystem's next slice), parens climbed after a 2-G→W fired leg. Three `#[ignore]`d tests now run — one expectation stale twice over, one arity claim recorded as a parse-level divergence. constAssertions 72, controlFlowBindingElement 24, indexSignatures1 24. `checker-notes-narrow.md` §104. checker-2 session |
| 2026-08-09 | `a370cfa` | *(checker_types)* **84.87%** | *lines 406,491* | **§101 LANDED at +152/0** | **The template fold consults the constant evaluator.** A span typed `number` can still have a constant VALUE — upstream hands the whole template to `c.evaluate` (`checker.go:7991`), so `1 - 3-4`-style spans fold to `"-1"`. The arm is the symbol-free evaluator slice (literals, parens, prefix sign, numeric arithmetic, `+` concatenation, templates recursively; identifiers and property accesses DECLINE, so it only ever adds folds); number spelling via `tsr_core::jsnum::format_number`. templateStringBinaryOperations family whole, zero adverse. The commit message rounds to 84.85%; the measured figure is 406,491/478,954 = **84.87%**, corrected here — the message is immutable. `checker-notes-narrow.md` §101. checker-2 session |
| 2026-08-09 | `139d1f6` | **28.68%** | **1,574** | **+8 cases, 0 lost, 0 wrong** | **`checkGrammarModifiers` taken WHOLE** — TS1030/1031/1038/1044/1090/1243 on top of the TS1028/TS1029 slices. §179 had left TS1038 because it is the sixth arm of its chain; the answer was the other five arms, not a bound. The counterfactual read 25 and the *delta* is 8 — **a counterfactual over a code set is a delta only when the port emits none of the set**. `checker-notes-diag2.md` §183–§184 |
| 2026-08-09 | `—` | 28.68% | 1,574 | **REFUSED: +5 for 14** | **TS2694**, the best-shaped row on the relation-free board (15 cases, concentration 1.2). 127 of its 141 wrong lines are one line of upstream — `getExportsOfSymbol(resolveAlias(namespace))`. **An empty symbol table can be declined; a partial one cannot** — §9's TS2339 refusal reappearing in the symbol tables. Second measured consumer of alias resolution. `checker-notes-diag2.md` §185–§186 |
| 2026-08-09 | `7547157` | 28.79% | 1,580 | **+6 cases, 0 lost, 4 wrong** | **TS2694/TS2724.** §186 had refused this and blamed unported alias resolution. `Checker::resolve_alias` was in `symbols.rs` throughout — alias resolution is unported in the **binder** and ported in the **checker**, and §186 read one as the other. Calling it took wrong 14 → 7; the TS2724 arm §185 named as its own falsifier took it to 4. `checker-notes-diag2.md` §187 |
| 2026-08-09 | `—` | **29.05%** | **1,594** | **+14 cases, 0 lost, 9 wrong** | **TS7026**, §171's rule restored unchanged and re-measured: `14 · 1 LOST · 65 wrong` became `14 · 0 · 9`. What changed was §166 and the `.types` workstream's `declare global` merge — **not `file_loader`, which followed the reference the whole time** (§188). `checker-notes-diag2.md` §188–§189 |
| 2026-08-09 | `571da58` | **29.30%** | **1,608** | **+14 cases, every rail unmoved** | **One `if` in the parser.** `parseErrorAtRange`'s guard (`parser.go:327`) — *don't report another error at the same location as the last* — which every upstream parser diagnostic routes through and this port lacked. `extraonly` had 41 TS1005 and 18 TS1012 lines that were all second diagnostics at an occupied position. **A divergence can live in a sink every producer feeds, and is invisible from any producer.** `checker-notes-diag2.md` §192–§193 |
| 2026-08-09 | `—` | 29.30% | 1,608 | **checker_types +6, diagnostics neutral** | **The `IsLeftHandSideExpression` conjunct** (`parser.go:4143`), refused at §191 on a `−6` that was entirely the TS1012 §193 removed. Fourth reversal of the session, and the one that extends §189's rule: a refusal must state what the measurement was taken **through**. `checker-notes-diag2.md` §194 |
| 2026-08-09 | `23ca0e9` | **29.35%** | **1,611** | **+3, every rail unmoved** | §193's guard spans the **scanner's** list too. Upstream has one diagnostic list; this port's scanner owns a `Vec` merged after parsing, so the guard could not see across. Third and last layer of the *"parse-error set is incomplete"* note four sessions carried. `checker-notes-diag2.md` §195–§196 |
| 2026-08-09 | `b00444d` | **29.39%** | **1,613** | **+2, every rail unmoved** | **A `.js` file is not a program input without `allowJs`.** `reported_for`'s parser/binder half walked every unit; its check half already agreed with the program. `extendsUntypedModule`'s `node_modules` units contain the prose *"This file is not read."* and this port parsed them. **Third divergence of the session living between two halves that build the world separately.** `checker-notes-diag2.md` §197 |
| 2026-08-09 | `1608d44` | **29.41%** | **1,614** | **+1, every rail unmoved** | **Parameter-list recovery, guarded by a SUBSET.** §198 ported `parseDelimitedList`'s continue with no guard and lost 64 parser files; §200 admits only what unambiguously starts a parameter and lets everything else keep the old `break`. *When a faithful port is too expensive, the question is not "port less" but "which direction does porting less fail in".* `checker-notes-diag2.md` §198–§201 |
| 2026-08-08 | *(this session)* | 27.77% | 1,524 | **ambient module symbols carry their quotes: real repo 1,550 → 1,440, `binder_symbols` still 100%** | §173 recorded this collision with a number and left it; fixed here. `getDeclarationName` (`binder.go:311`) names `declare module "fs"` as `"fs"`, quotes included, and `IsAmbientModuleSymbolName` is literally a quote test — they are what keep `"process"` and `process` apart in one table. The binder change is one function; **what it exposed is that five call sites had open-coded `tryFindAmbientModule`'s lookup**, all five went silently dead, and only one was findable by grep. Two of them were not equivalent to each other. **`binder_symbols` fell to 8,350/8,459 on a strictly more faithful change** because the suite normalised upstream's names and not ours — fixed by normalising both, per dotted suffix. Also ported two predicates that were being re-derived instead of read: `tspath.IsExternalModuleNameRelative` (already in `tsr-path`, and stricter than the hand-rolled prefix test) and `stringutil.StripQuotes` (new `tsr_core::stringutil`, now shared with the harness's own copy, which had forgotten the backtick). `checker_types` +21 lines, `diagnostics` no case changed verdict. `checker-notes-diag2.md` §202 |
| 2026-08-09 | *(this session)* | 29.43% | 1,615 | **TS7026's 1,642 false positives removed; every suite byte-identical** | §189 landed TS7026 at +14 cases. It ported `getJsxNamespaceAt`'s **third** road and neither of the first two — on a rustdoc claim that upstream falls back to the global `JSX` when there is no `@jsx` pragma. Road 2's name is a single choice (`jsx.go:1341`) and it is **`React`** by default; `@types/react` 19 has no global `JSX` at all. So every JSX element in a modern React build drew TS7026 — **1,642 across 22 packages against `tsc`'s zero**. Built roads 2 and 3 in upstream's order with the three alias hops real code needs (UMD global, `import * as`, default import) and the `export = React` follow. **Merged with §211**, which built the pragma half concurrently and **moved this repository by zero** — no real project writes a pragma. `checker_types` 3,982/406,491 and `diagnostics` 1,615 byte-identical before and after on four bases. The two measurements are each other's blind spot. `checker-notes-diag2.md` §221 |
| 2026-08-09 | *(this session)* | 34.58% | 1,898 | **`symbolIsValue`'s alias half: repo 1,488 → 104, `diagnostics` no case changed verdict** | `symbolIsValueEx` (`checker.go:22095`) has two disjuncts and this port had one. An alias's own flags carry no `VALUE` bit, so every module export written as a **specifier** answered "no property" and `nonexistent_property` reported TS2339 — **1,384 on a 22-package repository against `tsc`'s zero**, one per member access on a barrel module; `packages/ui` alone ~300 → 15. The decline's recorded reason (*"nothing follows aliases yet"*) had expired sessions earlier. Landing the alias half alone LOST 2 (`exportNamespace3`, `importEquals2`) — upstream's `excludeTypeOnlyMeanings` guard, named by `casequery` in one run and answered by §121's existing transitive walk rather than the syntax-only copy written first. `checker_types` +1 case / +45 lines; four tests, each red under the arm it exists for. `checker-notes-diag2.md` §400 |
| 2026-08-09 | *(this session)* | 36.33% | 1,994 | **heritage positions: repo 104 → 84, every suite unmoved** | An interface's `extends` is a **type** position (`isIdentifierInNonEmittingHeritageClause`, `ast/utilities.go:3132`) and this port keyed the distinction on the `extends` keyword, which an interface also uses — **19 false TS1361 and 1 false TS2686** on a 22-package repository, all on `interface P extends VariantProps<…>` over an `import type`. **Answering it once in `is_value_reference` was the wrong fix**: it silenced three rules and lost `compiler/protoAssignment`, because upstream *does* report TS2304 there through `resolveEntityName` at type meaning. Three rules, three different upstream gates. Also `core.Every` was read as `any` in TS2686's declaration test, which made every reference to `React` inside `@types/react` an error. `diagnostics` no case changed verdict, `checker_types` identical. Residual recorded: 3 TS1361 behind `bullmq`'s barrel, a different mechanism. `checker-notes-diag2.md` §502 |
| 2026-08-09 | *(this session)* | 36.86% | 2,023 | **TS1192's synthetic default: repo 162 → 84, both snapshots byte-identical** | The report is the **third** of three conjuncts (`checker.go:14566`) and only the first was ported. `canHaveSyntheticDefault` (`:14818`) is what makes `import React from "react"` legal against `export = React` — how every `@types` package ships and every consumer writes it — so TS1192 fired **78 times on a 22-package repository**, 12 in `packages/ui` alone (17 → 5). Both arms ported: the permissive declaration-file one (`:14850`) and `hasExportAssignmentSymbol` (`:14869`). `IsDeclarationFile` becomes a `ModuleHost` question (ADR-0016: no file name on the AST), defaulting to `false` so an ignorant host reports *more*, not less. The predicate itself already existed as `tsr_binder::is_declaration_file` and was made `pub` rather than rewritten — third time this session reading the tree beat writing the four-line version. Three unported suppressors named with their direction. `checker-notes-diag2.md` §531 |
| 2026-08-09 | *(this session)* | 37.43% | 2,054 | **an optional parameter's type includes `undefined`: repo 84 → 62, both snapshots byte-identical** | `getTypeOfParameter` (`checker.go:17042`) adds optionality for a `?` **or** an initializer, so `b?: string` and `b: string = "d"` are both `string \| undefined`. The argument check took the written annotation alone, so every `T \| undefined` argument at such a position drew TS2345 — **23 → 1** on a 22-package repository. Both arms had it (`call` and `new`, one upstream function apart). **Not one line of either snapshot moved**, which is the number that matters for a change that widens a type: the risk was a silenced case and there was none — the corpus's TS2345 fixtures use required parameters. The survivor is optional-chain narrowing, already recorded as unbuilt. `checker-notes-diag2.md` §541 |
| 2026-08-09 | *(this session)* | 37.76% | 2,072 | **TS1016's missing conjunct, and every rule this session paired with a true positive** | `checkGrammarParameterList` (`grammarchecks.go:714`) is `seenOptionalParameter && parameter.Initializer == nil`; the second conjunct was absent, so a **defaulted** parameter after an optional one was an error. Not the same test as the arm above it — §288 established that `seenOptionalParameter` is a `?` alone, and the initialiser exclusion is a separate arm. Repo 62 → 51, both snapshots byte-identical. **Then the audit**: each of the eight rules touched this session was deleted outright to find which test catches it, and four true positives were missing and are now written. **One could not be** — `globalThis.blockScoped` over a script `let` must report TS2339 and does not, and measurement shows §173's guard is *not* the cause (§33's minted type has no members, so completeness declines first), making that branch unreachable today; kept as upstream's rule and pinned by a divergence test. `checker-notes-diag2.md` §554 |
| 2026-08-09 | `9f4698e` | **29.43%** | **1,615** | **+1, every rail unmoved** | **`parseObjectBindingElement` branches on `isBindingIdentifier`**, read *before* the property name — `{ while }` is one `':' expected` upstream and was four errors here. It costs one condition only because **§193's same-position guard had already landed** for a different row: the third time this session a general fix changed what a later build costs. `checker-notes-diag2.md` §204–§205 |
| 2026-08-09 | `00d7d76` | 29.43% | 1,615 | **+0, and landed on purpose** | **A namespace in a `.d.ts` exports what it declares.** `bind_container`'s ambient test had three disjuncts and not `in_declaration_file`. Filed as *"imported namespace symbols carry no exports"* — imports had nothing to do with it. Landed at +0 because it is **observable and pinned** (a test red without the disjunct) where §207's +0 was unobservable; three rows queue behind it. `checker-notes-diag2.md` §208–§210 |
| 2026-08-09 | `7373eff` | 29.43% | 1,615 | **TS7026 wrong 9 → 3, `checker_types` +8** | **The `@jsx` pragma path, rebuilt on §208's table.** §207 built it across three crates, measured +0, and reverted; §208 fixed the table its second hop reads and landed at +0 **because it was observable and pinned**. Had §208 been reverted for scoring zero this rebuild would be unreachable. *Unmeasured and unobservable are different.* `checker-notes-diag2.md` §207–§211 |
| 2026-08-09 | `4e96921` | **29.45%** | **1,616** | **+1, every rail unmoved** | **A module specifier is an arbitrary expression upstream.** `parseModuleSpecifier` says so in its own comment; this port reported TS1141 at the right position and then returned a *missing* identifier without consuming, so the `)` was reported missing too. *"Read the upstream function" is not a uniform cost* — §215's `<a:` encodes its design in scanner state and was left; this one carries it in a comment. `checker-notes-diag2.md` §215–§217 |
| 2026-08-09 | `9f9a588` | **29.46%** | **1,617** | **+1 diagnostics, +6 `checker_types`** | **Object-literal member recovery**, with upstream's own `;` skip. Third instance of one shape: every hand-written list loop in this parser `break`s where upstream's single `parseDelimitedList` continues. Also: **`extraonly` moving the wrong way is not evidence of harm** — it classifies still-failing cases, so removing a *missing* line from a doubly-blocked case increases it. `checker-notes-diag2.md` §218 |
| 2026-08-09 | `aa475ac1` | **29.50%** | **1,619** | **+2 diagnostics, +2 `checker_types`** | **Array-literal recovery, and the list-loop sweep.** Ten loops share one `break`; §200/§204/§218 fixed three and this is the fourth. **Seven measured as having no case asking for them** — a result, not a deferral. Also: `extragap` shows ~1,150 invented parser lines against `extraonly`'s 27 cases — **`extraonly` is the narrowest projection of the extra column**, and those 1,150 are *not* in the list loops. `checker-notes-diag2.md` §221 |
| 2026-08-09 | `d74c9c35` | 29.50% | 1,619 | **−51 invented parser lines, `printer_round_trip` +10 files** | **`scan_unicode_escape` reports its own errors.** It returned `None` for four distinct upstream errors and let three callers each say *Hexadecimal digit expected* at whatever position they held — behind a comment reading *"close enough until the parser distinguishes the two"*. **A comment that says "close enough" is a bar with no number**, and this one cost 51 lines and two wrong codes. `checker-notes-diag2.md` §220, §222 |
| 2026-08-09 | `673319e8` | 29.50% | 1,619 | **invented lines −28, tagged-template semantics** | **A tagged template may contain an invalid escape** (ES2018). Upstream's initial template scan is *silent* and every such diagnostic comes from `ReScanTemplateToken(!isTagged)`; this port reported eagerly and never re-scanned. **A corpus aggregate can improve while one case gets worse in a new direction** — the first attempt missed one of four entry points and flipped the case from 32 extra to 26 missing, caught only by reading `diagcase` alongside the total. `checker-notes-diag2.md` §223 |
| 2026-08-09 | `f222980b` | **29.61%** | **1,625** | **+6 diagnostics, +2 `checker_types`** | **TS1121, legacy octal literals** — a code with **no producer at all**, 5 sole-obstacle cases, and it beat its `+3` bar. *A row with no producer is the one place a bar should be set high*: the missing lines are exactly what the rule would produce, so `diagmissing`'s count is close to the truth. §162 (+32 against 34) is the other instance and both undershot. `checker-notes-diag2.md` §224–§225 |
| 2026-08-09 | HEAD | **29.76%** | **1,633** | **+8, the entire ceiling** | **TS2524** — the second **no-producer** row to hit its ceiling, which upgrades §225 from observation to prediction: for a code this port never emits, `diagmissing`'s sole-obstacle count *is* the estimate. But **all eight cases report**, so a rule that never stopped climbing would also have scored +8 — the boundary is pinned in a mutation-checked unit test instead. *A row that hits its ceiling is exactly when to ask what the ceiling did not measure.* §226–§227 |
| 2026-08-09 | HEAD | **29.96%** | **1,644** | **+11, `WRONG` +4 against a bar of +2** | **TS2305** — a six-way fallback shipped by evaluating its four siblings' *conditions* and declining their *messages*. *A fallback is portable exactly when its siblings' conditions are computable, even if the siblings' messages are not.* **The bar was exceeded on the wrong side and is recorded as a breach, not widened.** Three of the four wrong lines are one shape (a `declare module` augmenting a resolved relative specifier); **two guards were written and measured against it and both changed nothing** — reverted, with the negatives on the record. §228–§229 |
| 2026-08-09 | HEAD | **30.05%** | **1,649** | **+5; a contaminated first measurement caught by clippy** | **TS2440/TS2441.** §140's trap fired: the new dispatch arm shadowed an existing `ImportEqualsDeclaration` arm, silently deleting `check_illegal_decorator`, and the **first coverage run was taken with that rule switched off** — it would have reported +3 with a regression hidden inside it. *§140 is not a lesson about `match`; it is a lesson about running clippy BEFORE believing a coverage number.* Build order corrected: a dispatch change gets clippy first. Also *ask whether the corpus contains the negative before deciding a unit test is redundant* — the opposite answer to §227, one build later. §230–§231 |
| 2026-08-09 | HEAD | **30.07%** | **1,650** | **+1, and §231's cross-build claim retracted** | **§231 said three consecutive builds' wrong lines were all merge divergence and called the merge layer the largest named owner in the wrong column. Built on a line nobody had read.** The baseline says column 13; this port said column 8 — right code, right line, wrong column, `getErrorSpanForNode` rather than merging. TS2440's wrong column is now **empty**. *A cross-build pattern needs every member verified, not the newest one assumed into the shape the older ones made.* Corrected claim: the merge table owns **two** wrong lines, not five. §232 |
| 2026-08-09 | HEAD | **30.12%** | **1,653** | **+3; new instrument `diagcolumn`; a −22 attempt reverted** | **A wrong column appears in BOTH the missing and extra columns at once, so no existing instrument could name it.** `diagcolumn` pairs them: 107 lines, **9 cases blocked by column alone** — needing no rule ported. The first attempt cost 22 cases: `GetErrorRangeForNode` switches on a **closed** list (and comments that it does), while `GetNameOfDeclaration` is a *different* function with an open tail. *Two functions both named for "the name of a declaration", one closed and one open.* Fix belongs in the binder's `name_node_of`; a checker-side twin measured **exactly zero** and was reverted. §233–§234 |
| 2026-08-09 | HEAD | 30.12% | 1,653 | **no code change — a claim retracted** | **§234 called TS1005's 53 column-only lines "the next head". Running the fixtures through this port's parser shows a different token, a different message and a different recovery** — `'export' expected` at col 1 upstream vs `';' expected` at col 9 here. `diagcolumn` could not see it because **the suite compares the tuple and not the message text, and the instrument is built on the same tuple**: *an instrument built on the suite's key can only ever be as discriminating as the suite.* Now splits by producer — parser 68 (assume recovery divergence), checker 17. **TS1005's 53 lines are not cheap.** *A measurement licenses a count, never a cause.* §235 |
| 2026-08-09 | HEAD | 30.12% | 1,653 | **built, mutated twice, reverted** | **TS2303.** The rule is unreachable — all ten cases are entity-name `import A = B`, a form `resolve_alias` does not dispatch. §236 also claimed the absent cycle guard was a latent hang; **two mutations disproved it**, since a pure re-export cycle fails to find the symbol at the first hop. *"There is no guard" and "a cycle can happen" are two claims, and only the first was checked.* Reverted for §234's reason with more force: **unreachable infrastructure reads as tested-and-working to everyone who finds it later.** §236–§237 |
| 2026-08-09 | HEAD | **30.14%** | **1,654** | **second whole build reverted after measurement** | **§237 named entity-name `import A = B` as TS2303's owner; §238 built it; §239 measured `+0` diagnostics and `−40` `checker_types`.** TS2303 never fires even with the guard — the alias resolves in one hop and never re-enters. *A named owner is still a hypothesis.* The 40 lines give `nameres` §14's older refusal the number it lacked. Real owner is a refactor: cycle detection sits in the per-walker `seen` sets, which **absorb** the cycle upstream reports. §238–§239 |
| 2026-08-09 | HEAD | **30.28%** | **1,662** | **+8, the whole ceiling — a channel, not a rule** | **TS6053 was never refused; it was *unroutable*.** The loader's own comment: *"a diagnostic upstream and is dropped here, as every other loader diagnostic is."* Four codes shared one missing field. Built `LoaderDiagnostic` → `Program::loader_diagnostics` (upstream's `fileProcessingDiagnostics`); `FileReference.span` had carried the doc *"so a diagnostic can point at it"* for a consumer that did not exist. ***"Not ported" and "has nowhere to go" look identical from the gap, and only one is fixed by writing the rule.*** Third no-producer row to hit its ceiling. §240–§241 |
| 2026-08-09 | HEAD | **30.50%** | **1,674** | **+12, `RIGHT 90 · WRONG 0` — the best ratio measured** | **TS2628/2629/2630/2631/2632/2539**, a six-way switch on symbol flags alone. *A rule gated on symbol flags has nothing to be approximately right about.* **The first measurement said 1,072** — a 590-case collapse that read exactly like the falsifier firing, and was §140 for the **third** time. §231's rule said clippy *before* coverage; both ran in one invocation and the coverage number was read first. ***A gate that runs before the measurement is not a gate; a gate that BLOCKS the measurement is.*** Also §242: the three loader arms §241 promised are worth **zero**, measured before building. §242–§244 |
| 2026-08-09 | HEAD | 30.50% | 1,674 | **third whole build reverted after measurement** | **TS2323** — bounded three times (no bound `WRONG 47`, skip `default` `WRONG 24`, skip binder-reported symbols **no change**), net negative throughout. The mechanism was read off two baselines: **upstream's `declareSymbol` gives a conflicting declaration a fresh symbol; this port's binder merges it.** ***A refusal that names a divergence in shared infrastructure is worth more than the eight cases that found it*** — the session's three prior refusals each named a *rule's* blocker; this names a **data-model** difference every future counting rule will meet. §245–§246 |
| 2026-08-09 | HEAD | 30.50% | 1,674 | **fourth build reverted; one dead call removed** | **TS2591's name table ported at both call sites, measured zero at each.** Parser hypothesis disproved by direct probe. Reading the function to find out why turned up `report_meaning_mismatch_in_value_position` called **twice in a row**, the second unreachable — a §169 rebase artefact, removed. ***Three builds this session found their real defect while reading code for a different reason*** (§232, §244, §248): the gap tells you which function to open; it does not tell you what you will find there. §247–§248 |
| 2026-08-09 | HEAD | **30.72%** | **1,686** | **+12 — §248's refusal reversed** | **The guard §248 could not name was `is_specially_diagnosed_name`, 3,900 lines away in the same file**, declining fourteen names *because the port had no message for them* — the exact names §247 had just built messages for. ***A rule and its decline are one change.*** Narrowed to `arguments`/`globalThis` (synthesised upstream, genuinely unresolvable here). The 27 wrong lines were all `.js`/`.cjs` CommonJS globals: ***a blanket decline that is wrong in general can still be right somewhere.*** `CONVERTS 12 · LOST 0 · RIGHT 40 · WRONG 1`. §249–§250 |
| 2026-08-09 | HEAD | **30.74%** | **1,687** | **+1 against a bar of +4** | **This port resolved identifiers at a narrower meaning than upstream.** `getResolvedSymbol` (`checker.go:13890`) uses `Value|ExportValue`; this used `VALUE` alone, so the flagless local an exported declaration leaves behind never matched. `export class A` hides it (its export entry is also `A`); `export default class A` does not. **Two of three plausible causes were checked and disproved first** — the binder was correct and matched upstream line for line. *"Same function" is not "same blast radius"*: §166 changed `resolve_name` and moved 8; this moved 1. §251–§252 |
| 2026-08-09 | HEAD | **30.76%** | **1,688** | **+1, +2 `checker_types`; `extraonly` 22 → 18** | **§243 ported TS2629's report but not the `return c.errorType` after it** — and that return is a *second* rule's guard: `checkArithmeticOperandType` is never asked about an error-typed operand. `arithAssignTyping` wants twelve TS2629 and **no** TS2362; this port emitted both. ***Porting a rule's report without its return ports half of it.*** §243 measured `WRONG 0` on its own codes and was still incomplete — the damage landed in a different code, which is exactly what per-rule isolation cannot see. §253 |
| 2026-08-09 | HEAD | 30.76% | 1,688 | **no code change — a comment corrected, `−14` recorded** | **`check_value_identifier`'s opening comment named `file_has_parse_errors` and the function never consults it.** Adding the guard the comment implies measures **−14 cases**: upstream *does* check identifiers in files that failed to parse. The family the comment names was fixed by `is_value_reference` declining recovered **positions** — a position test, not a parse-error test. ***A comment naming a flag the function does not read is worse than no comment: it is an instruction to restore something that costs 14 cases.*** `extraonly`'s remaining 18 lines are 11 TS2322 (the relation, other workstream) and 7 one-line cases, five of them parser recovery — the board is spent. §254 |
| 2026-08-09 | HEAD | **30.99%** | **1,701** | **+13 of a ceiling of 14 — §206's bound lifted** | **TS7026 fires on a JSX element's *closing* tag as well as its opening one.** §206 recorded the bound, named the mechanism and attached the right instruction — *do not lift it without re-measuring* — and it then sat for a session and a half, worth **13 cases at a one-line change**. `CONVERTS 27 · LOST 0 · RIGHT 371 · WRONG 5`, the five being §206's *other* bound (`<a:b>` names). ***A bound that records exactly how to lift it is a debt with the interest rate written on it.*** **Re-rank the gap before choosing; do not work from the last session's ordering** — TS7026 had grown from "3 residual lines" to 14 blocked cases unnoticed. §255–§256 |
| 2026-08-09 | HEAD | **31.03%** | **1,703** | **+2 against a bar of +10** | **TS2403's decidable fragment.** `isTypeIdenticalTo` is a *third* relation beside assignability and comparability and this port has none of the three — but identity **is** decidable between the intrinsic primitives, which are singletons, so `a != b` there needs no interning assumption. ***A decidable fragment of a relation is worth taking and worth pricing low***: it answers where it is certain, which is systematically the part the corpus least needs. Upstream's `SymbolFlagsAssignment` guard is ported and **idle** — recorded as idle, not as working. §257–§258 |
| 2026-08-09 | HEAD | **31.10%** | **1,707** | **+4, exactly the bar** | **TS1039/TS1254**, found by re-ranking the gap with relation-owned codes filtered out (§256's rule, working as intended). *The **annotation** decides which message, not the initialiser.* `isInitializerSimpleLiteralEnumReference` declined rather than approximated — a missing line where the alternative was a wrong one. **§244's trap attempted a second time**: clippy and coverage batched into one invocation again, coverage read first, 3 lint errors unread. ***Knowing the rule is not the same as having the rule*** — the correction lived only in prose and batching is the ergonomic default. §259 |
| 2026-08-09 | HEAD | 31.10% | 1,707 | **tooling — the ordering made mechanical** | **`cargo xtask measure`: clippy, and only if it exits 0, the conformance run.** §140/§231/§244/§259 are one defect four times — a new dispatch arm shadows an existing one, Rust deletes it silently, and the coverage number measures a compiler with a rule switched off. The first time it read as a **590-case collapse**. The correction *"run clippy first"* was written three times, correctly, and broken on the next multi-part build each time. ***A correction that lives in prose is a correction you get to make again*** — the difference is not insight, it is that this one can say no. Verified in both directions. §260 |
| 2026-08-09 | HEAD | 31.10% | 1,707 | **fifth whole build reverted after measurement** | **TS2874** — `+2` against 36 wrong lines and 4 lost. Every wrong line is a report about **`React` in a file that never mentions `React`**: the configured factory name is not arriving. ***A rule whose input is a configured name cannot be measured until the configuration is known to arrive*** — the rule was never the variable. Owner is smaller and more testable than the row, and sits **upstream of three rules**. Also §261's build was the new `xtask measure` gate's first real outing and it **blocked** on an unused parameter. §261–§262 |
| 2026-08-09 | HEAD | **31.20%** | **1,712** | **+5 — §262's refusal reversed by three lines in the harness** | **The `jsxFactory` directive never left the test harness.** `apply_test_directives` maps `jsx` and `jsximportsource` and not `jsxfactory`, so `Checker::jsx_namespace` took its `React` default and TS2874 reported a name the file never mentions. **Identical checker code**: `+2 / LOST 4 / WRONG 36` before, `+5 / LOST 0 / WRONG 2` after. ***When every wrong line names the same wrong value, suspect the input before the rule*** — §262 looked in the two places the value is *consumed* rather than the one place it is *produced*. **The harness is part of the compiler under test, and the part with no conformance suite of its own.** §263 |
| 2026-08-09 | HEAD | 31.20% | 1,712 | **harness audit + a mutation-checked regression test** | **Nineteen directives used by ≥20 cases each are dropped by `apply_test_directives`.** Seven can reach a diagnostic (`useDefineForClassFields`, `noImplicitThis`, `noImplicitOverride`, `allowUnusedLabels`, `noImplicitReturns`, `experimentalDecorators`, `skipLibCheck`) and **all seven are read by the checker in zero places** — so wiring them today is unmeasurable surface (§248) and they are recorded, not added. ***A silent default is worse than a missing feature, because it is indistinguishable from a working one*** — they are safe only because the checker is incomplete, which will stop being true one build at a time. §264 |
| 2026-08-09 | HEAD | **31.32%** | **1,719** | **+7 of a ceiling of 7, `WRONG 0`** | **TS1046.** Fourth no-producer row to take its whole ceiling, second at zero wrong lines. ***A rule whose upstream source contains its own specification is the cheapest kind to port, and the ranked gap does not distinguish it from any other*** — TS1046 sat at 7 cases behind rows five times its size all session, and what made it short was a comment block no instrument can see. Two details that would each have cost lines: the loop **returns on the first offender**, and it reports on the **first token**, not the name. §265–§266 |
| 2026-08-09 | HEAD | **31.43%** | **1,725** | **+6 of 7, `WRONG 0`, from a flag that is never set** | **TS2378** reads `HasImplicitReturn`/`HasExplicitReturn`, both declared here and set nowhere. ***A flag that is never set does not always block the rule that reads it — the question is whether the flag's meaning is decidable another way.*** These summarise a body's shape and the body is right there, which is why this fragment is `+6 of 7` where §258's identity-relation fragment was `+2 of 32`. The `throw` clause is the difference between `WRONG 0` and a wrong line in every getter-as-stub fixture. §267–§268 |
| 2026-08-09 | HEAD | **31.47%** | **1,727** | **+2; the same unset flag, recovered on one side and not the other** | **TS1308.** `AWAIT_CONTEXT` is read at one checker site and set at none, and the checker's use **is** "the nearest enclosing function is async" — one modifier lookup. Both residual wrong lines are calls to a *function named* `await`, which upstream's parser never turns into an await expression. ***A flag read after parsing can often be re-derived; a flag read during parsing changes the tree, and re-deriving it downstream is not available at any price.*** Owner: `AWAIT_CONTEXT` in `tsr-parser` — also why `YIELD_CONTEXT` will not yield to the same trick. §269–§270 |
| 2026-08-09 | HEAD | **31.49%** | **1,728** | **+1 against a bar of +4 — 17 right lines, 0 wrong, 1 case** | **TS7008.** The bar was priced off `diaggap`'s sole-obstacle count, which was *accurate*; nine cases moved from *blocked on one code* to `STILL SHORT`. ***A sole-obstacle count prices the row, not the slice*** — it answers "this case needs only TS7008", while a bounded slice answers "here are some TS7008". §258 and §267 hit the same thing and only §267's fragment cleared its bar. **A bounded slice should be barred against the lines it can decide, not the cases the code blocks — and no instrument reports the first.** §271–§272 |
| 2026-08-09 | HEAD | 31.49% | 1,728 | **new instrument — `diagslice`** | **§272 named an instrument that did not exist; this is it.** `diaggap` prices the *row*; a case passes only with **every** line of its code, so a fragment converts nothing unless the blocked cases want one line each. `diagslice` reports that distribution and **rules three rows out** — TS2430, TS2300, TS2769 cannot be reached by any fragment. Validated against this session's own builds (TS7008 7→6, TS2378 7→1, TS1308 4→2). ***A necessary condition, not a sufficient one*** — it rules rows out and does not promise the rest, which is still the half that was costing bars. Surfaces **TS2464** (13 cases, 9 single-line), unexamined. §273 |
| 2026-08-09 | HEAD | **31.61%** | **1,735** | **+7, `WRONG 0` — `diagslice`'s first recommendation** | **TS2464.** The rule was already complete; the miss was the *silence policy* declining a question whose answer is not in doubt. `symbolProperty59` types **identically to upstream** — only the relation could not decide it, and **no object type is assignable to `string|number|symbol`**. One conjunct, `CONVERTS 10 · LOST 0 · RIGHT 34 · WRONG 0`. ***A missing relation is a reason to decline, not a reason to stop asking whether the answer is decidable*** — four instances this session, and the two that cleared their bars are the two where the missing machinery summarised something still present. §274–§275 |
| 2026-08-09 | HEAD | **31.85%** | **1,748** | **+13, `WRONG 0`, from one eleven-word list** | **`checkTypeNameIsReserved`** — TS2368/TS2414/TS2427/TS2438/TS2457, five callers of one helper. `diagslice` listed TS2414 and TS2427 adjacently with `5 single` beside both, which is what made them recognisable as one rule seen twice; three more codes came free below every cutoff. ***Rows are ranked; rules are not*** — every instrument here sorts by *code*, and a helper five codes call appears as five small rows scattered by case count. §276–§277 |
| 2026-08-09 | HEAD | **32.02%** | **1,757** | **+9, `WRONG 0`; a sweep that ranks by upstream *function*** | **Seven still-missing codes live in `checkGrammarModifiers` alone**, and nothing that ranks by code puts them near each other — three are 2 cases or fewer. Four other functions carry groups of 25–67 cases. ***The unit of porting is the upstream function; the unit of measurement is the code — nothing here connected the two until now.*** Fourteen sessions of ranking by case count have been ranking the wrong noun. §278–§279 |
| 2026-08-09 | HEAD | **32.05%** | **1,759** | **+2, `WRONG 0`, `STILL SHORT 0`** | **TS1319/TS1248**, closing `checkGrammarModifiers` except for one predicate. ***`STILL SHORT 0` is what a complete arm looks like*** — §272's TS7008 posted 9 from a bounded slice, §275's TS2464 posted 4, §279 posted 7. That makes `STILL SHORT` the readback for §272's question: a bar can be missed because the row was smaller than its count suggested, or because the slice was partial, **and the two look identical in the case delta alone**. TS1206 declined — `NodeCanBeDecorated` is a four-argument predicate, not a lookup. §280–§281 |
| 2026-08-09 | HEAD | **32.12%** | **1,763** | **+4; `RIGHT 92 · WRONG 1`; an edit that silently did not apply** | **TS2588/TS2540**, `checkIdentifier`'s second assignment arm, four lines below §243's and mutually exclusive with it. **One of three edits asserted and did not apply, and the run still produced a plausible `+4`.** ***An edit that fails its own assertion is a silent partial build, and the measurement cannot tell you*** — §207 recorded the shape for when the number does *not* move; here it moved, so the failure was invisible in the measurement and visible only in the shell output above it. §282–§283 |
| 2026-08-09 | HEAD | 32.12% | 1,763 | **sixth build reverted; `xtask measure` now prints its own diff** | **TS7005** measured `−35` then `−4` across two bounds. `diagslice` said *7 cases, 5 single-line, convertible* — **true and not sufficient**: conversion also needs the line to be at a **position** the fragment produces. ***`diagslice` counts lines per case; it does not know where they are.*** §284 added `git diff --stat` to `xtask measure`, which caught this build's dispatch edit failing to apply before any number was produced — the §283 failure mode, now visible. §284–§286 |
| 2026-08-09 | HEAD | **32.29%** | **1,772** | **+9, `WRONG 0`, after one over-wide predicate** | **The parameter-list and accessor grammar families** — eight codes across two loops, one of which this port already had a single arm of. Every one of the six first-measurement wrong lines was `isOptionalDeclaration`, which is **`HasQuestionToken` alone**: an initialiser does not make a parameter optional, so `f(a = 1, b: number)` is legal. ***The name of a predicate is not its definition*** — `isOptionalParameter` sits four lines below it and *does* count the initialiser. Nothing in the gap could distinguish them; reading the call site did. §287–§288 |
| 2026-08-09 | HEAD | **32.42%** | **1,779** | **+7 of a ceiling of 7, `WRONG 0`** | **The heritage-clause family** — five codes, two walks, the whole row. Four builds into the `grammarchecks.go` seam: **+27 across fifteen codes**, every one purely syntactic and `WRONG 0`. ***A file can be a seam*** — it has no types, no relation, no flow, so its rules are decidable by construction, and **none of the fifteen was visible to a ranking by case count** (the largest is 8 cases; seven are 1). Thirty-nine still-missing codes remain in it. §289–§290 |
| 2026-08-09 | HEAD | 32.42% | 1,779 | **seventh build reverted; three measurements** | **The index-signature sequence.** ***An ordered guard sequence cannot be ported in fragments*** — its two type-reading guards sit in the middle, and without them every later guard reports where upstream never does. The third measurement is the keeper: **the syntactic guards alone move zero cases**, so *a cluster of no-producer codes in a syntactic function is not evidence the cases are reachable*. §287's family looked identical from the gap and was `+9`. §291–§292 |
| 2026-08-09 | HEAD | 32.42% | 1,779 | **§292's explanation retracted; `diagslice` grows a column** | **§292 said the syntactic guards were "already covered by another code". Wrong** — the new `occupied` column shows those positions **empty**. The real reason: **their codes have zero blocked cases**, absent from all 442 rows. ***Before porting a rule, check that its code is in the gap — not its function, not its neighbours.*** One `grep` would have saved §291 and §292. The column **failed the case it was built for** and is kept for the different question it does answer, said plainly. §293 |
| 2026-08-09 | HEAD | 32.42% | 1,779 | **eighth build reverted; two filters, both passed, still negative** | **Five grammar arms.** §293's filter worked — every code *was* in the gap — and the build still failed. ***Two codes with the same message text are not one rule***: TS2462 is emitted from at least two upstream sites and the gap, keyed on the **code**, shows them as one row (`objectRestPropertyMustBeLast` wants (1,9); this produced (1,6)). §278's *function* sweep separates them and was not run on this batch because §293's filter felt sufficient. Hook-placement hypothesis raised and **disproved** — all three are per-kind dispatch, exactly where hooked. §294–§295 |
| 2026-08-09 | HEAD | **32.51%** | **1,784** | **+5 of a ceiling of 5 — three filters, all passed** | **TS1108/TS1107.** `CONVERTS 19` against a row `diagslice` sized at **5**: the other fourteen were blocked on TS1108 *and* something else, and a complete arm supplied their lines too. ***A sole-obstacle count is a floor, not a ceiling, when the arm is complete*** — §272 established the opposite for *fragments*, and neither reading is visible in the instrument; only reading the upstream site distinguishes them. The two reverts that produced §293 and §295 each failed a filter that did not exist yet. §296–§297 |
| 2026-08-09 | HEAD | **32.54%** | **1,786** | **+2, `WRONG 0`; a fourth filter identified** | **TS2481.** `diagslice` said 2 of 6 blocked cases were single-line and **exactly those two converted**. ***When a complete arm converts exactly the single-line count, the row is finished and the residue is somebody else's*** — where §297's TS1108 converted **19** against a count of 5. Both are complete arms reading oppositely against the same instrument; what separates them is whether the *other* blocking code is already ported. `diaggap`'s **blocked-by-both** bucket answers that and has never been used to price a row. §298–§299 |
| 2026-08-09 | HEAD | **32.60%** | **1,789** | **+3; a bound took 60 wrong lines to 2 with no change to the score** | **TS2503**, found by the new `diagpair` (§300) as TS2304's top partner. ***The case delta was identical while the wrong column moved by a factor of thirty*** — nothing in the coverage number distinguishes the two builds. The bound: *"does not resolve as a namespace" and "does not resolve" are different conditions, and upstream's message names the first while its guard tests the second.* One lookup was serving two questions. §300–§302 |
| 2026-08-09 | HEAD | 32.60% | 1,789 | **ninth build reverted; and the scan that chose it was wrong** | **TS2353 through a type assertion measured `+0`.** The probe named the owner precisely — the types are *correct*, and the element hop is keyed on a **global-`Array` type reference** an array *type node* never produces. **The scan that selected this row was also wrong**: it read "no constant found" as "no producer" and both its hits were codes this port already emits. ***A scan whose answer is "I could not find it" must be validated against a known positive before it is trusted*** — `diagslice` was, this was not. §303–§304 |
| 2026-08-09 | HEAD | 32.60% | 1,789 | **§304's owner withdrawn, no replacement offered** | `get_type_from_array_type_node` **does** build the `Array` type reference §304 said it did not, so that owner is wrong too. ***"Zero movement is the proof" was the error*** — the inference is only valid once every *other* way the arm can decline is ruled out, and `check_excess_properties` has four early returns of its own. **A `+0` says something declined; it does not say which thing.** Three attributions in one neighbourhood, same shape each time: a plausible mechanism adopted without the probe that would separate it from its neighbours. §305 |
| 2026-08-09 | HEAD | 32.60% | 1,789 | **instrumented; three published attributions replaced by one measured** | **The TS2353 arm was firing all along** — the `+0` was a right line and a wrong line **cancelling**, and §303/§304/§305 each assumed a decline. Four cheap probes found it. ***`diagslice`'s `occupied` column measures exactly this and was not consulted***, because the row was read from `diagpair` and the reading stopped there. First time this session a probe was run **before** the third guess rather than after. §306 |
| 2026-08-09 | HEAD | **32.74%** | **1,797** | **+8 of a ceiling of 9 — twice the bar** | **TS17009**, a sound fragment of a rule whose real test is flow analysis. Two decidable shapes cover eight of nine cases. The three first-measurement wrong lines were `class D extends null`, where the guard was written against `SyntaxKind::NullKeyword` and **matched nothing** — ***this parser makes that `null` an ordinary `Identifier`***. *A guard written from upstream's node kinds can miss silently when the two parsers disagree about a shape neither documents as differing.* Fourth time this session a probe of **this port's own tree** answered what reading upstream could not. §307–§308 |
| 2026-08-09 | HEAD | **32.93%** | **1,807** | **+10, `WRONG 0`, after one dedupe** | **TS2610/TS2611.** A `get`/`set` pair is **one symbol upstream and two declarations here**, so every one of the six first-measurement wrong lines was a second accessor. ***Iterating declarations where upstream iterates symbols produces exactly the duplicates a merged symbol was hiding.*** §246 met the mirror image. The dedupe raised `CONVERTS` from 4 to 10 — **a wrong line and a missing conversion were the same defect**. §309–§310 |
| 2026-08-09 | HEAD | 32.93% | 1,807 | **tenth build reverted — the same fragment on a wider population** | **TS2355** took §267's fragment, which gave TS2378 `+6 of 7` at zero wrong, and measured **`−3` at fifty wrong**. ***A fragment is sound for a condition, not for a rule*** — a getter with no `return` has one shape; a function has overloads, `.js` files, declaration files and unreachable ends. §267 and §311 share every line of reasoning and differ only in which node kinds they run on. Also: testing the annotation's **keyword** rather than its resolved **type** was 43 of the 93 wrong lines. §311–§312 |
| 2026-08-09 | HEAD | **33.07%** | **1,815** | **+8 of a ceiling of 8, `WRONG 0` — §226's refusal reversed** | **TS2335.** Every arm §226 declined to compete with is a **node-kind list or an ancestor walk**, so their conditions are evaluated and their messages declined. ***A refusal is a claim about what is knowable now*** — §307's constructor walk changed that, and nothing about TS2335 moved. The seven first-measurement wrong lines were `isLegalUsageOfSuperExpression` being **two** lists four lines apart: a `super()` **call** is legal only in a constructor. *Two adjacent returns with near-identical lists are a distinction, not a repetition* — §288's shape again. §313–§314 |
| 2026-08-09 | HEAD | **33.11%** | **1,817** | **+2 against a bar of +5** | **TS2729**, sliced to the same-class member list. Both wrong lines are the conjunct §315 **recorded as declined** and did not build. ***A conjunct recorded as "declined" is a wrong line waiting, not a missing one, whenever it is a negative guard*** — five of six conjuncts here make the rule fire *less*, and omitting one of those adds output rather than withholding it. §309's ancestor walk is the machinery it needs: a named next step, not a refusal. §315–§316 |
| 2026-08-09 | HEAD | 33.11% | 1,817 | **flat score, `WRONG 2 → 1`** | **§316's named next step, done** — TS2729's sixth conjunct wired in using §309's base-class resolution unchanged. ***A build worth taking despite a flat score***: §302 measured the same shape from the other side (wrong column ÷30, case delta unchanged), and the argument is identical — **a wrong line is a claim about a program, and this one was false**. Residual is one hop of inheritance. *Two builds ago this was a decline in prose; one build ago two wrong lines; now one hop.* §317 |
| 2026-08-09 | HEAD | **33.18%** | **1,821** | **+4, `WRONG 0`; the no-producer seam is exhausted** | **TS2349's primitive slice** — no signature resolution can make a `string` callable, so the row answers without the machinery it normally needs. The three wrong lines were **one substitution**: upstream *replaces* the head message with TS6234 for a zero-argument call on a getter. ***A head message that is substituted rather than added means the row's code is not the only one its condition can produce*** — `diagpair` reports codes that block *together*, nothing reports codes that are *alternatives*. **Every remaining no-producer row at ≥4 cases is now relation-owned.** §318–§319 |
| 2026-08-09 | HEAD | **33.40%** | **1,833** | **+12, `RIGHT 105 · WRONG 0`, from adding no rule at all** | **TS1038's arm was complete and had never once run** — `check_modifier_order` was invoked from six dispatch arms, all class members or a parameter, where upstream runs it for every declaration with modifiers. ***"The arm exists" was never evidence that a row was deepened rather than dead.*** Opens the **deepening seam** — codes this port emits but under-fires — **unscanned in fourteen sessions**, because every instrument here ranks *missing* codes and a present-but-silent rule looks identical to a complete one. §320–§321 |
| 2026-08-09 | HEAD | 33.40% | 1,833 | **eleventh revert — a `−50` the rule's own isolation could not see** | **TS2694's depth bound.** `diag2307` reported `WRONG 3`; the board reported **−50 cases**. Both correct: the relaxed rule fires where a *different* code was reported correctly, and those losses are missing lines of **other** codes. ***`diag2307`'s per-rule isolation cannot see a rule that displaces a different code*** — §253 met the same blindness from the other side. §185's bound stands, and its comment was right for a reason its author did not state. §322–§323 |
| 2026-08-09 | HEAD | **33.47%** | **1,837** | **+4, exactly the bar** | **TS2564.** ***One predicate, two shapes*** — `is_error` conflates a property whose *type* did not resolve with one whose *name* did not, and TS2564 is about the **initialiser**. §43's decline was right and needed **splitting, not removing**. Also: `diag2307` reported `CONVERTS 271` for a `+4` build — ***it answers "what does this rule do", not "what did this change do"***, and for a rule already in the tree those differ. §324–§325 |
| 2026-08-09 | HEAD | **33.55%** | **1,841** | **+4, `LOST 0 · WRONG 0`; §321's lesson repeated one build later** | **TS2540 through `M["x"]`.** The first measurement was **`+0`** — the rule was widened and the **dispatch was not**, which is exactly what §321 published two builds earlier as the headline of a `+12` build. ***The lesson survived being written down and did not survive being needed.*** It cost one measurement rather than a build, because §284's `git diff --stat` made the edit's scope visible. §326–§327 |
| 2026-08-09 | HEAD | **33.58%** | **1,843** | **+2; and the dispatch sweep comes back empty** | **TS6133** — a self-reference is not a use, and upstream decides it in the **resolver**, not the unused pass. §328 made §321/§327's mechanism systematic and found **no new instance**: ***a mechanism that paid twice is worth making systematic once, and worth believing when it comes back empty.*** The deepening seam five builds in: `+12, −50, +4, +4, +2` — **three of the five turned on *where* a rule is called from rather than what it decides**. §328–§330 |
| 2026-08-09 | HEAD | 33.58% | 1,843 | **twelfth revert — a faithful widening that measured zero** | **TS7006.** `implicit_any_candidates` listed five kinds, all with **bodies**; a signature parameter is implicitly `any` too. List *and* dispatch widened (§327 applied **before** the zero this time) and the probe still shows nothing: `ParameterList6` sets only `target`, and the rule is gated on `no_implicit_any` — yet upstream reports the error. ***A `+0` from a faithful widening is a question about the gate, not the widening.*** Three candidate readings, **none established**; the next step is a baseline survey, not a guess. If the harness reading is right it is a **§263-family defect**. §331–§332 |
| 2026-08-09 | HEAD | **33.60%** | **1,844** | **+1; a survey that disproved two hypotheses, one of them published** | **TS7006.** The survey §332 asked for: **52 of 115** TS7006 cases set neither `noImplicitAny` nor `strict`, so the harness reading is **disproved** — and `ParameterList4` has the *same* options as `ParameterList6` and **already reported**, disproving §332's own gate diagnosis. ***A survey run to test one hypothesis disproved two, and the second was the one I had published.*** §331 widened two of **three** places; the third is `parameters_cannot_be_contextually_typed`. ***Count the places a rule reads its own applicability, and widen all of them.*** §333 |
| 2026-08-09 | HEAD | **33.73%** | **1,851** | **+7 — a position two correct predicates both declined** | **TS2304 in `implements I`.** `is_value_reference` excludes it *correctly* (only `extends` is a value); `check_type_reference_name` requires a `TypeReferenceNode` parent, *correctly* for its own shape. ***A gap between two correct predicates is invisible to both of them*** — and nothing here ranks **positions**, which is why a two-line fixture whose comment states its own expectation sat unconverted for fourteen sessions. Seam seven builds in: **+30 kept**, five of seven turning on *where a rule is asked*. §334–§335 |
| 2026-08-09 | HEAD | 33.73% | 1,851 | **new instrument — `diagnode`, the third axis** | §335 said nothing ranks **positions**. This ranks the node kind **and its parent** at every missing line — the parent being what makes a gap legible. First reading: ***`<no node at position>` is 967 lines***, the third-largest row, where this port's parse produced **no node at all** (§235 counted the same population as 68 *cases*). Also prices a row **out**: TS1487 is 86 lines and **zero** blocked cases — *a code with no sole-obstacle case converts nothing however many lines it carries*. §336 |
| 2026-08-09 | HEAD | 33.73% | 1,851 | **−3, reverted — TS2403 widened to `any`** | §257 bounded TS2403 to intrinsic singletons because *`a != b` between singletons is identity-false with certainty*. `any` is such a singleton, so §337 added it — and measured **−3**. ***`any` is a singleton type but not a singleton conclusion***: the widened-type path also returns it wherever this port cannot compute a better answer, so the guard was never testing *is this a primitive* but *did this port get a real answer*. **Before treating a type id as evidence, ask whether the code producing it has any other reason to.** Owner unchanged: **no `Identity` in `relater.rs`'s `Relation`**. §337–§338 |
| 2026-08-09 | HEAD | 33.73% | 1,851 | **two instruments; TS2454 refused with a measured owner** | **`diagdeepen`** splits the gap into *ABSENT* (rule missing) vs *UNDER-FIRES* (rule exists) — different work, reported in one column until now. ***All top-25 rows under-fire; ABSENT is empty until rank 26***, confirming mechanically that the no-producer seam is closed. **`diagmissing` now sorts cheapest-first with line counts** (§340: a row-level convertibility verdict does not transfer to the case you open). TS2454 probed twice — **−3** dropping the initialiser clause, **−8** dropping the annotation clause. ***`undefined` in a flow type is not a fact about the program — it is also what this port produces when it cannot narrow*** (§338's mechanism, one subsystem over). Owner: **flow narrowing precision**. §339–§342 |
| 2026-08-09 | HEAD | **33.75%** | **1,852** | **+1 — a `?` that merged two endings** | **TS2554 on `new Bar(0)` where `Bar` declares no constructor and has no base.** §90's hop terminated on `sole_extends_class_declaration(class)?`, which returns `None` both for *no `extends` clause* and for *an `extends` this port cannot resolve*. The first is **not a failure** — such a class has the implicit zero-argument construct signature. ***A `?` on a helper that can fail two ways silently merges those two ways***; §140 is the match-arm form and the compiler warns about that one. §343–§344 |
| 2026-08-09 | HEAD | **33.78%** | **1,854** | **+2 — a computed answer thrown away** | **TS2315** *Type is not generic*. `declared_type_parameter_arity` already returned `(0, 0)` for a non-generic class; the caller's `if maximum == 0 { return }` discarded it. Upstream does not treat that as an arity mismatch at all — `checkNoTypeArguments` (`checker.go:23220`) issues a **different diagnostic on the same error node**. Third spelling of one failure (§335 predicate, §343 `?`, §345 guard) — **ten cases from information this port already had**. Discriminator against §338/§341's losses: ***ask what the guard protects against, not what it tests***. §345–§346 |
| 2026-08-09 | HEAD | **33.84%** | **1,857** | **+3 — a question asked of one node kind and not of two others** | **TS2558** type-argument arity in a **call**. Written type arguments made *both* value-arity arms decline (`if !call.type_arguments.is_empty() { return }`) and no rule took over. The comparison, the minimum-count rule, the message and the resolution all already existed — what was missing was *asking a call*. Error node is the type-argument **list**, so the span starts at the first argument, not the callee; predicted from `checker.go:9852` and checked against the fixture's column **before** writing code. Seam now **13 cases, no new subsystem**: ***a rule's reach is the intersection of every predicate between the dispatch and the report***. §347–§348 |
| 2026-08-09 | HEAD | **33.89%** | **1,860** | **+3 — a predicate encoding its *reason*, not its *condition*** | **TS2341** on a **clodule** (`class Clod` + `namespace Clod`). `declared_members_are_complete`'s class-static arm requires every declaration to be a `ClassDeclaration`; a merged symbol carries a `ModuleDeclaration`, so `all` was false. The arm's stated gap is **inherited statics** — *a namespace is not a base*, and the arm above already trusts a pure namespace's table. ***A predicate that encodes why it declines rather than what it declines will decline things the reason does not cover.*** Shared predicate: `checker_types` **unmoved**. Seam now **16 cases, no new subsystem**. §349–§350 |
| 2026-08-09 | HEAD | **33.91%** | **1,861** | **+1, and a sound fix measured at +0** | **TS2448 in a class static block.** Upstream's arm is a *condition* — `ToFindAncestorResult(declaration.Pos() < usage.Pos())` — where this port had the kind in an unconditional `matches!` deferral list. ***A `matches!` list flattens a conditional into a membership***, and `ToFindAncestorResult(false)` means *keep walking*, not *deferred*. Separately §351 found §350's predicted shape on the first grep, fixed it soundly, and **moved nothing** — ***a predicate's defect matters in proportion to the traffic through it***; recorded so the next grep reads that it was measured at zero. §351–§354 |
| 2026-08-09 | HEAD | 33.91% | 1,861 | **three rows priced and declined with owners** | **TS2349** (11) → *intersection reduction to `never`* + instance call signatures, both in the relation. **TS2364** (5) → **all five cases are `parserGreaterThanTokenAmbiguity*`**, the `<no node at position>` population §336 sized at 967 lines; risks two 100% rails. **TS1344** (6) → not a checker rule at all — `A_label_is_not_allowed_here` is in `program.go:2151`'s **parser**-raised list and nowhere in `checker/`. ***A row that passes every convertibility filter can still be owned by another subsystem*** — `diagslice` prices whether a partial rule could convert the case, not which crate the rule lives in. §355 |
| 2026-08-09 | HEAD | 33.91% | 1,861 | **§355's claim tested and DISPROVED — the gap is checker-owned** | Split every blocked row by the **upstream package that raises the code** (message symbol → `grep -rl internal/`). `checker` **1,381** · `checker+project` **489** (TS2322 alone) · `checker+ls` 91 · `checker+compiler` 67 · **`parser` 25**. ***Parser-only is ~1% of the gap***, not "largely parser-owned" as §355 guessed from three consecutive declines. ***A run of three is not a rate*** — when a reason for declining repeats, that is evidence about the **ordering of the list**, not about the population. Corrects the record per the non-negotiables. §356 |
| 2026-08-09 | HEAD | **34.02%** | **1,867** | **+6 — a discriminator with a dead branch** | **TS7016**, untyped JS module import under `noImplicitAny`. `module_resolution_found` — the line TS2307's rule calls ***"the single most load-bearing line in this file"*** — separates *nothing resolved* from *a file resolved that is not in the program*. TS2307 read the first; **the second had no consumer at all**, and the rule's own doc table had named TS7016 as its code since the day it was written. Double the bar. Third `ModuleHost` method, which ADR-0041 permits — its cap is against porting `Program` speculatively, not against the third question a real caller asks. §357–§358 |
| 2026-08-09 | HEAD | **34.04%** | **1,868** | **+1 of a bar of +3 — one arm paid, one cannot fire** | **TS2306** *File is not a module* converts; **TS6142** does **not**. §359 assumed the three codes upstream lists together share a precondition — they do not: a `.tsx` under no `--jsx` **does** resolve to a module, and upstream complains that *the file will not be compiled*, not that there is no module. ***Codes listed together in one upstream function do not share a guard*** — I read a table of codes as a table of guards. TS6142's 2 cases stay open with that owner; the dead arm is kept with the note so the distinction is not re-derived. §359–§360 |
| 2026-08-09 | HEAD | **34.07%** | **1,870** | **+2 — a binder fact recorded, resolved against, never asked about** | **TS2686**, a UMD global referenced from a module. The binder already files `export as namespace N` into `global_exports` (`binder.rs:4620`); resolution already finds it; `is_external_module` already exists. Only the *error* was missing. Ten builds on this seam now — **26 cases, one structural addition**. ***A port's characteristic defect is not missing code; it is code that stops one step before the diagnostic*** — upstream's data structures come over because they are load-bearing for the next thing ported, and the error attached to them does not, because nothing forces it. §361–§362 |
| 2026-08-09 | HEAD | 34.07% | 1,870 | **four rows priced and declined — the seam's cheap end thins** | **TS2552** (5) → `getSpellingSuggestion`; it is TS2304 *with a suggestion*, at the same position, so a partial port trades 5 cases against **TS2304's 2,214 right lines**. **TS7027** (5) → `typeof`-switch exhaustiveness, which is narrowing, not the reachability walk. **TS2720** (4), **TS2693** (5) → the relation. ***A batch of declines that all name algorithms rather than branches is the honest end of a seam*** — §355's batch named other crates and was wrong (§356); this one names missing machinery **inside** `tsr-checker`, which is what §362 predicted would remain. §363 |
| 2026-08-09 | HEAD | **34.13%** | **1,873** | **+3 — and §363's conclusion refuted one build later** | **TS2314** on a bare `Array`. `Array`'s symbol is the standard-library merge — `interface Array<T>` **and** `declare var Array` — and the `VariableDeclaration` hit a `_ => return None`, silencing every bare reference to a lib generic. ***A `_ => return None` meaning "I have nothing to add" written as "nobody can answer"*** — third instance (§349, §353, §364), all `match`es over a **merged** symbol's declaration kinds. §363 had just concluded the seam was thinning from four consecutive hard rows; ***a run of four is not a rate either*** — the same error §356 corrected, repeated one paragraph after quoting it. §364–§365 |
| 2026-08-09 | HEAD | 34.13% | 1,873 | **a predicted defect checked for traffic *before* the build** | §365's family predicts more `_ => return None` arms over merged declarations. `index_constraint.rs:287` is word-for-word identical and owns **TS2411** (25 blocked, 15 single). ***§351's rule applied prospectively for the first time***: `numericIndexerConstraint` is a lone `class C`, not a merge, so the arm is **never reached** — TS2411's blocker is `number` vs `RegExp` assignability, the relation. **The prediction was right about the shape and wrong about the row**; the cost of checking was one fixture instead of one build. §366 |
| 2026-08-09 | HEAD | 34.13% | 1,873 | **−2, reverted — a position found, a code assumed** | **TS2503** on `import TypeScript = TypeScriptServices.TypeScript`. §335's widening of `check_type_reference_name` left its qualified-name twin blind to the same position, so widening it looked free. It is not: `resolveEntityName` for an `import =` reference reaches **TS2307, TS2305 and TS1340** depending on what the left name turns out to be, and TS2503 names a *namespace* failure specifically. ***A code and a position are separate facts, and finding the position does not tell you the code*** — the same lesson as §360's TS6142, found twice in six builds. §368–§369 |
| 2026-08-09 | HEAD | 34.13% | 1,873 | **`diagnode` gains the position↔code join** | §360 and §369 were one failure — *the position was found and the code was assumed*. New `claimed` column: missing lines sitting where this port already emits a **different** code. ***`claimed` low → the position is empty, widening converts*** (§335, §347, §357, §361 all lived there); ***high → a rule looks and picks another code, widening adds a second diagnostic*** (§368, −2). `Identifier in PropertyDeclaration` **111/266** is the densest claimed region; `Identifier in TypeReference` **6/530** is where §335's +7 came from. `diagslice`'s `occupied` answers this per **code**; this answers it per **position**, and §368 is where the two disagree. §370 |
| 2026-08-09 | HEAD | **34.15%** | **1,874** | **+1 — a complete small rule on a ten-line case** | **TS2313** `class C<T extends T>`. Found via `diagnode`'s new column: `Identifier in TypeReference` is **6/530 claimed**, the emptiest large region. ***§273's filter refuses partial rules on multi-line cases; it does not refuse complete small ones*** — `X extends X` is all of upstream's *direct* circularity arm and supplies all ten lines of the case at once. ***A complete small rule and a fragment of a large one look identical in `diagslice`***; only upstream's source distinguishes them, and `checkTypeParameter`'s direct arm is four lines. §371–§372 |
| 2026-08-09 | HEAD | **34.17%** | **1,875** | **+1 — the second row hiding behind "needs the whole rule"** | **TS7031** on the *elements* of an untyped pattern parameter — `function f1([a], {b})` wants columns 14 and 19, which are `a` and `b`. `implicit_any.rs:127` had declined it in one line as *"its own row"* since it was written. ***§273's column tells you the case needs every line; it cannot tell you how many lines the rule is*** — that number is in upstream's source and costs one grep. Two consecutive builds (§371, §373) came from rows behind that label. Both found via `diagnode`'s `claimed` column (6/530 and 13/357). §373–§374 |
| 2026-08-09 | HEAD | **34.18%** | **1,876** | **+1 — a code whose *second upstream site* was never ported** | **TS2661** on `export {X}` where `X` is a global from a script `.d.ts`. This port emitted TS2661 only from `checkAndReportErrorForExportingPrimitiveType` (`checker.go:1629`); the corpus wants `checkExportSpecifier` (`:5565`), a **different function** emitting the same code. ***A code this port already emits may still be missing most of its sites*** — the ABSENT/UNDER-FIRES split is really three-way, and the third case is invisible to every instrument here because instruments see codes while this lives in upstream's *function* structure. Detector: `grep -c` the message name in `internal/checker/`. §375–§376 |
| 2026-08-09 | HEAD | **34.26%** | **1,880** | **+4 — largest since §357, found by a `grep -c`** | **TS1042** `async class` / `async enum` / `async interface` / `async get`. `checkGrammarAsyncModifier` is **nine lines** and was never ported. Found by §376's detector — `grep -c` a message name across `internal/checker/` — run over every row with ≥4 blocked cases: **sixteen codes have more than one upstream site**, and the first unported all-single-line row on the list paid +4. ***Every other instrument here measures the gap from inside this port's output and can only rank what the port already does; a function that was never ported produces no output to rank.*** Fifteen unworked codes remain on that list. §377–§378 |
| 2026-08-09 | HEAD | **34.33%** | **1,884** | **+4 — a rule dispatched from one node kind only** | **TS7008** on class properties. The private-name decline I found by reading the rule was real and worth **+0**; `check_implicit_any_member` was dispatched **only from `PropertySignatureDeclaration`**, so no *class* property had ever reached it. ***A guard that declines and a dispatch that never calls are indistinguishable from the outside*** — both produce silence. Discriminator, one command: `grep -n "<rule_name>(node" check.rs`; **one call site for a rule that should cover several node kinds is the signal**. §379–§380 |
| 2026-08-09 | HEAD | 34.33% | 1,884 | **§380's sweep run; signal refined, no build** | 39 rules in `check.rs` have exactly one call site. The signal is too broad: a block of rules is called **generically for every node** (`check_type_parameter_list(type_parameters_of(typed))` covers every generic declaration from one site) — the *opposite* of §380's defect and indistinguishable by call count. Refined: *one call site **and** it is a kind-specific arm **and** upstream's counterpart is not kind-specific*, the third answered by §376's `grep`. ***A syntactic sweep finds candidates; only a semantic condition finds defects*** — second sweep this session to need a third condition (§351/§366 needed *is it reached*). §381 |
| 2026-08-09 | HEAD | 34.33% | 1,884 | **TS6133's third upstream site scoped, not started** | `checker.go:7124` reports **unused private class members**; this port has only the locals-and-parameters site. Four of the seven blocked cases are `…writeOnlyProperty…`, which upstream reports ***because a write is not a read*** — and `unused.rs` tracks members **by name** (`referenced_member_names`) without that distinction. Scoped as three items, of which threading `is_write_only_access` into `note_member_name_at` can regress TS6138 and the nonexistent-property path, since the set is shared. **Recorded rather than started at session end: a half-threaded flag is the shape that measures +0 and hides a −n elsewhere.** §382 |
| 2026-08-09 | HEAD | **34.37%** | **1,886** | **+2 — and §382's scoping corrected before it cost a build** | **TS6133** on write-only private members. §382 scoped three items from upstream's source; **two already existed** (`check_unused_class_members` covers all four member kinds *and* the set-accessor exemption), and the "shared" set it worried about has **only two readers, both in that rule**. Real gap: `note_member_name_at` marks a name from *any* property access, so `this.x = 1` counted as a read. ***Scoping a build from upstream's source alone will over-scope it*** — upstream says what the finished behaviour is, not which parts this port already has. §383–§384 |
| 2026-08-09 | HEAD | **34.38%** | **1,887** | **+1 — the `Name::Identifier` destructure, third instance** | **TS2540** on `this.#roProp = ""` where `#roProp` is getter-only. `check_readonly_property_assignment` destructured `MemberName::Identifier`, so private names declined. ***A `let X::Identifier(name) = … else { return }` is a decline nobody wrote on purpose*** — the shape you get from destructuring the common case, and the compiler never objects because the `else` is syntactically required. Three found this session (§379 `PropertyName`, §385 `MemberName`, §373 `BindingName`); the sweep is `grep -n "Name::Identifier(.*) = .* else"`, with §381's caveat that it finds candidates, not defects. §385–§386 |
| 2026-08-09 | HEAD | 34.38% | 1,887 | **§386's sweep run — 14 candidates, none promoted** | `grep -rn "Name::Identifier(.*) = .* else"` finds **fourteen** sites across five `…Name` enums. Three were checked this session and **two paid** (§379, §385). The remaining eleven each need §381's semantic condition — *does upstream report for the other variant here* — one `grep` apiece. Two hypotheses recorded for the next session: `nonexistent_property.rs:53` (TS2339, largest under-firing row, but mostly relation-owned) and `check.rs:1774` (TS2448, which already paid once via §353). **Recorded rather than built: a speculative one-arm widening at the tail of an 86-build session is how §368's −2 happened.** §387 |
| 2026-08-09 | HEAD | **34.42%** | **1,889** | **+2 — a row that came out of a disproof** | **TS18016** on `const obj = { #foo: 1 }`. §387's two sweep hypotheses were both **disproved by one grep each** — a binding-pattern parameter cannot be a parameter property (TS1187), and upstream reports **TS18016, not TS2339**, for an unknown private name. The second disproof named a code this port did not emit at all, with a nine-line upstream site. ***A disproof that names the right answer is worth more than the hypothesis would have been.*** §388–§389 |
| 2026-08-09 | HEAD | **34.44%** | **1,890** | **+1 — TS18016 closed to zero** | `type A = { #foo: string }` and `interface B { #foo }` — the same code as §388's object literal, two more node kinds. **All three of TS18016's blocked cases now convert; the first row this session taken to zero.** Fourth build in a row on a row `diagslice` labels *"needs the whole rule"* (§371, §373, §388, §390) where the whole rule was under fifteen lines. ***`diagslice`'s verdict is about the case, not the rule*** — nine lines wanted by one syntactic pattern is a `for` loop, and only upstream's source distinguishes that from nine lines wanting a cycle detector. §390–§391 |
| 2026-08-09 | HEAD | **34.48%** | **1,892** | **+2 — TS2331 closed; the row came from pricing a refusal** | `namespace M { var f = () => this }`. Pricing **TS2683** (12 blocked, refused — owner `tryGetThisTypeAtEx`) meant opening `checkThisExpression`, and its container switch **twelve lines earlier** is purely syntactic and unported. ***Three of the last four builds came from reads undertaken to say no*** — upstream groups a syntactic guard and a type-machinery check in the same function, and this port had neither. **Write refusals against upstream's source, not against the row**: *"TS2683 is 12 cases of `this`-type work"* would have been just as correct and found nothing. §392–§393 |
| 2026-08-09 | HEAD | 34.48% | 1,892 | **TS2493 declined; §393's neighbour check run and empty** | `let x = <[]>[]; let y = x[0]`. Upstream's guard asks **three type-side questions** before reporting — `isTupleType`, the target tuple's `combinedFlags`, `getTypeReferenceArity` — none syntactic. **Owner: tuple-type machinery, 9 cases.** The neighbour check §393 prescribes was run and came back **empty**: this function's other two diagnostics share the same guard, unlike `checkThisExpression`'s syntactic arm. ***Recording the negative neighbour check matters as much as the positive one*** — two of three functions read this way surfaced a neighbour, and only writing up the hits would inflate the method's rate. §394 |
| 2026-08-09 | HEAD | **34.51%** | **1,894** | **+2 — a twenty-five-line grammar check never ported** | **TS1156** on `if (x) using a = null`. `checkGrammarForDisallowedBlockScopedVariableStatement` + `containerAllowsBlockScopedVariable` is 25 syntactic lines, and `NodeFlags::BLOCK_SCOPED` already existed. §377's list is now **five builds and +13** against predicate-widening's ten builds and +24 — ***per build the better seam, and it did not exist as a category before §375***. Why it works: ***this port is a transliteration in progress, so its gaps are shaped like functions, not behaviours*** — every instrument built here ranks behaviour, which is the wrong unit for that class. **Ten codes unworked.** §395–§396 |
| 2026-08-09 | HEAD | **34.58%** | **1,898** | **+4 — the syntactic head of a type-machinery function** | **TS18050** on `null.foo` and `undefined[a]`. `reportObjectPossiblyNullOrUndefinedError` opens with two arms needing **no type at all**, and neither is gated on `strictNullChecks` — a literal `null` receiver is wrong under every flag. **§140's `unreachable pattern` fired for the fourth time this session** on the first dispatch attempt; without `xtask measure`'s clippy gate the arm would have compiled as dead code and measured +0, diagnosed as *the rule is wrong* rather than *the rule never runs*. §377's list: **six builds, +17**; nine codes unworked. §397–§398 |
| 2026-08-09 | HEAD | 34.58% | 1,898 | **TS2300 declined; its six sites inventoried** | TS2300 has **more upstream sites than any code in the gap** — and is `2 of 20` single-line with `occupied 3/71`. Three sites are unported (object-literal duplicate method, `prototype` export collision, `reportDuplicateMemberErrors`), one is **already ported** (duplicate type-parameter name), and two are §229's `declareSymbol` refusal. ***A code with six sites is not six opportunities*** — §377's list is ranked by site count, and site count is not convertibility; `diagslice` and `diagdeepen` must agree before the read is worth doing, and here they do not. Inventory recorded so a future session need not re-derive it. §399 |
| 2026-08-09 | HEAD | 34.58% | 1,898 | **detector v2 — sites by *file*; a top row proved dead** | §377's list counted upstream sites without naming their file. TS2353's four sites are **all in `relater.go`** — four sites, zero portable. Corrected detector reports sites per file. ***`TS2741` is 46 blocked, 32 single-line, and dead***: `diagslice` says *"yes — mostly single-line"*, `diagdeepen` says UNDER-FIRES with 48 emits, **both are right**, and its only upstream site is inside the structural walk. **`TS2339` carries twelve `checker.go` sites against this port's one rule** — the largest unexplored surface the detector has produced. Ordering for next session: **files first, then `diagslice` ∩ `diagdeepen`, then read the function.** §400 |
| 2026-08-09 | HEAD | **34.62%** | **1,900** | **+2 — TS2339 on `a["nope"]`; the board crosses 1,900** | Eight of TS2339's twelve `checker.go` sites are the **element-access cluster** and this rule saw none of them. Widening it alone measured **+0**; one `grep` showed **a single call site**, and adding the dispatch paid +2. ***The `+0` is not the failure — reading it as "the rule is wrong" is.*** First time a rail from this session (§380) was applied *as a rail* rather than rediscovered. §401–§402 |
| 2026-08-09 | HEAD | **34.69%** | **1,904** | **+4 — a relation-owned row with a decidable subset** | **TS2394** `function foo():number; function foo():string {}`. `isImplementationCompatibleWithOverload` is signature assignability — and sixteen cases of it are `number` versus `string`, which §257's intrinsic-singleton argument decides. `any` is excluded for **two independent reasons** (§338's, and upstream's own — an `any` implementation return is compatible with every overload), the first time the port's constraint and upstream's coincided. ***A relation-owned row can have a decidable subset, worth porting when upstream's own predicate degenerates on it*** — reframes TS2416, TS2411, TS2420, TS2352, one `diagcase` each. §403–§404 |
| 2026-08-09 | HEAD | 34.69% | 1,904 | **+0, reverted — and a stale instrument reading found** | §405 ported TS2416's decidable-primitive subset (§404's lead). It measured **+0**; §402's rail showed the dispatch was correct, so the declining guard is unisolated — **recorded, not guessed at** (§339/§341 are what guessing costs). The larger finding: `elaboratedErrors` needs **TS2416 *and* three TS2741 lines**, so it cannot convert on TS2416 alone, yet `diagmissing` listed it as sole-obstacle. ***An instrument's output is a measurement, and measurements expire*** — the scratch table behind the last six builds' row counts was taken at **1,873** and the board is **1,904**. Re-run `diagslice` before the next row. §405–§406 |
| 2026-08-09 | HEAD | **34.78%** | **1,909** | **+5 — largest since §357; *absent* is not a comparison** | **TS2420** on `declare class Buffer implements IBuffer {}` where `IBuffer` declares an index signature the class lacks. Came straight out of §406's correction: fresh table → **§293's `occupied` column** → fixture. TS2416 fails it at **15/34**; TS2420 passes at **0/21**, and reading a column that has existed since §293 is the whole difference between §405's +0 and this. ***A member the class does not declare cannot be assignable to one it must have, and no relation is consulted to know that*** — **ask whether the cases are about a *missing* member before a *wrong* one.** §407–§408 |
| 2026-08-09 | HEAD | 34.78% | 1,909 | **§404's re-pricing lead closed — +9 of four candidates** | All four relation-refused rows asked whether their cases fall in a decidable subset: **TS2394 +4**, **TS2420 +5**, **TS2416 0** (fails §293's `occupied` at 15/34), **TS2411 and TS2352 declined**. TS2411's owner is *the apparent type of a primitive* — `number` **is** assignable to a structurally empty interface, so `number` vs `RegExp` is the relation, not the singleton argument. ***`absent` beats `decidably different`***: §407 needed no type comparison, §403 needed the primitives to line up, §405 needed that **and** a free position. **Recording that a lead closes matters as much as recording that it paid** — §404 said "several standing refusals"; the honest count is two of four. §409 |
| 2026-08-09 | HEAD | 34.78% | 1,909 | **§400's "TS2741 is dead" verdict corrected** | TS2741 is the **missing-member** code — 46 blocked, 32 single, `occupied 0/91` — and §400 marked it *RELATION ONLY — dead* because its only upstream site is in `relater.go`. **Wrong.** The rule is already ported (`assignreport.rs:528`) and its own doc makes §409's argument: *"is a required property absent… answered by the member tables alone. No relation runs."* The position is dispatched too. The decline is inside `missing_required_property`, where `declared_property_table` answers `None` for an **object-literal source**. ***A file-level detector cannot see a rule that is already ported*** — `relater.go`-only means dead **unless this port already emits the code**, and `diagdeepen` said **48 emits** right beside it. §410 |
| 2026-08-09 | HEAD | 34.78% | 1,909 | **+0, reverted — §410's owner corrected one turn later** | §411 admitted object-literal sources to `declared_property_table`, the owner §410 named. **+0**: the call chain is `report_assignability_failure → pair_is_reportable → missing_required_property → declared_property_table`, and the **first** link runs only once the relation has returned `NotRelated`. ***A doc comment that says "no relation runs" describes the rule, not the path to it.*** TS2741's 46 cases are owned by **the relation's verdict on object-to-object**. Third distinct source of a `+0` this session: §351 nothing reaches the predicate, §380 nothing dispatches to the rule, §412 the rule's **trigger** never fires — discriminated by a fixture, a grep, and reading the call chain **upward**. §411–§412 |
| 2026-08-09 | HEAD | **34.80%** | **1,910** | **+1, and `checker_types` +1 — a refusal's reason became a bound** | **TS2678** on `switch (0) { case Foo: }` where `Foo` is a class. §409 refused TS2411 because *primitive vs arbitrary object* is the relation's — a primitive **is** assignable to a structurally empty interface. A **class constructor is not arbitrary**: it always carries `prototype`, so that escape cannot fire. ***A refusal's reason is a specification for the case that would survive it*** — "owner: the relation" would have said nothing. Second time this session a decline produced the next build (§393 was the first). §414–§415 |
| 2026-08-09 | HEAD | **34.97%** | **1,919** | **+9 — the session's largest, from a comment written four sessions ago** | **TS2683**, `this` in a plain function under `noImplicitThis`. §392 refused it with the owner *`tryGetThisTypeAtEx` returning `nil`* — and `expressions.rs:939` already said where this port answers `any`, **naming TS2683 by number in its own parenthesis**. Cost: **one field** (the fifth member of the strict family). ***The port's own explanatory comments are a diagnostic inventory nobody has read as one*** — and unlike §386's sweep this one needs no semantic condition: **a comment naming a code this port does not emit is already the finding**. §416–§417 |
| 2026-08-09 | HEAD | **35.02%** | **1,922** | **+3 — the board passes 35%; second build from the port's own comments** | **TS2669**, a `global` block outside the two legal positions. `binder.rs:1073` names the code, **cites upstream's two line numbers, and records a `tsc` 5.x verification of the exact predicate** — used to decide a *merge*, with the diagnostic one `if` away and nothing asking. §417's sweep is now **+12 over two builds** (§416 TS2683 +9, §418 TS2669 +3) from **62 codes named in comments but never emitted**. ***A comment explaining why a rule declines to do X specifies the diagnostic that fires instead.*** §418–§419 |
| 2026-08-09 | HEAD | 35.02% | 1,922 | **§417's sweep corrected — false positives in its "not emitted" half** | TS2391 was third on the list and **is** emitted (`check.rs:6510`, a fully ported rule with its own decline table). The sweep built its *emitted* set by **parsing `messages.rs`**, and a constant whose declaration wraps differently is missed — manufacturing a candidate that looks real all the way to the fixture. ***A sweep that joins two greps inherits both their error rates, and the failure is silent in one direction only.*** Fix: join against **`diagdeepen`'s measured emit count**, not a parse. §416 and §418 were verified against the board before shipping, so **+12 stands**; the other seven candidates are now *unverified*. §420 |
| 2026-08-09 | HEAD | 35.02% | 1,922 | **§417's sweep re-derived against a measured oracle: closed at zero** | Joined the comment scan against **`diagdeepen`'s measured emit column** instead of a parse of `messages.rs`. Result: **0 candidates** — every code this port names in a comment and still blocks cases on is **already emitted**. §416 (+9) and §418 (+3) took the only two; the seven that looked pending were §420's parse artefacts. ***A lead that closes at zero is a better outcome than one that closes at "nothing obvious left"*** — §363 ended a seam on a run of four and §365 refuted it one build later. Worth keeping as a **session-close check**: *did I write a comment naming a diagnostic I did not then wire?* §421 |
| 2026-08-09 | HEAD | 35.02% | 1,922 | **+0 twice, reverted — and the toolkit's honest boundary** | **TS2488** on `for (v of new MyStringIterator)`. First +0 was **§380's branch** (keyed on a `SyntaxKind` a `for…of` does not register as); fixed in one line via §402's rail. Second +0 is **§351's branch** — dispatched, reached, silent, because a `new` expression's type does not present as `TypeData::Named { members: Some(_) }`. **Not probed further at the end of a long session** (§339/§341 are what guessing costs). Fourth `+0` question added: ***is the DATA the shape assumed?*** — the expensive one, **because every instrument here measures diagnostics, not types**. §423–§424 |
| 2026-08-09 | HEAD | 35.02% | 1,922 | **TS2749 on a qualified name — scoped, not built** | `var b: A.B` where `B` is a function+namespace merge. The ladder that decides TS2709-vs-TS2749 **already exists** and is called from **one place** — `check_type_reference_name`, whose subject is a *simple* name; a `QualifiedName` reaches only `check_qualified_type_name`, which emits TS2503 alone. §347's shape. **Scoped rather than built because §369 measured −2 widening exactly this rule's position**: the ladder's first rung would answer **TS2709** for a fundule where upstream answers TS2749, and ***§103's rule governs — the `else if` order is the specification***, unread here for a namespace-and-value symbol. §425 |
| 2026-08-09 | HEAD | **35.06%** | **1,924** | **+2 — the scoping note paid for itself in one turn** | **TS2749** on `var b: A.B` where `B` is a fundule. §425 deferred it with a reason — *establish upstream's rung order first, because not doing so is how §369 happened*. One `grep` later the rung was in a **different function entirely** (`resolveEntityName`'s failure path, `canSuggestTypeof`, tested **before** the namespace branch), and §425's own plan would have reported **TS2709**. ***A scoping note is a cheap way to be wrong on paper instead of on the board.*** Fourth build this session out of a decline or deferral (§393, §415, §418, §426) — the **reason** paid every time, never the verdict. §426–§427 |
| 2026-08-09 | HEAD | **35.08%** | **1,925** | **+1 — a guard whose reason holds in one mode only** | **TS2365** on `var z = 3 + null` under `@strict: false`. The predicate was fine; the caller declined twenty lines earlier because *`checkNonNullType` reports TS2531/TS2533 in place of this code* — **true only under `strictNullChecks`**, and with the flag off that function is a no-op. ***A guard justified by another rule's behaviour inherits that rule's preconditions, including its flags.*** Mechanical search recorded: `grep "runs before|in place of this code|instead of this"` finds **eleven** such comments, each asking *does the function it names run unconditionally?* §429–§430 |
| 2026-08-09 | HEAD | 35.08% | 1,925 | **§430's "eleven" corrected to eight; the lead closes at zero** | The inherited-precondition search returns **8 hits / 4 distinct guards**, not eleven: one was flag-dependent and **§429 fixed it**, one (`checkForDisallowedESSymbolOperand`) is unconditional, two are structural. **Second time this session a mechanical search was exhausted in the turn it was proposed** (§421 was the first). ***Estimating a sweep's yield before running it is a habit worth dropping*** — §356 corrected a population, §365 a rate, §420 a false-positive set, this a size; each estimate was cheap to make and cheaper to check. **Rule: run the sweep in the same turn, or record it without a number.** §431 |
| 2026-08-09 | HEAD | **35.22%** | **1,933** | **+8 — double the bar, from a row a stale table had hidden** | **TS2309**, `export = B` beside `export class C`. It sat at **8 blocked / 8 single-line / `occupied 0/8`** — the cleanest possible signature — and was never looked at because the table was **six builds stale every time**. The refresh also surfaced TS2351, TS2507, TS2417, TS7013, TS18014 at the same signature. ***The rows that stay at the top of a stale table are the ones already converted, so the survivors sink out of view exactly as they become workable.*** **Re-run `diagslice` after any build moving the board by more than ~5.** §432–§433 |
| 2026-08-09 | HEAD | **35.26%** | **1,935** | **+2 — a registered prediction about the port's own type lookup** | **TS2351** on `new x()` where `x` is a class *instance*. §424 found `TypeData::Named { members: Some(_) }` failing for a `new` expression's own type and called it the toolkit's boundary; §434 **predicted in writing** that it would succeed for an identifier with a declared type, and it did. ***The same lookup answering on one shape and not another is a fact about the port's type construction, not about the lookup*** — crossed by choosing a shape on the working side, which is cheaper than an instrument and **not a substitute** for one. §434–§435 |
| 2026-08-09 | HEAD | **35.31%** | **1,938** | **+3, and `checker_types` +14 — a framing correction** | **TS2507** on `class B extends A` where a local `var A = 1` shadows the class. The **+14** on the other suite is the largest cross-suite effect this session: `check_expression` on the `extends` expression **evaluates a node the class path never touched**, filling entries `checker_types` asserts. ***Calling `check_expression` somewhere new changes the type snapshot, not only the diagnostic board*** — §350 and §411 list "`checker_types` unmoved" as the falsifier, which is right for predicate changes and **wrong for changes that evaluate a new expression**. The right falsifier there is *no case goes passing→failing*. §436–§437 |
| 2026-08-09 | HEAD | **35.44%** | **1,945** | **+7 — the row priced was half the build** | **TS7013/TS7011**, a construct or call signature with no return annotation. Six lines of upstream ported straight. The bar was +4 because `diagslice` priced **TS7013 alone**, and `checkSignatureDeclaration`'s `switch` emits **two codes** — the call-signature half carried cases nothing counted. ***When one upstream function emits two codes, the row you priced is half the build*** — **read the `switch` before setting the bar, not just the arm your row named.** §433's refresh has now yielded **+20 across four rows** a stale table had hidden. §438–§439 |
| 2026-08-09 | HEAD | **35.51%** | **1,949** | **+4 — a fifth refusal reopened by its reason** | **TS2355** on an **empty body**: reachability is not a question when there are no statements. The refusal (*`functionHasImplicitReturn` is reachability*) was and remains correct for the general check. **Five refusals reopened this session for +18** (§393 +2, §415 +1, §416 +9, §426 +2, §440 +4), by reading the *reason* rather than the verdict. ***The value of a refusal is proportional to how specifically it states what it cannot do*** — and the prediction that follows holds: refusals naming a **whole subsystem** (§412, §424) have been reopened **zero** times. §440–§441 |
| 2026-08-09 | HEAD | **35.59%** | **1,953** | **+4 — a diagnostic that fires during *setup*** | **TS2397**, a script declaring `undefined` or `globalThis`. **Neither guard lives in a `check*` function** — both run while the global symbol table is assembled, as TS2669's (§418) ran in the binder's merge logic. ***Diagnostics that fire during setup are invisible to every technique that walks the check traversal*** — §328, §381 and §386 all follow `checkSourceElement`'s callees. §376's message-name grep searches `internal/checker` **regardless of function**, which is why it and the comment sweep found these and nothing else did. §433's refresh: **+28 across six rows**. §442–§443 |
| 2026-08-09 | HEAD | 35.59% | 1,953 | **−14, reverted — three meanings of "ambient"** | **TS1038** on `declare namespace M { declare class C {} }`. §445 swapped `file_is_ambient` for the walk-threaded `ambient`, reasoning that ambient is a *context*. Sound about upstream, wrong about the substitute: ***`ambient` is `file_is_ambient` **OR** an enclosing `declare`***, so the swap **widened** rather than narrowed, and every `declare` member of every `ModuleBlock` in every `.d.ts` began reporting. Three distinct predicates now listed: `file_is_ambient` / threaded `ambient` / upstream's **`NodeFlagsAmbient`** parser flag. **Owner: `NodeFlagsAmbient` in `tsr-parser`**, which this port never sets. §445–§446 |
| 2026-08-09 | HEAD | **35.68%** | **1,958** | **+5 — a helper whose doc names a code, called only to `return`** | **TS2576** on `this.Foo()` where `Foo` is static. `other_side_of_class_has` **already computed the answer**, its doc comment **already named TS2576**, and the caller used it only to suppress TS2339. Fourth instance (§345, §357, §410, §447) with an identical tell. Composed sweep recorded: ***a helper whose doc names a diagnostic, whose every call site sits inside an `if … { return }`*** — the second half of §421's grep, and a **different question** (does the answer reach a report, vs does the port emit the code). §447–§448 |
| 2026-08-09 | HEAD | 35.68% | 1,958 | **§448's sweep run, self-corrected, and priced at ~zero** | Twelve candidates; **the first correction was mine in the same turn** — the regex counted `if let Some(x) = f() { report }` as suppress-only, which would have contradicted §363. ***A sweep for "the answer is discarded" must distinguish `if f()` from `if let Some(x) = f()`*** — one character of syntax, and it inverts the finding. Corrected: ten remain, most **suppressors by design** (`pair_is_reportable`, `declaration_is_constant`). One true §447-shaped candidate, worth **1 case**. ***The sweep's yield is the row behind the helper, not the helper count.*** §449 |
| 2026-08-09 | HEAD | **35.73%** | **1,961** | **+3 (and `checker_types` +8) — §257's argument, member-wise** | **TS2394** on `():{a:number}` vs `():{a:string}`. Two type literals whose every member is a written primitive: §257's singleton argument applies **member-wise**, and no relation is consulted — only two maps compared. §257's idea is now **+10 kept across five applications**, with its boundary sharp on both sides: `any` out (§338, measured), a *partial* map out by construction. ***A decidable-subset argument extends by making the unit smaller, not the domain wider*** — §450 shrank the unit from types to members; §338 kept the unit and widened the domain, and cost −3. §450–§451 |
| 2026-08-09 | HEAD | 35.73% | 1,961 | **two refusals merged into one 13-case parser item** | **TS1359** needs `NodeFlagsAwaitContext`/`YieldContext`; **TS1038** (§446) needs `NodeFlagsAmbient`. All three are **declared in `flags.rs` and never set by `tsr-parser`**. ***Two refusals naming the same unset flag family are one item, not two*** — separately they read as small dead rows, together they are a **13-case parser change with a single acceptance test**. First time this session two owners **merged**, which is an argument for naming owners precisely enough that merging is possible. Not built here: both parser rails are at **100%** and the falsifier is that they stay there. §452 |
| 2026-08-09 | HEAD | **35.75%** | **1,962** | **+1 — and a reverted build's helper resurrected** | **TS2320** on two bases contributing one name with different written primitives. §450's argument over a base chain — §257's idea now **+11 kept across six applications**, the unit shrunk three times (types → members → inherited members), each shrink preserving the proof. `written_primitive_members` was written for **§405, which measured +0 and was reverted whole**: ***reverting a build whole is right; concluding its parts were wrong is not*** — the helper was sound and its caller was the problem. First reverted code to come back this session. §454–§455 |
| 2026-08-09 | HEAD | **35.81%** | **1,965** | **+3 — a fixture that carries its own control** | **TS2464** on `[t]` where `T` is **unconstrained**. `computedPropertyNames51_ES6` writes `[t]` and `[k]` side by side with `K extends keyof T` and expects an error on exactly one — ***a fixture containing both the positive and the negative is worth more than two fixtures containing one each***. Read the fixture for the case it does **not** report; that line is the bound. Fourth way this session of stepping around the relation (§407 absent, §403 singletons, §436 no construct signature, §456 no upper bound) — **+22 together**. §456–§457 |
| 2026-08-09 | HEAD | **35.90%** | **1,970** | **+5 — one argument, five positions, +16** | **TS2351** on `` new `abc`(…) `` and `new (a ** b ** c)` — a **primitive callee** never constructs, which is §436's `extends` argument with nothing heritage-specific about it. §257's sentence has now been asked in five places (§403, §436, §450, §454, §458) for **+16 kept**, against one failed extension (§338, −3). ***A decidable-subset argument travels by position and breaks by domain*** — **before extending one, ask whether the change adds a position or a type. A position is free; a type needs its own measurement.** §458–§459 |
| 2026-08-09 | HEAD | 35.90% | 1,970 | **§459's rule applied prospectively — a position without the types** | **TS2349** (*not callable*, 11 blocked, 9 single, `0/14`) is the obvious next position for the primitive argument. **Checked before building**: the corpus's callees are a class instance, a missing member, an intersection reducing to `never`, and a generic call — **not one primitive**. §363's owner stands; what is new is that the decline cost **one `diagmissing` instead of a build**. ***A rule that predicts where an argument fails is worth as much as one that predicts where it works*** — first checked-and-rejected position, which is what makes §459 falsifiable rather than a description of past successes. §460 |
| 2026-08-09 | HEAD | **35.93%** | **1,972** | **+2 — a neighbour of the row just declined** | **TS2348**, calling a class without `new`. §460 had just declined **TS2349** — one code away — because the position lacked the *primitive* types; TS2348 pays on a **different argument entirely** (a constructor is not callable without `new`, decided from `SymbolFlags::CLASS` alone). ***Rejecting a row for one argument does not reject it for every argument*** — a decline is about the argument that was tried, and the note has to say which one, or a reader scanning declines will skip the neighbours. §461–§462 |
| 2026-08-09 | HEAD | **36.01%** | **1,976** | **+4 — the board passes 36%; a declined row reopened by a new argument** | **TS2417** on `static x: string` overridden by `static x() {}` — a **kind mismatch**, and no relation compares declaration kinds. The row was refused as *the relation's*, which was true of the comparison it names and **not of its cases**. ***A row's owner is the owner of the general check; the corpus decides which part of it the cases need.*** The fixture names (`inheritanceStaticFuncOverridingProperty`) were the whole diagnosis. Five relation-free arguments now total **+30**. §463–§464 |
| 2026-08-09 | HEAD | 36.01% | 1,976 | **TS2416 re-checked for the kind argument, rejected** | §463's falsifier and §464's lesson both point at TS2416 as the *instance* sibling. Checked: `class MyEvent<T> extends BaseEvent { target: T }` is a **type** mismatch on a **generic**, not a kind mismatch. TS2416 now carries three declines, each naming a different argument (§405 `occupied`, §409 not primitives, §465 not kinds). ***Two prospective rejections in five builds (§460, §465) against two prospective successes (§461, §463)*** — the honest rate for re-checking a declined row, and the check costs one `diagmissing` either way. §465 |
| 2026-08-09 | HEAD | **36.08%** | **1,980** | **+4 — the first position found by *searching* for the argument** | **TS2358**, a primitive on the left of `instanceof`. Five earlier positions arrived by luck of the queue; this one was found by scanning message **text** for type restrictions (*"must be of type"*, *"is not callable"*) and asking §459's question. ***An argument that has paid six times is worth a search, not just a recognition*** — the same claim §441 made about refusals, applied to the positive case. Primitive argument now **six positions, +20**, one rejected (§460), one reverted (§338). §466–§467 |
| 2026-08-09 | HEAD | **36.15%** | **1,984** | **+4 — second row from the message-text search; the procedure is written down** | **TS2466**, `super` in a computed property name — `checkSuperExpression`'s **first** arm, which tests the *position* not the container. Two consecutive builds from §467's search (+4, +4). The six-step procedure is now recorded end to end: refresh → filter on `occupied 0/n` and single-line → **grep the message text for a restriction** → read upstream's arm → read this port → bar, falsify, measure. ***A technique becomes a procedure when its steps can be run by someone who was not there for the reasoning.*** §468–§469 |
| 2026-08-09 | HEAD | **36.21%** | **1,987** | **+3 — third consecutive row from the message-text search** | **TS2376**, `super()` not first when the derived class has initialized state. First row of the three whose upstream arm was **not** nine lines — the statement scan plus the state gate is ~30 — and it still landed on the first measurement, because the subtree walk it needs already existed in the file. ***§391's "nine lines half the time" is about the arm, not the build*** — the estimate that matters is **how much of it this port already has** (§383), which is why that step sits at 5 and not 2. Search total: **+11**. §470–§471 |
| 2026-08-09 | HEAD | **36.22%** | **1,988** | **+1 — §467's search closes at +12 across four builds** | **TS1268**, `[a: boolean]`. §292 built seven guards of the same upstream function for **+0**; this row's `occupied` is **0/4** where those were taken, which is the whole difference. Search totals: TS2358 +4, TS2466 +4, TS2376 +3, TS1268 +1. ***Three mechanical sweeps, three measured endings — §421 zero, §444 zero, §473 +12*** — and the difference is not luck: the first two searched **this port's** text, this one searched **upstream's message catalogue**, the one artefact describing behaviour the port has not written yet. §472–§473 |
| 2026-08-09 | HEAD | **36.26%** | **1,990** | **+2 — the message search broadened from restrictions to prohibitions** | **TS1184**, a modifier on a statement inside a function body. §473 concluded upstream's catalogue is the seam that pays; §474 tested it by widening the grep to *any* syntactic prohibition (`cannot appear`, `not permitted`, `duplicate`, `already`) — five new candidates, first one +2. ***The message catalogue is a to-do list written by someone who finished the job*** — every entry names a condition, and the ones with no rule here are exactly the conditions this port does not check. **Search the artefact that describes the target, not the one that describes the progress.** §474–§475 |
| 2026-08-09 | HEAD | **36.33%** | **1,994** | **+4 — every blocked case in the row converted** | **TS1114**, a duplicate label. Six lines of upstream ported unchanged; **first row this session where all four blocked cases fell to one rule**. Message-catalogue search now **six rows, +18** (restrictions +12, prohibitions +6) against **zero** from two sweeps of this port's source. ***The catalogue is the only artefact in the repository that enumerates behaviour by condition rather than by implementation*** — `diagslice` ranks what is missing, `diagdeepen` what half-exists, `diagnode` where; none can name a condition nobody wrote a rule for. §476–§477 |
| 2026-08-09 | HEAD | **36.37%** | **1,996** | **+2 — a second unread compiler option** | **TS1203**, `export =` when emitting ECMAScript modules. `Checker` gained `module_kind` (`getEmitModuleKind`'s default) — the second option this session ported after §416's `noImplicitThis`, and the **same shape: the field existed on `CompilerOptions` and nothing read it**. ***`CompilerOptions` is ported ahead of its consumers, so an unread field is not a gap — it is a diagnostic nobody has asked for yet.*** Recorded as a third catalogue sweep (options), alongside message text (§467) and comments (§417). Message search: **seven rows, +20**. §478–§479 |
| 2026-08-09 | HEAD | 36.37% | 1,996 | **+0, reverted — the third unisolated zero, and all three are parser-side** | **TS1120**, `declare export = x`. The rule is dispatched and the insertion precedes every early return but one, so §402's fork narrows to two candidates: the parse-error gate, or **the parser not attaching `declare` to the `ExportAssignment`'s modifier list**. **Not isolated further** — that is a parser-side read, §424's boundary, and §446 measured the cost of crossing it on a guess (−14). ***Three unisolated zeros this session (§412, §424, §481) and all three end at the tree*** — every instrument here measures diagnostics; **none measures what the parser built**. §480–§481 |
| 2026-08-09 | HEAD | **36.39%** | **1,997** | **+1 — the message-catalogue search closes at +21 over eight builds** | **TS2337**, a `super` call outside a constructor — **§468's second arm**, six builds after the first, safe to add without re-reading it because ***§103's rule (the `else if` order is the specification)*** means arm one already returns for its own cases. Search totals: restrictions +12, prohibitions +9. **Fourth mechanical sweep to end on a measured total** — §421 zero, §444 zero, §473 +12, §483 +21 — ***and the two that paid both searched upstream***. The **options catalogue** (§479) is the one such artefact still unswept. §482–§483 |
| 2026-08-09 | HEAD | 36.39% | 1,997 | **the options catalogue swept and priced — a long tail** | **109 of `CompilerOptions`' 127 fields are unread by `tsr-checker`.** Joining option-gated upstream diagnostics against blocked rows yields **twelve rows of one to three cases each** (largest: TS1029, 3). The message catalogue's rows were three to seven and paid **+21** over eight builds. ***The two upstream catalogues are not equivalent*** — messages enumerate **conditions** (a missing one is a missing behaviour); options enumerate **switches**, and an unread switch usually gates behaviour that is rare in the corpus by construction. ***Sweep yield tracks what the artefact enumerates***, which is the qualifier §473's rule needed. §484 |
| 2026-08-09 | HEAD | **36.41%** | **1,998** | **+1 — and why four builds landed one under bar** | **TS2377**, a derived constructor with no `super` call — §470's neighbour in the same function, fifteen builds later. Under bar because four of five cases pair TS2377 with a *second* code. ***A row can be single-line per `diagslice` and multi-code per case*** — the column counts lines of the **blocking** code, and `diaggap`'s sole-obstacle population is computed **before** the build, so adding the missing line can move a case out of that population without converting it. Explains §455, §462, §473 and §486, all exactly one under bar. §485–§486 |
| 2026-08-09 | HEAD | **36.48%** | **2,002** | **+4 — the board passes 2,000 (25.0× from 80)** | **TS1192**, a module with no default export; `reportNonDefaultExport`'s second arm, with §186's empty-table decline holding. **Twenty consecutive builds, +62, no reverts — and every one came from reading upstream rather than from an instrument.** ***After fourteen sessions the instruments' job is to narrow, and upstream's source is what decides*** — a hundred rows had to be eliminated before a sentence in `checker.go` was worth reading, and the six instruments will not need a seventh. §487–§488 |
| 2026-08-09 | HEAD | **36.55%** | **2,006** | **+4 — the whole row converted, and why** | **TS1141**, a non-string `import(...)` type argument. Four blocked, four conversions — **second row this session to close completely** (§476 was the first), and both were **two-node shape tests**. ***The rows that close completely are the ones whose upstream guard has no conjunct this port cannot answer*** — neither consulted a type, a flag, or a symbol, while every partially-converted row had one that did, and the leftover cases are always where that conjunct decides. **Total conversion is a property of the guard, not of the row's size.** §489–§490 |
| 2026-08-09 | HEAD | **36.64%** | **2,011** | **+5 — six messages, three guards, one build** | The **`for…in`/`for…of` declaration-list grammar**: more than one declaration, an initialiser, a type annotation — three sequential shape tests, each with a `for…in` and a `for…of` message, shipped together per §230. Priced from **two** rows (TS1188, TS2404) and converted five. ***When upstream splits one guard's message by a kind test, the row you priced is a fraction of the build — and the fraction is the number of kinds.*** The right estimate: **count the messages, not the rows** — six were verified present before the build. Shape-test filter now **+13 over three builds**, two rows closed completely. §491–§492 |
| 2026-08-09 | HEAD | **36.77%** | **2,018** | **+7 — and a defect §492 had measured as a success** | **TS2491**, plus a fix to §491's `for_in` flag, which was **never true**: the node's own `SyntaxKind` distinguishes for-in from for-of, not the token field §491 read. §491 therefore shipped **six messages of which three could never fire**, emitting `for…of` text for every `for…in` — and **still measured +5 and met its bar exactly**, because every converted case was a `for…of` one. ***A bar met exactly is not evidence of a correct build.*** The missing falsifier: **a `for…in` and a `for…of` fixture asserted to produce *different* codes**. §493–§494 |
| 2026-08-09 | HEAD | 36.77% | 2,018 | **§494's differential check run — every arm fires, twelve rows closed** | Checked both message-splitting builds against a fresh `diagslice`: **TS1188, TS1189, TS2404, TS2483, TS2491, TS7011, TS7013, TS1114, TS1141 all at zero blocked cases.** ***A row at zero blocked cases is the strongest per-arm evidence available*** — it is the only signal distinguishing *this arm reported* from *this arm reported the other arm's code*, because a dead branch leaves its own row untouched while the sibling's falls, which is exactly what §491 produced and §492 read as success. **Twelve rows closed to zero this session**, five of them from one block once its kind test was right. §495 |
| 2026-08-09 | HEAD | **36.83%** | **2,021** | **+3 kept, −5 reverted — a bundle that hid both** | Two unrelated rules shipped together measured **−2**; split, **TS1021 is +3** and **TS1155 is −5**. ***Bundle only what shares a guard*** — §230 says ship every branch of *one* guard together, and the two failure modes are now both on the record: §491 **under**-bundled a check (one arm of a kind test, dead branch survived a green measurement) and §496 **over**-bundled a build (two guards, a good one hidden behind a bad one). TS1155's owner is **`NodeFlagsAmbient` again (§452)** — its 3 cases join TS1038's 6 and TS1359's 7: **sixteen behind one parser change.** §496–§497 |
| 2026-08-09 | HEAD | 36.84% | 2,022 | **`NodeFlagsAmbient` priced to an implementation point — 16 cases** | §452 deferred it at 13; §497 raised it to **16** (TS1038 6, TS1359 7, TS1155 3). The change is now specified: a **`context_flags` field on the parser**, saved/restored around a `declare` body, OR-ed in at `finish_node_with_flags` (`parser.rs:597`) — the existing `in_ambient_module` bool is in `references.rs`, a **separate pass**, and cannot supply it. **Still not built**: two suites at 100% read every node it touches and §446 measured **−14** from a cheaper approximation. ***A refusal that names an implementation point is worth more than one that names a subsystem*** — §441's prediction, with the counter-example written out. §499 |
| 2026-08-09 | HEAD | **36.86%** | **2,023** | **+1 — the other arm of an `if/else` built sixty sections later** | **TS2465**, `this` in a computed property name — the `if` whose `else` §392 ported as TS2331, so §392's rule had to be amended to decline where this fires. ***An `if/else` in upstream is a contract between two builds that may be months apart*** — §392 was recorded as *"TS2331, the module arm"*, which is accurate and does not say *"the other arm is unbuilt"*. **A build that ports one arm of an exclusive pair should say which arm it did not port** — §230's rule for branches shipped apart on purpose. Four such pairs listed, two still half-built. §500–§501 |
| 2026-08-09 | HEAD | 36.86% | 2,023 | **both unbuilt siblings checked — zero cases each; the four pairs close** | §501 named two arms it had not ported. Checked before building: the `super` switch's third arm (**TS2335/2336/2338**) and the constructor's root-level arm (**TS2401**) have **zero blocked cases each**. §293's filter declines both, at the cost of one grep against a table already on disk. ***Recording the unbuilt sibling is worth doing even when the sibling is worthless*** — the payoff is not a build, it is that the check was **possible**; unnamed, the next session would have re-derived both arms from upstream before finding the corpus does not exercise them. **All four pairs now complete or complete-for-the-corpus.** §502 |
| 2026-08-09 | HEAD | **36.90%** | **2,025** | **+2 (row closed), and `checker_types` +19 — a third cross-suite mechanism** | **TS1245**, an abstract method with a body — three shape conjuncts, both cases converted. The **+19** is the session's largest cross-suite move and has a *third* mechanism: not §350's shared predicate (neutral) nor §437's new expression evaluation (+14), but a **new dispatch arm** putting method declarations through the generic per-node section for the first time. ***`checker_types` moving is information, not noise, and its direction is not the falsifier — `passing→failing` is.*** **+108 this session without once aiming at it.** §503–§504 |
| 2026-08-09 | HEAD | **36.92%** | **2,026** | **+1 — and the queue's head is now three cases** | **TS1242**, `abstract` on a kind that cannot carry it; the *inner* nested test is a different code and is **named as not ported** (§501's rule). ***The queue's head was forty-six cases at this session's start and is three now*** — every row above three has been converted, closed, or given a named owner. **The cheap-and-large rows are gone; what remains is cheap-and-small or large-and-owned**, which makes **§499's sixteen-case parser flag the largest single buildable item on the board.** §505–§506 |
| 2026-08-09 | HEAD | 36.92% | 2,026 | **§499's handoff number verified before handing it over** | §506 called the parser-flag change the largest buildable item and passed forward **sixteen cases**. Checked per §486: **TS1038 6, TS1155 3, TS1359 7 — fifteen single-line sole-obstacle, and the sixteenth wants three TS1359 lines that one correct rule supplies.** The number survives. ***A handoff number should be checked by the session that writes it, not the one that receives it*** — this session has now audited three of its own forward-looking numbers (§420, §431, §507) and two were wrong. `asyncOrYieldAsBindingIdentifier1` needs **both** `AWAIT_CONTEXT` and `YIELD_CONTEXT`, making it the change's acceptance test rather than an outlier. §507 |
| 2026-08-10 | HEAD | **37.01%** | **2,031** | **+5 — a "parser change" that was a one-line predicate swap** | **TS1038** closes. Three predicates tried on one guard: `file_is_ambient` (too narrow), the **walk-threaded `ambient`** (§445, **−14** — true of every node in a `.d.ts`), and **`declaration_is_in_an_ambient_context`** (+5 — an enclosing `declare`, and only that). ***§499 priced this as a parser change and it was a one-line predicate swap*** — §446, §452 and §499 all reasoned about the **flag** and none re-examined the predicate already in the file, **documented since §99 as the substitute for `NodeFlags::AMBIENT`**. §507's sixteen is now **six done, three unknown, seven parser**. §508 |
| 2026-08-10 | HEAD | **37.06%** | **2,034** | **+3 — TS1155 closes; §497's −5 was one arm of three** | Removing the **`const` arm** turned the same rule from a five-line loss into a three-case gain. **Two refusals overturned in three builds by the same move** — §508's *"parser change"* was a predicate already in the file, §509's *"parser flag"* was arithmetic inside a three-arm switch. ***Both blamed a missing subsystem and both were inside a rule already written.*** The forcing function was **§501's discipline — name the arm you did not port**; without §497 having written that down there would have been nothing to subtract. §507's sixteen corrected: **nine needed no parser work; seven remain.** §509–§510 |
| 2026-08-10 | HEAD | **37.12%** | **2,037** | **+3 — TS1120 closes; the session's first parser change, rails held** | `parse_export` hardcoded an **empty modifier slice** while its caller held the real one — and the caller's comment names *"declare export = value"* as the recovery shape. Four call sites arena-allocate a slice they already had; **both parser rails stayed at 100%** because the slice is empty for every legal `export = x`. ***Three subsystem-named refusals, three local fixes, +11*** (§508 a predicate, §509 an arm, §511 an argument). ***A refusal that names a subsystem is not specific — it is a guess about where the work lives, measured wrong three times running.*** §511–§512 |
| 2026-08-10 | HEAD | **37.17%** | **2,040** | **+3 — the parser-flag family finishes at 15 of 16, with no flag set** | **TS1359**: `AWAIT_CONTEXT` is a **context**, and a context is a property of the ancestor chain. ***Every one of the sixteen cases §452 attributed to three unset parser flags came in without setting any of them*** — §508 a predicate, §509 an arm, §511 an argument, §513 a walk. §452 called it *"thirteen cases behind one parser change"*, §499 priced it to three numbered steps, §506 called it the largest buildable item on the board; **all three described work that did not exist**. Remainder: **one case**, wanting a second walk for `yield`. §513–§514 |
| 2026-08-10 | HEAD | 37.17% | 2,040 | **−4, reverted — `yield` is not `await`'s mirror, and §514 was too strong** | §515 added TS1359's `yield` arm as the symmetric case and measured **−4**. ***`async` propagates into nested arrows; `yield` propagates through nothing*** — a plain function inside a generator is **not** in yield context, and an ancestor walk cannot see that boundary. Measured correction to §514: **`Ambient` propagates through everything (walk works, +5); `AwaitContext` through arrows only (walk works, +3); `YieldContext` through nothing (walk fails, −4).** ***A context is walkable when its propagation rule is "everything below"*** — §514 claimed the general case after two successes, one measurement early. §515–§516 |
| 2026-08-10 | HEAD | **37.21%** | **2,042** | **+2 — a guard inserted *above* one ported nine builds earlier** | **TS1492**, a `using` declaration with a binding pattern — `checkGrammarVariableDeclaration`'s **first** guard, which returns before the initialiser guard §509 ported. ***A function's guards are a sequence, and porting them out of order is only safe if you re-establish the order each time*** — the insertion cost three lines because §509 was written as an early-return **chain**. ***Writing a ported guard as a chain, not a set, is what makes the next guard cheap to add*** (§491 chain → §493 one-line fix; §496 set → full revert). The `using` grammar family is now closed. §517–§518 |
| 2026-08-10 | HEAD | **37.24%** | **2,044** | **+2 — the `this` family completes; seventh repair of the shared tree** | **TS7041**, an arrow capturing the global `this` — §416's unnamed neighbour, the arm immediately above it. **§501's discipline should have caught this at §416 and did not**; both notes predate it. Four arms of `checkThisExpression` are now present and mutually exclusive in upstream's order (§392, §416, §500, §519). `main` arrived red again (a `doc_markdown` lint in the other workstream's `inference.rs`) — ***every repair has been a one-line fix, and every one blocked a measurement***, which is the failure §260's gate cannot prevent from the other side. §519–§520 |
| 2026-08-10 | `a7d10a54` | **gradient 85.55%** | right **409,748**/478,954 | **checker-1's stale-refusal window: five landings, +577 right, two parked entries unstuck** | The user directive "re-evaluate stale refusals now that binder/parser are 100%" paid five times. **§118** the types harness honours `@symlink`/`@link` (+46/33 — the `case.rs` `@link` parse was MISSING, upstream's `linkRegex` semantics `A -> B` links B→A; the wrongs are the recorded per-file import-spelling head). **§119** §113's ES-import arm RE-RUN post-§118: +235/40 at 5.9:1 — the twice-confirmed refusal was stale because its entire adverse class (symlink corpora) became findable; ramdaToolsNoInfinite2 130 whole. **§120** §92's written-intersection gate HALF-stale: single-hit distribution +172/31 at 5.5:1 over four iterations; multi-hit stays refused, twice-priced. **§121** purely-nullish receivers answer upstream's deliberate error-any (+47/6 gated; ungated 64:36 fired the unknown-narrowing falsifier exactly). **§122** statics inherit through the anonymous side's base walk (+77/0; `#`-private exclusion). Two fixture pins came due and were renamed (twenty-ninth, thirtieth). ***The rtk exit-code trap fired twice; `rtk proxy cargo test` is the verified form.*** Residual adverse named: tsconfig-unit machinery (paths/self-names) is the loader entry's remaining half, parked, re-priced upward. checker-2's parallel window: callres2 slice 1 (+723) and the InferenceInfo foundation stone (byte-identical). §118–§122 |
| 2026-08-10 | HEAD | 37.24% | 2,044 | **−26, reverted whole — a per-symbol loop is not a per-declaration test** | **TS2390/TS2391** ported `checkFunctionOrConstructorSymbol`'s trailing guard as a visitor arm; barred +4, measured **−26**. Three mechanisms, each found by reading a baseline: `.d.ts` is ambient and §508's helper does not say so (***§445 too wide, §521 too narrow — the same missing distinction from both sides***); the `subsequentNode` block is an early **return** to TS2389, not a refinement; and ***a declaration with no symbol is never visited at all***. The last is general: ***porting a `for symbol := range` as a node visit silently widens the domain, and no guard-matching closes that gap — the guards are inside the loop***. **Twenty-fifth whole revert and the first whose mechanism was *domain* rather than *predicate*.** Owner: the symbol-driven form, which also carries TS2389/2392/2393/2394. §521–§522 |
| 2026-08-10 | HEAD | **37.32%** | **2,048** | **+4 by deleting one line — and §143 skipped twice** | **TS2390/TS2391 close.** §521 and §523 both set out to build `checkFunctionOrConstructorSymbol`; ***it has been ported since §14 and complete since §64***, thirty lines from where §521 inserted a duplicate — which is the largest term in that build's −26. The four cases were held out by `if self.file_has_parse_errors { return }`, a **port-local** guard with no upstream counterpart. ***A port-local guard is a decline that no longer names its own bar***: right for rules whose input is a type, wrong for one whose input is a declaration list and a body-or-not, which error recovery preserves. §522's general claim stands; **its prescription was wrong and is corrected**. `extraonly` measured 84/84 across the build. §524–§525 |
| 2026-08-10 | HEAD | 37.32% | 2,048 | **A sweep of 111 sites priced in four runs and closed at ~zero** | §525 opened the `file_has_parse_errors` sweep at *unknown*. Measured: all guards off is **+4 but takes extra-blocked cases 56 → 71**; the halves give +1 and +2, ***not additive***, so the guards interact and no single removal has the effect it has alone. ***The blanket probe is the right way to price a sweep and the wrong way to take it.*** §524 removed **one** guard for the same +4 with `extraonly` unchanged — so ***the whole prize was ~+4 and §524 already took it***. **A hundred and ten candidate rows priced and closed by three measurements** rather than left standing as a cheap-wins note. §526 |
| 2026-08-10 | HEAD | **37.35%** | **2,050** | **+2 — the third ordering defect in ten builds** | **TS6142**: `.jsx` asks `needJsx` before `needAllowJs`; §359 grouped it with the extension family it *looks* like rather than the check sequence it belongs to. With §517 (+2) and §524 (+4) that is ***eight cases from order rather than logic***, in rules whose predicates were already correct and already measured. ***A ported guard that measured `+n` is evidence about the guard, not about its position*** — and nothing in the loop measures position, because a mis-ordered guard that **silences** rather than misfires costs zero and looks exactly like a row nobody has tried. New falsifier for switch-shaped rules: assert two **adjacent** discriminator values give **different** codes. §527–§528 |
| 2026-08-10 | HEAD | **37.39%** | **2,052** | **+4, double the bar — a rule reached by recursion is two rules** | **TS2694** for `D.inner.Class1`: §187 declined anything deeper than `A.B`, and ***the baseline column named the arm*** — upstream reports on the **middle** segment. The first form measured **−1**: the `canSuggestTypeof` arm reports on the **whole** name, so asked of an inner segment it produced three wrong TS2749 lines. One `outermost` flag → **+4, zero wrong lines**. ***Every arm whose error node is the node the entry was called with changes meaning when the entry changes.*** With §522 and §528 that is three ways a correct predicate lands in the wrong place, each with a written test. §529–§530 |
| 2026-08-10 | HEAD | **37.43%** | **2,054** | **+2 against a bar of +4 — and the missing half names one field** | **`libReplacement`** built in the *loader*, not the checker: `pathForLibFile` resolves `@typescript/lib-dom` as a module so `window` is undefined after `/// <reference lib="dom" />`. ***A missing diagnostic whose whole family shares one column is usually not a rule*** — four TS2304 lines all at `(6,1)` were one unread option. The two that stayed are the `Config` variants: upstream resolves from the **tsconfig's** directory and ***`config_file_path` is empty for every case in this suite***, so anything resolved relative to it is wrong the same way. **Recorded as unpriced** (§526's warning). `bd tsr-9or.5` scope reduced, not closed — the trace replay is untouched. §531–§532 |
| 2026-08-10 | `fd2d83e3` | **gradient 85.65%** | right **410,188**/478,954 | **checker-1's window closes: the member-miss seam mined whole, §123–§125** | **§123** the completed-walk property miss answers TS2339's error-any (+381/69 at 5.5:1 — §34's "gapped by design" was an unmeasured design claim; two gates: class/interface-declared owners only, non-JS positions; residual adverse is the narrowing-miss class, named per case). **§124** the Anonymous side's analogue (+83/2 at 41:1 over FOUR iterations, each converting an adverse class into a gate: FUNCTION owners out, static-index-signature chains out, Function-family/Object names never established-absent, CONST_ENUM skips that gate). **§125** the union receiver's established miss REFUSED at 39:164 — the class is narrowing-owned whole (instanceof/is-type/assertion guards, not discriminant-gateable), reverted byte-identical; re-open only after the predicate-narrowing legs land, citing the pair. Three more pins flipped (thirty-first through thirty-third). member_shapes rows 5/6: 2,384 → ~1,700 across the window; every remaining concentration names narrowing or tsr-4qx. Window total §118–§125: **+1,123 right, 85.40% → 85.65%**, one refusal priced, zero red gates shipped. §118–§125 |
| 2026-08-10 | HEAD | **37.46%** | **2,056** | **+2 here and +8 to the other workstream — a missing *input*, not a missing rule** | The diagnostics/`.types` producer built every program from `CompilerOptions::default()` + `@`-directives and ***never read a case's `tsconfig.json` at all***; `trace_case::compilation` has had the branch all along. Probed, not guessed: an `eprintln` printed `cfg=""` on a fixture that has one. ***The fourth kind of gap this session — after the predicate (§445), the domain (§522) and the order (§528), the absent input*** — the cheapest to fix and the hardest to see, because every rule downstream of it is correct in isolation. Both numbers are a **stash/unstash differential**, since the other workstream lands continuously. **`checker_types` was not touched; its input was.** 267 corpus cases carry a tsconfig; **no estimate offered** (§526). §533–§534 |
| 2026-08-10 | HEAD | **37.52%** | **2,059** | **+3 here, −2 to the other workstream — and the −2 is the interesting number** | §533 took the config's *options*; **§535 takes its *root files***, which `trace_case::compilation` has always done and this producer never did — a case whose config named three of its five units compiled all five, and the `tsconfig.json` was itself a program root. ***A harness change that costs cases is only a loss if the harness got less faithful*** — these two `checker_types` cases were passing on a program built from the wrong root set and ***stopped being credited for an accident***. The test is not *did the number fall* but *is the input now the one upstream uses*. Named in the snapshot diff for their owner; **nothing in `tsr-checker` touched**. §535–§536 |
| 2026-08-10 | HEAD | **37.63%** | **2,065** | **+6 here, +5 there — three missing producer inputs, +22 in total** | §537 wires `harnessutil`'s no-config root heuristic (a `require(` or `/// <reference path` in the **last** unit makes it the only root). With §533 (options) and §535 (root files) that completes what `trace_case::compilation` has always computed: ***+11 diagnostics and +11 `checker_types` from three inputs the harness already knew how to produce***. ***The most expensive thing this session was not a hard rule — it was a second implementation of a function that already existed***: §521 paid **−26** for one, §533–§537 were paid **+22** for another. **Nothing checks the harness** the way `xtask anchors` checks rules (2,698 references, 0 unresolved), which is why three inputs sat missing during a session auditing match arms one at a time. **Standing recommendation: diff `types_producer.rs` against `trace_case.rs` before the next rule.** §537–§538 |
| 2026-08-10 | HEAD | 37.63% | 2,065 | **+0 and +0 — the producer-input vein priced to exhaustion** | §538's standing recommendation, discharged immediately rather than deferred: `@currentDirectory` (**50 corpus cases**) and `@useCaseSensitiveFileNames` (9) were both dropped by the producer; both wired, **both worth zero**. Kept for fidelity — they move no suite and the next reader should not have to rediscover that `InMemoryFileSystem::new(…, true)` was a guess. Five inputs total: ***three worth +11/+11 and two worth nothing, with no way to tell which from the case counts*** — `@currentDirectory` appears in 50 cases and converted none; the no-config root heuristic has no directive to count and converted six. ***A harness input's corpus frequency does not predict its yield.*** One difference is deliberate and must be preserved: the current-directory **default** (`/` here, `/.src` in `trace_case`), because the two suites' baselines differ. §539–§540 |
| 2026-08-10 | HEAD | **37.72%** | **2,070** | **+5 over three builds — a predicate written twice, widened once** | **TS2364 closes.** Three blockers, none of them the rule's logic: the port-local `file_has_parse_errors` bail (§541, +1), the rule's `EqualsToken` test where upstream has `isAssignmentOperator` (§542, **+0**), and ***the dispatch carrying the same test one level up*** (§543, +4) — **§380's *dispatch never called*, for the fourth time**. ***A predicate written twice is a predicate that will be widened once***: the outer copy is a filter and the inner a selector, identical until one must change, and nothing checks the pair. Grep-testable: *a rule whose first `match` arm restates its dispatch guard*. **§541's bar shortfall is what exposed the other two** — the second time this session a missed bar beat a met one. §541–§543 |
| 2026-08-10 | HEAD | 37.76% | 2,072 | **A grep sweep discharged: three arms, one defect, +0 — and kept** | §543's proposed sweep, run immediately. The visitor has three guarded `BinaryExpression` arms and ***a `match` is exclusive***: `is_numeric_binary_operator` contains `-=`, `*=`, `/=`, `%=` and comes **first**, so those never reached the assignment arm and never saw `check_reference_expression`. ***Arm order is a guard nobody writes down*** — a `match` whose arms overlap encodes a precedence no comment states and no test asserts, and the only symptom is silence in the shadowed arm. **§543 found its defect because a row stalled at 4 of 5; this one had no row at all.** Measured **+0** (no corpus case targets a non-reference with `-=`), **kept for fidelity** — third such build this session, against twenty-five reverted for measuring negative. §545 |
| 2026-08-10 | HEAD | 37.76% | 2,072 | **+0, reverted — and a reasoning error corrected in the note that made it** | **TS2341**: `diagcase` shows the case's two TS2564 lines converting and only TS2341 missing, and two `eprintln` probes inside the rule ***printed nothing***. §546 read that silence as bracketing the decline to one guard, proposed §508's fix, and measured **+0** with `total missing TS2341 lines` unchanged. ***A probe that prints nothing is a measurement only if you know it ran*** — the conclusion followed from the premises and there was no positive control showing the probe would have printed had it been reached. The claim that this was a third appearance of §445's `ambient` defect is **withdrawn**: two appearances and one guess. Recorded as a refusal with a **corrected** diagnosis, because ***a wrong lead costs the next session more than no lead***. §546 |
| 2026-08-10 | HEAD | 37.76% | 2,072 | **TS2341 diagnosed to a named owner — and the probing method corrected** | The positive control §546 asked for was free: ***39 baselines carry TS2341 and 11 lines are missing***, so the rule converts 28 elsewhere and cannot be dead. A staged probe then pinned it: **`declared_members_are_complete` is `false`** for a class merged with a namespace — the port-local conservative gate, working as written. ***A probe placed by textual match is a probe placed somewhere***: §546's silent probes had landed in a **different function**, because three functions in that file open with the same four lines. Row now owned precisely: **2 cases** need the destructuring entry, **3** need the type side's merged-static representation. **No build attempted; the finding is the deliverable.** §547 |
| 2026-08-10 | `f1a09ea3` | **gradient 85.67%** | right **410,303**/478,954 | **checker-1 continues: the CALL flow-node seam opens, §126–§128.1** | **§126** the instanceof FALSE branch (+27 at 27:1 over three iterations — §83's "global var vs parameter" observation was the LITERAL discriminator: top-level script vars keep the whole union in the else, parameters narrow by derivation; the middle iteration was a gate bug reading as byte-identical). **§127** assertion calls narrow at CALL flow nodes (+17 at 8.5:1 over four iterations; upstream's bare `asserts x` narrows by the ARGUMENT AS CONDITION, and the syntactic pre-gate keeps mid-walk callee typing from perturbing creation-order prints). **§128** REFUSED at 4:13 then REVERSED BY ONE READ (+6/0): the never answer is a sentinel converted to the DECLARED type at the walk's exit (flow.go:111) — the cheapest un-refusal on record. **§128.1** this/method assertion callees (+3/0). controlFlowOptionalChain moved for the first time (7 W→R). The stale comment "assertion signatures need call resolution this checker lacks" is retired. §126–§128.1 |
| 2026-08-10 | HEAD | **37.97%** | **2,084** | **+12 against a bar of +10 — the cleanest build of the session** | **TS2433/TS2434** close: a namespace merged with a class in another file, or written before it. One predicate and a two-way `else if`, so ***§497's bundling test passed on the same evidence it did at §521*** — and this time the rule really was absent. ***A bar built from two "blocked alone" counts under-counts by exactly the cases the two codes share***; §521's intersection was negative, this one's positive, the difference being that these arms are genuinely exclusive. No probe, no revert, no correction: everything it needed was already ported by four earlier notes (`is_instantiated_module`, §508's ambient helper, §523's `symbol_of`). §548–§549 |
| 2026-08-10 | HEAD | **38.03%** | **2,087** | **TS1344 to zero lines for +3 — and the bar came off the wrong column** | `NINE: var y = 12;` — **a binder diagnostic**, and `tsr-binder`'s own docs say its strict-mode diagnostics are unported, so ***a code whose upstream producer is a declined subsystem is invisible to every scan that starts from the checker***. Built checker-side because the suite compares `(file, line, column, code)` and **not the producer**. Every TS1344 line converted, board moved 3: ***`cases` and `cases blocked alone` are different questions and only one of them is a bar***. With §549's mirror image the rule is now complete — `cases` over-counts by cases with other blockers; two codes' `alone` under-counts by their intersection. §550–§551 |
| 2026-08-10 | HEAD | **38.10%** | **2,091** | **+4, bar met exactly — the binder's twenty-two messages priced to two rows** | **TS1102** (`delete x` on a bare identifier) built in one measurement, because ***§156 had already established the guard that isn't there***: `binder.Binder` has no `inStrictMode` field and all seven sites dispatch unconditionally, with `parserStrictMode3-negative.ts` as the falsifier. First bar set from `cases blocked on X alone` **because §551 said to**, and it landed exactly. ***A subsystem's own note about what it does not do is not a statement about what the port does not do*** — `tsr-binder`'s "none of that is ported" described a crate, not a capability, and the family was two rows and eighteen zeroes. TS1212 (2 cases) left: needs the reserved-word table. §552–§553 |
| 2026-08-10 | `808a0dd7` | **gradient 85.67%** | right **410,326**/478,954 | **checker-1: §129 mooted in flight, §130 lands — and a sizing rule earns its name** | **§129** (types harness parses tsconfig units) was MOOTED between bar-commit and first edit — the THIRD session's §533–§539 landed the identical seam; visibility edits reverted unlanded, byte-identical verified; first true two-lane collision, kept a no-op by bar-to-main-first (attribution corrected in-record: initially mis-named checker-2). The LOADER entry is now BUILT ON BOTH HALVES — re-census before pricing anything against tsr-9or.1. **§130** the missing-export alias answers TS2305's any (+23/2 at 11.5:1; two gates: `default` rides interop, non-identifier keys spell doubly; residual 2 = type-only export specifiers the binder doesn't file, named owner). **THE SIZING RULE, now four landings wide** (§121, §126, §128, §130 all under-count): want-any census rows mix mechanisms and each arm takes only its own — size bars by MECHANISM SAMPLE, not census bucket. Thirty-fourth pin flipped. §129–§130 |
| 2026-08-10 | HEAD | 38.10% | 2,091 | **−1, reverted — a guard probe that named three outcomes and got the lever wrong** | **TS1212**: §143 first showed the rule is *fully ported*, so §554 attacked its `file_has_parse_errors` guard — and ***unlike §524's and §541's, this guard is upstream's***, transcribed from `binder.go:1303`. Measured **−1**: the two ASI lines ***did not move*** and two wrong TS1213 lines appeared. ***A hypothesis with three named outcomes still has to be right about which lever it is pulling*** — §554 inferred from the fixtures' family name that the recovery guard blocked them; the guard is innocent of those two and guilty of two others. Fourth guard probe this session and the first to fail; **the tell was visible beforehand — whether the guard has an upstream counterpart**. Owner for TS1212's residue: **unidentified**, stated rather than guessed. §554–§555 |
| 2026-08-10 | HEAD | 38.10% | 2,091 | **+0 and −4, measured separately — and the split is why that is readable** | **TS2694**: §556 built `resolveEntityName` over a qualified left (**+0**, no wrong lines, **kept for fidelity**) and, separately, the import-equals entry (**−4**, extras 54 → 62, reverted). ***Built together they would have measured −4 with no way to tell which half was wrong*** — §496's exact failure. Why part 2 over-fires: ***two syntaxes that ask the same question of the same tree can need different answers, because the *meaning* they resolve under differs***; upstream's `resolveEntityName` takes a `meaning` parameter and §556's resolver hardcodes the namespace one. Row's owner sharpened: TS2694 needs **the meaning parameter**, not a deeper resolver. Fourth +0 kept for fidelity this session. §556–§557 |
| 2026-08-10 | HEAD | 38.10% | 2,091 | **A row that needs two changes reads as two failed builds — compare the extras, not the totals** | **TS2694**: a probe showed `c.a.b`'s exports are ***legitimately empty*** (`import ma = a` without `export` is a local), which is why upstream reports and why §186's empty-table decline suppresses it. Narrowing that guard measured **−1** where the entry alone measured **−4** — ***with identical extras (54 → 62 both times)***. **The narrowed guard converts three lines; the entry produces eight wrong ones regardless.** New instrument reading: ***identical extras across two variants means the extra-producing change is common to both and the variable under test is innocent of them.*** Row fully owned by a **pair** — §557's meaning parameter and §558's guard narrowing — to be built together with the bar set on the pair. §558 |
| 2026-08-10 | `81c0026f` | **gradient 85.78%** | right **410,827**/478,954 | **THE DAY CLOSES — both lanes verified composed on one board** | checker-1's §131 (+10/0: a DEFAULT import whose module cannot have a synthetic default — TS files never do without `export =`, `can_have_synthetic_default` answering the TYPE question its TS1192 diagnostic build already knew; Node16/NodeNext gated whole, the mode road unmodeled) rode directly on checker-2's SUMMIT (+494 right / −193 wrong / −301 gap at 18a318b1). Clean-tree verification: no transitions vs baseline at 410,827 — the two lanes' day composed EXACTLY, fourteen checker-1 landings (§118–§131) + the summit + the third session's harness block, 84.63% → **85.78%**, +1.15 points, the largest single day on record. checker-1's fresh-board heads: ALIAS entity forms (ImportEquals 205, resolver-parity-owned), per-site printing (3 heads), the §110 lib-member residue; inferTypePredicates and temporal largely came home with the summit — re-census before touching. §118–§131 |
| 2026-08-10 | HEAD | **38.19%** | **2,096** | **+5 — the extras did not shrink, they *moved*** | **TS2694** falls 9 missing lines → 4. Three measured steps: the import-equals entry alone **−4** (extras 54 → 62, all TS2694); **+ the meaning on the membership test −1** (extras 54 → 62, ***all TS2749***); + the meaning on `canSuggestTypeof` **+5** (extras back to 54). ***A wrong-line count that holds steady while its composition changes is a fixed bug and a new one, not a null result*** — §558's *compare the extras, not the totals* is what made step two legible. Both arms were type-position-only and an import-equals may name a **value**. ***§547's positive control ended a thread three notes had each diagnosed wrongly***: they assumed the recursion because `c.a.b.ma` is four deep; every wrong line came from `m.m`, two deep, on the old path. §559–§560 |
| 2026-08-10 | HEAD | **38.23%** | **2,098** | **+2 — TS2694 falls from nine missing lines to one** | §558 narrowed §186's empty-exports decline on the branch it wrote and ***left the original copy untouched***; `var foge: N.S` takes the two-deep path. ***A guard duplicated across two paths must be narrowed on both, and nothing makes the second copy visible when you edit the first*** — §543 said this of a **predicate** written twice (+4), §561 says it of a **decline** written twice (+2). Row end to end: **+11 over four sittings, and ***not one change was the rule's logic*** — every one was to something the rule was handed (its entry, its meaning, its inherited decline). ***A rule that reports the wrong thing is rare; a rule asked the wrong question is the normal case.*** Also: ***check the residue before probing it*** — three probes ran against a fixture that had stopped failing two builds earlier. §561–§562 |
| 2026-08-10 | HEAD | **38.30%** | **2,102** | **+4 — TS2310 from fourteen missing lines to one, with no types touched** | `interface I5 extends I5` and the `i8`/`i9` cycle. ***A rule written against the type graph is not necessarily a type rule***: `hasBaseType` reads as type-system machinery and is, for interfaces and classes, the transitive closure of `extends` — thirty lines of symbol reachability. The one wrong line was `class C extends A implements B extends C`, ***a recovery fixture this parser accepts and upstream rejects***, so the parse-error guard was false here and true there; fixed by transcribing a **cardinality** — `getEffectiveBaseTypeNode` returns one node, so a class has one base however many clauses a recovered tree carries. ***When a recovered tree produces a wrong line, the fix is usually a bound upstream already has, not a test for the recovery.*** §563–§564 |
| 2026-08-10 | HEAD | **38.34%** | **2,104** | **+2 — §186's decline had a *third* copy, and a string literal is a legal export name** | **TS2305** falls 13 missing lines → 9. `import { "missing" as x } from "./empty"` tripped **two** declines in one rule: the empty-exports guard (third copy — narrowed here with a `SourceFile` witness) and a hard `ModuleExportName::Identifier` match that ***drops the string-literal export names ES2022 made legal***. Built as a pair per §558. New rule: ***halves that gate different cases should be split and read; halves that gate the same cases should be bundled, accepting that you will not know the split***. Bar missed because ***`cases blocked alone` predicts the board and says nothing about the work*** — the residue is one case worth six lines. §186's decline now narrowed in all three copies, **found three different ways**. §565–§566 |
| 2026-08-10 | HEAD | 38.34% | 2,104 | **+0 with *nothing moving*, reverted — the boundary on when a +0 is worth keeping** | §567 followed `export =` to a namespace target for **TS2305** and ***not one line moved***, so the new path is not demonstrably exercised and may be dead. ***A +0 is worth keeping when you can show the port now does what upstream does; it is not worth keeping when all you can show is that nothing broke.*** The session's four earlier +0 keeps (§539 ×2, §545, §557) each have a demonstration independent of the board — a directive read, an arm reached, a shape transcribed; this had only the board. ***An unexercised branch replacing an explicit decline is a net loss***: the decline documented a known limitation, the branch is a claim nobody can check. `namedImportNonExistentName` needs the **type** side for four of its six lines. §567–§568 |

## 8. Updating this file

**At the end of every session**, whoever ran it updates §1 (numbers + the
commit they were measured at), §3 (what landed), §4 (re-score from a fresh
`depend.rs` run; move finished items off, move measured items up from §4.3),
§5 (anything newly refused, **with its number**), and appends one row to §7.

**On §4's scores:** `reachable` is measured and must be re-taken, never
carried; `effort` and `feasibility` are judgement and should be argued with
rather than inherited. If an item lands, record its *actual* conversion against
the `reachable` it was scored on — that ratio is what keeps the 15–57% band in
§4.1 honest.

If a number here turns out to be wrong, **correct it and say so** — do not
silently edit. Three of this project's most expensive mistakes were numbers
that were true of a different population than the one they were quoted about.

