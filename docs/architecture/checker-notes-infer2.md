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
  widens it to `any` (`getWidenedType`, `checker.go:18355` — **corrected from
  `:16090`, see §7.2**); nothing on this
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

## 6. The contravariant bucket, sized and REFUSED — 6 own-node lines

Sixth session, continued, at the merge of `origin/main` (`tsr-4sa` and
`tsr-84iz` both landed under this worktree, so everything below is measured
against the current compiler, not the one §5 was measured on).

§5.2 and §5.3 both pointed here: the contravariant bucket owned all 3 residual
lost lines *and* the largest single group of the 33 residual wrong lines. The
same mechanism on both sides of the ledger is the strongest next-item signal
this project recognises, and it is exactly the signal that has to be **sized
before it is believed** — `STATUS.md`'s fourth rule does not stop applying
because the evidence is elegant.

### 6.1 What upstream actually does

`inferFromContravariantTypes` (`inference.go:308`) flips a `contravariant` flag
and re-enters `inferFromTypes`; candidates then land in `contraCandidates`
rather than `candidates`. `getInferredType` (`inference.go:1317`) prefers the
covariant inference when it is not `never` or `any`, **some** contravariant
candidate is a supertype of it, and no other type parameter constrained to this
one would conflict; otherwise the contravariant inference wins.
`getContravariantInference` (`inference.go:1463`) is `getCommonSubtype` — or an
intersection under one priority flag.

**And the flip is conditional.** `inferFromContravariantTypesIfStrictFunctionTypes`
(`inference.go:314`) only flips when `strictFunctionTypes` is on. That option
is **not modelled anywhere in this port**, so building this leg means inventing
a default for it — the same guess that cost 1,221 lines when it was tried the
wrong way round for `strictNullChecks` (`types_producer.rs:828`).

### 6.2 The counterfactual, extended

`infergen.rs` now threads a `contravariant` flag through the walk, keeps two
candidate buckets, and resolves them with the preference rule above. It was
also brought to **parity with the shipped arm** first — the `never` strike, the
matched-constituent strike, and per-case `strictNullChecks` read from the same
two directives `types_producer` reads — so the CONVERTS column measures the
*delta* and nothing else.

Over 1,228 classified own-node lines at the merge commit:

| | lines |
|---|---:|
| REFUSED — no candidate, the type parameter stays unmapped | 494 |
| an argument's own type is a gap | 456 |
| REFUSED — the return type is not rebuildable | 87 |
| a rest parameter | 56 |
| REFUSED — two **covariant** candidates disagree | 52 |
| a spread argument | 39 |
| REFUSED — a `null`/`undefined` candidate | 32 |
| **CONVERTS — the contravariant bucket's whole gain** | **6** |
| REFUSED — contravariant candidates disagree (`getCommonSubtype`) | 5 |
| MISS | 1 |

```
C1 classified-but-not-gap : 0    (expect 0)
C2 buckets sum 1228 vs classified 1228
C3 bare-only rule converts: 4    (expect 0)  <-- FIRED
```

**C3 fired and is printed rather than tuned**, as `docs/conventions.md`
requires. Four classified lines are answered by the *already-shipped* rule when
run through this harness, which means the harness and `check_generic_call`
disagree on four lines — the probe does not model the `hasCorrectArity` guard
(`checker.go:9107` — **corrected from `:8710`, see §7.2**) that gaps a call
missing a required argument. It does not
change the verdict; it makes it **stronger**, because up to four of the six
conversions could be that same artefact. The honest reading of the gain is
**between 2 and 6 own-node lines.**

### 6.3 The other two sides of the ledger, measured rather than quoted

The leg's case was never only the gaps. Re-measured at the merge commit:

- **Wrong lines it would fix: 4.** The four contravariant-named cases carry 120
  wrong lines between them, and reading them is the whole finding — they are
  mostly *not* this mechanism. `contravariantOnlyInferenceWithAnnotatedOptionalParameterJs`
  (10) and `contravariantOnlyInferenceFromAnnotatedFunctionJs` (4) are JS-file
  inference; `strictFunctionTypes1`'s head is `(x: A | number) => void` against
  `(x: number | A) => void` (union constituent order, 3) and
  `ReadonlyArray<T>` against `readonly T[]` (a printer form, 2). Exactly **4**
  — `number | undefined` where we print `number` — are the contravariant
  bucket.
