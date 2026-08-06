# Structural type-argument inference — `inferTypes`, the slice and its bar

`bd tsr-g30h`. Sixth session. This page is written **before the arm exists**;
§4 is the keep/revert bar and it is committed ahead of any checker behaviour, in
the commit that carries the probe.

`crates/tsr-checker/src/inference.rs`'s module doc states the cliff this page
attacks: of 1,145 declarations initialised by a call to a locally declared
generic function, **53% have no type parameter written bare in a parameter
position**, so the candidate has to be dug out of the argument's type. That is
`inferTypes` (`vendor/typescript-go/internal/checker/inference.go:53`), and
today `check_generic_call` ports only the "written bare" rule.

## 1. What this port can and cannot decompose, and why that picks the slice

The choice of slice is not a judgement about which shapes are common. It is
forced by a data-model fact recorded in
[ADR-0003](../adr/0003-tree-plus-side-tables.md) and its consequences: **a
type's payload here is a printed string.** `T[]`, `C<T>`, `{ a: T }` and
`(x: T) => U` are all a `TypeData::Named` or `TypeData::Anonymous` carrying
text. Inference needs the *structure*, so a shape is decomposable here exactly
when some side table already records how it was built.

Two such tables exist, and both were built for **substitution**, the opposite
direction:

| table | records | built for |
|---|---|---|
| `Checker::type_reference_targets` | `TypeId → (symbol, arguments)` | `bd tsr-el3` / `tsr-4qx`, so `instantiate_type` can rebuild `C<T>` |
| `Checker::signature_types` | `TypeId → Vec<Signature>` | `bd tsr-0hc`, so `instantiate_signature_type` can rebuild `(x: T) => U` |

That is the whole budget, and it is a large one: `T[]` **is** `Array<T>` through
`create_type_reference` (`declared.rs`), and an array literal's type is built by
the same function (`array_literals.rs`), so `T[]` against `number[]`,
`Promise<T>` against `Promise<string>` and `C<T>` against `C<X>` are all one
arm. Everything else — object-type members, index signatures, tuples, mapped
and conditional types, `keyof` — has no reverse index and is refused whole.

## 2. The counterfactual, which is the sizing

`crates/tsr-conformance/examples/infergen.rs`. It does **not** count
`callgate.rs`'s 2,324-line gate. It runs the proposed candidate walk, substitutes
the result through the **real** `instantiate_type`, prints it through the **real**
`type_to_string`, and compares that string to the baseline character for
character. `STATUS.md`'s fourth rule and `docs/conventions.md`'s counterfactual
rule both require this; four builds in the fifth session over-converted a sized
row and two rows dissolved under a counterfactual.

**Own-node lines only.** A variable initialised by such a call, and every read
of it, is a *cascade* line whose baseline text is the declaration-site form.
`newgen.rs` took the same cut, forecast 166 and converted 686. Cascade is
upside and is deliberately outside the floor.

At `1e40af4`, over **1,391** classified lines — own node is a call, the callee
resolves through `resolve_call_signature` to a **single generic candidate**, and
the line is a gap today:

| | lines |
|---|---:|
| REFUSED — no candidate, the type parameter stays unmapped | 489 |
| an argument's own type is a gap (someone else's item) | 462 |
| **CONVERTS — forecast matches the baseline exactly** | **131** |
| REFUSED — the return type is not rebuildable by `instantiate_type` | 86 |
| REFUSED — a `null`/`undefined` candidate, needs `getWidenedType` | 63 |
| a rest parameter — `getSpreadArgumentType`, unported | 56 |
| REFUSED — two candidates disagree for one type parameter | 49 |
| a spread argument — no position to land on | 39 |
| **MISS — forecast differs** | **16** |

```
C1 classified-but-not-gap : 0   (expect 0)
C2 buckets sum 1391 vs classified 1391
C3 bare-only rule converts: 0   (expect 0)
```

**131 converted against 16 wrong: 8.2 gained per new wrong**, on own-node lines.

### 2.1 C3 is the control that is pinned to the construct, not to arithmetic

C2 is arithmetic and, as `docs/conventions.md` records, an arithmetic control
over a partition cannot see that the partition is wrong. C3 can: it runs the
**rule already shipped** — a candidate only where the parameter's type *is* the
type parameter — through the same harness. Every classified line is a gap
today, so if the shipped rule converted any of them the harness would be
disagreeing with `check_generic_call` and the CONVERTS column would be
measuring the probe. It reads **0**.

