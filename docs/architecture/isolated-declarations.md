# `isolatedDeclarations` analysis (`tsr-dts`)

**Status:** slice 1 of Phase 3.5 (`bd tsr-49v.2`), converged at **13/15** with no
false positives. The printer landed in slice 3 (`bd tsr-49v.4`) and the `.d.ts`
text in slice 4 — see [declaration-emit.md](declaration-emit.md).

**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`.

This document is about *how the analysis is shaped and what it is measured
against*. The decision to write it rather than port it lives in
[ADR-0021](../adr/0021-isolated-declarations-is-not-a-port.md) and is not restated
here.

## What it does

`tsr_dts::analyze(source_file, nodes) -> Vec<Diagnostic>` answers one question per
declaration: **would emitting this into a `.d.ts` have required inferring a type?**
If so it reports the corresponding `TS9xxx` diagnostic. It never produces text.

Two passes, both purely syntactic:

| Pass | Module | Question |
|---|---|---|
| Visibility | `visibility.rs` | which top-level declarations reach the `.d.ts` at all? |
| Rules | `rules.rs` | of those, which would need inference? |

## Anchoring, for a component with no upstream counterpart

ADR-0021 replaced the usual rule (name your typescript-go counterpart) with a
narrower one, because there is no counterpart to name:

1. Each rule names the `TS9xxx` **code** it implements and the upstream message
   constant in `internal/diagnostics/`.
2. The drift tracker watches `internal/diagnostics/` for the `TS9xxx` range, **not**
   `internal/transformers/declarations/`.
3. Where a rule takes oxc's shape, it says so by file.

## The part that is harder than the rules: where the squiggle goes

A diagnostic with the right code at the wrong position fails a case exactly as
completely as a missing one. The anchors are not derivable from the messages and
were read off the baselines one at a time; the table lives in `rules.rs`'s module
documentation, next to the code that implements it, so the two cannot drift apart.

The organising principle is worth stating here because it shapes the API. Failure
is three-valued, not boolean:

- **`Ok`** — the type is apparent.
- **`Reported`** — it is not, and a *specific* diagnostic (`TS9017` for a mutable
  array, `TS9015` for a spread, …) has already been filed at the offending node.
- **`Generic`** — it is not, and only the caller knows what to say: `TS9010` at a
  variable's name, `TS9012` at a property's name, `TS9011` at a parameter's
  initialiser, `TS9013` at the sub-expression when the failure is nested inside a
  literal.

A specific diagnostic *replaces* the generic one rather than joining it:
`export let arr = [1, 2, 3];` produces `TS9017` at the array and no `TS9010` at the
name.

## Findings that changed the rules

Each of these was a case where a source or an assumption said one thing and the
baselines said another. They are recorded because none is recoverable by reading
the diagnostic messages.

### The corpus's own comments are wrong twice

**Function expressions.**

`isolatedDeclarationErrorsReturnTypes` marks a block of exported
`const fn = function foo() { return 0; }` declarations `// Should Error`, and a
block of class fields likewise. **Neither errors.** Not in typescript-go's baseline,
and not in TypeScript's own — the two files are identical, 31 diagnostics, the first
at line 13. Every `TS9007` in that 170-line file is anchored on a function used as a
**parameter default**.

Implementing what the comments said cost 54 false positives in that one case, which
is how it was found. The rule is now scoped to parameter defaults.

The one apparent counter-example is checker-driven and deliberately not matched:
in `isolatedDeclarationErrors.ts` an arrow *does* get `TS9007`, but only because an
expando assignment (`errorOnMissingReturn.a = ""`) makes upstream build the
function's type through `IsExpandoFunctionDeclaration`.

`isolatedDeclarationErrorsClasses` does it again: `[missing] = 1` is marked
`// Should not be reported as an isolated declaration error`, and TypeScript
reports it. Treat the comments in these files as commentary, not specification.

### `TS9007` is narrower still: only parameter defaults, and only block bodies

