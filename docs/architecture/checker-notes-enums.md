# Checker notes — `getTypeOfSymbol`'s remaining arms

Working notes for the enum-member, alias and accessor slices of
`Checker.getTypeOfSymbol` (`bd tsr-4sc.9`). Kept as a **separate file** rather
than appended to [`checker.md`](checker.md): four agents share one checkout, and
a shared markdown file is swept by whoever commits second no matter how
carefully either stages — `git commit -- <paths>` commits the working-tree state
of a path, so path-scoping protects you from a neighbour's edits to *other*
files and does nothing when you are both editing the same one. The lead folds
these notes into `checker.md` at cycle end.

The enum-member and alias sections already live in `checker.md` (under
"An enum member's type is read, not built" and "Aliases: the same-file slice").
This file starts at accessors.

## Accessors, and the one place `any` is the right answer (`bd tsr-4sc.9`)

`getTypeOfSymbol`'s **first** flags branch — `checker.go:16506` — is accessors,
and it answered `errorType` until now. With this, every `SymbolFlags` shape
upstream dispatches on is answered; what remains unported in that function is the
four `CheckFlags` shapes it tests *before* any of them (deferred, instantiated,
mapped, reverse-mapped).

### The prediction, stated before building

Per [`conventions.md`](../conventions.md)'s "Predict which histogram row you move,
and by how much, before measuring":

- **Row:** `declaration name, symbol has no type: SymbolFlags(GET_ACCESSOR)` and
  its `SET_ACCESSOR` twin — an accessor's own `>x : T` line.
- **Magnitude:** upper bound **~871 lines**. Counted over the corpus at
  `2c107a3`: 1,068 `get` and 804 `set` declarations, of which 352 getters carry a
  return annotation and 519 setters carry a parameter annotation. Those 871 are
  what the annotation arms answer.
- **Not claimed:** unannotated getters with bodies (the largest remaining group,
  ~716 getters) gap, so a large share of the 1,872 declarations stays red.
- **Commit pair that would show it:** this commit against the next
  `checker_types.snap` refresh.

If the next measurement moves that row by far less than ~871, the prediction was
wrong and the likely cause is that accessor *declaration names* are not what the
histogram row counts — in which case the arm still helps property access through
`crate::members`, but the row named above is the wrong instrument.

### Upstream's four sources, and which two are here

`getTypeOfAccessors` (`checker.go:18511`) tries, strictly in order:

1. the getter's return annotation — `checker.go:18522` — **ported**
2. else the setter's **parameter** annotation — `checker.go:18524` — **ported**
3. else an auto-accessor property's annotation — `checker.go:18527` — gapped
4. else the getter's inferred body return type — `checker.go:18531` — gapped
5. else `anyType` with an implicit-any diagnostic — `checker.go:18545` — **ported**

The order is the entire content of the function. It is observable only when a
getter and a setter carry *different* annotations, which is why the test uses
`get x(): number` beside `set x(v: string)`; with matching types no ordering bug
could be seen. Swapping the two arms turns that assertion red — verified, not
assumed.

**A setter's annotation is on its parameter, not on the setter.**
`getEffectiveSetAccessorTypeAnnotationNode` (`checker.go:20118`) reads the first
parameter's type. Reading the setter node's own `r#type` slot compiles, type-checks
and silently answers `None` for every setter in existence, falling through to the
`any` arm. Mutating the code to do exactly that prints `any` where `string`
belongs — that is how the trap was confirmed rather than argued.

### Why the fallback is `anyType` here and `errorType` everywhere else

This module's standing rule is that an unported form answers `errorType`, never
`anyType`, so a gap stays separable from a computed answer. Case 5 **inverts**
it: an accessor with no annotation anywhere and no getter body is implicitly
`any`, upstream *computes* that and reports a diagnostic alongside it. Answering
`errorType` would mark a line this port gets right as missing.

That is only safe because case 4 is separated out **first**. A getter with a body
would be inferred upstream — `accessorBodyInTypeContext.types` records
`get foo() { return 0 }` as `>foo : number` — so it must gap rather than fall
into the `any` arm. Without that ordering, every inferable accessor in the corpus
would produce a plausible, wrong, indistinguishable-from-computed `any`: the same
failure the `errorType`-not-`anyType` rule exists to prevent, arrived at from the
opposite direction.

**How I would know this was wrong.** If the next measurement shows the accessor
rows moving but the *wrong-answer* differential rising by a similar amount, the
`any` arm is claiming lines it should be gapping, and the fix is to narrow case 5
to the declarations that provably have no body and no annotation rather than to
treat it as the default.

