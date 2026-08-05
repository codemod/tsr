# Substitution: `instantiate_type` and the reverse index

Status: living. Covers `Checker::instantiate_type`
(`crates/tsr-checker/src/inference.rs`) and
`Checker::type_reference_targets` (`crates/tsr-checker/src/checker.rs`).

## The forcing constraint

`docs/architecture/checker-notes-infer.md` previously recorded that
substituting into `T[]` and `C<T>` required changing the type representation.
**That was wrong, and this document is the correction.**

`create_type_reference` (`crates/tsr-checker/src/declared.rs`) already interns
on `(symbol, arguments)` — that is what makes `string[]` and `Array<string>`
one type. The pair therefore *exists*. It is the intern map's **key**, so it is
reachable from the pair but not from the `TypeId`, and substitution is exactly
the operation that starts with a `TypeId` and needs the pair. Nothing about the
model was missing; one direction of an existing edge was.

The measured size: of the 40 calls in the corpus with written type arguments
whose callee has a written return annotation, 14 return a bare type parameter
(already answered), 5 mention no type parameter (already answered), and **21
return a type that merely contains one** — `T[]`, `[T, U]`, `C<T>`. The same
shapes gap on the *inference* path, so this is a shared dependency of two
paths rather than an adjacency between them.

Concentration, measured over
`vendor/typescript-go/testdata/baselines/reference/submodule` on `.types`
assertion lines whose subject is a call and whose type is `X[]` or `C<Y>`:
**2,389 lines across 545 files, top ten files holding 722 (30%)**. Distributed,
which is the shape that pays the line gradient and the wrong shape for flipping
whole cases.

## What was built

`type_reference_targets: TypeId -> (SymbolId, Vec<TypeId>)`, written on
`create_type_reference`'s **miss** path, so it costs one insert per distinct
reference rather than per lookup. The ADR-0003 move: a cyclic-ish back-edge
lives in an id-keyed side table, not on the type.

`instantiate_type(id, map, parameters, names)` has four arms and a floor:

1. identity — `id` is a mapped type parameter;
2. unchanged — `id` mentions no type parameter of this signature;
3. reference — arguments substituted, rebuilt through `create_type_reference`
   so interning is preserved (`Array<number>` and `number[]` stay one type);
4. union — constituents substituted, rebuilt through `get_union_type`
   (`crates/tsr-checker/src/unions.rs`), because `T | string` with
   `T := string` must collapse and only that function knows how;
5. everything else — **`errorType`**.

Upstream is `Checker.instantiateType` (`checker.go:22100`) and
`instantiateTypeWorker` (`checker.go:22220`), which read the target and
`resolvedTypeArguments` straight off a `TypeReference` and so need no such
table.

The inference path was generalised at the same time: it now infers a candidate
for *each* type parameter from bare positions and substitutes, instead of
requiring the return type to *be* a type parameter.

## The alternatives

**Put the pair on the type (`TypeData::Reference { symbol, arguments }`).**
This is what upstream does and what the superseded note assumed was necessary.
It would also let the printer stop precomputing text and let member lookup
instantiate. Rejected *for now* because it changes a variant every arm of
`printing` and `members` matches on, while the reverse index buys the same
substitution for one field. **It wins the moment a second consumer needs the
pair** — instantiated members (`bd tsr-4sc.7`) is the likely one — at which
point the side table should be deleted rather than kept alongside.

**Substitute on the printed string.** Rewrite `T[]` to `number[]` textually.
Rejected: it produces a type that prints right and has no identity, so
`Array<number>` written elsewhere would be a different `TypeId` that compares
unequal. That is the failure mode `checker.md` warns about — answering a type
"with another type that merely prints alike".

## Consequences accepted

- **Tuples and function types are still gaps.** `[T, U]` and `(x: T) => U` are
  not built through `create_type_reference`, so there is no pair to reverse.
  This is a *property of the intern key*, not an oversight, and it is the
  boundary the negative test pins.
- A tuple return type gaps *earlier* than substitution — in
  `get_signature_from_declaration`, which answers `None` when the return
  annotation does not resolve — so the call never reaches this module at all.
  Measured, by writing the assertion and watching it panic on "a signature".
- **An unmapped type parameter answers `errorType`,** not a half-substituted
  type: `m<T, U>(x: T): C<U>` inferred from one argument gaps rather than
  printing `C<U>`.
- **No recursion limit,** deliberately. Upstream carries `instantiationDepth`
  and `instantiationCount` because a mapper can build unboundedly deep types
  from bounded source. Nothing here can: every argument reached came from a
  written type node, so recursion is bounded by source nesting. **That ceases
  to hold the moment a generic's members are instantiated** — `bd tsr-el3.2`.

## How this would be shown wrong

- If the corpus gain on written-type-argument calls is materially below the 21
  the histogram predicts, the shapes in that bucket are not the ones counted
  and the bucket needs re-enumerating rather than the code re-tuning.
- If a second consumer of the `(symbol, arguments)` pair appears, the side
  table is the wrong home and `TypeData::Reference` should be built.
- If `mentions_type_parameter`'s printed-text scan (documented in
  `checker-notes-infer.md` as deliberately over-eager) produces a false
  *negative*, arm 2 returns a type unchanged that should have been rebuilt —
  a wrong line rather than an absent one. That is the one place in this
  function where the never-wrong-only-absent property rests on a heuristic.
