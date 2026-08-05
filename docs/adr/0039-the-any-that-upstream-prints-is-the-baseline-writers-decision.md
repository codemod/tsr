# 0039 — The `any` upstream prints is the baseline writer's decision, not the checker's

**Status:** accepted, 2026-08-05
**Supersedes:** [0038](0038-errortype-prints-error-and-the-corpus-has-a-ceiling.md), whose
decision stands and whose reasoning and ceiling estimate do not.

## What 0038 got wrong

ADR-0038, written earlier the same day, asserted:

> The node builder renders anything carrying `TypeFlagsAny` as the `any`
> keyword, so **upstream's `errorType` prints `any` in a `.types` baseline.**

and concluded that ~6,000 lines are **unreachable by construction**, giving the
metric a ceiling near 98.7%.

**Upstream prints both.** In one file, for one element:

```
conformance/inlineJsxFactoryOverridesCompilerOption.types
><h></h> : error      <- errorType, printed "error"
>h : any              <- the SAME errorType, printed "any"
```

The choice is made by the **baseline writer**, not the checker.
`internal/testutil/tsbaseline/type_symbol_baseline.go:378` prints
`t.AsIntrinsicType().IntrinsicName()` — the literal string `"error"` — and only
falls through to the node builder, which renders `any`, when a condition of
**eight guards** holds:

```go
if !walker.hadErrorBaseline &&
    checker.IsTypeAny(t) &&
    !ast.IsBindingElement(node.Parent) &&
    … && !isIntrinsicJsxTag(node, sourceFile) && …
```

So the mechanism is not "upstream renders errorType as `any`". It is "upstream
renders errorType as `error`, **except** in eight enumerated positions".

## What this changes

**The lines are not unreachable. They are unported.**
`crates/tsr-conformance/src/types_producer.rs` renders unconditionally through
`checker.type_to_string(t)`. It has no counterpart to any of the eight guards —
`grep` for `had_error`, `is_type_any`, `intrinsic_name`, `intrinsic_jsx` returns
zero. Porting that condition is ordinary work in the producer, which is exactly
where upstream keeps it, and it is the same kind of compensation already ported
once (the `extends`-clause rule from `type_symbol_baseline.go:371`).

The ceiling claimed by 0038 is therefore **not established**. Some residue may
remain — a case where our checker reaches `errorType` by a different path than
upstream's — but the ~6,000-line figure was measuring an unported guard, not a
limit.

## What 0038 got right, and what stands

**The decision stands: do not change what the checker prints.** `errorType`
still prints `error` in `crates/tsr-checker`, and 0038's rejection of rendering
it as `any` for comparison is still correct, for the reason it gave — upstream
prints `any` for a *genuine* `any` too, so a blanket substitution would falsely
credit ~6,100 lines that our port merely failed on.

This ADR does not weaken that. It relocates the fix: **the guard belongs in the
producer, per-node, as upstream has it** — not in the checker, and not as a
blanket rendering change. A per-node guard credits only the positions upstream
credits.

Independently confirmed by the same evidence: making a checker arm answer
`anyType` for a JSX tag name would pin an answer upstream's *checker* does not
give. Upstream's `checkIdentifier` on an intrinsic tag name resolves nothing and
returns `errorType`; `getIntrinsicTagSymbol` (`internal/checker/jsx.go:1216`)
caches on the opening element, not the tag name, and never feeds that path.

## Consequences accepted

- The producer grows a per-node condition it did not have. That is a faithful
  port of upstream's writer rather than a compensation invented here.
- **Scope warning.** Porting only the `isIntrinsicJsxTag` guard yields a clean
  ~1,780 lines across 288 baselines. Porting the honest whole condition moves
  non-JSX rows anywhere we print `error` and upstream prints `any`, and that
  population has not been measured. Measure before calling it one item.
- Any target quoted against 0038's ~98.7% ceiling should be requoted. The
  ceiling is not known.

## How we would know this is wrong

- If the eight guards, ported faithfully, leave most of the ~6,000 lines still
  mismatched, then a real ceiling exists after all and 0038's estimate was
  closer than this ADR allows. **Measure the residue after porting, not before.**
- The discriminating fixture for the JSX guard specifically: a lowercase tag
  name that *does* resolve to a value in scope — `<foo/>` where `foo` is a
  local. The corpus has 32 lines printing `() => any` and 24 printing `typeof
  foo`. If our implementation prints `any` for those, it is reaching `errorType`
  by falling off a dispatch rather than by upstream's resolution miss, and the
  string matches while the path differs.

## Resolved, 2026-08-05: the ceiling is 29,936 and the deliverable is ~1,500

This ADR left the ceiling unknown and asked for a per-guard measurement before
scoping. That measurement exists (`crates/tsr-conformance/examples/writer_guards.rs`,
commit `fba67d2`, corrected in `edb37c3`). **Ceiling 29,936 lines / +6.38
points. Honest deliverable ~1,500.**

> **Corrected.** The figures first recorded here (40,759 / +8.69) came from a
> probe that re-implemented the harness and so loaded no `lib.*.d.ts`. Rewired
> and re-measured; the ceiling falls 27%. The verdict is unchanged and the
> control rate that carries it moved 7.0% -> 7.06%. See
> `docs/architecture/checker-notes-guard.md`.

**`hadErrorBaseline` must not be ported**, and the reason is a *kind* difference
rather than a size one — which is what this ADR got wrong by treating the eight
guards as one item:

- It is `len(result.Diagnostics) > 0` (`testrunner/compiler_runner.go:501`),
  **case-scoped**, true for 56.3% of cases, firing at every node regardless of
  position. This ADR justified relocating the work into the producer on the
  ground that *"a per-node guard credits only the positions upstream credits"*.
  `hadErrorBaseline` does not qualify. Porting it is precisely the blanket
  substitution [0038](0038-errortype-prints-error-and-the-corpus-has-a-ceiling.md)
  refused, wearing the clothes of a positional guard.
- The control bucket proves it empirically. Where the fast path is live and no
  guard fires, on lines we print `error`, upstream's type is an any-flagged
  intrinsic 5,537 times and is `errorType` on **391** — 7.06%. The other 93% are
  a genuine `anyType` upstream computed and we did not.
- 33.4% of the 29,936 is **one case**, `largeControlFlowGraph`, which
  `conventions.md` already records as upstream computing `any` through
  `autoArrayType` machinery this port lacks. Provably false credit, from
  evidence already in the repository.

So 0038's decision survives in the place that matters. What 0039 correctly
relocated was the *positional* guards; the case-scoped one was never a rendering
rule for a position.

Ported: the label-name arm (`53588b1`), 209 claims, 0 residue. Global scope
augmentation and meta property are **empty, not small** — nothing to port.

### The per-guard numbers are floors, not values

The label arm predicted 209 and converted **597** (that pair is itself lib-less
and unverified; see the note). Attribution follows upstream's
conjunction order, so the dominant arm was claiming lines a subordinate arm would
convert — 391 of the 600 sat in errors-baseline cases. **Every positional figure
in that table is an under-estimate for the same reason.** The `hadErrorBaseline`
verdict is unaffected: it is the arm that absorbs, not one absorbed.