### 2.2 Three refusals the measurement bought, each with its number

Each of these was **added because the misses named it**, and each cost
conversions. They are recorded here because the ratio without them is the
number a less careful build would have shipped on.

- **A reference source facing a reference constituent it does not share a
  symbol with.** `Promise<void>` against `TResult1 | PromiseLike<TResult1>`:
  upstream matches the *reference* constituent through `Promise`'s base type
  and infers `void`; this port has no base-type walk, so the naked variable
  swallows the whole thing and prints `Promise<Promise<void>>`. Refusing it
  moved the table from **161 converts / 59 misses (2.7:1)** to **157 / 35
  (4.5:1)**.
- **A `null` or `undefined` candidate.** With `strictNullChecks` off upstream
  widens it to `any` (`getWidenedType`, `checker.go:16090`); nothing on this
  port's inference path widens. Nineteen of the remaining misses were
  `want Promise<any>, forecast Promise<null>` and its variants. Refusing it
  moved **157 / 35 (4.5:1)** to **131 / 16 (8.2:1)** — it cost 26 conversions
  and it is still right, because a gap beats a wrong answer.
- **A union target carrying more than one naked type variable, or a union
  source.** That is `inferToMultipleTypes`' branch that strikes matched
  constituents and re-unions the remainder, which needs `get_union_type` plus a
  subtype decision this port does not have (`removeSubtypes`, refused at
  `bd tsr-eak`).

### 2.3 What the largest refused bucket is, so it is not re-derived

The 489 no-candidate lines split by *where the walk stopped* — the target's
shape against the source's:

```
   47  (no parameter position mentions the type parameter at all)
   23  target named `T`      :: source named `A`      — two type parameters, no relation
   20  target union (`then`) :: source `null`         — the argument is the null literal
   20  target union (`then`) :: source `undefined`
   11  target named `T`      :: source `""`
    7  target signature `(a: A) => B` :: source `<T>(a: T) => T[]`  — needs getErasedSignature
    4  target named `{ keys: T[]; }`  :: source named `{ keys: string[]; }`  — object members
    4  target named `{ value: T | undefined; }` :: source named `{ value: number | undefined; }`
    4  target reference `Box<T>` :: source named `T`
```

and the 86 unrebuildable ones by return shape: `[T, U]` 32, `[T, U, V]` 10,
`T & U` 8, `{ value: V; }` 6, construct/call type literals 9. **Tuples are the
single largest, and they are an `instantiate_type` limit, not an inference
limit** — `[T, U]` has no intern key to reverse, exactly as `inference.rs`'s
module doc already says. The object-member family (`{ keys: T[] }` against
`{ keys: string[] }`) is the next coherent arm and needs a members reverse
index; it is not built here.

## 3. What will be built

In `crates/tsr-checker/src/inference.rs`, replacing the bare-position loop in
`check_generic_call` with `infer_from_types` — `inferTypes`/`inferFromTypes`
(`inference.go:53`, `:1236`) reduced to four arms:

1. **identity** — the target *is* a type parameter; candidate is the source.
   This is what ships today, unchanged.
2. **`inferFromTypeArguments` (`inference.go:1046`)** — two references to the
   same target symbol with equal argument counts, argument for argument.
   Variance is not consulted; every position is walked covariantly, and a
   disagreement between positions gaps rather than picks.
3. **`inferToMultipleTypes` (`inference.go:700`)**, restricted: the target is a
   union with **at most one** naked type variable, the source is not a union,
   and no reference constituent faces a reference source of a different symbol.
   The source is inferred into every constituent; the ones that cannot match
   contribute nothing.
4. **`inferFromSignature` (`inference.go:1112`)** — one call signature each,
   neither generic, no rest parameters, positions paired up to the shorter list
   (`applyToParameterTypes`, `inference.go:1140`), then the return types.

Everything else contributes no candidate, which leaves its type parameter
unmapped, which gaps the whole call. The existing resolution rules are kept:
two disagreeing candidates gap, a candidate that is itself `errorType` gaps,
and the `fillMissingTypeArguments` default fallback keeps its
`structural_source_supplied` guard.