Following on: a function used as a parameter default needs a return type, but an
arrow with a *concise literal body* does not — `(cb = () => 1)` is clean while
`(cb = function(){ })` and `(cb = () => {})` are not.
`isolatedDeclarationErrorsReturnTypes` pairs the two spellings on consecutive
lines nine times and reports only the block-bodied one each time.

### `TS9009` fires only for a *lone* accessor

A get/set pair is never reported, even when neither half is annotated. Only an
accessor with no counterpart is, and a setter anchors its diagnostic at its
**parameter** rather than its name. One asymmetry is taken from the baselines
rather than derived: a lone unannotated getter reports in a class and is silent in
an object literal.

### `TS9020` is three-valued and transitive

Reading enum members as "constant or not" gets every outcome wrong. `A = f()` is
*not* an error — a call is not a constant expression, so the member emits with no
value at all. `AB = A | B` over constant siblings is fine, by any spelling
(`Flag.AB`, `Flag["A"]`). Only a constant-shaped expression that reaches *outside*
the enum errors. And it is transitive: in `enum F { A = E.A, B = A }` both members
are reported, because `B` names a sibling that is itself not constant. Members are
therefore folded in declaration order, each seeing the verdicts before it.

### A computed property name must be *written* as a literal

Not have a literal type — be written as one. `isolatedDeclarationErrorsObjects`
puts six computed names in one object literal and accepts only `[1]`; `[s]` where
`const s: unique symbol`, `[E.V]` where `E` is an enum, and `[k]` where
`const k = "a"` are all reported. `[-1]` is accepted, `[1 - 1]` is not.

An earlier version resolved names through a scope index to ask whether they had a
literal type. That machinery bought nothing and was removed — twice, in fact: the
second attempt was aimed at `interface I`'s `[noAnnotationLiteralName]()`, which
turned out not to be a computed-name rule at all. What that member is missing is a
*return type*, and the diagnostic is `TS9013` paired with `TS7010`.

### `as const` is a `TypeReferenceNode` with no name

`const` is a keyword, so it never becomes an identifier; the parser gives a const
assertion a `TypeReferenceNode` whose `type_name` is `None`
(`crates/tsr-parser/src/types.rs:403`). Matching on the name `"const"` therefore
matched nothing, and every `as const` was silently read as an ordinary `as T` — an
*explicitly stated* type, so nothing inside one was ever examined. It failed
silently in the safe-looking direction and was caught by a unit test, not by the
corpus. Both spellings are matched now.

## The oracle, and what it can and cannot see

The gate is the `isolated_declarations` conformance suite: the `TS9xxx` diagnostics
of every case that sets `@isolatedDeclarations: true`, compared **by code and
position** against upstream's `.errors.txt`.

Measured at the pin:

| | |
|---|---|
| Corpus cases setting `@isolatedDeclarations: true` | **21** |
| Judged (after exclusions) | **15** |
| Passing | **13** (86.67%) |
| False positives across all judged cases | **0** |
| Excluded: known divergence (`.errors.txt.diff`) | 5 |
| Excluded: upstream recorded no baseline | 1 |
| Positioned `TS9xxx` in those baselines | **172**, across 20 distinct codes |

> **Correction to the record.** ADR-0021 states 22 cases. The corpus the harness
> enumerates — the TypeScript submodule's `compiler/` and `conformance/` — contains
> **21**. The 22nd was almost certainly `isolatedDeclarationsTypePredicate`, which
> lives in typescript-go's *own* `testdata/tests/cases/compiler/` and has no
> submodule counterpart, so `Corpus::discover` never sees it. There are five such
> tsgo-local cases; they are a real oracle nobody reads yet (`bd tsr-49v.7` —
> this line originally cited `tsr-49v.6`, an issue that had never actually been
> filed; `.6` was subsequently taken by slice 4, so the reference is corrected
> here rather than silently repointed).

Only the **header block** of an `.errors.txt` is parsed. A baseline states every
diagnostic twice — once positioned in the header, once as a `!!!` line under the
source echo — so a naive `grep -c` returns exactly double and every count comes out
suspiciously even. `crates/tsr-conformance/src/errors_baseline.rs` documents this
and tests it.

