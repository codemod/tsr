# Namespace-qualified naming, re-priced — the counterfactual `STATUS.md` §5 asked for

**2026-08-07, seventh session. Measured at `22a5a51` by the new
`crates/tsr-conformance/examples/qualname.rs`. No compiler code was changed;
this is a measurement.** Reproduce:

```
cargo run --release -p tsr-conformance --example qualname
```

`STATUS.md` §5 carries the refusal and its own instruction to re-take it:

> **qualified naming build** | ~~1,318~~ **4,473 on the `TypeReference` root
> alone** | 90.7% accurate on target row; counterfactual **lost 3,202 lines,
> regressed 753 cases**. The refusal STANDS on that measured cost — but its
> population was understated … A refusal priced against 1,318 has not been
> priced against 4,473, and the *cost* side (3,202 lost) was measured on a
> compiler seven builds older. **This is the strongest candidate on the page for
> a fresh counterfactual**, and it needs a new bar rather than an inherited one.

Run. **The refusal does not survive contact with a current compiler, and the
reason is that it was a refusal of the wrong design.** The design that lost
3,202 lines is not the only design, and the one this page measures has an
at-risk column of **zero** on the half that produces its conversions.

```
                              CONVERTS   WOULD PRINT WRONG   AT RISK
W  the written entity name       1,770                  99         0
R  the computed bare name          384               1,025         0
P  the symbol chain          (525 / 614 on record, cycle 20b)       36
```

`W`, with one positional refusal this page prices, is **1,702 conversions
against 20 would-be-wrong lines and 0 lines at risk**.

---

## 1. The item is two halves, and the refusal only ever priced one

The family has a **resolution** half and a **printing** half, and
`checker-notes-typerefgap.md` §6 already separated them:

- **Resolution.** `Checker::get_type_from_type_reference`
  (`crates/tsr-checker/src/declared.rs`) answers `errorType` for a qualified
  name whose leftmost identifier resolves as a namespace, deliberately and with
  a comment saying why. Upstream resolves it through `resolveQualifiedName`
  (`internal/checker/checker.go:15828`), which resolves the left as a namespace
  and reads the right out of `getExportsOfSymbol`
  (`internal/checker/checker.go:15851`). **4,557 gap lines** sit here.
- **Printing.** A resolved `M.I` still has to be *named*. Cycle 20b
  (`checker-notes-nameres.md` §49) built `getSymbolChain`
  (`internal/checker/nodebuilderimpl.go:1086`) plus `needsQualification`
  (`internal/checker/symbolaccessibility.go:688`), measured **−2,677 net /
  3,202 lost / 753 cases regressed**, and reverted.

The refusal on `STATUS.md` is the second half's. It was then quoted against the
first half's population, which is how a 1,318-line refusal came to block a
4,557-line row. **The two halves have different at-risk columns**, and that is
the whole finding of this page.

## 2. What the instrument does

`qualname.rs` is modelled on `namedcallee.rs`, this project's exemplar
counterfactual. It computes all three columns in one pass over the corpus, on
lines it is already visiting, as `docs/conventions.md` requires of a mechanism
that fires on a position rather than on a defect.

**The target population** is `depend.rs`'s own bucket, copied rather than
re-derived — `gaps`, `step` and the walk are verbatim from `depend.rs` via
`typerefgap.rs`, because a re-implementation that drifts by one edge measures a
different population and reports it under the published number. Admitted: a gap
line whose chain roots on a `TypeReferenceNode` whose type name is a
`QualifiedName` whose leftmost identifier resolves as a namespace.

**4,557 lines**, which reconciles exactly with `checker-notes-typerefgap.md` §5's
`4,002 + 471 + 84 = 4,557`. The endings:

| ending | lines |
|---|---:|
| `no further dependency` — the §4.3 row | 4,002 |
| `the dependency types — the root is here` | 471 |
| `cycle` | 84 |