- **Lost lines it would recover: 3**, §5.2's, unchanged.

### 6.4 The number that refuses it

```
own-node conversions            6   (2–6 after C3)
wrong lines fixed               4
lost lines recovered            3
                               --
own-node reach                 13   x 2.8 cascade  ~=  36 lines
```

Against that: a variance flag threaded through every arm of the walk, a second
candidate bucket, `getInferredType`'s preference rule, `getCommonSubtype` for
the disagreeing case, **and a new `strictFunctionTypes` compiler option whose
default this port would be guessing.**

**~36 lines is smaller than anything this project has yet refused.**
`removeSubtypes` was refused at 1.03 gained per lost across ~1,100 quoted
lines; the multi-distinct return aggregate at 81 lines for 12–46 converted;
`TemplateExpression` at 0.76 gained per wrong on 1,036. This is a tenth of the
smallest of those, and it carries a modelling guess none of them carried.

**Refused.** `bd tsr-g30h` records it. The signal in §5.2/§5.3 was real and it
was still a **ceiling on a row**, not a forecast of a mechanism: 4 of the 33
wrong lines were the mechanism and the other 116 lines in those same cases were
five other people's items. That is the fourth rule of `STATUS.md` catching this
page's own recommendation, one section after it was written.

### 6.5 What the same run says to look at instead

The refusal splits, re-measured at the merge commit, are unchanged in shape and
name their owners:

```
  494  no candidate   — head: 47 no parameter mentions it, 23 two unrelated type
                        parameters, ~70 `then` shapes under `@strict: false`,
                        ~11 object-type members ({ keys: T[] } vs { keys: string[] })
  456  the argument's own type is a gap        — downstream, not this module's
   87  return not rebuildable — [T, U] 32, [T, U, V] 10, T & U 8, { value: V } 6
```

The largest *buildable* family remains **tuple return types (42 lines)**, and it
is an `instantiate_type` limit — a tuple has no `(symbol, arguments)` intern key
to reverse — not an inference limit. It becomes answerable when a tuple gains a
structured `TypeData`, which is the same prerequisite `inference.rs`'s module
doc has named since `bd tsr-0hc`. Nothing in the inference walk itself is worth
more than that.

## 7. The PRIORITY LATTICE, sized — and refused beside the contravariant bucket

Sixth session, measured at `a57a04b` by the extended `examples/infergen.rs`.
§6 refused the contravariant bucket at **6 own-node lines**. The obvious
objection to that refusal was that it sized one leg of a mechanism with two, and
that the **priority lattice** — the other leg, and the one `inference.go`'s own
structure makes look larger — had never been sized. It has now.

### Why this needed sizing at all: the row said 2,056

`STATUS.md` §4.2 carried inference as the board's **top live row** on a
`callgate.rs` figure of **2,056** admitted call lines stopping at *"inference
gapped"*, at only 1.6% want-any. That is a **row**, and this project's fourth
rule says a row is a ceiling. Measured as a mechanism:

```
classified (own-node call, one generic candidate, gap line)   1,231

   494  40.1%  REFUSED — no candidate: the type parameter stays unmapped
   458  37.2%  an argument's own type is a gap          <- DOWNSTREAM, not this item
    88   7.1%  REFUSED — return type not rebuildable by `instantiate_type`
    56   4.5%  a rest parameter — `getSpreadArgumentType`, unported
    52   4.2%  REFUSED — two COVARIANT candidates disagree
    39   3.2%  a spread argument — no position to land on
    32   2.6%  REFUSED — a `null`/`undefined` candidate, needs `getWidenedType`
     6   0.5%  CONVERTS
     5   0.4%  REFUSED — contravariant candidates disagree (`getCommonSubtype`)
     1          MISS
```