Diagnostics outside the `9000..9100` range are filtered out of the comparison.
Three of the sixteen baselines mix ordinary checker errors in with the `TS9xxx`
ones (12 in total); those are Phase 4's, and comparing whole baselines would make
this suite unpassable for reasons unrelated to what it measures.

### The gate has teeth, and it did not always

Five mutations, applied to the code under test and re-measured over the corpus:

| Mutation | Pass rate | Snapshot lines changed |
|---|---|---|
| Visibility pass disabled entirely | 10/15 | 10 |
| Accessor pair rule dropped | 11/15 | 8 |
| Reference collection widened back to all expressions | 12/15 | 6 |
| Enum fold loses transitivity | 12/15 | 6 |
| `readonly` no longer a const context | 12/15 | 6 |

against a baseline of **13/15**. All five move the rate.

That is worth contrasting with how this suite behaved at 5/15, because the change
is a property of the *measurement*, not only of the compiler. At that rate four
mutations — including disabling the visibility pass — moved the pass rate by
**zero**: a case that already fails an exact-match comparison keeps failing when
made worse, so the rate could not distinguish "wrong" from "more wrong". Only the
snapshot's per-case detail moved, and for the visibility pass not even that.

Two rules follow from having watched it both ways:

1. **Below convergence, read the snapshot diff, not the rate.** An exact-match
   gate is close to useless as a gradient until it is nearly satisfied.
2. **A pass the gate cannot see is untested, whatever its author believes.** The
   visibility pass was in exactly that position at 5/15 and it was said out loud
   here rather than assumed away. It became observable only once the rules around
   it were right — which is also why it is now the *most* load-bearing thing in the
   crate by this measure (13 → 10).

## Two kinds of const-ness, which were one flag

`infer` took a single `is_const`, passed the *declaration's* const-ness and
documented as the *assertion* context. Two rules read it and they want different
answers. `compiler/isolatedDeclarationErrorsExpressions` settles it line by line:

| | `let` | `const` / `readonly` | `as const` |
|---|---|---|---|
| `` `s${1}` `` | clean | **TS9010** | **TS9010** |
| `[1, 2, 3]` | **TS9017** | *(no case)* | clean |

The template row is about **widening**. A `let` widens to `string`, which is
emittable; a `const` keeps the template literal type and would need its
substitutions' types. So here the *binding* decides, and a `readonly` property
behaves like `const` — the same file shows both, and the class half of it repeats
the pattern with `TS9012`.

The array row is about **assertion**. `[1, 2, 3]` is `number[]` under `let` and
`const` alike, and neither can be restated from written syntax without widening
and unioning the elements. Only `as const` makes it a `readonly` tuple of literal
types, each of which is written down.

One flag cannot serve both, and the corpus never caught it because **there is no
plain-`const` array in any `@isolatedDeclarations` case** — the empty cell above.
It is now `Ctx { fresh, asserted }`: `fresh` for the template rule, `asserted` for
the array rule, both set by `as const`.

The empty cell is filled by deduction, not measurement: const-ness of a binding
does not make an array literal a tuple, so `const x = [1, 2]` is `TS9017` like its
`let` counterpart. `rules.rs` says so at the definition of `Ctx`, so the next
person can overturn it with a baseline rather than by re-deriving the argument.

**How it was found, since the conformance suite could not.** A case with a
`TS9xxx` is excluded from `dts_emit`'s denominator by construction, so a
disagreement about *whether* to report one is invisible there.
`crates/tsr-declarations/tests/analysis_agreement.rs` asserts that this analysis
and the emitter's type builder accept the same constructs, and it caught the array
half on its first run — the emitter refused where the analysis did not. The fix
moves exactly one corpus case (`dts_reachable_target` 496 → 495), which is the
honest measure of its size and no argument against making it.

## Known approximations## Known approximations

Each of these is a place where upstream consults the checker and this analysis
guesses. They are the expected source of divergence under ADR-0021.