231 cases; top-1 `compiler/resolvingClassDeclarationWhenInBaseTypeResolution` at
**2,022 = 44.4%**, under the 50% bar this project reads a bucket against. 40
lines want `any`.

**Three designs are forecast**, and each forecast is compared to the baseline's
right-hand side **character for character**:

| | what it prints for `M.I` |
|---|---|
| **W** | the written entity text `M.I` — exactly what `Checker::unresolved_type_reference` already emits for the *unresolvable* half of the same construct, and the same written-node reuse `ff49871` built for `typeof` |
| **R** | `type_to_string` of the resolved type, i.e. the symbol's bare name `I` |
| **P** | `getSymbolChain`'s qualifier. Its conversion column is on record from cycle 20b; only its **at-risk** column is re-measured here |

## 3. Two shapes the probe cannot score, separated instead of counted

The forecast is the *reference's* printed text and the baseline is the *whole
line*. Those coincide only when the reference is the line's own answer. Two
shapes where they do not are given their own buckets and counted in **neither**
column:

- **the reference is upstream of the line** — `depend.rs` reached it in more
  than one step, so the line's answer is computed *from* it
  (`>_.isFunction : (object: any) => boolean` roots on `Underscore.Static`);
- **the reference is nested inside the line's printed type** —
  `() => privateModule.publicClass` around `privateModule.publicClass`.

| steps from the assertion line to the root reference | lines |
|---|---:|
| **1 — the scorable population** | **2,270** |
| 2 | 1,277 |
| 3 | 630 |
| 4+ | 380 |

Together the two unscored buckets are **1,302 (W)** and **2,627 (R)**: a stated
ceiling on further conversions and a floor of zero on further wrong. They are
the reason the W column below is a **floor**, not a forecast.

### The boundary rule inside the nesting test was wrong on the first run, and it mattered

The first version demoted a line to *unscored* whenever `wanted.contains(forecast)`.
That swallowed `want Underscore.Static / forecast Static` — design R's single most
important **wrong** shape, where the baseline carries the qualifier and R drops
it. R's wrong column read **12**. With `.` excluded as a boundary character it
reads **1,025**.

> A containment test used to *demote* a line is still a claim about structure,
> and its boundary rule is where the claim lives. `Static` inside
> `Underscore.Static` is not a nested type; it is the same type missing its
> qualifier — which is the defect being priced.

## 4. The three columns

| design | CONVERTS | WOULD PRINT WRONG | want-`any` | DECLINES | UNSCORED |
|---|---:|---:|---:|---:|---:|
| **W** — written entity name | **1,770** | **99** | 2 | 1,384 | 1,302 |
| **R** — computed bare name | 384 | 1,025 | 0 | 1,534 | 2,627 |

108 converting cases for W, 35 for R.

W's declines are each a refusal with a name, not a shortfall:

```
  1085  a written type ARGUMENT gaps — downstream
   291  the namespace has no export of that name
     8  the export exists but not with the wanted meaning
```

The 1,085 is the `docs/conventions.md` *"every part is handled, something
downstream gapped"* bucket, and it is 23.8% here — reported before the rest, as
the object-literal row's 77% requires.

R's largest decline is **1,157 generic targets**: the resolved symbol takes type
parameters and instantiating it is `bd tsr-4qx`'s seam, a different item.
Refused rather than approximated.

**R is refused on its own number: 384 conversions against 1,025 wrong lines,
0.37 gained per wrong.** Every ratio this project has refused an item on — 2.1,
2.5, 2.7, 9.1 lost per gained — is better than that. The head says exactly why:

```
   60  want `privateModule.publicClass`, forecast `publicClass`
   50  want `() => privateModule.publicClass`, forecast `publicClass`
   20  want `import45.NgControlStatus`,       forecast `NgControlStatus`
   14  want `quasiater.carolinensis`,         forecast `carolinensis`
```

Upstream's baselines **want the qualifier**. A design that resolves the name and
then prints it bare is the one design guaranteed to be wrong wherever the source
bothered to write a qualified name.