**77.3% of the row is two things that are not the lattice**: 494 lines where no
candidate is found at all, and 458 where an argument's own type gaps — the
latter is downstream by construction and belongs to whatever gaps the argument.

### The three columns, per leg, as deltas

Each leg is measured as a **delta over the shipped arm**, not as an absolute, so
the legs cannot double-count each other. The at-risk column is computed in the
same pass, as `docs/conventions.md` requires of a mechanism that fires on a
position.

| leg | changes | CONVERTS | WOULD PRINT WRONG | AT RISK |
|---|---:|---:|---:|---:|
| contravariant bucket (§6) | 13 | **6** | 1 | **0** |
| priority lattice, cheap leg (delta over contravariant) | 12 | **11** | 1 | **0** |
| lattice leg **alone** (no contravariant, no `strictFunctionTypes` guess) | 16 | **11** | 4 | **0** |

```
right lines admitted in the same pass    792
wrong lines admitted in the same pass    202
wrong lines either leg would FIX           0
```

### The verdict: REFUSED, and §6's refusal is strengthened rather than replaced

**Both legs together are worth ~17 conversions.** The lattice is not the larger
leg the objection assumed — it is 11 lines, and it is the *better* of the two.
Against a board whose last two builds converted 3,590 and 2,973, and against §5's
smallest recorded refusal, this is not an item.

Three things make the refusal safe to rely on rather than merely small:

1. **Every leg's at-risk column is 0**, computed in the same pass over 792
   admitted right lines. So this is not a case of a good conversion hidden behind
   a bad trade — there is no trade, there is just very little there.
2. **The lattice alone is *worse*, not better** — 11 converts against 4 wrong
   rather than 1. Sequencing it first to dodge §6's `strictFunctionTypes`
   dependency does not rescue it.
3. **C4 is honest about the probe's direction.** The counterfactual fails to
   reproduce **239** of the 792 admitted right lines, and in every one of the
   239 it *gaps* rather than answering differently. The probe is conservative:
   it can understate conversions, and it cannot manufacture a false zero in the
   at-risk column. A control that under-reports in the safe direction is worth
   stating, because the opposite would have made the 0 unreadable.

### Where the row's mass actually is, for whoever comes next

Not here. **494 lines find no candidate at all** — the type parameter is never
mapped, which is upstream of the lattice and of the contravariant bucket both;
and **458 are downstream** of a gapping argument. Neither is a priority-lattice
item, and quoting 2,056 — or 1,400 — as inference work would be the row-for-
mechanism substitution this project has now made and caught four times.

**`bd tsr-g30h` closes.** Its first slice landed (+372); its remaining legs are
refused with the numbers above.

## 8. The row re-sized at `8ac225d`, and the bar for what is left

Sixth session, continued. `STATUS.md` §4.2 carries this as its **top live row**
at **2,056 lines / score ~1,400**, describing the remainder as *"the priority
lattice and contravariant tracking"*. This section sizes that remainder as a
**mechanism** rather than quoting the row, and the two numbers are not close.

§6 refused the contravariant bucket at 6 own-node lines. That refusal is
re-taken here and it **stands** — but §6 measured only one of the two mechanisms
the row names, and the other one had never been measured at all.

### 8.1 The row against the mechanism, reconciled rather than asserted

`callgate.rs` at `8ac225d`: of **7,303** admitted call lines, **2,056** stop at
`inference gapped`, want-any **33 (1.6%)**. `infergen.rs` admits **1,231** of
them — own node *is* the call, no written type arguments, no optional chain, the
callee resolves through `resolve_call_signature` to a single generic candidate,
and the line is a gap. The residue of ~825 is cascade, written-type-argument and
multi-candidate lines; cascade is upside and is priced below at the **2.8×** this
mechanism measured on itself (§5: 131 forecast, 372 delivered).

The 1,231, decomposed by **what specifically stops it**, with each bucket's
want-any and top-1 case share:

| own-node lines | want-any | top-1 case | what stops it | owner |
|---:|---:|---|---|---|
| **494** | 3.0% | `promiseTypeStrictNull` 10.3% | no candidate — the walk finds nothing for some type parameter | five sub-families, §6.5; none has a reverse index |
| **458** | 0.4% | `inferFromGenericFunctionReturnTypes2` 7.4% | an argument's own type is already a gap | **downstream — not this module's** |
| **88** | 0.0% | `genericDefaults` 45.5% | the return type is not rebuildable | `instantiate_type` limit: `[T, U]` 32, `[T, U, V]` 10, `T & U` 8, `{ value: V }` 6, call/construct literals 9 |
| **56** | 0.0% | `promiseTry` 30.4% | a rest parameter | `getSpreadArgumentType`, unported |
| **52** | 3.8% | `genericCallWithNonSymmetricSubtypes` 19.2% | **two candidates disagree** | **`getCovariantInference` — measured below** |
| **39** | 0.0% | `genericRestParameters1` 46.2% | a spread argument | no position to land on |
| **32** | 3.1% | `promiseType` 93.8% | a `null`/`undefined` candidate | `getWidenedType` (`checker.go:18355`) |
| **6** | 0.0% | `strictFunctionTypes1` 83.3% | — converts | **the contravariant bucket** |
| **5** | 0.0% | `genericCallWithGenericSignatureArguments3` 60.0% | contravariant candidates disagree | `getCommonSubtype` (`inference.go:1579`) |
| **1** | — | — | miss | |

**So the two mechanisms `STATUS.md` §4.2 names own 63 of the 1,231 lines as a
ceiling**: the 52 disagreement lines plus the 6 + 5 contravariant ones.
Everything else in the row belongs to a named owner, and the largest single
entry — 458 lines — is somebody else's gap arriving here.

### 8.2 The three columns, computed in one pass, for three designs

`infergen.rs` now runs the **same walk** with the contravariant bucket consulted
and not consulted, and with the common-supertype pick on and off. Every column
below is therefore a **delta attributable to the mechanism**, not a difference
between the probe and the compiler — which is what §6.2's CONVERTS column could
not distinguish, and why it had to report its gain as "between 2 and 6".

`docs/conventions.md` requires the at-risk column in the **same pass** as the
target one when a mechanism fires on a POSITION rather than on a defect. This
one fires on every generic call, so it does: **792 lines that are RIGHT today**
are admitted by the identical classifier and re-forecast.