**Case 4 landed.** `7b366d8` cut the seam as
`get_return_type_from_body(declaration) -> Option<TypeId>` — the `Option` shape
requested before the seam existed, because a bare `TypeId` returning `errorType`
for both "inference failed" and "inferred an error" would have forced this arm to
gap conservatively and left the ~716 unannotated getters red anyway.

`None` becomes `errorType`, never `anyType`, and that single `unwrap_or` is what
keeps case 5 honest: upstream's `getReturnTypeFromBody` always produces a type,
so upstream never falls from case 4 to case 5. Every `None` here is a declaration
upstream *would* have inferred, so letting it reach the `any` arm would print a
plausible wrong `any` on exactly the accessors that have a real answer.

A getter whose body cannot complete is `void`, not `never`: `mayReturnNever`
(`checker.go:20312`) covers a function expression, an arrow and an object-literal
method, and an accessor is none of the three.

**And a live instance of the discrimination check, on my own test.** The first
fixture for that rule was `get foo() { }` — an empty body, which answers `void`
under *either* reading. Flipping `may_return_never` to `true` left it green. The
fixture had to `throw` before the two readings diverged. Written down because it
is the check catching its own author, two commits after being written.

## The most common test defect in this project: exercising without discriminating

Five instances in one cycle, across three agents. It is worth naming because it
is **invisible to every gate**: the code is right, the test passes, the test is
mutation-checked, and the mutation *applies* — and the test still asserts nothing.

A test discriminates when its fixture produces **different output under the
correct implementation than under the plausible wrong one**. A test merely
exercises when both readings produce the same output. Only the first is a test.

### The five

1. **A fixture that never reaches the mutated branch.** My first mutation of the
   qualified-alias arm resolved a 2-level entity name; the fixture was
   `foo.bar.baz`, 3 levels, so the mutated code returned early and never ran.
   `grep -c` said 1/0 — the text had changed — and the test stayed green. Fixed by
   mutating the whole walk instead. **`grep -c` proves the text changed, not that
   the code ran.**
2. **`contains` over a bag of pairs.** `types_producer.rs`'s import-equals test
   used `contains(&("M", "typeof M"))` and survived a mutation disabling the rule
   entirely, because `namespace M` emits its *own* identical pair at index 0. A
   bag cannot tell one occurrence from another. Fixed by whole-vector equality.
3. **Two readings that agree on the fixture.** The arrow-transparency test used
   `function* g() { var h = () => { yield 1; }; }`. With the arrow transparent the
   walk reaches `g`, an unannotated generator, which *also* answers `any`. Fixed
   by annotating `g` so the two readings diverge. (Recorded in `checker.md`.)
4. **The `typeof` constituent order**, same cycle, same shape. (Second-hand;
   see `checker.md`.)
5. **Designed around rather than discovered:** the accessor getter/setter
   ordering is observable *only* when the two annotations carry different types.
   `get x(): number` beside `set x(v: string)` discriminates; `get x(): number`
   beside `set x(v: number)` would have passed under either order and pinned
   nothing. The fixture was chosen for that reason.

### The check that catches all five

Before writing a fixture, ask: **what is the plausible wrong implementation, and
what does this exact fixture print under it?** If the answer is "the same thing",
the fixture is decoration however carefully the assertion is written. This is
cheaper than mutation testing and catches the cases mutation testing misses,
because a mutation that never executes reports success.

Two corollaries, both paid for:

- **A gap fixture must be a failure, not a form.** Using a syntactic form as the
  stand-in for "the checker cannot type this" means the test silently changes
  meaning the day someone ports the form. Prefer an unresolvable name.
- **When printing cannot distinguish two implementations, assert about the
  type.** An enum's declared type printed `E` both before and after it became a
  real union, so the test asserts `UNION` in the flags and one constituent per
  member. Reverting the arm reddens it; nothing about the printed line would have.

### Sixth instance, and a new sub-kind: the fixture whose SUBJECT gets ported

`types.rs::a_symbol_shape_this_slice_does_not_port_is_an_error_type` asserted
that an unported `getTypeOfSymbol` shape answers `errorType` and not `anyType`,
using whatever shape happened to be unported as the fixture. It was re-pointed
once — function symbol to accessor — when its first fixture was ported, and the
accessor arm then ported the replacement.

This is **not** the failure the other five are. The fixture discriminated
perfectly well at every moment; what decayed is that its *subject* kept being
ported out from under it. The defect is a test whose claim is "something here is
unported" rather than "this specific thing behaves this way" — a claim with no
stable referent.