## 5. The at-risk column, which is what refused this item before

| | lines at risk | cases |
|---|---:|---:|
| **W** — resolution + written text | **0** | 0 |
| **R** — resolution + computed name | **0** | 0 |
| **P** — the symbol chain, strict bound | **36** | 21 |
| **P** — the symbol chain, loose bound | 4,801 | 255 |

**The resolution half has no at-risk population at all.** Both halves of that
claim were measured rather than argued:

- C1 reads **0**: every classified line's reference answers `errorType` today,
  so nothing downstream of it can be printing a right answer *through* it.
- The route by which it could be right anyway — the printed text coming from
  written-node reuse rather than from the reference's computed type — is tested
  directly, as the conjunction of *"the declaration writes a qualified name
  whose root resolves as a namespace"* and *"the printed answer contains that
  written text"*. It reads **0**.

C7 is what makes that zero readable: **8,157** right lines *do* print a dotted
name whose root resolves as a namespace here, so the predicate has a population
to find and finds none. (Those 8,157 are enum member types — `E.A` from
`new_named_type`'s enum arm — and the like, which no qualified *type reference*
produced.) `checker-notes-nameres.md` §48's rule, applied: a control reading zero
proves nothing until a line can reach it, and here one can.

### The printing half fell from 3,202 to 36, and C5 says why

**C5, registered before the run with an expected value of 0, reads 0.**

`checker-notes-nameres.md` §51 named the cause of the reverted build's largest
single loss exactly — `conformance/parserRealSource10` at **−481** — and it was
one defect: `BindResult::resolve_name` could not see a namespace's `exports`, so
`export enum TokenID` referenced from inside `namespace TypeScript` was invisible
to the stop test and the walk concluded a qualifier was needed. That page closed
with an instruction: *"Nobody should build the chain again until (1) is fixed and
the counterfactual re-run."*

`resolve_name` gained the `nameresolver.go:104`–`:146` arm at `3b7fa44`
(`bd tsr-56r`). **That case now contributes 0 at-risk lines**, and the whole
strict at-risk population is **36 lines in 21 cases** against a denominator
(C6) of **7,092** right lines that name a namespace-declared symbol — 0.5%.

The 36 are one shape, and it is not the §51 shape:

```
  3  M : typeof M         [compiler/giant]
  3  eM : typeof eM       [compiler/giant]
  2  Point : Point        [conformance/importAliasIdentifiers]
  1  A : typeof A         [compiler/declFileWithInternalModuleNameConflictsInExtendsClause1]
  ...
```

A class and a namespace sharing a name, or a name declared twice at different
depths — the merged-declaration cases. Top-1 case is 6 of 36 = 16.7%.

**The strict bound is gated exactly as the reverted build was gated.**
`checker-notes-nameres.md` §49: *"Gated so that it applies only where the printed
form IS the symbol's name … because without that gate it climbs from an anonymous
`__function` symbol to no parent and gaps every function and object type in the
corpus."* Without that gate this bound reads **5,543** — 686 of them one
`() => void` in one case — which is a measurement of the ungated design nobody
proposes. The gate is the design, so the gate is in the instrument.

### The loose bound is reported and it is not the number to quote

4,801 lines / 255 cases, counting a right line any of whose printed identifier
tokens names a namespace-declared *type* symbol that does not resolve bare at
the site. It is an upper bound on a **fully** qualifying node builder — one that
rewrites names inside signature and type-literal renderings too — which is not
the design cycle 20b built or that this page prices. Its first version read
**111,321** because it indexed every symbol declared inside a namespace,
including locals and parameters; its head was `number`, `this`, `T`, `value`.
Restricting the index to `SymbolFlags::TYPE` minus type parameters —
`lookupSymbolChainWorker` excludes type parameters from the chain by
construction (`internal/checker/nodebuilderimpl.go:1069`) — brought it to 4,801
with a head of `publicClass`, `privateClass`, `Promise`. **Recorded because the
first number was published inside this session and the correction is the point:
a token-matching bound is a bound on the tokenizer until its index is the set
the mechanism can actually name.**

