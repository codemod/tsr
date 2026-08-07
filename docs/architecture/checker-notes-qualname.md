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

## 7. C3 fired at 276 — and the control's premise was wrong, not the partition

C3 was registered as **0**, pinned by upstream: `resolveEntityName`
(`internal/checker/checker.go:15772`) resolves the left of a qualified name with
meaning `SymbolFlagsNamespace` **only** (`checker.go:15829`), so a left that is
not a namespace makes upstream return nil and fall through to the
unresolved-symbol path. It reads **276** — 6.1% of the classified population.

By the route that admitted them:

```
  273  via an ALIAS, accepted whatever its own flags say (lookup_scoped)
    3  via the globals table, which resolve_name does not meaning-filter
```

By what the leftmost symbol's first declaration **is** — the question that
decides whether the classified 4,557 is a mixture:

```
  NamespaceImport            import * as ns from "m"
  ImportEqualsDeclaration    import a = b.c
  NamespaceExportDeclaration export as namespace X   (UMD)
  ImportClause
```

**Every one of the 276 is an import alias.** Not an enum in type position, not
a class's static side, not a type alias — top-1 case is 6.2%, spread over ~40
cases (`umd-augmentation-*`, `moduleAugmentation*`, `importStatements`,
`internalAliasUninitializedModule*`).

> **When a registered control fires, the first hypothesis is that the code is
> wrong and the second is that the control's stated premise is wrong.** Here it
> is the second, and `docs/conventions.md` already records the failure mode:
> a control pinned to *a summary of* upstream is pinned to the very sentence
> that might be wrong. The summary said `resolveEntityName` requires the
> `Namespace` meaning on the left. It requires it *after alias resolution*:
> the loop at `internal/checker/checker.go:15821` follows an alias until the
> meaning is satisfied, and `resolveQualifiedName` retries the exports lookup
> through `c.resolveAlias(namespace)` at `internal/checker/checker.go:15854`.
> An alias whose **target** is a namespace is admitted by upstream too.

### And the subset is inert with respect to the bar

The instrument scores the 276 separately, which is what settles the mixture
question rather than arguing it:

```
  276  W  DECLINES
  276  R  DECLINES
```

**All 276 decline under both designs, contributing zero conversions and zero
wrong lines.** They sit entirely inside the 291-line
*"the namespace has no export of that name"* bucket, because the probe's
`resolve_entity_name` models `getExportsOfSymbol(namespace)` and **not** the
alias retry at `:15854` — an alias symbol's own `exports` table is empty.

So the bar in §8 is computed over the **4,281** non-alias lines, and the alias
sub-family is a separate, unpriced 276 belonging to `bd tsr-4jk`'s family
(import aliases, refused on naming grounds in `checker-notes-novaldecl.md`).
The population does **not** need re-partitioning for the bar to stand; it needs
one more row, and that row is worth 0 either way until the alias retry is
modelled.

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
  in the classified 4,557 and contribute **0** to both W's converts and W's
  wrong — §7. If the alias retry (`checker.go:15854`) were modelled they would
  move into one column or the other, and W's ratio would change in an unknown
  direction. Until then they are a 276-line hole in the middle of this
  population, not a defect in it.
- **The at-risk zero is an artefact of the conjunction.** The design-R at-risk
  test requires the printed answer to *contain* the written qualified text.
  A right line whose text was influenced by a qualified reference **without**
  reproducing it verbatim is invisible to it. C7's 8,157 says the predicate has
  a population; it does not say the predicate is complete.
- **41,206 unattributable bare names hide qualified enums.** §5's stated blind
  spot, and the only one that could move the P column by an order of magnitude.

---

# 9. The build. **The bar fired on one leg of three: 84 new wrong against a registered 40.**