| Area | Upstream | Here |
|---|---|---|
| Visibility | `EmitResolver.IsDeclarationVisible` | reachability from the exports by top-level name; no scope chain, no declaration merging |
| Computed property names (`TS9038`) | the name expression's *type* | the name expression's *spelling*: a literal, optionally signed. This turned out to agree with upstream on every corpus case |
| Expando assignments (`TS9023`) | `IsExpandoFunctionDeclaration` | a top-level assignment to a property of a top-level name; cannot tell a function from any other binding |
| Enum initialisers (`TS9020`) | constant-expression evaluation | literals and arithmetic over them; a bare identifier is accepted, since it is nearly always an earlier member of the same enum |

Reference collection for visibility descends into everything except function
bodies, which over-approximates: a declaration may be judged that upstream would
have dropped. The direction is deliberate — an over-approximation reports a
diagnostic upstream does not and shows up as a conformance failure, while an
under-approximation stays silent.

## What is not done, and why each is out of reach here

Two diagnostics remain, in two cases, and neither is a gap in the rules — both
need information a single-file syntactic pass does not have. They are the honest
end of this slice rather than a to-do list.

**`TS9025` — implicitly adding `undefined` to a parameter type**
(`isolatedDeclarationsAddUndefined`). Upstream answers this with
`EmitResolver.RequiresAddingImplicitUndefined`, which is checker-backed, and the
corpus gives one discriminating pair: `foo(p = (ip = 10, v: number) => {})` is
clean while `foo2(p = (ip = 10 as T, v: number) => {})` — with `type T = number` —
is reported. The difference is what the initialiser's *type* is, not what it looks
like. `bd tsr-49v.1`'s audit named this as the item most likely to resist a
syntactic treatment, and it does. Deferred to Phase 4 rather than guessed at.

**`TS9026` — preserving an import for augmentations**
(`isolatedDeclarationErrorsAugmentation`). Deciding it means knowing that
`child1.ts` contains `declare module './parent'`, which is a fact about *another
file*. This needs the `Program`, not the checker, so it is reachable earlier than
`TS9025` — but not from `analyze(file, nodes)`.

Both are filed under `bd tsr-49v.2`.

## The reachable target, and ADR-0021's falsifier

[ADR-0021](../adr/0021-isolated-declarations-is-not-a-port.md) committed to a
falsifier rather than to the work:

> If `bd tsr-49v.3` finds that fewer than a few hundred of the 1,414
> `@declaration` cases are annotated enough for a checker-free emitter, then the
> artifact this phase ships is not usable on real TypeScript, and the phase should
> be cut rather than built.

Measured, by the `dts_reachable_target` suite:

| | |
|---|---|
| Cases whose `.js` baseline embeds an **emitted** `.d.ts` section | **1,162** |
| Of those, **reachable** — no declaration needs inference | **495 (42.60%)** |
| Blocked, needing inference somewhere | 667 |
| Not judged: configuration-varied baselines | 611 |
| Not judged: no `.js` emit baseline / no baseline at all | 3,094 |