Plus one new resolution rule: **a `null` or `undefined` candidate gaps**
(§2.2).

## 4. The bar, registered before the arm

Measured over a `git stash` pair with `examples/casedelta.rs` and
`examples/wrongdelta.rs`. Baseline at `1e40af4`: `wrongdelta` TOTAL **42,417**.

**Keep only if all four hold. Any leg fires → the first hypothesis is that the
build is wrong, the second that a premise here is wrong, and there is no
third.**

| leg | rule | what would make it non-zero |
|---|---|---|
| **1 — net floor** | net **≥ +120** lines | The forecast is 131 own-node conversions and cascade is excluded. Below 120 means own-node conversions did not materialise — the probe's walk and the arm's diverge, which is the failure C3 cannot see because C3 only proves the *old* rule agrees. |
| **2 — lost** | **lost ≤ 5** lines | This has a denominator, unlike the `&&` build's empty leg (`docs/conventions.md`). `check_generic_call` answers today through the bare rule; adding structural positions can find a *second* candidate for the same type parameter that disagrees with the bare one, which turns a right line into a gap. A loss is therefore evidence of a wrong rule, not of a bad trade, which is why the bar is near-absolute rather than a ratio. |
| **3 — cases** | cases **regressed < finished** | A whole-baseline verdict can flip either way; the arm touches every generic call in the corpus, including ones currently answered. |
| **4 — gap→wrong** | **gained ≥ 6 × new wrong**, by `comm -13` on `wrongdelta` | The forecast is 8.2:1 on own-node lines; 6 leaves headroom for cascade lines converting and missing at different rates, which is the one asymmetry the own-node cut cannot measure. `casedelta` is blind to gap→wrong by construction. |

**Falsifier for the whole page.** The claim is that *structural inference over
the two existing reverse indices converts more than it breaks*. It is refuted
if the corpus pair shows fewer than 120 net lines, or more than 5 lost lines, or
new wrong lines exceeding a sixth of the gain. Any of those and the arm reverts
and this page records the number, as `STATUS.md` §5 requires.

**And the residual wrong lines get read even if leg 4 passes.** That discipline
found a one-line printer defect worth +114 in the fifth session and a real
gap→wrong that a ratio had hidden.

## 5. Scored against the bar

Corpus pair over a `git stash`, `casedelta` and `wrongdelta` at both ends.

```
net            +372
gained lines    375
lost lines        3
cases gained     40
cases lost        2
cases finished    6
cases regressed   0
wrongdelta     42,417 -> 42,423   (Δ +6: 33 new wrong, 27 fixed)
```

| leg | rule | measured | verdict |
|---|---|---|---|
| 1 — net floor | ≥ +120 | **+372** | **pass** — 2.8× the own-node forecast, the cascade `newgen.rs` predicted |
| 2 — lost | ≤ 5 | **3** | **pass**, and see §5.1: it fired twice at 15 and 9 before it passed |
| 3 — cases | regressed < finished | **0 < 6** | **pass** |
| 4 — gap→wrong | gained ≥ 6 × new wrong | **375 ≥ 198** (11.4:1) | **pass** |

**+372 against a forecast of 131** is 2.8× — inside the same band as
`newgen.rs`'s 166 forecast to 686 measured (4.1×), and for the same reason: the
own-node cut excludes every cascade line, and a call whose type is now known
types the variable it initialises and every read of it.

### 5.1 Leg 2 fired twice, and both times the build was wrong

`docs/conventions.md`: *when a registered bar fires, the first hypothesis is
that the build is wrong, the second that the bar's premise is wrong, and there
is no third.* It was the first hypothesis both times, and the leg is the only
reason either defect was found — neither shows up in the net, which was already
+311 on the first run.

**Run 1 — 15 lost.** The `null`/`undefined` candidate refusal had **no
strictness test**. `getWidenedType` widens `null` to `any` only with
`strictNullChecks` *off*; under strict mode upstream keeps `null`, so the
refusal turned right lines into gaps in `conformance/strictNullChecksNoWidening`
(4) and `compiler/undefinedInferentialTyping` (2). The counterfactual could not
see this: it measures gap lines only, and a right line turning into a gap is
invisible to it by construction. Gating on `strict_null_checks`: **net +365,
lost 9.**