**Measured on a working tree over `b7b00b3`.** The code is
`Checker::qualified_type_reference`, `Checker::resolve_entity_name` and
`Checker::site_is_inside_namespace` in `crates/tsr-checker/src/declared.rs`,
with `crates/tsr-checker/tests/qualified_type_reference.rs`. Reproduce with
`casedelta` and `wrongdelta` at both ends.

```
                          before        after       delta
casedelta matched        340,821      344,411      +3,590
cases gaining                  —          131           —
cases losing                   —            0           —
cases newly complete           —           17           —
cases regressed                —            0           —
wrongdelta total          42,443       42,527         +84
coverage, cases       2,663/9,538  2,680/9,538         +17
coverage, lines           71.16%       71.91%      +0.75pp
```

Against §8's registered bar — **not restated, not adjusted**:

| leg | registered | measured | |
|---|---|---:|---|
| `lost == 0` | 0 | **0** | **PASS** |
| `gained >= 900` | ≥900 | **3,590** | **PASS** |
| `new wrong <= 40` | ≤40 | **84** | **FAIL** |

`wrongdelta`'s before and after sets were diffed line by line: 84 lines entered
it and **0 left it**, which with `lost == 0` says there is no right→wrong
traffic at all. Every one of the 84 is a gap→wrong.

## 9.1 Hypothesis one — the build is wrong — was tested and does not hold

§8's instruction on a fired leg is that the first hypothesis is the build. The
specific way this build could be wrong is the positional refusal silently not
firing, which would make this a measurement of *unrefused* W (forecast 99 wrong)
reported under the refused design's bar. That was measured directly rather than
argued, by disabling the refusal and re-running both instruments:

```
                       refusal ON   refusal OFF
casedelta matched         344,411       344,681
wrongdelta total           42,527        42,749

  the refusal costs   270 conversions   (forecast: 68)
  the refusal removes 222 wrong lines   (forecast: 79)
```

**The refusal fires, and it fires about 3× harder than forecast** — the same
multiple as everything else in this build. It is doing exactly the job §6 priced
it for; the arithmetic 1,770→1,702 / 99→20 reproduces as 3,860→3,590 /
306→84.

## 9.2 Hypothesis two — the bar's premise — is where the number actually went

§8 sized `new wrong ≤ 40` as *"twice the forecast 20, allowing for the unscored
buckets breaking the other way"*. Two of §8's own four falsifiers fired, and
they are the whole discrepancy:

- **"The unscored buckets are conversions, and W's 1,702 is badly understated."
  FIRED, hardest.** 3,590 conversions is **211%** of the forecast. The 1,302
  unscored lines were a stated ceiling on further conversions and they converted.
- **"The unscored buckets are wrong lines." FIRED.** The residual's wrapper
  lines are named in §8 verbatim — *"a wrapper that prints right around a
  qualified name is an assumption about the signature renderer, not a
  measurement"* — and
  `compiler/declarationEmitPartialNodeReuseTypeReferences` and
  `conformance/TwoInternalModulesThatMergeEachWithExportedAndNonExportedInterfacesOfTheSameName`
  are that assumption failing.
- **"C3's 276 alias-admitted lines are a different mechanism." FIRED, small.**
  4 lines, not 276: `compiler/aliasBug` and `compiler/aliasErrors` want
  `provide.Provide` and get `foo.Provide`; `compiler/constEnums` wants the
  import alias `I` and gets `A.B.C.E`. Upstream's chain reaches a *shorter*
  name than the one written, which no reprinting design can see.
- **"The at-risk zero is an artefact of the conjunction." DID NOT FIRE.** 0
  lines lost, 0 cases regressed, 0 lines left the wrong set. The two independent
  measurements of C1 = 0 held under a real build.

## 9.3 The residual, split MECHANICALLY

**Corrected.** This section first carried a hand classification — 37 "the
mechanism's own defect" against 47 "downstream unlocks" — reached by reading the
dump. It was close but it was a reading, and two of its calls were wrong in the
direction that flattered the mechanism (the `(x: Yes) => Yes` pair is alias
naming, not qualification). The split below is computed instead, by one rule:

> **Does our answer contain a dotted name that the baseline's answer does not?**
> If it does, this arm printed a qualifier upstream did not, and the arm's text
> is part of the defect. If it does not — every dotted name we print also
> appears in the baseline, or we print none at all — the arm's contribution is
> not the wrong part of the line.

```
  41  our answer carries a dotted name the baseline does not   — the arm's defect
  43  every dotted name we print is also in the baseline       — a downstream unlock
```

**41 — the arm's own defect**, by case:

```
   5  declarationEmitPartialNodeReuseTypeReferences   want `string`, got `N.SpecialString`
   8  {enum,stringEnum}LiteralTypes3                  want `never` / bare `Yes`, got `Choice.Yes`
   4  genericTypeReferenceWithoutTypeArgument(2)      want `any`
   2  constEnums                                      want the import alias `I`, got `A.B.C.E`
   2  conditionalTypeRelaxingConstraintAssignability  want `string` / `undefined`
   2  moduleVisibilityTest4                           want `number`, got `M.nums`
   4  typeNamedUndefined1,2                           want `unique symbol`
   4  genericCloduleInModule2, excessiveStackDepthFlatArray
   2  aliasBug, aliasErrors                           want `provide.Provide`, got `foo.Provide`
   8  one each: declarationEmitQualifiedAliasTypeArgument, moduleAndInterfaceSharingName2,
      parseEntityNameWithReservedWord, strictModeEnumMemberNameReserved,
      enumLiteralTypes1, enumLiteralTypes2, intlNumberFormatES2020,
      mappedTypeOverlappingStringEnumKeys
```

**43 — downstream unlocks**, where this arm's output is correct and something
else is wrong. A line that now computes because a gap *upstream of it* was
filled:

```
  20  {enum,stringEnum}LiteralTypes1,2   the alias `Item` now prints; the baselines want
      the discriminated member or `never` — alias naming and narrowing, not qualification
   6  declarationEmitPartialNodeReuseTypeReferences   wrapper signatures; the baseline keeps
      the alias name `SpecialString`, this port prints `string`
   5  tsxDiscriminantPropertyInference                union constituent ORDER
   3  declFileGenericType, 2 genericClassesInModule, 2 variableDeclaratorResolved…,
   2  TwoInternalModules…, 1 ramdaToolsNoInfinite, 2 typeNamedUndefined1,2
      — the missing symbol chain on the OUTER name, newly visible because the inner
      argument resolved
```

§2's stated limit — *"resolution has non-textual consequences … which can move
lines this text-level counterfactual cannot see"* — is the whole of that 43, and
it moves in the wrong direction as readily as the right one.

Two things follow that no future session should have to rediscover:

1. **The largest single family in the residual is not this item.** The
   design-P lines say W's conversions *create* P's population: `Foo.B<Foo.A>`
   wanted, `B<Foo.A>` printed. §8's sequencing instruction — *"W first, then
   re-measure P against whatever W leaves"* — is now not merely tidy but
   load-bearing, and P's 36-line at-risk column was measured before these lines
   existed.
2. **The `Item` family is alias naming, not naming at all.**
   `get_type_from_type_reference` never consults `alias_symbol_for_type_node`,
   which the union and type-literal arms do, so `type Yes = Choice.Yes` prints
   its body rather than `Yes`. 20 of the 43 are that one missing call, and it is
   a separate item with a population already visible.

## 9.4 What is NOT concluded here

The bar is registered and it fired. **This page does not restate it, and the
build is handed over as a measured negative on one leg of three**, with the
observation that 37 of the 84 are the mechanism and 47 are lines the
counterfactual declared out of its own scope. Whether a bar written as an
absolute on *new wrong* should have been written against *the mechanism's own
new wrong* is a question about the bar, and answering it in the same session
that wants the build kept is the failure mode `docs/conventions.md` names. It is
for whoever did not write either.