**Delete rather than re-point when no fixture remains that is unported for a
STRUCTURAL reason rather than a not-yet-done one.** With every `SymbolFlags`
shape `getTypeOfSymbol` dispatches on now answered, that condition was met. Both
its claims survive where they belong:
`the_intrinsics_that_print_alike_are_still_distinct_types` pins the
`errorType`/`anyType` identity, and `an_unported_expression_form_is_error_not_any`
pins the discipline on the expression side, where unported forms still exist.

**The tell is mechanically greppable**, unlike the other five, because
re-pointings leave a trace in the comment and in the history:

```
grep -rn -i "used to name\|this test used to\|used to assert\|re-pointed\|\
was written when\|no longer\|stopped being" crates/tsr-checker/tests/*.rs
```

Run over the whole suite it returned **eleven hits and no new defects** — every
other one is a *healthy* re-pointing that records its own history. Two are worth
copying:

- `relater.rs`'s `two_structurally_identical_interfaces_relate` replaced a
  characterisation test that pinned `false` while structural comparison was
  unported, "deleted rather than inverted in place, because the thing it
  asserted no longer exists" — and its doc names the mutation that reddens it.
- `types.rs::a_property_that_is_not_there_is_a_gap_and_not_a_free_name` records
  an assertion **removed** rather than inverted when inherited members landed.

So the sub-kind is real but rare here, and the suite's habit of writing down why
a test moved is what makes it findable at all.

### The argument that survives this session

Every one of those examples **predates the write-up**. `relater.rs` deleted a
characterisation test rather than inverting it, and named the mutation that
reddens its replacement, before "delete rather than re-point" was a rule.
`types.rs` removed an assertion rather than inverting it when inherited members
landed. `members.rs::the_shapes_typeof_x_still_gaps` declined to assert a
guaranteed `None` because it "would dress a guaranteed None up as coverage" —
which is the discriminates-versus-exercises check, stated in a test body, by
someone who had never read a word about it.

That matters more than the taxonomy does. A principle formulated during one long
session, by people who spent that session persuading each other, is exactly the
kind of thing that feels true because it was recently argued rather than because
it is right. The defence against that is not more argument — it is finding the
principle already in the codebase, arrived at independently, by authors who could
not have been influenced by the formulation. Three times, in three files, in work
none of the formulators wrote.

So the check is not this session's idea. It is a name for something the careful
authors here were already doing, and the value of naming it is that it can now be
applied deliberately and greppably rather than only by people who happen to have
the instinct. **If a later reader finds this file unconvincing, the three
examples above are the evidence to re-check — not the reasoning in it.**

## Predicting against a row you have already excluded part of

The accessor prediction failed and the way it failed is worth more than the
number. Stated: ~871 lines, from 352 annotated getters and 519 annotated
setters. Measured across the window: 231.

The row breakdown is what makes it diagnosable:

| row | before | after |
|---|---|---|
| `SET_ACCESSOR` | 133 | **0** |
| `GET_ACCESSOR \| SET_ACCESSOR` | 302 | 206 |
| `GET_ACCESSOR` | 182 | **180** |

The setter row closing completely is the annotation arm working as predicted.
The getter row moving by **two** against a predicted 352 is the finding — and
the asymmetry rules out "the row is the wrong instrument", which was the
falsifier I had written.

**The likely cause, stated as an unverified hypothesis:** the measurement
predates the case-4 commit, so the ~716 unannotated getters — the group the
prediction *explicitly excluded* — were still red and still sitting in the same
row. A 352-line improvement inside a row dominated by 716 unmoved lines reads as
noise. If so the ceiling was sound and the row was fine; what was wrong was
predicting against a row whose other occupant I had already declared out of
scope, which makes the exclusion invisible in the number.

**Settling it is cheap:** measure `GET_ACCESSOR` at the case-4 commit. A drop of
roughly 700 confirms it. A second flat result means accessor lines are not
reaching that row at all, and both accessor commits are worth much less than they
appear — which is worth knowing before anything is built on them.

**The rule, for next time:** *if a prediction excludes a large group, name the
row that group occupies and predict for it separately.* Otherwise the exclusion
cannot be seen in the result, and a correct prediction and a badly wrong one look
identical. Measuring at the commit pair's parent fixes attribution across
commits; it does not fix this, because this is one commit predicting against one
row.

## Size a slice from the baselines, not from the source

Both accessor predictions were high by about 2.5x — ~871 against 231, and ~716
against 386 — and they were wrong the *same* way, because they were built the
same way: by grepping the corpus **source** for `get x()` / `set x(v)`
occurrences and treating each as an assertion line. It is the wrong unit. The
board counts assertion lines produced by the instrument; source occurrences are a
different quantity, and the two diverged by the same factor twice.