### One stated blind spot, in the direction that flatters the design

**41,206** right lines print a bare name whose type carries **no symbol** —
enums and type parameters, where `new_named_type` sets `members: None`
deliberately (`declared.rs`). The strict bound cannot attribute those to a
container, so if any of them would be qualified, this page under-counts. Type
parameters are excluded by upstream's own chain construction; enums are not, and
that residue is the honest ceiling on the 36.

## 6. The sub-family split, and the positional refusal it prices

This is the `bd tsr-4sa` move — the one where two positional refusals cost
**zero** conversions and removed 366 wrong lines.

| sub-family | W conv | W wrong | R conv | R wrong |
|---|---:|---:|---:|---:|
| user namespace · site **outside** · 2 segments · no arguments | 1,389 | 14 | 307 | 895 |
| user namespace · site **outside** · 2 segments · arguments | 16 | 0 | 0 | 0 |
| **lib** namespace · site outside · 2 segments · no arguments | 281 | **0** | 0 | 26 |
| user namespace · site **INSIDE the namespace** · 2 segments · no arguments | 48 | **69** | 73 | 88 |
| user namespace · site **INSIDE the namespace** · 2 segments · arguments | 19 | **6** | 0 | 0 |
| user namespace · site outside · 3 segments · no arguments | 9 | 3 | 0 | 11 |
| user namespace · site outside · 3 segments · arguments | 7 | 0 | 0 | 0 |
| user namespace · site **INSIDE the namespace** · 3 segments · no arguments | 1 | **4** | 4 | 2 |
| user namespace · site outside · 4 segments · no arguments | 0 | 3 | 0 | 3 |

Read the `INSIDE` rows. **79 of design W's 99 wrong lines (79.8%) come from
references written inside the very namespace they qualify**, and those rows
contribute only 68 conversions. Refusing the arm positionally there:

```
  W, unrefused         1,770 converts   99 wrong    17.9 gained per wrong
  W, INSIDE refused    1,702 converts   20 wrong    85.1 gained per wrong
```

**The refusal is upstream's own rule, not a tuning knob.** `needsQualification`
(`internal/checker/symbolaccessibility.go:688`) answers *no qualifier needed*
the moment a symbol table in scope holds the symbol itself
(`symbolaccessibility.go:697`). Inside `namespace M`, `I` is in scope, so
upstream prints `I` and the written text `M.I` is over-qualified by
construction. A design that reuses written text cannot see that on its own —
which is exactly why it needs the one predicate `resolve_name` can now answer.

The `lib namespace` row is the other finding: **281 conversions, 0 wrong.**
`compiler/temporal`'s 480 lines were flagged in `checker-notes-typerefgap.md` §4
as possibly needing lib symbols rather than in-file namespaces; they do, and W
handles them without a single wrong line.

## 7. C3 fired at 276, and it is a finding about this port's resolver

C3 was registered as **0**, pinned by upstream: `resolveEntityName`
(`internal/checker/checker.go:15772`) resolves the left of a qualified name with
meaning `SymbolFlagsNamespace` **only** (`checker.go:15829`), so a left that is
not a namespace makes upstream return nil and fall through to the
unresolved-symbol path. It reads **276**, and the instrument splits it:

```
  273  via an ALIAS, accepted whatever its own flags say
    3  via the globals table, which resolve_name does not meaning-filter
```

Both routes are documented in `BindResult::resolve_name`'s own comments and both
are deliberate: `lookup_scoped` accepts an `ALIAS` because resolving the target
needs the checker a `BindResult` does not have, and the closing globals lookup is
unfiltered. So `declared.rs`'s admission predicate is **wider than upstream's**
at this position by 276 lines — 6.1% of the classified population.

