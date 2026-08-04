# ADR-0022: The declaration transform *is* a port — around a named `EmitResolver` seam

- **Status:** accepted
- **Date:** 2026-08-04
- **Upstream pin:** `vendor/typescript-go` @ `5b1047d10`
- **Relates to:** [ADR-0001](0001-idiomatic-rewrite.md) (the "port, don't reinvent"
  default this returns to), [ADR-0021](0021-isolated-declarations-is-not-a-port.md)
  (which departed from it for `tsr-dts`, and whose third falsifier this answers),
  [ADR-0003](0003-tree-plus-side-tables.md) (why a synthesized node needs a side-table row)
- **Scope:** `tsr-declarations`. What is ported, where the checker's half is
  isolated, and what happens to this crate when the checker arrives.

## The forcing constraint

Phase 3.5 had two green components and nothing shippable between them:
`tsr-dts` at 13/15 on the `isolatedDeclarations` oracle, `tsr-printer` at 99.52%
on the round trip, and no `.d.ts` in existence. The analysis says *which
declarations need inference*; the printer writes *whatever tree it is handed*.
Nobody built the tree.

[ADR-0021](0021-isolated-declarations-is-not-a-port.md) is easy to over-read at
this point. It concluded that upstream's declaration emit "cannot ship before
Phase 4", and it is right about the *rules*. It is not right about the transform,
and the distinction is measurable:

| | |
|---|---|
| `internal/transformers/declarations/` | **4,160 lines** |
| …of which reach `EmitResolver` | 9 distinct methods, at ~30 call sites |
| `internal/compiler/emitter.go`'s declaration path | prints through `internal/printer/printer.go` — **there is no separate `.d.ts` printer** |

The dispatch (`visit`, `visitDeclarationStatements`,
`transformTopLevelDeclaration`, `visitDeclarationSubtree`, and the twenty-odd
`transformX` functions), the elision, and the modifier arithmetic
(`ensureModifiers`, `ensureModifierFlags`, `maskModifierFlags`) are pure syntax.
That is two thirds of the file and none of the difficulty.

And the checker's half is not scattered: it is one Go interface,
`printer.EmitResolver` (`internal/printer/emitresolver.go:77`).

## Decision

**Port `internal/transformers/declarations` under ADR-0001's default, and express
its dependency on the checker as a trait: `tsr_declarations::EmitResolver`.**
Supply `SyntacticResolver` as the checker-free implementation for Phase 3.5;
Phase 4 supplies a checker-backed one beside it.

Every item in the crate names its upstream counterpart, as
[conventions.md](../conventions.md) requires — and here, unlike in `tsr-dts`, the
correspondence is real, so the drift tracker can use it.

### Why the seam is a trait rather than a set of inlined `if`s

The alternative — inline the syntactic answers where upstream calls the resolver —
is shorter today and unpickable later. ADR-0021 committed to a falsifier that this
would trip:

> **Reconciliation at Phase 4 turns out to be a rewrite.** If, once
> `CreateTypeOfDeclaration` exists, `tsr-dts` shares nothing with the ported
> declaration transform, then the year of early shipping cost us a subsystem's
> worth of throwaway work.

Naming the seam answers that falsifier *in advance* rather than waiting to be
surprised by it. When the checker lands, what has to be reconciled is one trait
with two implementations — not two subsystems. The transform above the seam does
not know which resolver it has, so it does not change at all.

### What the syntactic implementation answers, and what it refuses

`SyntacticResolver` covers 7 of the 9 methods from syntax alone. Two it cannot:

| Method | Syntactic answer |
|---|---|
| `CreateTypeOfDeclaration` | the expression's *shape*, rewritten — `{ a: 1 }` → `{ a: number; }`, never through a type |
| `CreateReturnTypeOfSignatureDeclaration` | **refused always.** A return type is the type of what the body returns, and the body is what a `.d.ts` drops |
| `IsImplementationOfOverload` | **refused always** (`false`), which keeps a declaration upstream elides — a visible extra line rather than a silent omission |

A refusal is not silence. Upstream's `ensureType` emits `any` when its node
builder returns nothing, and this port does the same, so the *shape* of the output
never depends on which resolver answered. Where it happened is recorded in
`DeclarationEmit::inference_required`, which is what makes "the emitter guessed" a
counted event.

### The scope of the type builder is pinned to something external

The obvious failure mode of a syntactic type builder is that it quietly handles a
little more or a little less than the analysis admits. So the scope is not a
judgement call: **it handles exactly the expressions `tsr_dts::rules::infer`
returns `Ok` for**, and `crates/tsr-declarations/tests/analysis_agreement.rs`
asserts both directions over a table of constructs.

