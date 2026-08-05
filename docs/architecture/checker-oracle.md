# The checker's oracle (`checker_types`, `diagnostics`)

**Status:** wired, and non-zero for the first time on 2026-08-05.

```
checker_types    571/9538      5.99%   the type of every expression
                 gradient     34.06%   ...share of individual assertion lines
diagnostics       80/5488      1.46%   every diagnostic, by code and position
```

Movement, all of it 2026-08-05, and all of it in the order the histogram ranked:

| | cases | gradient |
|---|---:|---:|
| the producer is wired | 278 | 22.39% |
| binary operators (`bd tsr-4sc.13`) | 313 | 28.72% |
| named types (`bd tsr-4sc.7`) | 554 | 33.57% |
| anonymous object types | **571** | **34.06%** |

Two ranked items, +11.67 gradient points and +293 cases. That is the evidence
that the ranking is any good, and it is why the next item should be re-derived
from the histogram rather than from this list.

### The first non-zero number was mutation-tested before being believed

This document has said since the suite was written that *"the first time this
suite is non-zero, mutate it before believing it"*. Done, through the **whole**
path — checker to renderer to judge — not just the judge:

| mutation | cases | gradient |
|---|---:|---:|
| *(none)* | 278 (2.91%) | 22.39% |
| every type rendered as `any` | 231 (2.42%) | 14.24% |
| declaration names no longer typed | 67 (0.70%) | 16.50% |

Both move both numbers, so the path is live and responds to checker correctness
rather than to the harness. Worth noting that rendering *everything* as `any`
still passes 231 cases: a great many corpus cases are error cases whose types
genuinely are all `any`. That is not a flaw in the suite — it is why the case gate
is the one that matters and why 2.91% is not 231/9538 of "working checker".

The second `checker_types` row is the **per-assertion-line tally**
([ADR-0031](../adr/0031-a-gradient-beside-the-gate.md)): the 9,538 judged cases
carry 478,954 assertion lines between them, and how many of those match is
reported beside the case rate and never in place of it. The case rate is the
gate; a case passes only if all of its lines do, so the gradient is always the
more forgiving number. It exists because a binary gate over 60,269 lines of
upstream gives one bit of feedback per case and reads 0% for months.

**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`.

## Why this was built before the checker

`internal/checker` is 60,269 lines — a third of the core port, and 120–200
sessions by PLAN.md's estimate. Starting that without a gate means months of work
before the first honest number.

Nothing in this repository currently measures a type. `binder_symbols` reads
98.03% and that is *symbol tables* — name resolution, declaration merging, scope
walking. A checker can be arbitrarily wrong while every symbol resolves.

This repository keeps relearning that the instrument comes first:
`dts_reachable_target` sized the emitter's target before an emitter existed, and
[ADR-0021](../adr/0021-isolated-declarations-is-not-a-port.md) committed to a
falsifier that could fire on the measurement alone. The same applies here at ten
times the scale.

## The target

Measured at the pin across `compiler/` and `conformance/`:

| | |
|---|---|
| `.types` baselines | **12,155** |
| positioned type assertions in them | **594,122** |
| cases this suite judges | **9,538** |
| assertions in the cases it judges | **478,954** |

The last row is new (2026-08-05) and is the gradient's denominator. It is smaller
than 594,122 for exactly the reason the case count is smaller than 12,155 — the
skips below — and the two shrink together, because a skipped case is out of both
denominators. Nothing was corrected here; the figure had simply never been taken.

(The `diagnostics` figure quoted above moved from 54/5,488 to 80/5,488 between
2026-08-05 sessions, as binder over-report fixes landed. That is the number
changing, not a number having been wrong.)

Upstream writes `.types` and `.symbols` in lockstep — 12,155 of each, at the same
positions. That pairing is worth knowing when the checker lands: `.symbols` tests
*resolution* and `.types` tests *inference*, so a failure in one localises against
the other.

The 12,155 baselines and 9,538 judged cases differ for the reasons the suite
prints, and the difference is files-versus-cases as much as exclusions — the same
reconciliation as the `.d.ts` denominator:

```
 1397  configuration-varied baseline (bd tsr-bb4.1)
  924  upstream recorded no .types baseline
  450  upstream records a known divergence from TypeScript
  135  the .types baseline has no assertions