| design | CONVERTS | WOULD PRINT WRONG | AT RISK | ratio |
|---|---:|---:|---:|---:|
| **C** — the contravariant bucket alone (§6's design) | **6** | **1** | **0** | 6.0 : 1 |
| **L** — the common-supertype pick alone | **11** | **4** | **0** | 2.8 : 1 |
| **C + L** — both | **17** | **2** | **0** | **8.5 : 1** |

**C + L is strictly better than L**, which is the finding that makes this table
worth having: two of L's four wrong lines are `want B, got A` and `want B[], got
A[]`, and the contravariant bucket is what tells `B` from `A`. Adding C both
converts 6 more lines and removes 2 of L's wrong ones.

**Design L is `getCommonSupertype` (`inference.go:1530`) restricted to its
unambiguous case** — exactly one candidate that every other is assignable to, so
the union reduces to it and no subtype decision is invented.
`getCovariantInference` (`inference.go:1434`) reaches it whenever
`InferencePriorityPriorityImpliesCombination` is clear; the other branch is a
`UnionReductionSubtype` union, which is `removeSubtypes`, refused at
`bd tsr-eak`. Neither `getCommonSupertype`'s literal-types-with-a-common-base
branch nor `getCovariantInference`'s literal widening is modelled, so **L is a
floor on its own family, not a ceiling**.

**L is not the priority lattice.** The row's phrase is inherited and it is
imprecise: `InferencePriority` (`checker.go:299`) ranks *where* a candidate was
found and discards all but the best-priority ones. That is a different rule,
its population is a subset of the same 52, and **it is still unsized**. What is
sized here is how a surviving disagreement is *resolved*.

#### Controls, with the values stated before the run

- **C1** classified-but-not-gap: expect 0, read **0**.
- **C2** buckets sum 1,231 = classified 1,231. Arithmetic, and as
  `docs/conventions.md` records it cannot see a wrong partition.
- **C3** the pre-`tsr-g30h` bare-only rule converts: expect 0, read **4** —
  unchanged from §6.2, and still the `hasCorrectArity` guard (`checker.go:9107`)
  the probe does not model. **Anchor corrected**: §6.2 cited `checker.go:8710`,
  which is `someSignature`. `xtask -- anchors` scans Rust sources and not this
  page, so the citation was never gated; the two stale ones §7 repeated are
  fixed here and left visible.
- **C5** — the control pinned to the **upstream construct** rather than to
  arithmetic, and it is the one that upgrades §6.2. `structural + consult_contra
  off` is `getInferredType`'s single-bucket path *before*
  `inferFromContravariantTypes` (`inference.go:308`) exists — i.e. exactly what
  `inference.rs` ships. Every classified line is a gap in the real compiler, so
  the shipped rule must convert **0** of them. **Predicted 4** (the same arity
  artefact C3 sees); **read 0**. The prediction was wrong in the useful
  direction: the structural walk finds a second, disagreeing candidate on those
  four lines and gaps, so the bare rule's artefact does not reach the CONVERTS
  column. **§6.2's "the honest reading of the gain is between 2 and 6" is
  therefore corrected to a firm 6.**
- **C4** — the at-risk column's own control. Of 792 right lines, the probe
  cannot reproduce **239**, and the split is the diagnosis: **239 of 239 are
  lines the probe GAPS and the compiler answers, 0 are lines it answers
  differently.** The probe is strictly *more conservative* than the shipped arm
  — it models neither `fillMissingTypeArguments`' default fallback nor the arity
  guard. **AT RISK 0 is therefore a zero over the 553 right lines the probe
  reproduces, and a lower bound overall**, in the same sense C4 made `valgap.rs`'s
  70 a lower bound (`STATUS.md` §4.3).

### 8.3 Two numbers in §6.4 are corrected, both downward

§6.4 summed the ledger as `6 conversions + 4 wrong fixed + 3 lost recovered = 13
own-node reach`. Both added terms are wrong, and `STATUS.md`'s third rule says
so here rather than silently.

- **"Wrong lines it would fix: 4" re-takes to 0.** That figure was counted by
  hand at the merge commit, before design W and design P landed (+6,563 lines
  between them). The same instrument now admits **202 wrong lines** by the
  identical classifier and the mechanism fixes **0**. The contravariant-named
  case is still there — `contravariantOnlyInferenceWithAnnotatedOptionalParameter`
  contributes **2** admitted wrong own-node lines — and the bucket does not
  reach them. §6.3's 4 also counted 2 *cascade* lines (`:13`, `:21` of that
  baseline) against an own-node ledger.
- **"Lost lines recovered: 3" was double-counted.** §5.2's three lost lines are
  `compiler/strictFunctionTypes1` 2 and
  `conformance/genericCallWithFunctionTypedArguments` 1; the CONVERTS bucket's
  cases are `strictFunctionTypes1` 5 and `genericCallWithFunctionTypedArguments`
  1. They are gap lines today and they are **inside the 6**, not additional to
  it. (Established by case coincidence, not line by line — stated as the weaker
  claim it is.)

**So §6.4's own-node reach of 13 was really 6.** The refusal was right and its
arithmetic was generous to the thing it refused.

### 8.4 What this does to `STATUS.md` §4.2's top row

```
the row                                         2,056 lines, score ~1,400
own-node lines the row's two named mechanisms own   63   (ceiling)
own-node lines they CONVERT                         17
  x 2.8 cascade, this mechanism's own measured multiplier
                                                   ~48 lines  =  0.010% of 478,954
```

**The board's top live row is worth about 48 lines, not about 1,400.** That is
`STATUS.md`'s fourth rule again — a population is a ceiling — and it is the
third time in two sessions that the rule has caught a row scored by its size.

### 8.5 The bar, registered before the arm — design C + L

**Not built. This section is the registration and nothing else.** Measure over a
`git stash` pair with `casedelta.rs`, `wrongdelta.rs` and `verdictdump.rs`.

**Keep only if all four hold. Any leg fires → the first hypothesis is that the
build is wrong, the second that a premise here is wrong, and there is no third.**

| leg | rule | why that number |
|---|---|---|
| **1 — net floor** | net **≥ +15** | The forecast is **17 own-node** conversions and cascade is excluded, exactly as §4's leg 1 was. `tsr-g30h` set its floor at 92% of its own-node forecast (120 against 131) and delivered 2.8×; 15 is 88% of 17. Below 15 means the own-node conversions did not materialise — the probe's resolution rule and the arm's diverge — which is the failure **C5 cannot see**, because C5 only proves the *shipped* rule agrees. |
| **2 — lost** | **lost ≤ 3**, and `tsr-g30h`'s own 3 residual losses must be **recovered** | An absolute, not a ratio, and the shape is chosen from the measurement: **AT RISK read 0** in the same pass, so there is no designed-in at-risk population for a ratio to price — this is design W's situation and not design P's. It is 3 rather than 0 because C4 makes that zero a lower bound over 553 of 792 right lines, and because §5.1 measured *this exact walk* losing **15, then 9, then 3** as its rules were corrected. A loss is evidence of a wrong rule, not of a bad trade. The recovery clause is separable: §5.2's three lost lines are inside the 6 conversions, so a build that converts 6 and does not recover them has converted different lines than forecast. |
| **3 — cases** | cases regressed **== 0** | Stricter than `tsr-g30h`'s `regressed < finished`, and deliberately, on design P's leg-4 move: register the zero the construct implies so that a non-zero reading is a **diagnosis** rather than a trade. C + L changes **29 own-node lines corpus-wide** (13 + 16) and only where candidates disagree or arrive contravariantly. A whole baseline flipping to worse cannot come from 29 lines unless the change reached somewhere this page did not model. |
| **4 — gap→wrong** | **new wrong attributable to `inference.rs` ≤ 8**, by `verdictdump.rs`'s `GAP→WRONG` matrix. Global Δwrong is **reported beside it, not gated** | `docs/conventions.md`'s post-W rule: *an absolute on GLOBAL Δwrong tightens as the build improves*, because conversions expose other mechanisms' defects — design W's leg fired at 84 against 40 and had to be overridden for exactly that reason. So the absolute is written against **the mechanism's own new wrong**: forecast **2** own-node, × 2.8 cascade = 6, plus a third for the asymmetry the own-node cut cannot measure = **8**. The downstream bucket to report beside it is named in advance: **the 458 lines whose argument's own type is a gap**, which this build does not touch and which will supply most of any global movement. |

**Falsifier for this section.** The claim is that *resolving a candidate
disagreement by `getCommonSupertype`'s unambiguous case, with a contravariant
bucket to tell `B` from `A`, converts ~17 own-node lines and risks none*. It is
refuted by fewer than 15 net lines, more than 3 lost, any regressed case, or more
than 8 new wrong lines attributable to inference. Any of those and the arm
reverts and the number goes to `STATUS.md` §5.

**And the residual wrong lines get read even if leg 4 passes**, which is the
discipline that found a one-line printer defect worth +114 in the fifth session
and found §5.1's two candidate-resolution bugs behind a passing net.

### 8.6 The verdict, and what stays refused

- **The contravariant bucket stays REFUSED as a standalone item.** Its own delta
  is **6 own-node / ~17 with cascade**, its ledger is *smaller* than §6.4
  recorded (§7.3), and it still costs a `strictFunctionTypes` default this port
  would be inventing — **83.3% of its conversions are in `strictFunctionTypes1`,
  a case whose directives are `@strict: true`**, so the guess is not incidental
  to the gain, it *is* the gain.
- **Design L — the common-supertype pick — is new, and it is not refused.** 11
  own-node conversions, 4 would-be-wrong, **0 at risk**, no compiler option
  invented, and it lives in candidate *resolution* rather than in the walk. It
  is the cheaper half of a small item.
- **Design C + L, at 17 / 2 / 0 and 8.5 : 1, is the one with a registered bar.**
  ~48 lines with cascade is small — but it is not below what this project has
  shipped (`tsr-xs0` +37 for a four-line fix, the defaults arm +66), and it is
  well above what this project has refused (`removeSubtypes` at 1.03 gained per
  lost, `TemplateExpression` at 0.76 gained per wrong, the multi-distinct return
  aggregate at 81 lines for 12–46 converted). **The refusal test here is effort,
  not ratio**: if the build is a variance flag through the walk plus a second
  bucket plus a preference rule, §6.4's judgement holds and 48 lines does not pay
  for it. If it is the resolution rule alone — design L — it is two arms on
  machinery that exists.
- **`STATUS.md` §4.2's top row must be re-scored from ~1,400 to ~48** (§7.4).
  Its mass is not this mechanism's: 458 lines are downstream, 494 have no
  reverse index to walk, 88 are an `instantiate_type` limit whose largest
  buildable family is still **tuple return types (42 lines)** — unchanged since
  §6.5, and still waiting on a structured `TypeData` for tuples rather than on
  anything in the inference walk.


## 9. Adjudication — design L is REFUSED, on size rather than on ratio

§7 and §8 were written independently, in the same session, from the same
instrument, and they do not agree about one leg. §8 concludes *"design L — the
common-supertype pick — is new, and it is not refused"*, and registers a bar for
**C + L**. §7 refused both. This section settles it rather than leaving two
verdicts in one file, and the difference is **not** a disagreement about any
measurement — every number below is §8's own.

**§8 is right that L's ratio is good.** 11 converts against 1 would-be-wrong,
at-risk 0 over 792 admitted right lines, and it fixes none of the 202 admitted
wrong lines but breaks none either. On ratio alone, `removeSubtypes` was refused
at 1.03 gained per lost and L is an order of magnitude better than that.

**It is refused anyway, and the test is the one §8 itself names: effort.** Priced
honestly, using §8's own cascade multiplier rather than a fresh guess — this
mechanism measured **2.8×** on itself (§5: 131 forecast, 372 delivered):

```
  design L, own-node                        11 converts
  × 2.8 cascade, the mechanism's own rate  ≈ 31 lines
  effort                                     5  (a priority lattice is a subsystem)
```

**~31 lines for an effort-5 subsystem.** The two builds either side of it in this
session converted **3,590** and **2,973** at effort 2, and both came from
re-measuring a refusal rather than from new machinery. There is no reading of
`score = (reachable / effort) × feasibility` on which 31/5 competes.

Three things keep this from being a ratio argument dressed as a size argument:

1. **It is not close.** A 100× error in the cascade estimate would be needed to
   reach the board's next-smallest live item.
2. **The row cannot rescue it.** §8's own reconciliation is what makes this
   safe to say: the 2,056-line row's mechanism population is 1,231, and **494 +
   458 = 77.3% of that is elsewhere** — no candidate found at all, and downstream
   of a gapping argument. Neither is a lattice item, so there is no larger
   version of L waiting behind the row.
3. **§8 strengthens §6 rather than weakening it.** Its §8.4 found that *"§6.4's
   own-node reach of 13 was really 6 — the refusal was right and its arithmetic
   was generous to the thing it refused."* The contravariant refusal is now
   confirmed by a second pass that was actively looking for a reason to overturn
   it.

**§8.5's registered bar is therefore not spent and not fired — it is
unexercised**, and it is left in place deliberately. If the 494 "no candidate"
population is ever built, L's 11 lines may arrive free on top of it, and that bar
is the right one to score the combination against. What must not happen is L
being built *for its own sake* on the strength of its ratio.

> **A good ratio on a small population is still a small population.** This board
> has refused items for bad ratios five times and this is the first refused for
> size against a good one — worth naming, because the two arguments feel alike
> and only one of them is about whether the work is worth doing.