The fix is not a conversion constant. It is to **count the thing the instrument
counts**, which is directly available: a `.types` baseline records an accessor
declaration as the declaration line followed by its `>name : T` assertion, so the
lines can be counted where they actually live.

```python
# accessor assertion lines, straight out of the baselines
acc = re.compile(r'^\s*(?:(?:public|private|protected|static|abstract|readonly'
                 r'|declare|export)\s+)*(get|set)\s+[A-Za-z_$][\w$]*\s*\(')
# ... for each *.types file, count a match whose NEXT line starts with '>'
```

Measured over
`vendor/typescript-go/testdata/baselines/reference/submodule/**/*.types`:

| population | getter lines | setter lines | total |
|---|---|---|---|
| all baselines | 997 | 711 | 1,708 |
| **excluding configuration-varied** | **413** | **331** | **744** |
| observed in the accessor rows at `2c107a3` | — | — | **673** |

**744 predicted against 673 observed — within 11%**, where the source-text count
was out by 150%. Configuration-varied baselines (a parenthesised suffix in the
filename) must be excluded because the harness skips 1,397 of them; the residual
~10% is the other skip categories — 924 cases with no `.types` baseline, 450
known divergences, 135 with no assertions.

**The rule:** *if a prediction is about assertion lines, count assertion lines.*
Grepping source is fine for deciding whether a form is worth looking at; it is
not a prediction until it has been converted, and the cheapest conversion is to
skip the source entirely and count in the baselines.

**How you would know this is wrong:** apply it to a slice whose measured result
is already known and check it lands inside ~15%. It has been checked once, on
accessors, against a number produced after the method was invented — so it is one
confirmation, not a validated technique. `arrays`' ~10,000-line pair and
`indexsig`'s ~1,500-line pair are the next two chances to falsify it, and both
are being measured anyway.

## FALSIFIED: the baseline-counting method is not a predictor

The method above — count assertion lines in the `.types` baselines instead of
occurrences in the source — was published as *one confirmation* (744 vs 673 on
accessors) with the fitted-method risk named, because it was invented after
seeing the number it had to reproduce. It was then given a genuine blind test on
two forms whose measurements neither the author nor the lead had seen.

**It failed both, in opposite directions.**

| form | predicted | measured | error |
|---|---|---|---|
| `any` receiver on a member access | 6,300 (range 5,000–7,100) | **950** | 6.6× high |
| call/construct signature members | 432 (range 380–432) | **1,537** | 3.6× low |

Opposite-direction errors are worse than a consistent bias, which could have been
calibrated. And the form nominated *in advance* as the method's best case —
Form B, one assertion per annotation, no cascade — is the one that missed by
3.6×. The stated failure mode ("works only where I already understand the form")
did not occur; the real one is that the count has no reliable relationship to the
measured quantity in either direction.

### Two distinct failure modes, which is why the errors diverged

**1. Counting where a construct APPEARS, not the lines its failure BLOCKS.**
Form B counted 432 annotation sites. But one un-renderable annotation blocks the
declaration's own line *and every reference to that variable*. Checked rather
than assumed:

| | |
|---|---|
| annotation sites (what was counted) | 286 |
| further assertion lines naming those variables | 1,507 |
| implied blocked total | **1,793** |
| measured | **1,537** |

So the correction lands within 17% where the original was out by 260%. This
failure mode is **real and correctable** — it is the same multiplier `overloads`
found running the other way, where one `var d = Object.assign` costs three lines.

**2. The baselines cannot say whether this port can compute a PREREQUISITE.**
Form A's movement depends on whether the port produces `any` for the *receiver*.
A baseline records what **upstream** computes, never what this port can currently
compute for a dependency. Two attempted bounds, both wrong, in opposite
directions:

| bound | value | vs measured 950 |
|---|---|---|
| every `any`-receiver access (after the evolving-array exclusion) | 7,068 | 7.4× high |
| only receivers with an explicit `: any` annotation | 225 | 4.2× low |

The answer sits between two proxies that bracket it by a factor of 31. **This
failure mode is not correctable from baselines at all** — the information is not
in them. It is a property of the port.

The method cannot tell in advance which kind a form is, which is the fatal part:
a form of kind 1 is predictable after correction, a form of kind 2 is not
predictable at all, and nothing in the baselines distinguishes them.

### What survives

- The 744-vs-673 accessor agreement stands for that one form, and now reads as
  luck rather than evidence — accessors happen to be kind 1 with a multiplier
  near 1.