> **Corrected 2026-08-04, and the correction is not a rounding.** This table first
> read **575/1,289 (44.61%)**. A baseline echoes every *input* unit before the
> emitted files, so a case with a `foo.d.ts` **input** — an ambient library, a
> `node_modules` stub — carries a `//// [foo.d.ts]` section upstream never emitted,
> and the suite counted it as declaration output. 127 cases are that shape.
>
> It surfaced in slice 4 rather than here: `dts_emit` asked the emitter for
> `foo.d.d.ts` and reported 215 failures reading "emitted x, upstream did not",
> every one of them the harness's fault. The discriminator is now exact rather than
> heuristic — a declaration section is output iff no input unit has that name — and
> both suites apply it. See
> [declaration-emit.md](declaration-emit.md#the-denominator-was-wrong-and-by-more-than-a-rounding).
>
> The same trap as the 617 no-output cases and the 611 configuration-varied ones:
> **the presence of a file is not evidence of what produced it.**

**The falsifier does not fire.** 496 is comfortably above "a few hundred", and it
is 43% of the population rather than a tail. Phase 3.5 proceeds, `bd tsr-49v.4` —
the printer — was worth its cost, and `bd tsr-49v.6` — the emitter — is built on
top of both. See [declaration-emit.md](declaration-emit.md).

### The denominator is 1,289, not 1,894, and the difference is not a rounding

`bd tsr-49v.3` and ADR-0021 both quote 1,894 `.js` baselines carrying a `.d.ts`
section. That figure counts baseline **files**; this suite counts **cases with a
plain baseline**, and the two differ by exactly the configuration-varied ones:

```
1,900 baseline files with a .d.ts/.d.mts/.d.cts section
  611 of them configuration-varied — case(target=es5).js
1,289 plain — the suite's denominator
```

The counts reconcile exactly, which is what validates the section reader against
something other than itself. Two corrections fall out: the total is **1,900**, not
1,894 — the earlier grep matched `.d.ts` only and missed 495 `.d.mts`/`.d.cts`
sections — and the 611 varied cases are not unreachable, merely unjudged until
`bd tsr-bb4.1` runs per-configuration.

### Which direction the number is wrong in

ADR-0021 calls this an upper bound, and for byte-matching it is: a reachable case
is one where nothing needs *inferring*, not one we could emit correctly, because
the text also depends on visibility decisions upstream makes through
`IsDeclarationVisible`.

But it is *also* an under-count of "cases needing no inference", and that pull is
in the opposite direction. The analysis was validated on a 15-case oracle and is
here applied to 1,289; its deliberately-approximate rules can over-report, and
each false positive removes a case from the target. That is not hypothetical —
running it corpus-wide immediately exposed one such class. The expando rule
(`TS9023`) fired on `module.exports = […]`, which is CommonJS export assignment
rather than a property bolted onto a function. Eight cases rested on that rule
alone; excluding `module` and `exports` moved the target from 569 to 575, and cut
`TS9023`'s reach from 43 cases to 21.

So: an upper bound on what could be *emitted*, and a lower bound on what needs no
*inference*. Both, at once, for different reasons.

### What blocks the other 666

Distinct `TS9xxx` codes across the blocked cases, by how many cases each appears
in. The counts below were measured against the pre-correction denominator of 714
blocked cases and are left as measured rather than silently rescaled; the shape,
not the absolute count, is what they are for:

| Code | Cases | |
|---|---:|---|
| `TS9010` | 318 | variable needs an annotation |
| `TS9007` | 289 | function return type |
| `TS9008` | 124 | method return type |
| `TS9011` | 90 | parameter type |
| `TS9038` | 38 | computed property name |
| `TS9013` | 26 | expression not inferable |
| `TS9021` | 24 | extends clause is an expression |
| `TS9023` | 21 | expando assignment |
| others | ≤19 each | |

This is what ordinary TypeScript looks like when it is not written for
`isolatedDeclarations`, and it is the expected shape rather than a defect: the
corpus was not written to this constraint. Nothing here is actionable for the
emitter — these cases are correctly out of scope.

### Mutations

| Mutation | Result |
|---|---|
| Analysis silenced entirely | **1,289/1,289** — the analysis accounts for exactly the 714 |
| `.d.mts`/`.d.cts` not counted as declarations | 570/1,283 — denominator loses 6 |
| Every unit parsed as TypeScript, ignoring `.tsx` | 580/1,289 — **5 spurious passes** |
| Header line treated as a section | no change |

The third is worth keeping: mis-parsing a `.tsx` file as TypeScript makes the
number go *up*, because a tree that is wrong rather than merely different can
swallow the declarations that would have been analysed. A suite that reads higher
when its input handling is broken is the kind of thing only a mutation finds.

The fourth changes nothing, and that is a fact about the code rather than a hole
in the suite: `parse` tests for a header before it tests for a section, so the
guard inside `section_name` is unreachable through it. It is covered by a direct
unit test instead — the same treatment the visibility pass got when the corpus
could not see it.