This does not invalidate the split: an alias to a namespace *is* a namespace for
`resolveQualifiedName`'s purposes once resolved, and W's wrong list contains the
handful where it is not (`want import("lodash").LoDashStatic, forecast
_.LoDashStatic`; `want alias.Point, forecast moduleA.Point`). It is recorded
because the control was pinned to the *upstream construct* rather than to
arithmetic over the partition, which is the only kind of control that can see
this — an arithmetic control over a partition cannot see that the partition is
wrong, because a wrong partition still partitions.

## 8. The verdict

**A fresh bar clears, for design W with the INSIDE refusal, and it clears
comfortably. Design R is refused with a number. The old refusal is superseded
rather than withdrawn.**

```
design W + refuse when the reference site is inside the namespace it qualifies

  forecast converts     1,702   (floor: 1,302 unscored lines are excluded)
  would print wrong        20
  right lines at risk       0   (C1 = 0, C7 = 8,157 — the zero is readable)
  cases converting        108
  cases at risk             0
```

The shape of the bar matters, and `docs/conventions.md` says to pick it from how
the mechanism can fail. This mechanism's failure mode is **manufacturing a
specific wrong answer** — an over-qualified name — not trading badly, so the bar
is an **absolute** on the wrong side, as `bd tsr-4sa`'s was:

> **Registered before any code: keep if `lost == 0` and `new wrong ≤ 40` and
> `gained ≥ 900`.**
>
> `lost == 0` because the at-risk column is 0 by two independent measurements,
> so a single lost line means the arm is firing somewhere the counterfactual
> says it cannot — a wrong rule, not a bad trade. `new wrong ≤ 40` is twice the
> forecast 20, allowing for the unscored buckets breaking the other way.
> `gained ≥ 900` is ~53% of the forecast 1,702, the *bottom* of the observed
> 15–57% conversion band applied generously, because unlike most items on this
> board the forecast here is a floor rather than a ceiling: 1,302 lines are
> excluded for being unscorable, not for declining.

### What must not be rebuilt

- **Design R — resolve and print the bare name. 384 converts, 1,025 wrong,
  0.37 gained per wrong.** Worse than every ratio this page's neighbours were
  refused on. Do not re-open it as "just resolve the name"; the baselines want
  the qualifier.
- **Design P as the *first* leg.** Its conversion column is 525/614 (cycle 20b)
  and its at-risk column is now 36 rather than 3,202 — so it is no longer
  refused on its measured cost, but it is a different and smaller item than W,
  and W's conversions do not depend on it. Sequence: W first, then re-measure P
  against whatever W leaves.

### How you would know this page is wrong

- **The unscored buckets are conversions, and W's 1,702 is badly understated.**
  1,302 lines were excluded because the probe cannot compose the wrapper around
  the reference. If a build converts far above 1,702, this is why, and the
  `typeof` build's 122% of its sized row is the precedent.
- **The unscored buckets are wrong lines.** The opposite reading, and the one
  the registered `new wrong ≤ 40` is sized against. The 459 nested W lines are
  the risk: a wrapper that prints right around a qualified name is an assumption
  about the signature renderer, not a measurement.
- **C3's 276 alias-admitted lines are a different mechanism.** They are counted
  in the 4,557 and in W's 1,770. If the alias half turns out to need
  `resolve_alias` and a specifier — `bd tsr-4jk`'s family, refused on
  `checker-notes-novaldecl.md`'s naming grounds — then W's forecast is 276 lines
  optimistic and the sub-family table has a column missing.
- **The at-risk zero is an artefact of the conjunction.** The design-R at-risk
  test requires the printed answer to *contain* the written qualified text.
  A right line whose text was influenced by a qualified reference **without**
  reproducing it verbatim is invisible to it. C7's 8,157 says the predicate has
  a population; it does not say the predicate is complete.
- **41,206 unattributable bare names hide qualified enums.** §5's stated blind
  spot, and the only one that could move the P column by an order of magnitude.