- **The exclusion analysis is the transferable part**, and it is independent of
  the count being predictive: finding that `largeControlFlowGraph.types` alone
  contributed 10,000 of a naive 17,100, recognising it as the evolving-array case
  the author had pre-registered as out of scope, and cutting it. That was correct
  and load-bearing. It also did not save the prediction.
- Counting assertion lines rather than source occurrences remains the right
  *unit*. It is simply not sufficient.

### The rule that replaces it

**A line count without an independent check against the instrument is not a
prediction, whatever unit it uses.** `indexsig` reached this from the other side:
its estimate held because it cross-validated against the instrument before
quoting a number. Both blind counts here had no cross-check available *by
construction* — that is what made the test fair, and it is what the counts could
not survive.

Trust a number that arrives with a cross-check; discount one that arrives with a
story. This section is the worked example, and the story was mine.

## The `undefined` global, and the 22 sites its divergence touches

`undefined` is the only global with no declaration anywhere — it is in no
`lib.*.d.ts` — so merging the bundled libs, the mechanism that makes `Array` and
`String` resolve, could never produce it. Every reference answered `errorType`:
1,675 lines by the instrument's name tally, 1,738 by an independent count of
`>undefined : undefined` in the baselines, agreeing within 4%.

### The divergence, and why it was accepted

Upstream's symbol carries `undefinedWideningType`, not `undefinedType`
(`checker.go:955`). The two print alike and widen differently: `const x =
undefined` is `undefined`, `let x = undefined` is `any`. This port has exactly
one `undefined` intrinsic and no widening variant, so `let` answers `undefined`.

**Gap and wrong both score as not-right**, so these sites cost no gradient and no
cases — they answered `errorType` before. What they cost is diagnostic
separability, on 22 sites, against ~1,675 lines made right.

It is a **missing intrinsic, not a merged identity**, which is the distinction
that made it acceptable where `038def4` was not: there both types existed and one
was used for the other, undiscoverable from the code; here the second type does
not exist, and anyone grepping `intrinsics.rs` finds one `undefined` where
upstream has two.

### The 19 files, so the follow-up can verify it fixed exactly these

```text
compiler/constructorWithIncompleteTypeAnnotation.ts
compiler/controlFlowJavascript.ts
compiler/controlFlowNoImplicitAny.ts
compiler/implicitAnyWidenToAny.ts
compiler/protoAsIndexInIndexExpression.ts
compiler/typeCheckObjectCreationExpressionWithUndefinedCallResolutionData.ts
compiler/widenedTypes1.ts
conformance/expressions/unaryOperators/plusOperator/plusOperatorWithAnyOtherType.ts
conformance/functions/functionImplementations.ts
conformance/salsa/prototypePropertyAssignmentMergeAcrossFiles2.ts
conformance/salsa/typeFromJSInitializer.ts
conformance/statements/ifDoWhileStatements/ifDoWhileStatements.ts
conformance/statements/throwStatements/throwStatements.ts
conformance/statements/VariableStatements/everyTypeWithInitializer.ts
conformance/types/any/assignAnyToEveryType.ts
conformance/types/any/assignEveryTypeToAny.ts
conformance/types/primitives/undefined/directReferenceToUndefined.ts
conformance/types/typeRelationships/widenedTypes/initializersWidened.ts
conformance/types/typeRelationships/widenedTypes/strictNullChecksNoWidening.ts
```

22 `let`/`var` sites across 19 files. Regenerate with:

```sh
grep -rlE '^\s*(let|var)\s+[A-Za-z_$][A-Za-z0-9_$]*\s*=\s*undefined\s*;' \
  vendor/typescript-go/_submodules/TypeScript/tests/cases
```

Several of those names — `implicitAnyWidenToAny`, `initializersWidened`,
`strictNullChecksNoWidening`, `widenedTypes1` — are widening tests specifically,
which is a reassuring sign the enumeration found the right population rather than
an arbitrary slice of it.

### The bug the probe caught, which is worth more than the feature

The first implementation seeded a type for whatever symbol occupied the
`globals["undefined"]` slot. `merge_globals` puts a script's own top-level names
there, so a file declaring `var undefined: string` had its **declared type
clobbered** and answered `undefined` — a silent wrong answer in a case that
previously worked, which is strictly worse than a gap.

The binder now records *which* symbol it synthesised (`None` when the program
declared its own), so the checker seeds only what it is entitled to. Found by a
shadowing fixture, written only because the enum work established the habit of
asking what the wrong implementation prints. No amount of staring at the
1,738-line population would have surfaced it.