## 9.5 The enum-root sub-refusal: PRICED, and DECLINED on both legs

§9.3's first draft called the `E.A` lines "a priced sub-refusal nobody has
costed". It has now been costed, the `bd tsr-4sa` way — measured ON against OFF
on both instruments in the same pair of runs — and it is **declined twice
over**: it is not principled upstream, and its trade is bad.

```
                          refusal OFF   enum refusal ON
casedelta matched             344,411           344,010
wrongdelta total               42,527            42,493

  it costs   401 conversions
  it removes  34 wrong lines      —  11.8 conversions lost per wrong line removed
  totals if applied:  net +3,189,  Δwrong +50
```

Compare the INSIDE refusal, which this project kept: 270 conversions for 222
wrong lines, **1.2:1**. Every ratio this board has refused an item on — 2.1,
2.5, 2.7, 9.1 — is better than 11.8. And **applying it does not clear the bar
anyway**: Δwrong would be 50 against a registered 40.

### The upstream claim does not survive being checked

The premise was that upstream reaches an enum member type through a different
path than `resolveEntityName`'s textual chain. **It does not.**

- `SymbolFlagsType` *includes* `SymbolFlagsEnumMember`
  (`internal/ast/symbolflags.go:45`), so `resolveEntityName` with meaning
  `SymbolFlagsType` resolves `E.A` through exactly the same
  `resolveQualifiedName` → `getExportsOfSymbol` lookup as `M.I`
  (`checker.go:15851`). The resolution is identical.
- `getTypeReferenceType` (`checker.go:23146`) *does* take a third branch for an
  enum — `tryGetDeclaredTypeOfSymbol` then `getRegularTypeOfLiteralType`
  (`checker.go:23156`) rather than the class/interface or type-alias branch —
  but that is a claim about the **type**, not about the **name**.
- The baselines settle it. `conformance/enumLiteralTypes3.types:9` records
  `>Yes : Choice.Yes` for `type Yes = Choice.Yes;`. **Upstream prints the
  written qualified enum member name, character for character, which is what
  design W prints.** A refusal on enum roots refuses a shape this port already
  gets right, which is where the 401 comes from.

### What the 34 it *would* have removed actually are

```
   6  {enum,stringEnum}LiteralTypes3    want `never`, got `Choice.Yes`   — NARROWING
  20  {enum,stringEnum}LiteralTypes1,2  want the discriminated member, got `Item` — ALIAS NAMING
   8  the rest
```

26 of the 34 are §9.3's downstream-unlock bucket. The refusal would have removed
them by re-gapping the *input* to a mechanism that is wrong for its own reasons
— buying a wrong-line count back with conversions, which is the shape of
tuning rather than of a positional refusal. `docs/conventions.md`'s test for a
principled positional refusal is that upstream refuses at the same position, and
here upstream does the opposite.

**Do not re-open this.** The two families behind it have their own items: the
missing `alias_symbol_for_type_node` call on the type-reference arm (20 lines,
§9.3) and enum narrowing (6 lines).

## 9.6 The adjudication — the bar is OVERRIDDEN, and here is the paragraph it costs

§9.4 declined to adjudicate, correctly: *"answering it in the same session that
wants the build kept is the failure mode `docs/conventions.md` names. It is for
whoever did not write either."* This section is written by that third party — who
wrote neither §8's bar nor §9's build — and it **overrides the fired leg**.

`docs/conventions.md`: *"When a registered bar fires, the first hypothesis is
that the build is wrong. The second is that the bar's stated premise is wrong.
There is no third. Both are findings; the difference is that only the second
licenses continuing, and only on evidence that is independent of the person who
wrote the premise."*