**Run 2 — 9 lost.** Two separate defects, both in candidate *resolution*:

- `f([], 3)` against `<T>(arr: T[], elemnt: T) => T` produced `never` from the
  array position and `3` from the bare one, and the disagreement rule gapped
  the call. Upstream unions the candidates and **`never` is the union's
  identity** — `add_type_to_union` already drops it — so a `never` candidate
  beside any other can be struck without deciding anything a subtype relation
  would have to decide. **net +369, lost 5.**
- `f1(1, "hello")` against `<T>(x: T, y: string | T) => T` poured `"hello"`
  into the naked `T` of `string | T`. `inferToMultipleTypes` strikes the
  constituents the source already matches *before* anything reaches the naked
  variable. Adding that strike through `is_type_assignable_to` — over exactly
  the primitive/literal/union domain that relation is proved on — fixed it.

The strike's first form cost **84 converted lines** for the 2 it fixed, because
`never` and `any` are assignable to everything and struck every union position.
Excluding them as sources: **net +372, lost 3.** That number is why the
exclusion is in the code with its measurement beside it, and it is the second
time in this build that a rule which looked obviously right was wrong in one
direction only.

### 5.2 The three residual lost lines, and who owns them

`compiler/strictFunctionTypes1` 2, `conformance/genericCallWithFunctionTypedArguments` 1.
All three are one mechanism: `f2("abc", fo, fs)` against
`<T>(obj: T, f1: (x: T) => void, f2: (x: T) => void) => T` infers `"abc"` from
the bare position and `Object` from a *parameter* position of a function
argument, and they disagree. Upstream keeps contravariant candidates in a
**separate, lower-priority bucket** and consults them only when no covariant
candidate exists — the priority lattice this slice explicitly refuses (§3).
They are the cheapest remaining extension and they are not built here:
`bd tsr-g30h` stays open with that leg named.

### 5.3 The residual wrong lines, read even though leg 4 passed

33 new wrong, and every one is a *named* unported inference mechanism rather
than a printer or a resolution defect. Grouped by owner:

| lines | shape | owner |
|---:|---|---|
| 5 | `want "foo"`, got `"bar" \| "foo"` (`stringLiteralTypesAsTypeParameterConstraint02`) | candidate selection under a type-parameter constraint |
| 4 | `want number \| undefined`, got `number` (`contravariantOnlyInferenceWithAnnotatedOptionalParameter`) | the contravariant bucket — same mechanism as §5.2 |
| 4 | `want (x: Date) => Date`, got `(x: T) => T` (`genericCallWithGenericSignatureArguments2`) | `getErasedSignature` |
| 4 | tuple shapes (`inferTupleFromBindingPattern`) | `bd tsr-84iz`, pattern-implied tuple inference |
| 3 | `Apply<TTypeLambda, A>` (`inferenceAndHKTs`) | conditional and indexed-access types |
| 3 | `{ __typename: string; }[]` (`inferFromGenericFunctionReturnTypes3`) | literal widening inside an object member |
| 3 | `never` winning inside a reference (`subtypeRelationForNever`, `neverInference`) | §5.1's `never` strike is **per type parameter**; a `never` *inside* a reference argument still wins, and striking that needs the union `getCovariantInference` builds |
| 2 | `Box<42>` vs `Box<number>` (`isomorphicMappedTypeInference`) | mapped types |
| 2 | `{ bar(x: any): void; }` (`contextualTypingOfOptionalMembers`) | contextual typing, refused with a number in `STATUS.md` §5 |
| 3 | one each: `SetupImages<string>`, `Maybe<string>`, `number` for `1` | object-rest binding, base-type matching, widening |

Nothing here is a defect in code that already worked, and the largest group is
the same contravariant bucket §5.2 names — which is the argument for building
that leg next rather than any of the others.

### 5.4 What was not built, so it is not re-derived

From §2.3, in size order: **tuple return types** (`[T, U]` 32 lines, `[T, U, V]`
10) are an `instantiate_type` limit, not an inference one; **object-type
members** (`{ keys: T[] }` against `{ keys: string[] }`) need a members reverse
index that does not exist; **intersections** (`T & U` 8) need the same;
`getErasedSignature`; and the **contravariant bucket**, which §5.2 and §5.3
both point at and which is the one leg with measured demand on both sides of
the ledger.