```

## What a pass will mean

Every `>expression : type` line reproduced **verbatim, in order, for every file of
the case**.

Whole-line, not expression-and-type separately. A line is
`>{expression} : {type}` and **the expression can contain `" : "`** — a
conditional does:

```text
>Math.random() > 0.5 ? "abc" : "def" : "abc" | "def"
```

773 assertion lines in `compiler/` alone carry two or more, so there is no
unambiguous split. Rendering the line ourselves and comparing strings sidesteps
that and is the stricter check. `TypeAssertion::split` exists only to make a
failure readable, and its test demonstrates it getting that line wrong.

The first draft of this claimed the ambiguity came from type annotations and used
`>function (this: any) { } : (this: any) => void`. That is not ambiguous —
`this: any` has no space before the colon, so it never looks like the separator.
The unit test asserting the wrong split failed, which is the only reason the real
shape was looked up. Both forms are now tested.

## The suite reports 0%, deliberately

[conventions.md](../conventions.md): *a conformance suite whose subject does not
exist reports 0%, loudly, with the reason stated. It does not skip, and it does
not omit the row.* Every judged case comes back `Unsupported { "no checker
(bd tsr-4sc)" }`, so the row is in the summary table from today and the number
moves the first time a type is computed.

## A skip reason that was wrong in two suites

`checker_types` first checked for a plain `.types` file before checking whether
the case was configuration-varied. A varied case has **no** plain baseline — only
`case(target=es5).types` — so all 1,397 of them landed in the "no baseline" bucket:
the right exclusion under a label that said something else, which is how a skip
count stops meaning anything.

`binder_symbols` had the identical flaw and had had it since it was written. It
was never checking for varied `.symbols` at all.

**Its pass rate was not affected**, and that was checked rather than assumed: no
case in the corpus has both a plain and a varied baseline, so no case was ever
judged against an arbitrary configuration. 8,278/8,449 before the fix and after.
(The denominator has since moved to 8,457 as previously unparseable units entered the
suite; see ADR-0026 and ADR-0027.)
What was wrong was the breakdown — 2,321 cases reported as "upstream recorded no
baseline" when 1,397 of them were varied, which overstates how much of the corpus
upstream never ran.

Both suites now test varied first, and their skip lists are directly comparable.

## The diagnostics half

`.types` gates the types a checker computes; `.errors.txt` gates the errors it
reports, and that is the larger half — the baseline is what a user sees. Before
this, the only diagnostic comparison was `isolated_declarations`, filtered to the
`9000..9100` range: 20 codes out of TypeScript's ~1,600.

A pass is the **exact multiset of (file, line, column, code)**. Not a subset and
not "the codes we know about": a diagnostic we invent fails a case as surely as
one we miss.

Only cases expecting **at least one** diagnostic are judged. Reproducing a clean
file by reporting nothing is real conformance, but it is already measured by
`parser_typescript` and `scanner_clean_files`, and folding those 5,082 cases in
here would bury the number that matters — of the diagnostics upstream reports, how
many do we? The false-positive direction survives anyway: a case expecting three
and getting four fails.

### It found two defect classes on its first run

0.98% is the checker's absence, and expected. What was not expected: **865 of the
5,434 failures are cases where this port emits a diagnostic upstream does not.**
Those are unblocked by the checker and ours to fix today.

| code | cases | |
|---|---:|---|
| `TS2300` duplicate identifier | 395 | **a binder diagnostic** (`bd tsr-y4u.18`) |
| `TS1005`, `TS1003`, `TS1012`, `TS1109`, `TS1125` | ~470 | parser codes (`bd tsr-pum.11`) |

Both were invisible to the suites that ostensibly cover those components, and for
structural reasons rather than by oversight:

- `binder_symbols` (98.03%) compares symbol **resolution** against `.symbols`. A
  binder can resolve every name correctly and still invent 395 duplicate-identifier
  errors, because it never raises them into that comparison.
- `parser_typescript` (99.38%) judges whether a file parses **cleanly** and skips
  the 7,413 cases that legitimately contain parse errors — so *which* errors we
  report in exactly the cases designed to produce errors was never compared.

That is the argument for this suite in one paragraph: a component-shaped gate
measures the component's model, and only an output-shaped gate measures what
ships.

## Three things to know before trusting either number

Learned 2026-08-05, while using both suites to fix five binder defects. All three
are properties of the instruments, not of the compiler.

**1. `checker_types`'s judging path has been proved, 2026-08-05 — before it was
used.** The rest of this note is what that took, and it is left in place because
the *reason* for doing it applies to the next suite as much as it did to this one.

The problem as it stood: all 9,538 cases were classified `Unsupported`, so the
comparison code had run zero times. Worse than untested — there was no comparison
code at all, only the `Unsupported` return. The `.types` baseline *parser* had four
unit tests; the suite consuming it had none. A suite that has only ever reported 0%
is as unproven as one that reads 100% on its first run — the failure this project
already hit with `file_loader`, which read 76/76 until four deliberate mutations were
applied to the code under test.

So `types_suite::compare` was written and proved before anything measured with it.
Eight tests, and each was checked against a **deliberately weakened judge** rather
than assumed to bite. The seven mutations and what each turned red:

| mutation of `compare` | tests that went red |
|---|---|
| sort both sides — multiset, not positional | `the_right_types_in_the_wrong_order_fail` |
| drop the assertion-count check | `a_missing_assertion_fails`, `an_extra_assertion_fails…` |
| drop the file-name check | `output_for_the_wrong_file_fails…` |
| stop after the first file | `a_multi_file_case_fails_when_only_its_second_file_is_wrong` |
| count lines, do not compare their text | `a_wrong_type_with_the_right_count_fails`, `the_right_types_in_the_wrong_order_fail`, `a_multi_file_case…` |
| take the denominator from our output, not the baseline | `a_missing_assertion_fails`, `an_extra_assertion_fails…`, `producing_nothing_fails_and_the_denominator_survives` |
| never match a line — the over-strict direction | 6 of 8, including `identical_output_passes_and_the_tally_is_full` |

Every mutation turned at least one test red, **and every test was turned red by at
least one mutation** — including the positive control, which the seventh mutation
exists to check. A control that no mutation can break is the no-op test this project
has already shipped once (see [checker.md](checker.md) on the memo counter).

Two of these earn their place beyond ceremony. The multiset mutation covers a real
defect class — every right type attached to the wrong expression — that a
set-based comparison would pass. And the denominator mutation is the one that
matters for [ADR-0031](../adr/0031-a-gradient-beside-the-gate.md): taking `total`
from our own output makes a checker that produces nothing read **100%**, which is
the most dangerous possible failure of a gradient.

What is still unproven is the *producer*: nothing yet renders our checker's types
in baseline form, so `compare` is exercised only by its unit tests and by cases
where our side is empty. When the producer lands, the same discipline applies to
it — the first non-zero number is not evidence until something deliberately wrong
has been fed through the whole path.

**2. `diagnostics` is capped at 90.9%, permanently, until the parser is fixed.** 500
of its 5,488 judged cases carry a diagnostic *we* emit and upstream does not — 2,675
parser/scanner over-reports and 126 binder ones, measured by
`examples/over_reports.rs`. The checker cannot remove those: a false positive from an
earlier stage sits underneath the checker's output. So a rising number will look
better than it is, and ~9 points of it are unreachable. `bd tsr-pum.11`.

**3. `binder_symbols` is a weak instrument for symbol-table defects.** It sat at
8,278 through three separate defects that merged unrelated declarations into a single
symbol — a class's type parameters, static versus instance members, and block-scoped
declarations. Its `.symbols` baselines do not distinguish those. **A flat
`binder_symbols` is not evidence that a symbol-table change is safe.** What caught all
three was a diagnostic they happened to produce, bucketed by declaring construct
(`examples/ts2300_constructs.rs`); what pins them now is unit tests in
`crates/tsr-binder/tests/bind.rs`, each verified to fail with its fix reverted. See
[binder.md](binder.md).

## The producer: what it has to reproduce

`checker_types` cannot move until something renders our types the way upstream's
baseline writer does. That writer is
`vendor/typescript-go/internal/testutil/tsbaseline/type_symbol_baseline.go`
(490 lines), and it is specified here because re-deriving it is most of the work
and because two of its properties are not what a reader would guess.

**Node selection** (`visitNode`, `:304`) is a pre-order DFS via `ForEachChild`,
children pushed reversed so they come out in source order, keeping nodes where:

```go
ast.IsExpressionNode(n) || n.Kind == ast.KindIdentifier || ast.IsDeclarationName(n)
```

`writeTypeOrSymbol` (`:345`) then **drops** a kept node when it `IsPartOfTypeNode`
— "don't try to get the type of something that's already a type" — when it is an
`Identifier` whose parent's `GetMeaningFromDeclaration` carries no `Value`
meaning and it is not a type alias's own name, or when it is an omitted
expression.

**The line text** is the raw source slice, not anything printed:
`source[skipTrivia(node.Pos()) .. node.End()]`, with line delimiters stripped,
and the line number is that of the post-trivia position. `iterateBaseline`
(`:196`) interleaves the file's own source lines between the assertions.

Two things that will bite:

- **The type string is not `type_to_string`.** Upstream builds a type *node* with
  `NodeBuilder.TypeToTypeNode` and prints it (`:390`), under
  `NoTruncation | AllowUniqueESSymbolType | GenerateNamesForShadowedTypeParams`,
  short-circuiting to the bare intrinsic name when the type is `any` and the node
  is not in one of eight listed positions. Our printer is equivalent only for the
  intrinsic and literal types this port has, and that equivalence ends the moment
  object types exist. The node builder's depth limit of 10
  (`nodebuilderimpl.go:3158`) belongs with it — `bd tsr-el3.2`.
- **`push_children` is a superset of `ForEachChild`.** Ours is generated from
  `ast.json` and includes token-valued fields; upstream's does not
  ([ADR-0033](../adr/0033-the-parser-fills-the-node-map.md) measured the gap at
  0.41% of nodes). The three selection predicates probably filter those out
  anyway — but that is an assumption, and it must be measured rather than
  believed.

### How to prove the walker without the checker

This matters more than it sounds, because **a wrong walker and a wrong checker are
indistinguishable in the line gradient**: both simply fail to match, and the
gradient cannot say which is at fault. A producer built and measured only against
types would give no way to tell.

There is a sound separation. Upstream's assertion is `{sourceText} : {type}`, and
`sourceText` can itself contain `" : "` — which is why this suite compares whole
lines and never splits them. But a line can still be tested for the **prefix**
`our_source_text + " : "`. That needs no type at all, and isolates node selection
and text extraction completely.

So the order is: build the walker, report *of upstream's assertion lines, how many
do we emit at the same index with the same expression text*, and get that number
high **before comparing a single type**. If it reads high the denominator is
trustworthy; if it reads low the gradient is meaningless no matter how good the
checker gets.

### First measurement of the walker

Built 2026-08-05 (`crates/tsr-conformance/src/types_producer.rs`), measured by
`cargo run -p tsr-conformance --example types_walker --release` over the same
9,538 cases the suite judges:

Against upstream's 478,954 assertion lines throughout:

| | first build | + meaning | + heritage | + `const` | + instance state | + callee span |
|---|---:|---:|---:|---:|---:|---:|
| lines we emit | 499,816 | 481,365 | 479,771 | 479,767 | 479,019 | **479,019** |
| **excess** | +20,862 | +2,411 | +817 | +813 | +65 | **+65** |
| text agreement | 65.08% | 91.05% | 95.58% | 95.58% | 97.77% | **97.85%** |
| cases with right count | 58.73% | 85.37% | 90.47% | 90.48% | 94.14% | **94.14%** |
| cases matching every line | 56.36% | 82.59% | 87.54% | 87.57% | 91.10% | **92.44%** |

Four ports took this from two-thirds to 97.77%, and **excess emission from 20,862
lines to 65**:

- `GetMeaningFromDeclaration`, worth 26 points. Type declarations are dense in
  the corpus and every one of their names was producing a line upstream drops.
- `isPartOfTypeExpressionWithTypeArguments`, worth 4.5. The two halves of a class
  header are **not symmetric**: `class C extends B` evaluates `B` as a value and
  gets a line, `class C implements I` names a type and gets none.
- `permitConstAsModifier` **in the parser**, worth almost nothing here but a real
  fidelity fix — `static const H = 1` was parsed as *two* members named `const`
  and `H`, where upstream has one named `H` with an erroneous modifier. Found by
  diffing trees rather than guessing at a predicate, which was the right call:
  the symptom looked like a walker bug and was not.
- `GetModuleInstanceState`, worth 2.2 points and — more tellingly — most of the
  remaining excess. A namespace has a value side only when *instantiated*, so
  `namespace N {}` and `namespace N { interface I {} }` contribute no name.
- **A `new` expression's callee span** in the parser. In `new provide.Provide()`
  the property access spans `provide.Provide`; it was being finished from the
  `new` position and spanned `new provide.Provide`. Invisible to the printer,
  which round-trips either way, and wrong for every consumer that reads a node's
  source text. Worth only 0.08 points on lines but **1.3 on whole cases** — 128
  more cases match every line — because the shape is common and one wrong line
  fails a case.

**No type is involved in any of those numbers.** They are the prefix test
described above, so they measure node selection and text extraction alone.

Two causes account for most of the gap, both visible in the sampled first
divergences and both known rather than mysterious:

- **65 excess lines**, 0.014% of the total. Whatever remains is now rare enough
  that it is worth finding case by case rather than by pattern.
- **`export { ... }` inside a namespace** answers `Instantiated` unconditionally,
  because `getModuleInstanceStateForAliasTarget` resolves the specifier against
  enclosing statements and that is a name resolution this predicate should not be
  doing. Upstream's own fallback on a failed lookup is `Instantiated`, so the
  error stays one-directional: over-emit, never under-emit.
- **The keyword-token cases have been resolved** — they were a *parser* error
  recovery difference, not a predicate defect, and the fix is above. Recorded
  because the symptom pointed at the wrong component and only a tree diff
  separated them.

Read 97.77% as *the walker is mostly right and its remaining errors are
concentrated in two identified places*, not as a pass rate. The bar for wiring it
into the suite is that the residual be small enough that a moving gradient means
the checker rather than the walker.

### What blocks it

All four selection predicates ask "is this node its parent's `name` / `expression`
/ `initializer`?", and nothing in `tsr-ast` can answer that: `ast.json` defines 41
`name` fields, 37 `expression` and 10 `initializer`, with no accessor reaching any
of them through the `Node` union. They have to be **generated**
(`xtask/src/gen_nodes.rs`, alongside the existing `node_id()` dispatch) rather
than hand-written, for the reason that dispatch is generated in the first place.
`bd tsr-5e7.8`, blocking `bd tsr-4sc.3`.

## The failure histogram (`bd tsr-4sc.6`)

`examples/types_shapes.rs`, 2026-08-05. The gate says how many cases pass and the
gradient says how many lines do; neither says **what** is failing, and the
ranking of 60,269 lines of remaining checker was being made without that. The
results and the re-ranking they forced are in
[checker.md](checker.md#the-order-the-rest-is-built-in-and-why); this section is
the instrument.

### The alignment test is the split

A `.types` line is `>{expression} : {type}` and cannot be split — the expression
may contain `" : "` — which is why the suite compares whole lines. But a
histogram needs upstream's *type*, on its own. It gets it without splitting
anything: where our walker produced the same expression text at the same
position, upstream's line **starts with** `{our text} : ` and everything after
that prefix is exactly upstream's type.

That is only available for lines the walker aligned, which is the point rather
than a limitation: 468,921 of 478,954 lines (97.91%), and the other 10,033 are
reported as their own row and attributed to nothing. A line the walker lost is
not evidence about the checker, and the *reason* this document insists on
measuring the walker separately is that a wrong walker and a wrong checker are
indistinguishable in the gradient.

(97.91% here against 97.85% in the walker table above: the same test over the
same population, counted per line rather than per line-with-a-produced-line. The
tables are not in conflict and neither number has been corrected.)

### Three dimensions, because one of them cannot rank work

- **Upstream's answer shape** — the bucket table. `crate::type_shape::classify`
  reads the *printed* form, since a baseline records nothing else.
- **Gap versus wrong.** A line we answered `error` on is an unported form; a line
  we answered anything else on is a defect in what is ported. 350,184 against
  11,499, and the two want completely different work.
- **Where the checker stopped**, for every gap line: `types_producer::gap_reason`
  names the construct — an expression form, an annotation, a symbol kind with no
  type, a name that does not resolve. This is the dimension that actually ranked
  the work, and the shape table could not have supplied it: `a + b` → `number` is
  an *intrinsic answer* that needs binary-operator checking.

### The classifier was mutation-proved before it was believed

`type_shape::classify` decides every share in the table, so it got the same
treatment as the judge. Seven tests, each checked against a deliberately weakened
classifier:

| mutation of `classify` | tests that went red |
|---|---|
| ignore bracket depth when scanning for operators | `a_function_type_returning_a_union…`, `an_operator_inside_brackets…` |
| let a top-level `\|` beat an earlier `=>` | `a_function_type_returning_a_union…` |
| stop skipping quoted text while scanning | `a_separator_inside_a_string_literal…` |
| treat any trailing `]` as an array suffix | `an_indexed_access_is_not_an_array`, `the_plain_shapes` |
| drop the keyword rejections | `what_is_not_a_name` |
| do not consult the intrinsic table | `the_plain_shapes`, `a_wrapped_type…` |
| do not unwrap enclosing parentheses | `a_wrapped_type…` |
| do not recognise literals | `the_plain_shapes`, `a_separator_inside_a_string_literal…` |

Every mutation turned at least one test red and every test was turned red by at
least one mutation. The precedence tests are the ones that earn their place:
`() => void | number` is a *signature* returning a union, because upstream writes
`(() => void) | number` when it means the other thing, so the leftmost top-level
operator decides and three independent `contains` checks would misfile the two
largest tail buckets.

An eighth guard was **removed** rather than tested: the array check also
required the head to be bracket-balanced, and no mutation could make that
observable, because TypeScript closes every object, tuple and type-argument list
before a `[]` suffix. Untestable code that cannot be shown to matter is
complexity, not safety.

### The instrumentation invented a finding, and was caught by reading it

`gap_reason` first reported **22,768 lines** as *"a declaration name whose parent
bound no symbol"* — a startling number, and false. It had taken the
declaration-name branch as soon as a node was its parent's `name`, while
`type_at_location` takes that branch only when the parent actually bound a
symbol. Every one of those lines was the `b` of an `a.b`: a `PropertyAccess`
binds no symbol, so the real code falls through and resolves `b` **as a free
name in the enclosing scope**.

Two things came out of that. The instrument now mirrors the code it explains,
line for line — an explanation that does not follow the implementation reports
its own behaviour. And the fall-through is a real defect (`bd tsr-tl8`): today
those names mostly fail to resolve and answer `errorType`, but a local called `b`
in scope would give `a.b` that local's type, which is a wrong answer where a gap
belongs — the exact failure the `errorType`-not-`anyType` rule exists to prevent.

### `errorType` prints `error`, not `any`

[checker.md](checker.md) said gaps and `any` were indistinguishable in output.
They are not: `errorType` carries the intrinsic name `error` (upstream's own,
`checker.go:979`) and `type_to_string` prints it. That is what makes the
gap/wrong split above possible at all. The correction is recorded at the
paragraph that was wrong. Upstream prints `error` on 1,436 of its own baseline
lines, so a gap *can* coincide with a right answer — rarely, and never silently
across a bucket.

## What is still missing for Phase 4

- **Per-configuration runs** (`bd tsr-bb4.1`), which would return 1,397 cases to
  `checker_types` and 793 to `diagnostics`.
- **Message text.** Both suites compare codes and positions, never the rendered
  message. Two diagnostics with the same code and different arguments are equal
  here, and `.errors.txt` records the full text. That is a real gap and a
  deliberate one: the localised message tables are `bd tsr-5e7.6`.