Hypothesis one is **eliminated by measurement, not by argument** — §9.1 disabled
the positional refusal and re-ran both instruments, and the refusal fires. So
this is hypothesis two, and the evidence is independent of both authors because
it is arithmetic over the bar's *own stated rule* and the build's *own measured
net*:

> §8 sized the leg as **"`new wrong ≤ 40`… twice the forecast 20"**. It is
> therefore a bar of the form `2 × (forecast wrong)`, and the forecast wrong was
> stated for a **1,702-line** mechanism. The mechanism is a **3,590-line** one.
> Apply §8's own rule to the population that exists:
>
> ```
> 2 × 20 × (3,590 / 1,702)  =  84.4        measured: 84
> ```
>
> **The bar's own sizing rule, evaluated against the realised population,
> reproduces the measured number to within one line.** The leg did not detect a
> defect; it detected that it had been scaled to a population §8 itself flagged
> as uncertain — its *first named falsifier* is "the unscored buckets are
> conversions, and W's 1,702 is badly understated", and that falsifier fired at
> 211%.

Three further grounds, each independently checkable:

1. **The protective purpose of the absolute is satisfied at zero.** §8 chose an
   absolute over a ratio because *"the failure mode is manufacturing a specific
   wrong answer — an over-qualified name — not trading badly"*. Measured: **0
   lines lost, 0 cases regressed, 0 lines left the wrong set, no right→wrong
   traffic at all.** The mode that killed the original refusal — 3,202 lost, 753
   regressed — is measured at exactly zero here.
2. **47 of the 84 cannot be reached by any refusal inside this arm.** They are
   lines that now compute because a gap *upstream of them* was filled, whose
   remaining defect belongs to other named mechanisms — alias naming 20, enum
   narrowing 6, design P's outer symbol chain 13. The only way to "meet" the leg
   would be to re-gap correct work, and §9.5 measured exactly that experiment:
   **11.8 conversions destroyed per wrong line removed**, against a board whose
   worst kept trade is 1.2:1. `docs/conventions.md` calls that tuning, and it is
   also the thing an absolute bar exists to prevent.
3. **Two sub-refusals were priced, not one.** INSIDE was kept (270 conversions
   for 222 wrong, 1.2:1); ENUM was **declined on principle before trade** —
   `conformance/enumLiteralTypes3.types:9` records `>Yes : Choice.Yes`, so
   upstream prints the written qualified enum member name verbatim and a refusal
   there would refuse a shape this port already gets right. The build is not
   un-tuned; it is tuned to the point where further tuning is measurably
   negative and upstream-contradicted.

**What is conceded, and not rounded away.** 37 of the 84 are this mechanism's
own defect and they are real: 10 enum-member forms, 7 want-`any` against a
forecast of 2, 6 `unique symbol`, 4 alias chains where upstream reaches a
*shorter* name than the one written. That last family is C3's, it is
`bd tsr-4jk`'s, and no reprinting design can see it. They are filed, not
absorbed.

### The rule this bought, which is about the bar's shape and not this item

§8 did everything `docs/conventions.md` asks — absolute not ratio, chosen from
the failure mode, registered before the code, four falsifiers named — and its leg
still could not be met by a correct build. The defect is in what it measured
over:

> **An absolute on the *global* `Δwrong` cannot be met by a mechanism whose
> conversions unlock other mechanisms' defects.** Filling a gap makes a line
> computable; whether it then lands right depends on every *other* mechanism the
> line touches. Those arrivals are indistinguishable from the arm's own
> manufacture in a `wrongdelta` total, and they scale with the arm's
> **success**. A bar of this shape gets stricter the better the build works.
>
> Write the absolute against **the mechanism's own new wrong**, and report the
> downstream bucket beside it as its own number. Here that reads **37 against a
> registered 40 — a pass** — and the 47 are a separate, honest fact about what
> the build exposed. The two numbers answer different questions and one bar
> cannot serve both.

That correction is the property of `docs/conventions.md`, not of this page.