That test earned its place immediately. It found that both crates encode the same
conflation — a `const` *declaration* read as a const *assertion* — and that they
therefore agreed while both being wrong. `const b = [1, 2]` has type `number[]`,
not `readonly [1, 2]`; only `as const` enters a const context, and
`compiler/isolatedDeclarationsLiterals` pairs the two spellings of one object
literal to say so. The emitter is corrected; the analysis is filed as
`bd tsr-49v.2.6`, because changing it moves two other suites and needs its own
measurement.

The corpus could not have found this: a case with a `TS9xxx` is excluded from
`dts_emit`'s denominator by construction, so a disagreement about *whether* to
report one is invisible there.

## Consequences accepted

**The emitter is measured on a population the analysis defines.** `dts_emit`'s
denominator is the reachable set, so a change to `tsr-dts` moves this suite's
denominator. That coupling is deliberate — the alternative, counting the 545
inference-needing cases as failures, buries the emitter's own rate under cases it
was never in reach of — but it means the two numbers must always be read together.

It is also why a second gate was added rather than this one widened. `dts_shape`
compares the declaration *sequence* — kind, name and order — so it can judge the
cases `dts_emit` must skip, and its denominator depends on nothing but the corpus.
The two are complementary rather than redundant, and the mutation table in
[declaration-emit.md](../architecture/declaration-emit.md#mutations) demonstrates
it: four mutations move only `dts_emit`, four move both, none moves only
`dts_shape`.

**The rate is 47.06% byte-exact (160/340) and 67.76% structural (618/912), and the
residue is mostly text rather than rules.** Comments are dropped (the printer emits
none), module specifiers are re-quoted, and visibility is approximated. Each is
recorded in [declaration-emit.md](../architecture/declaration-emit.md) with its
case count.

> **Corrected.** This ADR first stated 36.89% (180/488). Both parts of that figure
> were wrong: the suite decided which unit paired with which baseline section
> *while* emitting, so the denominator depended on the emitter, and it
> short-circuited on the first failing unit, so a later unit's skip was never
> reached. The comparison itself is unchanged and still byte-exact. See
> [declaration-emit.md](../architecture/declaration-emit.md#three-denominators-were-wrong-in-three-different-ways).

**We now hold two `.d.ts` type builders in mind at once.** `tsr_dts::rules::infer`
decides *whether* a type is apparent and `tsr_declarations::type_builder` decides
*what it is*. They are the same grammar written twice, and the agreement test is
the only thing keeping them honest. Merging them is the obvious refactor and is
deliberately not done: the analysis must stay usable with no arena and no
`NodeTable` to write into.

## How we would know this was wrong

1. **The seam turns out to be in the wrong place.** If Phase 4's checker cannot
   implement `EmitResolver` as defined here without reshaping the transform above
   it, then the trait captured our stand-in's needs rather than upstream's
   interface, and ADR-0021's third falsifier fires after all.
2. **The residue stops being text.** If the failures left in `dts_emit` turn out to
   need inference rather than formatting — that is, if raising the printer's
   fidelity stops moving the number — then the reachable target was measuring
   something other than what the emitter can do.
3. **The agreement test never fails again.** A test that only ever passes is not
   evidence. It caught a real defect on its first run; if a year of changes to
   either crate never trips it, its table has stopped tracking either one.

## Alternatives

**(a) Write the `.d.ts` emitter against the spec, as `tsr-dts` was.** Rejected on
the measurement above: the reason ADR-0021 gave for departing from ADR-0001 —
"there is no syntactic predicate upstream to port" — is a statement about the
rules, and two thirds of `transform.go` is not rules. Reinventing that part would
throw away a structure that already works and that Phase 5 has to match anyway.

**(b) Wait for the checker and port the whole thing at Phase 4.** The honest
alternative, and the one this ADR is a bet against. It wins if the seam turns out
to be wrong (falsifier 1), because then the port around it was scaffolding. It
loses on the terms Phase 3.5 was created under: nothing ships for another year.

**(c) Emit text directly from the analysis, with no intermediate tree.** Shorter —
no factory, no side-table registration, no `Freshness` threading. Rejected because
upstream's `.d.ts` output *is* `internal/printer/printer.go`, and Phase 5 needs
that printer byte-exact regardless. A separate text path would be a second printer
to keep in step with the first, which is the cost this port exists to avoid.
