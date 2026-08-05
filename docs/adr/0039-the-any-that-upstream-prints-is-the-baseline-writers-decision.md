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
