# Wall 2 — per-site type printing: the specification

**Lane**: checker-1, §527. **Status**: spec. No checker code is written from
this page until §5's build order names a slice and §6 registers its bar.

This page exists because twenty-plus refusals across nine months lean on one
divergence, and each has had to re-derive it. It is the transcription target,
the debt ledger it must pay, and the build order — written before the code,
per the project's own rule.

---

## 0. The divergence, stated once

Upstream's node builder takes the **enclosing declaration** as an argument
(`TypeToTypeNode(type, enclosingDeclaration, flags)`) and renders a type's
name at each site by walking symbol containers for an accessible spelling.
This port computes a type's printed form **once, at creation**
(`TypeData::Named { text, .. }` and `TypeData::Anonymous { text, .. }`,
`crates/tsr-checker/src/types.rs`) — ADR-0003's print-at-creation consequence.

A type can therefore be printed exactly one way, everywhere.

[ADR-0043](../adr/0043-a-type-must-be-renderable-differently-at-different-sites.md)
named this limit and priced three options **without deciding**; it is marked
*Proposed* and says so in its first line. This page supplies the evidence its
"how we would know this is wrong" section asked for, and the decision record
that follows it is ADR-0044.

---

## 1. What the port already has — read this before assuming anything is missing

ADR-0043 wrote that acting on site-dependence is *"impossible today, because
the port has nothing else to print."* **That is no longer true**, and the
correction matters more than anything else on this page: between §95 and §513
the port grew a working per-site rendering layer. The spec's job is to say
where that layer becomes authoritative — not to build one.

`Checker::type_to_string_at(id, reference) -> Option<String>` (`checker.rs`)
is the per-site entry. Every consumer of a rendered `.types` line goes through
it (`types_producer.rs:1355`). It holds two kinds of arm:

**(a) Composite re-render arms — true site rendering, already built.** Each
decomposes a type through a side table and re-renders every slot *at the
site*, with `rendering_composites` as the cycle guard:

| arm | side table | what it rebuilds |
|---|---|---|
| single-signature (§10.13) | `signature_types` | the whole signature, slot by slot |
| multi-signature type literal (§99) | `signature_types` | each member, with per-site type-parameter renames |
| union with origin (§97) | `union_origin` | each constituent |
| generic reference (§95) | `type_reference_targets` | the name **from the symbol** + each argument |

**(b) The leaf road — `qualified_name_at`, which is string surgery.** For a
`Named`/`Anonymous`/aliased-`Union`/`Intersection` type it takes the **baked
text** and patches it:

1. `best_name` rename, spliced in at the offset `split_around_name` found;
2. the §10.9 baked-`{parent}.{name}` segment rename;
3. §509's bare-accessibility guard (no qualifier where the site's bare name
   resolves to this symbol, gated on first-declaration equality and lexical
   containment);
4. the `default` refusal;
5. otherwise `symbol_chain`'s qualifier, prepended at the same offset.

### 1.1 `split_around_name` is the choke point, and naming it is this page's first finding

`Checker::split_around_name` (`checker.rs:1829`) recognises exactly **three**
baked shapes: `typeof C`, a bare `C`, and `C<…>`. Any other embedding of the
name in the baked text — `typeof M.inst`, `A.B`, `{ a: C; }`, `(x: C) => void`
— returns `None`, and `qualified_name_at` hands back the baked text unchanged.

So the port's naming is not "absent". It is **present for three spellings and
absent for every other**, and the composite arms exist precisely because
someone hit that wall from the inside and decomposed around it, one shape at a
time (§95, §97, §99, §10.13 are four such rescues).

### 1.2 The symbol is already retained — ADR-0043's expensive option is cheaper than it priced

`TypeData::Named` carries `members: Option<SymbolId>` and
`TypeData::Anonymous` carries `symbol: SymbolId`. Of 38 `new_named` call
sites, the ones that mint a *nameable* type pass a symbol
(`declared.rs:1143`/`:3014`/`:3119`, `objects.rs:1352`); the ones passing
`None` mint types that have no name to render — tuples, the §29 alias
placeholder, generic-alias `Tree<T>` texts, minted composites.

`reference_text_at` (`checker.rs:1558`) already proves the render works:
for a generic reference it computes the name as
`best_name(target, site)` → else `symbol_chain(target, site) + target_name` →
else the bare name, and renders the arguments recursively at the site. **The
baked text is never consulted on that road.**

**The finding:** the name-from-symbol render is built; it is reachable only
for types recorded in `type_reference_targets`. Extending it to every
symbol-carrying type is not ADR-0043's Option 1 ("retain the structural
rendering alongside the name" — a wider `TypeData`, a slower store, every
consumer touched). It is one new arm in `type_to_string_at` ahead of the leaf
road, with the baked text kept as the fallback for symbol-less mints.

---

## 2. The two mechanisms are independent, and conflating them is why the wall looked monolithic

Every ledger entry in §3 belongs to exactly one of these. They share a name
("per-site printing") and nothing else.

**M1 — the NAME render.** Which spelling a symbol gets at a site: bare,
qualified (`A.B`), renamed through an alias (`typeof React`), module-qualified
(`import("./m").X`), or `globalThis.`-prefixed. Input: a symbol and an
enclosing declaration. Owns the accessibility walk.

**M2 — CONSTRUCTION ORDER, not node reuse.** Which order a union's
constituents print in. **This section was written as "the node-reuse rule" and
the upstream read overturned it; the correction is left visible because the
wrong framing is the one every prior session reached for.**

The `.types` baseline writer calls `builder.TypeToTypeNode`
(`internal/testutil/tsbaseline/type_symbol_baseline.go:395` →
`nodebuilder.go:267`), **not** `SerializeTypeForDeclaration` /
`SerializeReturnTypeForSignature`. The whole annotation-reuse machinery —
`tryReuseExistingNodeHelper` (`nodecopy.go:222`), `serializeTypeForDeclaration`
(`nodebuilderimpl.go:2228`), the pseudo-type node builder — is on the
DECLARATION-EMIT path and is **bypassed for every line the conformance corpus
compares against**. The only reuse a baseline line can reach is the
instantiation-expression `typeof X<…>` case (`nodebuilderimpl.go:2837`) and
type-parameter constraints (`:1615`).

So §517's +80 and §519's 356 cannot both be node reuse, because on the
baseline path there is none. What splits them is how each union was
CONSTRUCTED: a type-literal member's union is built by reading its annotation
(constituents in written order); a narrowed declaration's union is built by
flow analysis (constituents in flow order). This port interns unions and the
first construction fixes the text for every later identical set — which is
also §513's collapse seen from the other side.

**This is stated as the leading hypothesis, not as a finding.** The probe that
settles it: build the same constituent set two ways in one file (an annotation
and a narrowing) and read upstream's two printed orders. If they differ,
construction order is the rule and the port needs per-construction union
identity. If they agree, the rule is elsewhere and §519 needs a different
diagnosis. **Nobody has run it**; it is §5's first non-blocking item and costs
one `probefile` run.

---

## 3. The debt ledger — this is the acceptance set

Every row is a measurement with the number that produced it. **M** names the
owning mechanism.

| # | debt | measured | M |
|---|---|---|---|
| §519 | written-union carriage at declaration lines | **356 R→W** (typeGuards families want the computed order); `unionTypeCallSignatures6`'s **4 W→R** is the counter-population | M2 |
| §517 | the same carriage at type-literal members | **+80/0**, landed | M2 |
| §503/§222 | `umd-augmentation-1`: `typeof m` → error, `m.Vector` → `Vector` | **14 R→W**, three independent measurements | M1 |
| §499 | declarationEmit/jsxNamespace naming families | **22 G→W** | M1 |
| SS197 | `globalThis.` qualification as a shadowing probe | **−66 cases**; 12+ deficit-1 cases (`importAndVariableDeclarationConflict1-4`, `collisionCodeGenModuleWith*`) still want it | M1 |
| §158 | ES import kinds — minted texts TRAVEL across units | **213:72 (3.0:1)**, refused | M1 |
| `tsr-4jk` | alias-named module objects | **692 lines** | M1 |
| design W | qualified naming, printing half | **1,770 converts / 99 wrong / 0 at risk** (design R stays refused at 0.37 per wrong) | M1 |
| §491/§501 | RENAMED specifiers decline in both | residue | M1 |
| §521 | `allowsImportingTsExtension`, `dynamicImportsDeclaration` want the WRITTEN specifier (`import("./a.ts")`) | 11 adverse | M2 |
| — | `es6ExportEqualsInterop`'s `import(...)`-qualified rows | 19 R→W in failing cases | M1 |
| §168 | the anonymous-class half | refused, reopens here | M1 |
| §373 | 37 cases : 2,849 lines | refused, reopens here | M1 |
| §311 | — | refused, reopens here | M1 |
| §158 | `checkJsdocTypeTagOnExportAssignment` | refused, reopens here | M1 |

**M1 carries the overwhelming majority of the debt.** That is the build-order
argument: M2 is one refused row plus one landed row, and its rule is a
transcription; M1 is everything else.

---

## 4. Upstream, transcribed

Both reads are at the pinned submodule. `SA` =
`internal/checker/symbolaccessibility.go`, `NB` =
`internal/checker/nodebuilderimpl.go`.

### 4.1 The walk — `getAccessibleSymbolChain` (SA:373)

Scopes are yielded by `someSymbolTableInScope` (SA:746-804), walking
`location = enclosingDeclaration` up through `.Parent`, and the FIRST table
that yields a non-empty chain wins (SA:466-473):

1. `location.Locals()` where `canHaveLocals && !IsGlobalSourceFile`
   (SA:752-756) — `isLocalNameLookup = true`;
2. a SourceFile that is an external module, or a ModuleDeclaration → that
   symbol's `Exports` (SA:758-765) — `isLocalNameLookup = true`;
3. Class/ClassExpression/Interface → `Members` **filtered to type parameters**
   (`Flags & (Type &^ Assignment)`, SA:766-787) — `isLocalNameLookup = false`,
   and raw `Members` rather than `getMembersOfSymbol` to avoid late-binding
   recursion;
4. always, last: `c.globals` (SA:803).

Two guards frame it: members are never nameable by chain
(`isPropertyOrMethodDeclarationSymbol`, SA:728-744 → nil), and a per-symbol
visited set of table ids makes the walk re-entrant-safe (SA:481-499, inserted
before `trySymbolTable` and **deleted on the way out** — a recursion guard,
not a memo).

### 4.2 `trySymbolTable` (SA:535-593) — three arms in order

1. **DIRECT** (SA:544-547): `symbols[symbol.Name]` present and
   `isAccessible` → return `[ctx.symbol]`. This is the arm that makes a bare
   name win over any alias, and it is why §509's guard was right to answer
   `None` on a direct hit.
2. **ExportSymbol** (SA:554-558), a vendor-local compensation for arm 3
   iterating only alias-flagged entries.
3. **ALIAS** (SA:562-580), over alias-flagged entries only, with these
   exclusions ANDed:
   - name is not `export=` (SA:564) and not `default` (SA:565);
   - not a UMD export symbol while the site is inside an external module
     (SA:566);
   - `!useOnlyExternalAliasing || IsExternalModuleImportEqualsDeclaration`
     (SA:568) — **the baseline path passes `false`** (see §4.4), so every
     alias kind is admitted;
   - when `isLocalNameLookup`, exclude namespace re-exports
     (`export * as ns from "m"`, SA:570/616-618) — kept when building a
     DOTTED name, excluded for a bare one;
   - `ignoreQualification || no ExportSpecifier declarations` (SA:573).

**Candidates are then SORTED and the SHORTEST CHAIN WINS**
(`compareSymbolChains`, SA:582-586 / SA:595-610; ties break element-wise by
`compareSymbols`). The port's `module_alias_at` declines on ambiguity instead;
that decline is a refusal where upstream has a total order, and it is the
cheapest correctness gap on this page.

`getCandidateListForSymbol` (SA:620-645) is the recursion: accessible → the
one-element chain; otherwise descend into `getExportsOfSymbol(resolveAlias)`
with `ignoreQualification = true, isLocalNameLookup = false` (SA:636-640),
gated by `canQualifySymbol` (SA:641), and prepend
(SA:644 — element 0 is the OUTERMOST name).

`canQualifySymbol` (SA:677-686) = `!needsQualification` OR the PARENT has an
accessible chain, **sharing the visited map into the recursion** (SA:685).
`needsQualification` (SA:688-726) walks the same scopes and answers true when
a DIFFERENT symbol of the same MEANING owns that name (SA:716-719). The port
has this as `Checker::needs_qualification` (`checker.rs:2091`) — one
`resolve_name` and a merged-symbol comparison, which is the same predicate
reduced to the one scope walk the binder already offers.

### 4.3 Chain → text (NB:644-748, `symbolToTypeNode`)

- `isTypeOf := mask == SymbolFlagsValue` (NB:649).
- **ImportTypeNode** iff `chain[0]` has a non-global-augmentation external
  module declaration (NB:650-730): the module is dropped from the dotted name
  (`stopper = 1`, NB:654), the specifier comes from
  `getSpecifierForModuleSymbol`, and `import("m").X` is assembled at NB:721.
- **TypeQueryNode / TypeReferenceNode** otherwise (NB:733-746): the dotted
  name from `createAccessFromSymbolChain` (`stopper = 0`), wrapped in
  `typeof …` when `isTypeOf`.
- `createAccessFromSymbolChain` (NB:757-851) builds the dots; at index 0 the
  name is `getNameOfSymbolAsWritten` under `InInitialEntityName`
  (NB:769-773), and at index > 0 the symbol is re-looked-up **by name in the
  parent's exports** so export aliases win (NB:776-798).

**`getSpecifierForModuleSymbol` (NB:1249) does NOT reuse the written
specifier.** The written one is consulted only to derive the resolution mode
(NB:1275-1280); the printed text always comes from
`modulespecifiers.GetModuleSpecifiers` with hardcoded
`ImportModuleSpecifierPreferenceProjectRelative` (NB:998). **This retires
§521's open question**: `allowsImportingTsExtension`'s `import("./a.ts")` is
not written-specifier preservation — it is what the module-specifier
generator computes under those options, and the port's §521 fallback needs
that generator, not a carriage.

### 4.4 The baseline path's flags (the fact that reframes M2)

`type_symbol_baseline.go:395` calls `TypeToTypeNode` with
`NoTruncation | AllowUniqueESSymbolType | GenerateNamesForShadowedTypeParams`
plus `IgnoreErrors` and `AllowUnresolvedNames`. **`UseOnlyExternalAliasing` is
not among them** (it is set only from `printer.go:268` and hover,
`nodebuilder_hover.go:431`), so `NB:1088` always passes `false` — local
aliases are eligible on every corpus line. And, per §2, the declaration-emit
reuse machinery is not on this path at all.

### 4.5 `globalThis` — the exact condition, and why SS197 measured −66

`globalThisSymbol` is an ordinary `Module` symbol whose `Exports` IS the
globals table (CH:962-964); there is no special case in the printer. The
ONLY injection point is SA:588-591, at the very bottom of `trySymbolTable`:

> when the DIRECT arm failed, the ExportSymbol arm produced nothing, the
> alias loop produced zero candidate chains, **and the table is `c.globals`**,
> return `getCandidateListForSymbol(globalThisSymbol, …)` — which recurses
> into the globals table with `ignoreQualification = true` and yields the
> two-element chain `[globalThis, target]`.

In one sentence: **`globalThis.` is prefixed exactly when the target is
reachable through the globals table but no in-scope table can name it,
because a different symbol of the same meaning shadows the name and no alias
offers a route.**

SS197 measured **−66 cases** because it asked a *shadowing* question at every
site — "does the bare name resolve to a different symbol?" — which fires in
local scopes, module scopes and member scopes where upstream's rule cannot
reach the globals arm at all. The rule is not "shadowed ⇒ qualify"; it is
"shadowed AND global AND nothing else could name it ⇒ `globalThis.`". Those
differ on exactly the ~78 sites SS197 damaged.

---

## 5. Build order

The ledger splits into work of very different sizes, and the read says the
first slice is small and safe rather than architectural.

**Slice 1 — `globalThis.` (M1), this session.** SA:588-591 transcribed onto
the port's existing walk: `symbol_chain` already fails where upstream reaches
the globals arm, so the arm is one fallback at that failure point, gated on
(a) the symbol being reachable through globals, and (b)
`needs_qualification` answering true at the site. Population: the
`typeof globalThis.X` census row — `importAndVariableDeclarationConflict1-4`
(4 deficit-1 cases), `collisionCodeGenModuleWith*` (12+). SS197's −66 is the
falsifier: the narrow rule must not fire where the broad one did.

**Slice 2 — shortest-chain selection (M1).** Replace `module_alias_at`'s
ambiguity DECLINE with upstream's `compareSymbolChains` total order
(shorter wins, then `compareSymbols`). Pays §14's two-alias refusal,
`tsr-4jk`'s 692, and part of §501/§491's renamed residues.

**Slice 3 — the name render becomes authoritative (M1).** §1.2's arm: a
symbol-carrying `Named`/`Anonymous` renders its NAME from the symbol at the
site (the `reference_text_at` road generalised), baked text as fallback for
symbol-less mints. Pays design W's 1,770, §158, §168, §373, §311, §499,
§503's 14. This is the big one and it needs slices 1-2 under it.

**Slice 4 — module specifiers (M1/M2 boundary).** Port
`modulespecifiers.GetModuleSpecifiers`' relative-path computation to replace
§521's flat-mount approximation; retires §521's 11 adverse.

**Non-blocking probe, before any M2 work:** §2's construction-order question,
one `probefile` run.

---

## 6. The bar for slice 1, registered before the code

- **Population**: rendered lines whose printed name would gain a
  `globalThis.` prefix under the SA:588-591 rule.
- **Predict**: the four `importAndVariableDeclarationConflict` cases convert
  (+4), and the `collisionCodeGenModuleWith*` family moves.
- **Refuse if**: any R→W lands in a PASSING case (the standing full-stop
  rule), or the arm fires anywhere SS197's broad probe fired and upstream
  does not want it — measured as: total wrongs must NET DOWN, and the
  adverse must not exceed converts.
- **Falsifier named in advance**: if the arm fires on more than a handful of
  sites, the globals-reachability gate is not doing its job and the build is
  SS197 again under a new name.

---

## 7. Slice 1, measured: the arm is REFUSED, and its bar caught a real bug on the way

### 7.1 The refusal

The globals-table fallback was built exactly as §4.5 transcribes it — at
`symbol_chain`'s no-parent exit, gated on the target being reachable through
`binder.globals()`. It produces the wanted spelling on the head fixture
(`namespace m { export var m = "" }` + `import x = m.m` prints
`typeof globalThis.m`, matching `importAndVariableDeclarationConflict1`).

**Corpus measurement: 38 W→R against 25 R→W, and two of the damaged cases
were PASSING** (`collisionCodeGenEnumWithEnumMemberConflict`,
`strictModeReservedWord2`). Slice 1's registered bar refuses on either
condition — an R→W in a passing case, or the arm firing beyond a handful of
sites. Both fired. **Reverted.**

The wins are real and stay on the board: `nameCollision` 5,
`collisionCodeGenModuleWithModuleReopening` 4,
`declarationEmitTypeParameterNameInOuterScope` 4.

### 7.2 What the refusal cost, and the reopening condition

The damage is NOT in the globals gate. It is one level up, in
`needs_qualification`'s inputs, and the arm merely made it observable — the
same shape as §162's constructor arm, where a predicate that had never been
exercised turned out to be wrong the moment something reached it.

Worked case: `enum Color { Color, Thing = Color }` — upstream prints `Color`,
the arm printed `globalThis.Color`. Upstream's `needsQualification`
(`symbolaccessibility.go:716-719`) qualifies only when the shadowing symbol
carries the meaning being printed, and `symbolToTypeNode`
(`nodebuilderimpl.go:649`) passes ONE meaning. The enum MEMBER shadows in
`Value`; the reference is a `Type`; upstream does not qualify. §7.3 fixes
exactly that and the enum case still flipped under the arm — so a THIRD
input is wrong, and naming it is the reopening condition:

> **Reopen slice 1 when `needs_qualification` is verified against
> `symbolaccessibility.go:688-726` line by line** — specifically its scope
> walk (which tables, in which order, with which skip conditions at
> `:692-695` and `:708-715`), rather than the port's single `resolve_name`
> call. The port asks the BINDER for a name's resolution; upstream walks the
> same tables the chain walk walks and applies `getSymbolFlags` to the
> resolved alias. Those differ wherever `resolve_name`'s meaning handling and
> upstream's `flags & meaning` test disagree — enum members are one such
> place and there are certainly others.

Do not rebuild the globals arm before that verification. It is three lines of
code sitting on a predicate that has now been measured wrong twice.

### 7.3 What DID land from slice 1: the meaning is one flag, not two (+2 cases, +3 W→R, 0 adverse)

`qualified_name_at` passed `TYPE | VALUE` to `symbol_chain` at every site.
Upstream passes `mask` — `Value` for a `typeof` position, `Type` otherwise —
and `needsQualification` tests `flags & meaning`. The union let a Value-only
shadow qualify a Type reference.

Derived from the baked text (`typeof …` ⇒ `VALUE`, else `TYPE`), which is
upstream's own discriminator read off the only signal available before the
name render lands. Measured ALONE, after the globals arm was reverted:
**+3 W→R, zero adverse** — `interMixingModulesInterfaces3/5` (completing
§509's quartet) and `typeNamedUndefined2`. Two cases.

**This is the session's evidence that the spec was worth writing before the
code**: the bug is nine months old, sits on the hottest naming road in the
checker, and was invisible until the transcription named what the argument
was supposed to be.

---

## 8. The floor, measured — `needs_qualification` CONFLATES two upstream conditions, and that is the whole diagnosis

§7.2 named the reopening condition as *"verify `needs_qualification`
line-by-line against `symbolaccessibility.go:688-726`"*. Done, and the answer
is better than a line-by-line diff: **the port's predicate is not upstream's
predicate at all — it is upstream's predicate OR'd with a second condition,
and the OR is load-bearing.**

### 8.1 The transcription

Upstream (`symbolaccessibility.go:688-726`) starts `qualify := false`, walks
the scope tables, and per table: name absent → continue; the entry IS this
symbol → stop, **no qualification**; otherwise resolve the alias (unless it
is an export specifier), take `getSymbolFlags`, and `flags & meaning != 0` →
**qualify**, stop. A name present in NO table leaves the callback never
firing: the function answers **false**.

The port (`checker.rs:2097`) asks `binder.resolve_name` once and answers
`Some(found) => found != symbol`, **`None => true`**.

### 8.2 The counterfactual: `None => false` measures −6,850

Applied and measured against the §527 baseline: **6,850 R→W** —
`compiler/temporal` 3,318, `resolvingClassDeclarationWhenInBaseTypeResolution`
1,022, the whole `privacy*CannotName*` family. Reverted immediately.

That is not a bug in the change; it is the proof of what the `true` is for.
Those 6,850 lines are names the port's `resolve_name` **cannot resolve at the
site** — cross-file, namespace-member, lib — and every one of them genuinely
needs its qualifier. `None => true` was supplying it.

### 8.3 What the port actually collapsed

Upstream's node builder asks TWO questions and this port asks one. At
`nodebuilderimpl.go:1093-1094` the qualifier walk begins when:

> `chain == nil` **OR** `needsQualification(chain[0], enclosingDeclaration, qualifierMeaning)`

— *no accessible chain exists*, **or** *a chain exists but the leading name is
shadowed*. `needs_qualification`'s `None => true` is the **first** disjunct
wearing the second one's name: an unresolvable name is the port's proxy for
"no accessible chain".

So the predicate is doing correct work for the wrong stated reason, which is
exactly the shape `docs/conventions.md` warns about — and it is why slice 1's
globals arm misfired. That arm asked "was qualification needed?" and got back
"either it was shadowed, or we could not resolve it", then treated both as
shadowing. `enum Color { Color }` is the clean witness: `Color` is perfectly
accessible as itself (chain ≠ nil, `needsQualification` false, upstream prints
`Color`), but the port's `resolve_name` answers `None` at that position, the
conflated predicate says "qualify", and the globals arm spelled
`globalThis.Color`.

### 8.4 The reopening condition, restated and now precise

> **Split the predicate before rebuilding any arm on top of it.**
> `accessible_chain_exists(symbol, site, meaning)` and
> `needs_qualification(symbol, site, meaning)` are different questions with
> different upstream sources (`getAccessibleSymbolChain`, SA:373, versus
> `needsQualification`, SA:688). Today's function is their disjunction and
> **must keep behaving as the disjunction at every existing call site** — the
> −6,850 is the price of changing that blindly. The split is additive: give
> the shadowing question its own honest transcription (the table walk, not
> `resolve_name`), keep the existing conflated predicate for the qualifier
> road until each call site is re-pointed with its own measurement, and let
> slice 1's globals arm consult the SHADOWING half alone — which is what
> `symbolaccessibility.go:588-591` actually sits behind.

This is a bigger correction than §7.2 anticipated and it is the reason slices
2 and 3 should not be attempted first: both call the same conflated
predicate, and both would measure noise attributable to it rather than to
themselves.

---

## 9. Slice 3 SIZED — and the number retires the "wall 2 carries the campaign" premise

Measured at `fbe9a10a` by bucketing every non-RIGHT line in
`target/verdict_baseline.tsv` on the STRING relationship between the wanted
and printed type (`scratchpad/sizew2b.py`; the dump is the whole input, no
checker API needed). This is the counterfactual `docs/conventions.md` asks
for — *size the conversion, not the population* — and it was run BEFORE
slice 3 was built, which is the only reason it could change the plan.

### 9.1 The buckets

| bucket | lines | share |
|---|---:|---:|
| **structural** — a different type shape entirely | 22,095 | 53.4% |
| **type-differs** — same shape, but a KEYWORD differs (`string` vs `number`) | 9,280 | 22.4% |
| gap (we print `error`) | 8,701 | 21.0% |
| **RENAME** — same shape, only non-keyword identifiers differ | **817** | **2.0%** |
| **QUALIFIER** — identical after stripping dotted prefixes | **474** | **1.1%** |
| whole-line diff with equal type text | 8 | 0.0% |
| | 41,375 | |

**Cases whose ONLY damage is name-shaped: 79** (282 lines). Not 1,770, and
not the ~400 the first, looser pass reported — that pass counted `string` vs
`number` as a rename because they share a punctuation shape, which is the
same over-count STATUS.md §2 warns the near-miss board carries.

### 9.2 And most of the RENAME bucket is not slice 3's either

The top pairs (wanted ← printed), from the same run:

```
 221  T_1  <-  T        54  U_1 <- U       38  S_1 <- S      12  fn_1 <- fn
  45  Infinity <- inf
  40  TestA <- Test     22  Yes <- No      16  ConcreteA <- AbstractA
  14  SharedArrayBuffer <- ArrayBuffer     11  Error1 <- Correct
```

Three different mechanisms, and **only the third is wall 2**:

1. **~325 lines are SHADOWED TYPE-PARAMETER renaming.** The baseline path
   passes `GenerateNamesForShadowedTypeParams`
   (`type_symbol_baseline.go:395`), so upstream spells an inner `T` that
   shadows an outer one as `T_1`. The port has §102's
   `rename_type_parameters_for_site` and it is evidently partial. **This is a
   separate, cheaper, larger mechanism than slice 3** and it is now the
   best-priced naming item on the board.
2. **`Infinity` ← `inf` (45 lines)** is a NUMBER-PRINTING bug, not naming.
3. **`TestA` ← `Test`, `Yes` ← `No`, `AbstractA` ← `AbstractB`,
   `SharedArrayBuffer` ← `ArrayBuffer`** — the port resolved to the **wrong
   symbol**, not the right symbol under a wrong name. No renderer fixes
   those.

So slice 3's honest population is the 474 QUALIFIER lines plus whatever
minority of the 817 is genuine aliasing — call it well under 1,000 lines and
bounded above by 79 cases, of which several belong to (1).

### 9.3 What this retires, and what it opens

**Retired: the premise that wall 2 carries the campaign to 70%.** It does
not. Three quarters of the remaining damage (53.4% structural + 22.4%
type-differs = **75.8%**) is the checker computing a DIFFERENT TYPE, not
printing a name wrong. That work is inference, flow, members and relations —
not the node builder. Design W's "1,770 converts / 99 wrong" was measured on
a compiler many sessions older and should not be quoted again without this
re-take beside it.

**Opened, and cheap:** the shadowed-type-parameter rename (§9.2 item 1),
~325 lines, one upstream flag whose rule is local to a signature's own scope
and needs none of the accessibility machinery this page specifies.

**Still worth building, at its true size:** slices 1–4 remain correct and
remain blocked on §8's predicate split. They are worth ~79 cases, which is a
good session, not a campaign.

### 9.4 The falsifier for this sizing

The buckets are STRING tests, so a line whose want and got differ
structurally *because* a name resolved wrongly upstream of the printer counts
as structural here and would convert if the name were fixed. That biases the
name buckets DOWN. The check: take the 22,095 structural lines and re-bucket
them by whether their two type texts share a symbol-derived skeleton. Not
run. Until it is, treat 79 cases as a floor for wall 2 and 75.8% as a ceiling
for "not wall 2".

---

## 10. The other three quarters, sized — where the campaign's distance actually is

§9 established that naming is 3.1% of the remaining damage. This sizes the
rest, by the transition `(what we print) -> (what is wanted)`, and then asks
the only question that matters for planning: **how many failing cases are
blocked by exactly ONE transition** — i.e. would convert whole if that single
mechanism were fixed. Instrument: `scripts/typegap_sizing.py`, same input.

### 10.1 The transitions

| we print | wanted | lines | share |
|---|---|---:|---:|
| `any` | function | 5,231 | 12.6% |
| `any` | primitive | 5,216 | 12.6% |
| function | function | 2,859 | 6.9% |
| `error` | function | 2,813 | 6.8% |
| `any` | object | 1,897 | 4.6% |
| object | object | 1,725 | 4.2% |
| `any` | name | 1,704 | 4.1% |
| `any` | generic-ref | 1,598 | 3.9% |
| `error` | primitive | 1,598 | 3.9% |
| `any` | union | 1,352 | 3.3% |

**`any` is the printed value on roughly 45% of all remaining damage.** Not
`error` — `any`. The port answers a confident `any` where upstream computes a
real type far more often than it gaps.

### 10.2 The number to plan against

**1,259 of 3,522 failing cases (36%) are blocked by exactly one transition.**
The largest single-mechanism populations, by CASES:

| cases | lines | transition |
|---:|---:|---|
| 100 | 396 | `any` → function |
| 93 | 329 | `any` → primitive |
| 93 | 218 | function → function |
| 82 | 359 | `any` → object |
| 60 | 219 | `error` → function |
| 58 | 268 | object → object |
| 51 | 163 | `any` → name |
| 41 | 143 | `error` → `any` |
| 39 | 108 | `error` → object |
| 39 | 88 | typeof → typeof |

Two readings, both load-bearing:

1. **Function types are the single largest owner** — `any → function` (100)
   plus `error → function` (60) plus `function → function` (93) is **253
   single-transition cases**, and that is signature/contextual computation,
   the territory `checker-notes-fnexpr.md` §10 measured at 86% entangled and
   refused. The refusal was correct about the *contextual* half; this number
   says the family is worth re-pricing as a whole, because it is now the
   biggest thing on the board by cases.
2. **`function → function` is 2.3 lines per case** — the signature IS
   computed and one slot is wrong. Sampling shows at least four distinct
   small mechanisms in it (union constituent order, a missing `| undefined`,
   missing type parameters on the printed signature, `Promise<never>` vs
   `Promise<void>`). Those are near-misses, not architecture.

### 10.3 What this means for the 70% target

`checker_types` is at 5,869/9,538. Reaching 6,677 needs +808. The
single-transition population is 1,259 cases across ~20 mechanisms, and wall 2
owns 79 of them. **No single build gets there; roughly eight to twelve of
these mechanisms would.** That is a real, countable plan for the first time —
and it says the campaign is a function-type-and-inference campaign, not a
printing one.

### 10.4 How you would know this is wrong

The transitions are computed from PRINTED text, so a case whose lines all read
`any → primitive` may be blocked by one cause or by five unrelated ones; the
"single transition" test bounds the mechanism count from BELOW, not above.
And a case with two transitions may still have one cause. Treat 1,259 as the
count of cases with a *coherent* shape, and re-derive per family before
building — exactly as §9's first loose pass had to be tightened.

---

## 11. The `function -> function` near-miss premise, OPENED and CORRECTED — there is no cheap large vein left

§10.2 read `function -> function`'s **2.3 lines per case** as the signature
being computed with one slot wrong, and called it "near-misses, not
architecture". On that reading it was ranked the next build: 93 cases,
unblocked, several independent causes.

**Opened, and the ratio does not mean what it looked like.** Bucketing all
218 lines by how the wanted and printed texts differ:

| lines | cases | difference |
|---:|---:|---|
| **141** | — | **token COUNT differs — the two signatures are different SHAPES** |
| 19 | 3 | `T_1` ← `T` (shadowed type-parameter renaming) |
| 12 | 1 | `number` ← `T` (an uninstantiated parameter) |
| 12 | 1 | `fn_1` ← `fn` (the same rename, on a value name) |
| 8 | 2 | `C` ← `this` (§164's this-type substitution) |
| 8 | 1 | `Date` ← `T` |
| 7 | 2 | `any` ← `undefined` |
| everything else | 1–2 each | a long tail of one-case substitutions |

**65% of the bucket is a shape difference, not a slot difference.** The
2.3-lines-per-case ratio measured how many ASSERTION LINES a case carries,
not how close the answer was — a case with two `>f : …` lines scores 2.3
whether the printed signature is one token wrong or unrecognisable. The
metric was structurally incapable of distinguishing them, and nothing in §10
noticed.

The same check on `any -> primitive` (93 cases, the other candidate) shows
the same diffuseness from the other side: 31 cases want `string`, 40 want
`number`, spread across unrelated fixtures with no shared shape.

### 11.1 What this actually establishes

**There is no cheap large vein left in the corpus.** Three independent
sizings this session — naming (§9), transitions (§10), and now sub-causes
(§11) — all terminate in the same place: the remaining damage is many small
mechanisms, and the largest coherent ones are the expensive subsystems the
project has already priced and refused (contextual typing, the inference
pipeline's quality residue, cross-file binding).

That is a **planning** result, not a defeat, and it is the honest answer to
"what is next": the next window's work is a *sequence of small transcriptions
against a stale-resistant instrument*, not a lever. The per-window rate of
+20-45 cases observed across the last several sessions is the rate, and
6,677 is 18-40 windows away at it.

### 11.2 The one item that survives the sizing with a real population

`T_1` ← `T` / `fn_1` ← `fn` — **shadowed name renaming**,
`GenerateNamesForShadowedTypeParams` — is the only mechanism that appears in
BOTH the naming sizing (§9.2, ~325 lines corpus-wide) and this one (31 of
these 218 lines). The machinery exists
(`rename_type_parameters_for_site`, §102) and is applied at exactly two
sites: `checker.rs`'s multi-signature composite arm and
`signature_to_string_at` (`signatures.rs:3523`). Everything printed through
the BAKED text never reaches it — which is §1.1's `split_around_name` choke
point again, wearing a third hat.

So it is not independent of wall 2 after all; it is wall 2's reach problem
applied to a different substitution. Recorded here so the next reader does
not re-rank it as cheap.

### 11.3 The methodological finding, which is the durable one

Three ratios were quoted as evidence this session and **two were wrong in the
same way**: the loose rename bucket (§9, said ~400 cases, actually 79) and
this one (§10, said near-miss, actually 65% shape differences). Both were
aggregate string statistics standing in for a mechanism, and both survived
until someone printed the underlying rows.

**A ratio computed over rendered text is a hypothesis, not a measurement.**
The instruments in `scripts/` are for *finding candidates*; the row dump is
what decides. This belongs in `docs/conventions.md` and is recorded here
pending that edit.

---

## 12. §529 — the predicate is SPLIT, additively, and the split measures zero

§8.4's reopening condition, built. `Checker::is_shadowed_at`
(`crates/tsr-checker/src/checker.rs`) is upstream's `needsQualification`
(`symbolaccessibility.go:688-726`) transcribed honestly — the
`someSymbolTableInScope` walk (`:746-803`), not `resolve_name` — and it sits
**beside** `needs_qualification` rather than replacing it. No call site moved.

### 12.1 The measurement, which is the point of the step

```
TOTAL 474196  right 432821  gap 8701  wrong 32674
no transitions vs baseline
```

Byte-identical, as §8.4 requires. A step-1 pair that moved a line would have
changed the disjunction and been wrong by construction; this is the one build
in the sequence whose *success criterion is zero*, and it is worth naming why
that is not a wasted window: the −6,850 of §8.2 was the cost of finding out
what the conflated predicate was for **after** changing it. This finds out
first, and leaves the finding executable.

### 12.2 What the walk ports, and the three things it does not

Per table, upstream's callback exactly: name absent → continue; the entry IS
this symbol → stop, **no** qualification (`:702`); otherwise resolve the alias
unless it is an export specifier, take `getSymbolFlags`, and
`flags & meaning != 0` → **qualify** (`:713-723`). A name in no table leaves
`qualify` false — the arm the port never had.

Three omissions, each a *miss* rather than a wrong answer, meaning each can
only make this answer `false` where upstream answers `true`:

1. **The script-source-file `locals` skip.** Upstream skips a global source
   file's locals because those names are merged into `c.globals`. This port's
   `globals` is populated only by `bind_into`, so a single-file program keeps
   its top level in `locals` and skipping it would consult no table at all.
   Both are visited; every hit goes through `merged_symbol`, so the two tables
   cannot disagree about which symbol a name denotes.
2. **`getClassExpressionNameTable`** (`:809`) — a class expression's own name
   is in no table in this port either.
3. **The external-module gate on the `exports` arm** — nothing is ever routed
   to a *script* file's exports in this binder, so the table is empty and the
   arm cannot fire. Same reasoning `BindResult::resolve_name` records for its
   own unconditional exports arm.

### 12.3 The divergence is now pinned by tests, not by a paragraph

Four tests in `crates/tsr-checker/tests/symbol_chain.rs`. The load-bearing one
is `an_unreachable_name_is_not_shadowed_though_it_still_needs_a_qualifier`:

```ts
namespace M { export class C { p: number; } }
var x = 1;
```

At `x`, `M.C` is in no table in scope. `is_shadowed_at` answers **false**;
`needs_qualification` answers **true**; and the site really does print `M.C`
(pinned two tests up by `a_class_in_a_namespace_prints_qualified_from_outside`).
The qualifier is therefore owed to the `chain == nil` disjunct, **not** to
shadowing — which is §8.2's 6,850 R→W compressed into one fixture that runs in
a millisecond.

The other three fix the boundaries: the identity stop (`:702`), real shadowing
by a different symbol of the same name, and the meaning filter (`:720`) — a
`var C = 1` does not shadow a **type** question about `M.C`.

### 12.4 What this unblocks, and what it does not

Slice 1's globals arm (§7's refusal) may now be rebuilt against
`is_shadowed_at`, which is the predicate `symbolaccessibility.go:588-591`
actually sits behind — §8.3's `enum Color { Color }` misfire is exactly a
consultation of the conflated predicate, and that consultation is now
avoidable.

It does **not** unblock re-pointing `symbol_chain` itself. That needs
`getAccessibleSymbolChain` (`:373`) ported to answer the *first* disjunct
honestly, and until it is, `needs_qualification`'s `None => true` is the only
thing supplying 6,850 lines' worth of qualifiers. **The conflated predicate is
not deprecated and must not be marked as such.** Its `None` arm is a correct
proxy for a function this port does not have.

---

## 13. §531 — slice 1 REBUILT on the shadowing half: **+16 cases, 35 W→R, ZERO adverse**

§7.1 refused this arm at **38 W→R against 25 R→W with two PASSING cases
damaged**. Rebuilt against `is_shadowed_at` rather than the conflated
predicate, the same arm measures:

```
TOTAL 474196  right 432856  gap 8701  wrong 32639
WRONG->RIGHT: 35   (no adverse transition of any kind)
checker_types 5,869 -> 5,885  (+16 cases), gradient 90.39%
```

**The wins are the same wins; the losses are gone.** `nameCollision` 5,
`collisionCodeGenModuleWithModuleReopening` 4,
`declarationEmitTypeParameterNameInOuterScope` 4 — §7.1 recorded exactly those
three and said they "stay on the board". They did.

### 13.1 `Checker::global_this_chain`, and its three gates

At `symbol_chain`'s no-container exit, which is not the end of the road
upstream but `trySymbolTable`'s **last** arm (`symbolaccessibility.go:588-591`):
the two-element chain `[globalThis, target]`.

1. `globals()[name]` merges to this symbol — reachable through the globals
   table, as this symbol.
2. `is_shadowed_at` at the site — §12's honest predicate. **This is the entire
   difference from §7.1.**
3. No in-scope alias names the symbol itself — upstream's alias loop would have
   yielded that chain first and printed the alias; this port cannot spell it
   here, so it declines rather than print a form upstream did not.

### 13.2 The falsifier, checked against what §6 registered

§6: *"if the arm fires on more than a handful of sites, the globals-reachability
gate is not doing its job and it is SS197 (−66 cases) again"*. The arm moves
**35 lines**, and §5 named the population in advance —
`importAndVariableDeclarationConflict1-4` and `collisionCodeGenModuleWith*`.
The filtered probe on the head family reads 4 W→R with **35/35 lines right**,
i.e. all four cases now PASS, which is the `+4` §6 predicted to the case.
Falsifier not triggered.

### 13.3 §7.3's bug had a SECOND site, and this arm is what found it

The first full pair read 35 W→R against **2 R→W in `conformance/noInfer`** —
an already-failing case (25 wrong lines in the baseline), so recordable rather
than a full stop. The rows, printed before being reasoned about (§11.3):

```
noInfer:0:143  want <Props>(Component: Component<Props>, …) => void
               got  <Props>(Component: globalThis.Component<Props>, …) => void
noInfer:0:144  want Component<Props>
               got  globalThis.Component<Props>
```

The fixture writes `type Component<Props> = { props: Props; }` and then
`declare function doWork<Props>(Component: Component<Props>, …)`. The
**parameter** named `Component` shadows the type alias in `Value` only; the
reference is a `Type`; upstream does not qualify.

A trace in one filtered run named the cause in a single line: the meaning
arriving at `symbol_chain` was the full `TYPE | VALUE` union. §7.3 fixed that
at `qualified_name_at` and **`reference_text_at` had the identical bug** —
`self.symbol_chain(target, reference, SymbolFlags::TYPE | SymbolFlags::VALUE, 0)`
on a road that renders a generic TYPE reference, where `symbolToTypeNode`
(`nodebuilderimpl.go:649`) passes one meaning. Corrected to `SymbolFlags::TYPE`,
and the adverse pair went to **zero** with no other line moving — the +35 is
identical before and after, so the correction's whole measured effect is the
two `noInfer` lines it stopped damaging.

> **The arm keeps earning its bar in the same way twice.** §7.1's refusal
> caught the conflated predicate; §531's first pair caught the surviving union
> meaning. Both were nine-month-old defects on the hottest naming road in the
> checker, and neither was visible until something reached them.

### 13.4 What this does not do

`globalThis.` is now spelled where the globals arm reaches. The *shortest-chain*
selection (slice 2) and the name render (slice 3) are untouched, and
`symbol_chain`'s entry gate is still the conflated predicate — see §12.4 for
why that must stay until `getAccessibleSymbolChain` is ported.

---

## 14. §533 — slice 2: the ambiguity DECLINE becomes `compareSymbolChains`' order (+120 W→R, +3 cases)

§5's slice 2, and §14 of `checker-notes-nameres.md` retired.

`module_alias_at` answered `Err(true)` — decline — whenever two distinct names
in scope reached the same module, on the reasoning that upstream picked *some*
alias and guessing wrong prints a name upstream did not. Upstream does not
guess and does not decline (`symbolaccessibility.go:582-586`):

```go
// pick first, shortest
slices.SortStableFunc(candidateChains, c.compareSymbolChains)
return candidateChains[0]
```

`compareSymbolChainsWorker` (`:595-610`) is shorter-chain-first then
`compareSymbols` elementwise; `compareSymbolsWorker` (`utilities.go:366-391`)
is **first declaration's position** — file index, then offset (`compareNodes`,
`:393-412`) — then name, then symbol id. A total order that always answers.

### 14.1 The sizing, taken before the code, and the row that decided it

A trace on the decline: **238 firings across the corpus, 16 distinct
module/alias-pair shapes.** The largest is `compiler/importDecl` at 68 —
`multiImport_m4` versus `m4` for `./importDecl_require`.

The row that settles which one upstream picks, printed rather than reasoned
about (§11.3):

| | |
|---|---|
| `importDecl.ts:33` | `import m4 = require("./importDecl_require")` |
| `importDecl.ts:79` | `import multiImport_m4 = require("./importDecl_require")` |
| baseline | `>d : m4.d` |

The **earlier declaration**, which is `compareNodes`' answer exactly. §14's
"~96% coincidence" between candidate tie-breaks was measured when the tie-break
was a guess; it is not a guess, it is a transcription.

### 14.2 What was ported, and the one key deliberately not ported

`Checker::compare_symbols` takes `compareSymbolsWorker`'s keys in order:
first-declaration position, then name, then symbol id. `compareNodes`' file
index is the enclosing `SourceFile`'s `NodeId`, which the parser allocates in
program order — the two agree on ordering without agreeing on the numbers.

**`compareSymbolChains`' length key is not ported, because every candidate here
is a one-element chain**: an alias in scope naming the module directly.
`len(a) - len(b)` is always zero and the order reduces to `compareSymbols`.
Porting the length key would be porting a comparison over chains this function
does not build.

**The scope walk still dominates the sort.** Upstream sorts *within one table*
— `trySymbolTable` runs per table, innermost first, and returns as soon as one
yields candidates — so the tie-break only separates aliases in the SAME scope.
This collects per table and stops at the first that yields anything. Flattening
and sorting globally would let a file-scope alias at offset 10 beat a
block-scope alias at offset 500 that upstream returns first; the previous code
flattened, which was invisible while any second name meant decline.

### 14.3 The measurement, and the two adverse rows

```
TOTAL 474196  right 432983  gap 8692  wrong 32521
WRONG->RIGHT: 120   compiler/importDecl 54, umd-augmentation-1 14, importsImplicitlyReadonly 10
GAP->RIGHT:     7
GAP->WRONG:     2   conformance/exportsAndImports4-es6
RIGHT->WRONG:   0
checker_types 5,885 -> 5,888 (+3 cases), gradient 90.39% -> 90.42%
```

**127 favorable against 2 adverse, and zero R→W.** The two land in a case that
was already failing (7 wrong, 2 gap in the baseline), so they are recordable at
a favorable multiple rather than a full stop.

### 14.4 The two adverse rows are SLICE 3's, and they are worth more than they cost

```
exportsAndImports4-es6:0:6   want typeof c    got typeof a
exportsAndImports4-es6:0:14  want typeof e2   got typeof a
```

The fixture imports the same module six ways in one file:

```ts
import a = require("./t1");
import * as c from "./t1";
import e1, * as e2 from "./t1";
```

`a`, `c` and `e2` all reach `./t1`, so the tie-break picks `a` — the earliest —
for all of them. **But this is not a container-qualifier question at all.** The
type being printed at `:6` IS the alias binding `c`, and at `:14` it IS `e2`;
each is its own symbol carrying its own name, and the right answer is available
without any tie-break. That is precisely §1.2 / slice 3's rule — *a
symbol-carrying type renders its NAME from the symbol at the site*.

So the two rows are not a defect in this transcription; they are **slice 3's
population, newly made visible**. They were `GAP` before (the decline rendered
nothing) and are `WRONG` now, which is the honest direction: a wrong name is a
thing slice 3 can find and fix, and a gap is not.

**Falsifier for that claim**: if slice 3 lands and these two rows do not
convert, the diagnosis here was wrong and the tie-break is reaching a road it
should not.

---

## 15. §535 — slice 3 is REFUSED, and §1.2's premise is REFUTED by its own dump

Slice 3 is ADR-0044's decision and §5's "big one": *a symbol-carrying
`Named`/`Anonymous` renders its NAME from the symbol at the site, baked text as
fallback for symbol-less mints*. §1.2 argued it was cheap — the render already
exists in `reference_text_at` and only needs generalising ahead of
`qualified_name_at`.

**Opened, sized, and refused. Two measurements killed it, in this order.**

### 15.1 The cheap half measures ZERO

`reference_text_at` is gated to generic references (`!arguments.is_empty()`).
Dropping that gate so every recorded reference takes the name road:

```
no transitions vs baseline
```

A zero-argument reference already reaches the identical text through
`qualified_name_at`, because `split_around_name` fires exactly where the printed
form IS the symbol's own name — which is every zero-argument reference.
Reverted per §515, and the gate now carries a comment saying so, because the
next reader's first instinct will be to re-run it.

### 15.2 §1.2's premise, and what the dump says instead

> *"Of 38 `new_named` call sites, the ones that mint a **nameable** type pass a
> symbol; the ones passing `None` mint types that have no name to render."*

That is the load-bearing claim under the whole slice, and it is **false**.
Instrumenting the leaf road to print every `Named { members: Some(symbol) }`
whose site-computed name differs from its baked text — **43,423 firings**, led
by:

| firings | baked | site name |
|---:|---|---|
| 1,544 | `{}` | `__object` |
| 971 | `{}` | `__type` |
| 372 | `{ x: number; }` | `__type` |
| 357 | `{ p2: number; }` | `__object` |

**Object and type literals carry a symbol** — the binder's synthetic `__object`
/ `__type` — and they have no name. Rendering "the name from the symbol" there
prints `__object` where the corpus wants `{ x: number; }`. Carrying a symbol is
not the same property as being nameable, and §1.2 read one for the other.

### 15.3 The gated population is worse, not better — the arm would DELETE correct qualifiers

Excluding the synthetic `__`-prefixed symbols leaves a population that argues
against the slice more strongly than the synthetic one did:

| firings | baked | site name |
|---:|---|---|
| 517 | `this` | `C` |
| 227 | `JSX.Element` | `Element` |
| 209 | `C<T>` | `C` |
| 168 | `quasiater.carolinensis` | `carolinensis` |
| 136 | `Intl.NumberFormatOptions` | `NumberFormatOptions` |
| 130 | `Underscore.Static` | `Static` |
| 120 | `privateModule.publicClass` | `publicClass` |

**In the dominant shape the baked text is already QUALIFIED and correct, and
the site name is the BARE name.** `best_name` answers a symbol's own name or an
alias's; it does not answer a qualified one — `qualified_name_at` adds the
qualifier, and it already does, which is exactly why §15.1 measured zero. Making
the name render authoritative would replace `Intl.NumberFormatOptions` with
`NumberFormatOptions` several hundred times over.

The rest of the population differs for reasons a name render cannot fix:
`C<T>` → `C` drops the type arguments, and `this` → `C` is §164's this-type
substitution, a separate mechanism with its own decision.

### 15.4 The refusal, and its reopening condition

> **Slice 3 as specified is refused.** The baked text is not an "inside view"
> that a site-aware name render supersedes; for the symbol-carrying `Named`
> population it is either already correct and *richer* than the symbol's name,
> or it differs for a reason naming does not own. ADR-0044's decision rests on
> §1.2's premise and that premise is refuted — **the ADR needs a superseding
> record, not an edit.**
>
> **Reopen when** a render exists that produces a name at least as complete as
> the baked text — i.e. `getAccessibleSymbolChain` ported so the site name
> arrives already qualified, rather than bare with the qualifier bolted on
> afterwards. That is the same missing function §12.4 names as the blocker for
> re-pointing `symbol_chain`, which makes it the single highest-value unbuilt
> item on this page. Until it exists, "render the name from the symbol" and
> "render the qualified name from the symbol" are different functions, and only
> the second one is the one ADR-0044 wanted.

### 15.5 §11.3, a third time — and this one cost an ADR

The loose rename bucket said ~400 cases and was 79 (§9). The `function ->
function` near-miss ratio said one-slot and was 65% shape differences (§11).
§1.2 said a symbol means a name, and 1,544 object literals say otherwise.

All three were aggregate claims about a population that nobody had printed.
**§11.3's rule is now carrying its third confirmed instance and should be
promoted out of this page into `docs/conventions.md`** — it has caught more
wrong work this campaign than any other single check.

What it did NOT catch is worth stating too: §529, §531 and §533 were all sized
by dumping rows first, and all three landed at or above their registered bars.
The rule is not "distrust measurement", it is "distrust a ratio whose rows you
have not seen".

---

## 16. §11.2's reach, resolved: blocked by §535, and not independently

§5's build order ends with *the shadowed-name rename's reach* (§11.2) —
`GenerateNamesForShadowedTypeParams`, `T_1` ← `T` and `fn_1` ← `fn`, ~325 lines
corpus-wide. §11.2 recorded that `rename_type_parameters_for_site` fires at
exactly two sites and that slice 3 is what would make it reachable.

Both sites confirmed, unchanged: `checker.rs`'s multi-signature composite arm
and `signature_to_string_at` (`signatures.rs:3523`). Both are **signature**
roads. The single-signature composite arm reaches the rename too, because it
renders through `signature_to_string_at` — so the whole signature surface is
already covered, and the unreached population is exactly *what is printed
through baked text*.

That is slice 3's population, and slice 3 is refused (§15). **§11.2 therefore
carries §15.4's reopening condition verbatim and has no independent path**: it
is not a separate item on the board, and a future session should not rank it as
one. It converts when a site-aware render can produce a name at least as
complete as the baked text — which is `getAccessibleSymbolChain`, again.

---

## 17. Wall 2, closed out

| step | §5 name | result |
|---|---|---|
| 1 | split the conflated predicate | **§529** — landed, `no transitions vs baseline`, which was its success criterion |
| 2 | slice 1, the globals arm | **§531** — landed, +16 cases, 35 W→R, **zero adverse** |
| 3 | slice 2, shortest-chain selection | **§533** — landed, +3 cases, 120 W→R + 7 G→R against 2 G→W, zero R→W |
| 4 | slice 3, the name render becomes authoritative | **§535 — REFUSED**, §1.2's premise refuted at 43,423 rows, reopening condition recorded |
| — | §11.2, the rename's reach | **blocked by §535**, not independent (§16) |

`checker_types` 5,869 → **5,888** (61.53% → 61.73%), gradient 90.39% → 90.42%.

**The wall's own sizing said 79 cases and the built half delivered 19.** The
missing 60 are slice 3's, and they are not missing because the work was hard —
they are missing because the mechanism §5 specified would have made those lines
*worse*. That is the wall's real result: three of its four steps were
transcriptions that landed at or above their bars, and the fourth was a design
whose premise had never been checked against a row dump.

**The single highest-value unbuilt item on this page is now
`getAccessibleSymbolChain` (`symbolaccessibility.go:373`).** It is named as the
blocker in three separate places — §12.4 (re-pointing `symbol_chain`), §15.4
(slice 3), §16 (§11.2's reach) — which is the strongest signal on the page that
it is one piece of work paying three debts.

---

## 18. §537 — the fallback's first pick, and the instrument finding that reprices the rest of it

Wall 2's slice 3 measured negative and is recorded (§15), so the session moved
to the registered fallback: the single-transition population, rows printed
first, a shared **fixture** shape preferred over a shared type shape.

### 18.1 The pick

Re-dumping `target/verdict_baseline.tsv` after §533 (the old dumps were three
landings stale): **563 failing cases carry exactly one non-right line.**
Clustering those by their exact (want, got) pair surfaced one shape that is not
a type-computation question at all:

```
4 cases | want: {}  | got: { : any; }
   conformance/templateStringInPropertyName1, 2, ES6_1, ES6_2
```

`{ : any; }` is a shape no compiler emits — a colon with no name in front of it.
`var x = { `a`: 321 }` is a syntax error; the parser reports it and hands the
property assignment a **missing identifier**, an `Identifier` whose `text` is
empty, and `check_object_literal` put that in the members list.
`templateStringInPropertyName1.types` records `>{ : {}`.

One arm, ahead of the general `Identifier` arm and gated on the empty text
rather than on a node kind — the recovery placeholder is the only way an
identifier reaches there with no text, and gating on the kind would need one arm
per token the parser might have swallowed. It is the same treatment the
private-name arm beside it already had, for the same stated reason: **a member
with no spellable name is not a member.**

```
WRONG->RIGHT: 9   (bigintPropertyName 3, templateStringInPropertyName* 4,
                   templateStringInObjectLiteral{,ES6} 2)
no adverse transition of any kind
```

The three `bigintPropertyName` lines were not predicted — a bigint property name
reaches the same recovery placeholder — which is the good kind of surprise: the
arm is keyed on the defect, not on the fixture.

### 18.2 The instrument finding, which matters more than the nine lines

**`checker_types` cases: 5,888 → 5,888. The four cases did not convert.** The
filtered pair reads `TOTAL 12 right 12 gap 0 wrong 0` for them — every line
scorepair scores is right — and coverage still fails them.

The reason is in the two totals:

| instrument | assertion lines |
|---|---|
| `scorepair` / `verdict_baseline.tsv` | **474,196** |
| `coverage` | **478,855** |

**Coverage judges 4,659 lines that the verdict baseline does not carry**, and a
case passes only if *every* line of it matches. So a case that is "one line from
passing" in the baseline may be several lines from passing in coverage.

> **The 563 deficit-1 cases are 563 cases one SCOREPAIR line from passing, and
> that is not the same claim as one case from the gate.** The dump ranks
> candidates by line-conversion, not by case-conversion, and the first family
> drawn from it converted 9 lines and 0 cases.

This is §11.3's **fourth** instance and the first to catch an instrument rather
than a ratio: ~400-cases-actually-79 (§9), near-miss-actually-shapes (§11),
a-symbol-means-a-name-actually-object-literals (§15), and now
deficit-1-actually-line-deficit-1. The rule generalises past ratios:
**a count computed over one instrument's population is a hypothesis about
another's.**

### 18.3 What the next session should do with this

Do not plan case-count work off the deficit-1 dump without first checking a
candidate family against `coverage`. The cheap check is the one used here: run
the filtered pair, see 12/12 right, then run `coverage` and see whether the case
count moved. It costs one coverage run per family and it is the difference
between a plan and a hypothesis.

The two other families this dump surfaced, both with a shared fixture shape and
both **unverified against coverage**, recorded so the rows do not have to be
re-dumped:

- **generator return types — 16 deficit-1 cases.** `function*` wants
  `() => Generator<Y, R, N>` and the port answers `any` or `error`
  (`castOfYield`, `generatorTypeCheck37/57/58/61`, `templateStringInYieldKeyword`,
  `YieldExpression5_es6`, …). The largest coherent family on the board.
- **tuple wanted, array got — 12 deficit-1 cases.** `[T, U]` against
  `(T | U)[]`, `[number, number]` against `number[]`
  (`typeParameterFixingWithContextSensitiveArguments2/3/4`, `callWithSpread`,
  `functionParameterArityMismatch`). Several of these are contextual-typing
  residue, which is priced and refused; the family needs splitting before it is
  ranked.

### 18.4 The generator family, PROBED AND REFUTED before it was built

§18.3 ranked *generator return types* as the largest coherent family (16
deficit-1 cases). One `probefile` run retires it:

```ts
function* g() { yield 1; }
var h = g;
```
```
>g : () => Generator<number, void, unknown>
>h : () => Generator<number, void, unknown>
```

**The machinery works.** `signatures.rs` computes generator return types
(§§ around `:1072-1219` carry the aggregate model), so the 16 cases do not share
a missing mechanism — they share a printed WANT TEXT. Reading their names says
what they actually share: `castOfYield`, `FunctionDeclaration10_es6`,
`templateStringWithEmbeddedYieldKeyword`, `YieldExpression5_es6` — **generators
inside syntactically erroneous code**, where each case's cause is its own
recovery path.

That is §11.3 one more time, and the cheapest catch of the session: **a cluster
keyed on the printed want-text is not a mechanism.** One probe, thirty seconds,
against a transcription that would have been days.

The rule for the next session's first move, stated so it does not have to be
rediscovered: *before building for a family from any dump, write the two-line
fixture the family claims is broken and run `probefile` on it.* If it already
prints correctly, the family is a text cluster and needs re-deriving by cause.

---

## 19. §18.2 CORRECTED, and §539 — an object literal's index signature was printed but not consultable

### 19.1 The correction first

§18.2 read §537's `+9 lines / +0 cases` as *"the 563 deficit-1 cases are a
hypothesis about cases"* and left the impression that the dump does not predict
case conversion. **That was too strong, and the arithmetic that settles it is
one line:**

```
cases in the baseline           9,518
all-right in scorepair          6,015
coverage passes                 5,888
                                -----
all-right but coverage fails      127   (2.1%)
```

So a case that becomes all-right in scorepair converts to a coverage pass
**about 98% of the time**. The instruments *do* score different populations —
474,196 lines against 478,855, and that part of §18.2 stands — but the practical
consequence is a 2% miss rate, not an unusable dump.

§537 drew a family that was **entirely inside that 2.1%**: all four
`templateStringInPropertyName*` fixtures are syntax-error fixtures, which is
exactly where coverage scores lines the verdict baseline does not carry. An
unlucky first draw, read as a property of the instrument. Corrected here rather
than silently edited, per the project's own rule.

**The usable form of the rule**: deficit-1 is a good ranking, and the 2% it
misses is concentrated in error-recovery fixtures. Prefer families whose
fixtures are ordinary code, and keep §18.4's probe-first step, which is what
actually catches a bad family.

### 19.2 §539, drawn on the corrected reading, and it converts exactly as predicted

The next pair off the dump — `conformance/computedPropertyNames23_ES6` and
`26_ES6`, both wanting `number` and getting `any`, both ordinary fixtures.

`§18.4`'s probe first:

```ts
var y = { [this.bar()]: 1 };      // >y : { [x: number]: number; }   correct
var z = { [this.bar()]: 1 }[0];   // >z : error                      wrong
```

**The index signature is computed correctly and the lookup cannot see it.**
`check_object_literal` builds it (§206, `getObjectLiteralIndexInfo`,
`checker.go:19721`) and pushes it into the members list it *prints* from; the
minted type is a `TypeData::Named` over the binder's `__object` symbol; and
`get_index_infos_of_type` recovers index infos from a symbol's **declarations**,
which an object literal has none of. So the type printed a signature it could
not consult, and every `{ [computed]: v }[k]` answered `errorType`.

The fix is a side table keyed by the minted type id —
`Checker::object_literal_index_infos`, per ADR-0003 and matching the two tables
already declared beside it (`js_literal_types`, `fresh_object_literal_types`),
because the information exists only where the literal was checked. Consulted
ahead of the symbol road and never after it: a literal that minted a signature
has no declared ones to merge with.

`symbol` keys are deliberately not recorded. `is_applicable_index_type` decides
applicability for the `string`/`number` intrinsics and their literal types only,
and a key it cannot judge must stay a gap rather than become a confident wrong
value.

```
WRONG->RIGHT: 15   (includes §537's 9, whose baseline had not been re-accepted)
GAP->RIGHT:    4    compiler/checkJsObjectLiteralIndexSignatures
no adverse transition of any kind
checker_types 5,888 -> 5,890 (+2 cases)
```

**+2 cases, which is exactly what the deficit-1 dump predicted for that pair** —
the corrected §19.1 reading, confirmed on its first use. The reach beyond the
prediction is `checkJsObjectLiteralIndexSignatures`, 8 lines the arm picked up
without being aimed at them, which is the same good sign §537's
`bigintPropertyName` was: the fix is keyed on the defect, not on the fixture.

---

## 20. §541 — an empty array binding pattern is `Iterable<any, void, undefined>`, not the empty tuple (+4 cases, 0 adverse)

Third family off the corrected deficit-1 reading (§19.1), picked because all
three fixtures are ordinary code: `emptyArrayBindingPatternParameter01/02/03`,
one shape, `([]: [])` where the baselines record
`([]: Iterable<any, void, undefined>)`.

§18.4's probe first, before any code:

```
function f([]) { }     ours  >f : ([]: []) => void
                   upstream  >f : ([]: Iterable<any, void, undefined>) => void
```

`getTypeFromArrayBindingPattern` (`checker.go:17964-17969`) short-circuits
**before** it builds any tuple:

```go
if len(elements) == 0 || len(elements) == 1 && restElement != nil {
    if c.languageVersion >= core.ScriptTargetES2015 {
        return c.createIterableType(c.anyType)
    }
    return c.anyArrayType
}
```

§455 added the empty pattern to this arm and read **both** halves off the
OBJECT rule — `{}` really is the empty object literal — but the array half has
its own upstream answer and never was the empty tuple.

### 20.1 The arity, which is the whole reason the first attempt measured zero

The arm's first version used `global_type_symbol("Iterable")` and measured
`no transitions vs baseline`. That is §515's revert condition, and reverting
would have been wrong: `global_type_symbol` is
`global_type_symbol_with_arity(name, 1)`, and the modern lib declares

```ts
interface Iterable<T, TReturn = void, TNext = undefined>
```

— **arity 3**. `getGlobalType`'s arity check is what selects the right
declaration, so the lookup answered `None` on every case and the arm never
fired. At arity 3 the three predicted cases convert and the print matches the
baseline's three arguments exactly.

> **A zero-measuring arm is a revert *or* a lookup that never fired, and those
> are not the same thing.** §515's rule is right for an arm that ran and paid
> nothing; before applying it, check that the arm's preconditions were met at
> all. One `grep` of `global_type_symbol`'s body was the difference here between
> +4 cases and a recorded refusal of a correct transcription.

### 20.2 The measurement

```
WRONG->RIGHT: 4   emptyArrayBindingPatternParameter01/02/03,
                  compiler/declarationEmitDestructuring4
no adverse transition of any kind
checker_types 5,890 -> 5,894 (+4 cases), gradient 90.43%
```

Three predicted, one not (`declarationEmitDestructuring4`) — the same
keyed-on-the-defect signal §537 and §539 each showed.

Two halves deliberately not ported, each stated rather than dropped:

- **The pre-ES2015 `anyArrayType` half.** This port does not track the language
  version at this site, and answering `any[]` for a target it cannot check would
  be a guess.
- **The rest-only shape** (`len == 1 && restElement`), which takes the same exit
  upstream. It cannot arrive here — the guard above requires every element to
  have no `...` token — and it is named so that relaxing that guard does not
  silently mint a one-element tuple for `function f([...r])`.

A missing global `Iterable` keeps §455's empty tuple rather than gapping: the
lib may simply not be mounted, upstream's own answer there is `anyArrayType`,
and a decline must not be worse than what the arm already produced.

---

## 21. §543's candidate, PROBED AND REPRODUCED — two spread arguments in one call gap the call

Handed off with a minimal repro rather than built, because the window ended
here. This is the next family off §19.1's corrected deficit-1 ranking:
`iteratorSpreadInCall7/8/9/10` — **4 deficit-1 cases**, ordinary fixtures.

§18.4's probe first, and it took two rounds to find the real shape:

```ts
function foo<T>(...s: T[]) { return s[0]; }
class A { next() { return { value: Symbol(), done: false }; }
          [Symbol.iterator]() { return this; } }

var r = foo(...new A);          // >r : symbol      CORRECT
var a = [...new A];             // >a : symbol[]    CORRECT
var r2 = foo(...new A, ...new B);  // >r2 : error   WRONG
```

**One spread works; two spreads in one call gap it.** The iterator seam
(§284/§285) and the single-spread call road are both fine — the defect is in
combining two spread arguments, and `iteratorSpreadInCall7`'s own source is
exactly `foo(...new SymbolIterator, ...new _StringIterator)`.

That is worth recording precisely because the first probe *passed*: the obvious
two-line fixture (`foo(...new SymbolIterator)`) prints `symbol` correctly, and a
session that stopped there would have written the family off as another
want-text cluster (§18.4) when it is a real, narrow, reproducible defect.

> **Probe the fixture's ACTUAL shape, not the shape its name suggests.** §18.4's
> rule catches false families; this is its complement — a probe that passes has
> only cleared the shape you wrote, so read the failing fixture's own line
> before concluding.

Expected population: the 4 deficit-1 cases plus whatever else combines spreads,
which the row dump does not bound from above. Unbuilt; no measurement claimed.

### 21.1 §543 BUILT — and the union was the wrong generalisation

§21's repro built. `getSpreadArgumentType` (`checker.go:31285`) builds one type
out of every spread argument, so §341's single-spread arm generalises to N
spreads against a bare `...s: T[]`. Two drafts:

**Draft 1 — `T := union of the element types`.** The repro reads
`string | symbol`, which is what the shape suggests. Corpus: **`no transitions
vs baseline`** — and that zero was *not* §515's revert case. Dumping the row
showed the arm firing and the line moving from `any` to `string | symbol`
against a baseline that wants **`symbol`**: a different wrong answer, which
scores no transition because WRONG→WRONG is not one.

**Draft 2 — `T := the FIRST spread's element.**` Upstream infers through the
ordinary candidate machinery (`getInferredType` → `getCommonSupertype`), and
with no common supertype among the candidates the first one wins. `symbol` and
`string` have none, so upstream records `symbol`.

```
WRONG->RIGHT: 3   iteratorSpreadInCall7, 8, 9
no adverse transition of any kind
checker_types 5,894 -> 5,897 (+3 cases), gradient 90.43%
```

**All-spread only.** A mix (`foo(1, ...new A)`) unions the fixed arguments in
too, and the fixed half carries its own literal-widening question
(`getSpreadArgumentType` widens through `checkExpressionWithContextualType`).
Separate measurement; it keeps the gap it always had.

> **Two zeros in one session, neither of them §515's.** §541's arm never fired
> because a global lookup asked the wrong arity; §543's fired and produced a
> different wrong answer. `no transitions vs baseline` means *nothing scored
> moved* — it does not tell you whether the code ran. **Dump a row from the
> target family before concluding that a zero is a refusal.**

---

## 22. §545 REFUSED — the destructuring-default re-widen is load-bearing (8 R→W against 1 W→R)

Next family off the ranking: `for-of36/43` and `for-of46`'s siblings, wanting
`number | true` where the port prints `number | boolean` — a destructuring
default's literal type surviving into the union.

Probed and reproduced outside `for-of` first, which is what made it worth
opening:

```ts
var q = { p: 0 };
var { p: c = true } = q;    // ours: number | boolean   upstream: number | true
```

`destructure.rs`'s annotation-less leg (§315, `checker.go:17789`) runs
`getUnionTypeEx([strip(t), checkDeclarationInitializer], UnionReductionSubtype)`
and then, when any constituent of the reduced union is FRESH, re-widens every
constituent and re-reduces. That re-widen is what turns `number | true` into
`number | boolean`.

**The hypothesis was that the re-widen is redundant** — subtype reduction
already absorbs the case it was added for (`var [x = 20] = [1, 2]` records
`number` because `20` is a subtype of `number`), while `true` is not a subtype
of `number` and should survive. Upstream has no such re-widen.

**Measured, and the hypothesis is wrong:**

```
RIGHT->WRONG: 8   destructuringWithLiteralInitializers2 3, declarationsAndAssignments 2,
                  for-of43 1, literalTypesAndDestructuring 1, literalTypesAndTypeAssertions 1
WRONG->RIGHT: 1   for-of43 1
```

**8 R→W against 1 W→R, and `for-of43` appears on BOTH sides** — removing the
re-widen fixes one of its lines and breaks another. Reverted; the tree
re-measures `no transitions vs baseline`.

### 22.1 The reopening condition

The re-widen is doing real work that upstream does somewhere else, and the
8 losses name where to look: `destructuringWithLiteralInitializers2` and
`declarationsAndAssignments` are the shapes it protects.

> **Reopen when the port has upstream's fresh/regular literal distinction at
> this boundary.** Upstream widens a fresh literal at the *declaration* through
> `getWidenedType` and keeps a regular one; this port has one literal type and
> approximates the difference with a whole-union re-widen, which is too coarse
> in one direction (`true` beside `number`) and exactly right in the other
> (`20` beside `number`, `"a"` beside `string`). The blanket re-widen cannot be
> narrowed by a predicate over the *reduced* union — that is what this
> measurement rules out — so the distinction has to arrive with the
> constituents.

Recorded per §445's treatment: the refusal, its number, and what would make it
revisitable. The `for-of` family is NOT available at this boundary and should
not be re-ranked from a row dump without reading this section.

---

## 23. §547 — the rest element of a NON-TUPLE array pattern (+3 cases, 26 favorable : 1 adverse)

The gap this arm left was named in its own comment and never closed:

> *"A non-tuple parent declines too (upstream builds `T[]` from the iterated
> type)."*

`getTypeForBindingElement` (`checker.go:17797`) reaches
`checkIteratedTypeOrElementType` when the parent is not a tuple and wraps the
element in an array. §321 built the tuple half (`sliceTupleType`) and declined
everything else.

Probed first, and the probe found more than the row dump promised:

```ts
var [a, ...b] = new SymbolIterator;   // b : error   -> symbol[]
var [d, ...e] = [1, 2, 3];            // still error, and so is `d`
```

The element comes from `for_of_element_type`, the same §284/§285 seam the
array-spread road uses, so a custom iterator and a plain array take one path. A
parent whose iterated type this port cannot decide keeps the gap — `None` here,
never a guess. The arm is reached only when the parent is **not a tuple at all**,
so it cannot silently answer the sliced-mask question §321 refused: a `readonly`
or optional-masked tuple still declines above.

```
WRONG->RIGHT: 20   noUncheckedIndexedAccessDestructuring 10, generatorAssignability 2,
                   restParameterWithBindingPattern3 1
GAP->RIGHT:    6   objectRestAssignment 3, iterableArrayPattern12, 14
GAP->WRONG:    1   compiler/declarationEmitDestructuring3 (already failing: 5 wrong, 1 gap)
RIGHT->WRONG:  0
checker_types 5,897 -> 5,900 (+3 cases), gradient 90.43%
```

**26 favorable against 1 adverse, zero R→W.** The reach is again far past the
family that surfaced it — `noUncheckedIndexedAccessDestructuring` alone is 10
lines and was not in the deficit-1 dump at all, which is the third time this
session a defect-keyed arm paid more than its aimed-at rows (§537's
`bigintPropertyName`, §539's `checkJsObjectLiteralIndexSignatures`, §541's
`declarationEmitDestructuring4`).

### 23.1 The residue, named

`var [d, ...e] = [1, 2, 3]` still gaps, and **`d` gaps with it** — a *non-rest*
element of the same pattern. That is a different blocker: the array-literal
contextual road (`tuple_from_array_literal`) declines the whole pattern when it
carries a rest, so the parent type never becomes a tuple and this arm's
`for_of_element_type` is asked about a parent that already failed. Separate
measurement, and it is the next thing to look at in this file.

---

## 24. §549 — §23.1's residue: a declined tuple context was CONTAGIOUS (+2 cases, 86 favorable : 12 adverse)

§547 left `var [d, ...e] = [1, 2, 3]` gapping, and **`d` gapped with it** — a
*non-rest* element whose positional read works fine on `number[]`. §23.1 named
the blocker and this closes it.

`tuple_from_array_literal` refuses several shapes: a rest in the pattern, a
spread in the literal, a pattern longer than the literal. **Every one of those
refusals is about the TUPLE CONTEXT, not about the initializer having no type.**
Upstream's `checkDeclarationInitializer` still answers `number[]` for
`[1, 2, 3]` when the contextual tuple does not apply. Returning `error` made the
refusal contagious — it poisoned the parent type, and every element of the
pattern gapped with it.

One change: a decline falls through to the plain
`check_expression` + `widenTypeInferredFromInitializer` road already sitting
below it, instead of returning.

```
WRONG->RIGHT: 51   intraBindingPatternReferences 14,
                   destructuringArrayBindingPatternAndAssignment3 13,
                   declarationEmitDestructuringArrayPattern2 3
GAP->RIGHT:    35  destructuringArrayBindingPatternAndAssignment5SiblingInitializer 13,
                   declarationEmitDestructuringArrayPattern4 8, parserForStatement9 8
GAP->WRONG:    12  declarationEmitDestructuringArrayPattern4 (already failing: 6 wrong, 20 gap)
RIGHT->WRONG:   0
checker_types 5,900 -> 5,902 (+2 cases), gradient 90.43% -> 90.45%
```

**86 favorable against 12 adverse, zero R→W.** All 12 land in one already-failing
case, which also gains 8 G→R in the same run — the arm moves that case's gaps
into answers, 8 of them right and 12 wrong, and the whole-corpus ratio is 7:1.

### 24.1 What this says about decline plumbing generally

The bug was not in what `tuple_from_array_literal` refuses — every one of its
refusals is correct and each carries its own recorded reason. It was in **how
the refusal was returned**: `error` is this port's gap sentinel *and* its "not
applicable" answer, and those are different claims. A function that declines an
OPTIONAL enrichment must not return the value that means *this construct has no
type*.

> **Worth a sweep**: any helper whose contract is "compute a better type if you
> can" and whose decline path returns `error` is a candidate for the same bug.
> This one cost 86 lines across 8 cases and was invisible because the decline
> was correct and the plumbing was not.

### 24.2 §551's candidate, opened and NOT reproduced — read before re-ranking it

`declarationEmitExpressionInExtends2/5/6`, 3 deficit-1 cases, all shaped as
*"want the instance type, got the constructor type"*:

```
2  want C<string, number>  got typeof C
5  want IFace              got new () => IFace
6  want A.Foo              got typeof A.Foo
```

The obvious reading is that a class extending an EXPRESSION takes the base's
construct-signature type instead of its return type. **The probe does not
reproduce it:**

```ts
interface IFace { }
declare function getClass<T>(): new() => T;
class Derived extends getClass<IFace>() { }   // >Derived : Derived   already correct
```

So the base-type computation is not the defect, and this family needs its
failing line read in place — the fixture wraps everything in a `namespace` and
enables `--declaration`, and neither is in the probe above. Per §21's rule, a
probe that PASSES has only cleared the shape you wrote.

Unbuilt, no measurement claimed, and recorded specifically so the next session
does not re-derive the same non-reproduction.

---

## 25. §551 — the NUMBER-key slice of `getObjectLiteralIndexInfo`'s filter (+3 lines, 0 adverse, +0 cases)

§24.1's sweep, run. Probing the decline paths of `check_object_literal` found
two poisoned shapes:

```ts
declare var k: number;
var a = { x: 1, [k]: 2 };          // error   -> { x: number; [x: number]: number; }
var b = { [k]: 1, ["s"+""]: 2 };   // error   (mixed key kinds, still declines)
```

**Upstream does not decline a mixed literal.**
`getObjectLiteralIndexInfo` (`checker.go:19721`) filters `propertiesArray` by
whether each property's name suits the key, and for a NUMBER key that is the
numerically-named members only. `x` is not numeric, so it stays a PROPERTY and
contributes nothing to the index value.

The §206 decline's stated reason was *"this port has no numeric-name predicate
for a written name"*. **It does** — a written numeric name is normalised through
`printing::normalise_number` at the mint a few lines above, so an all-digits
member name is exactly the numeric case. When NO named member is numeric the
filter removes nothing and the two computations agree by construction, which is
the slice taken.

Three shapes still decline, each for its own recorded reason: a numerically-named
member beside a number key (it joins the index value union, whose ORDER against
upstream's is unverified); a STRING key beside named members (upstream's filter
keeps everything but symbol-named, so every member contributes — a different
computation); and mixed key kinds (one index info per kind, in string/number/
symbol order).

```
WRONG->RIGHT: 3   modularizeLibrary_ErrorFromUsingES6FeaturesWithOnlyES5Lib
no adverse transition of any kind
checker_types 5,902 (+0 cases), gradient 90.45%
```

**+0 cases, and landed anyway** — it is not §515's zero: three lines moved right,
nothing moved wrong, and it removes a shape (`{ x: 1, [computed]: 2 }`) that
gapped whole where upstream answers. The case count does not move because the
lines are spread across cases with other blockers, which §19.1 already
quantified as the ordinary situation for a 3-line arm.

### 25.1 §553's candidate — three more poisoned literal shapes, with their upstream answers

§24.1's sweep continued past §551's slice. Every one of these gaps whole today:

```ts
declare var s: string;
var a = { x: 1, [s]: 2 };          // error  -> { x: number; [x: string]: number; }
var b = { m() {}, [1]: 2 };        // error  -> { m(): void; [x: number]: number; }
var c = { get g() { return 1; }, [1]: 2 };   // error
```

Each is `getObjectLiteralIndexInfo`'s filter (`checker.go:19721`) again, and
each is a *different* arm of it:

- **`a` — the STRING-key half.** Upstream's filter keeps every property but the
  symbol-named ones, so the named member's type joins the index VALUE union:
  value is `number | number` = `number`. §551 deliberately left this out because
  it is a different computation from the number-key slice, **and because the
  named members are held as printed TEXT at that point in
  `check_object_literal`, not as `TypeId`s** — the union cannot be formed
  without threading the member types alongside. That plumbing is the work, not
  the rule.
- **`b`, `c` — a non-`Property` member beside a NUMBER key.** §551's
  `numeric_named` predicate answers `true` for every member variant that is not
  `Member::Property`, which reads as conservative-rather-than-correct: a method
  or an accessor is not numerically named either, so upstream's filter should
  drop it from the value union exactly as it drops `x`.
  **TRIED, AND IT IS NOT THE BLOCKER.** The predicate was relaxed to read a
  `Member::Signature`'s name off its printed prefix (`m(): void` → `m`, numeric
  exactly when that prefix is all digits) and **both shapes still gap** — so the
  decline for a method or accessor beside a computed key happens BEFORE this
  point, on the SS307 method/accessor arm rather than in the index-info filter.
  The change was reverted unlanded: it measured nothing and its premise was
  wrong. Whoever takes this next should start at the method arm, not here.

**Sized honestly: not sized.** No row dump has been taken for these three, so no
case count is claimed. The shapes are recorded because the probe found them
cheaply and re-finding them costs another sweep.

> §24.1's sweep has now paid twice (§549's 86 lines, §551's 3) and is **still
> only run over `destructure.rs` and `objects.rs`.** The pattern — a decline
> that returns the gap sentinel, or a decline whose stated reason has gone
> stale — is generic, and no other module has been checked.

---

## 26. §553 — the LITERAL halves of `StringOrNumberLiteralOrUnique` (+4 cases, 126 favorable : 20 adverse)

§25.1's b/c lead, run down properly. The corrected diagnosis there said the
decline happens before the index-info filter; it does, and this is where.

`computed_member_index_key` implements upstream's guard
(`checker.go:13317`) faithfully — a name type carrying
`STRING_LITERAL | NUMBER_LITERAL | UNIQUE_ES_SYMBOL` is `LateBound`. But
`late_bound_symbol_member_name` only ever answered the **symbol** half, so the
two literal halves routed to a function that returned `None`, and the caller's
`return error` gapped the whole literal:

```ts
var z = { [1]: 2 };        // error  ->  { 1: number; }
var y = { ["a"]: 2 };      // error  ->  { a: number; }
var x = { [1]: 2, [2]: 3 };// error  ->  { 1: number; 2: number; }
```

A late-bound literal name **is** the member's name, and it takes the same
spelling rules a written property name takes a few lines below: an
identifier-valid string prints bare, anything else re-quotes through the shared
`printing::quote`, a number prints through `printing::normalise_number`.

**Reading the literal TYPE rather than the written expression is what keeps
`{ ["a" + ""]: 1 }` on the index route** — its name type is plain `string`, not
a literal, so the arm does not fire.

```
WRONG->RIGHT: 77   literalsInComputedProperties1 22, dynamicNames 17,
                   computedPropertiesNarrowed 12
GAP->RIGHT:    49  controlFlowAssignmentPatternOrder 12, controlFlowForInStatement2 8,
                   declarationEmitPropertyNumericStringKey 7
GAP->WRONG:    20  controlFlowInOperator 6, controlFlowForInStatement2 3, …
RIGHT->WRONG:   0
checker_types 5,902 -> 5,906 (+4 cases), gradient 90.45% -> 90.48%
```

**126 favorable against 20 adverse, zero R→W.** The adverse are all GAP→WRONG —
lines that rendered nothing now render something wrong — and they are spread
across nine cases rather than concentrated, which says they are downstream
consequences of newly-existing members rather than a second defect in this arm.
`controlFlowForInStatement2` appears on both sides (8 G→R, 3 G→W), the same
signature §549's `declarationEmitDestructuringArrayPattern4` showed.

### 26.1 Why this was invisible for so long

The guard and the speller were written in different sessions against the same
upstream line, and **each was correct about its own half**. SS206 ported the
guard including both literal flags — its comment even records catching a draft
that turned `{ [1]: 1 }` into `{ [x: number]: number; }`. SS323 then built the
speller for unique symbols and did not widen the guard, because the guard
already covered it.

Nothing was wrong with either change. The defect lived in the *seam*: a
classifier that promises three kinds and a speller that answers one, with the
caller turning the mismatch into a gap. That is the third seam-shaped defect this
session after §539 (printed but not consultable) and §549 (the contagious
decline), and all three were found by probing shapes rather than by reading code.

### 26.2 §553's own residue, named: a late-bound LITERAL method prints the arrow form

The arm makes these members exist; one of them prints the wrong shape:

```ts
var d = { m() { return 1; } };        // { m(): number; }        correct
var e = { ["m"]() { return 1; } };    // { m: () => number; }    upstream: { m(): number; }
```

§413 already settled the rule — *"numeric and string method names take the SAME
spelling rules the property path has"*, `{ 0() { } }` is `{ 0(): void; }` — so a
late-bound literal method should keep method syntax.

**§555 LANDS IT, and the first attempt's failure was MINE, not the code's.**

`late_bound_symbol_member_name`'s second return element is the method arm's
*"keep the method spelling"* switch — SS323 named it for `UNIQUE_ES_SYMBOL`
because that was the only thing that reached it. Flipping the literal arms to
`true` is the whole fix:

```
WRONG->RIGHT: 6   computedPropertyNames28_ES6, 30_ES6, SourceMap2_ES6, …
no adverse transition of any kind
checker_types 5,906 -> 5,909 (+3 cases)
```

**The first attempt reported `no transitions` and I read it as "the branch is
not the code path".** It was not. `cargo fmt` had collapsed the arm's `if/else`
onto one line between writing it and editing it, the multi-line search string
therefore matched nothing, and **the edit silently did nothing** — a scripted
replace with no assertion. The probe was unchanged because the binary was
unchanged.

> **A scripted edit without an assertion is not an edit, and a zero measured
> over one is not a measurement.** This is §20.1's rule (`check whether the arm
> FIRED`) meeting a new way to fail it: there, a lookup's precondition was
> unmet; here, the code was never written. Both present as `no transitions vs
> baseline`. Every `python - <<PY` edit in this file's history that lacks an
> `assert` should be treated as unverified.

§413's rule was right all along — numeric and string method names take the
property path's spelling, so `{ ["m"]() { } }` is `{ m(): number; }` exactly as
`{ "foo"() { } }` is `{ foo(): void; }`.

---

## 27. §557 — a NEGATIVE numeric computed name keeps the bracketed form (+1 case, 6 W→R, 0 adverse)

§553's own residue, found by probing its neighbourhood rather than by a row
dump. The arm spelled every number-literal name through
`printing::normalise_number` and printed it bare:

```ts
var e = { [-1]: 1 };   // ours: { -1: number; }   upstream: { [-1]: number; }
```

Two baselines settle it, and both are cases §553 itself moved:

```
computedPropertiesNarrowed.types              >t6 : { [-1]: number; }
duplicateObjectLiteralProperty_computedName1  >{ "-1": 1, [-1]: 0 } : { [-1]: number; }
```

**A negative number is not spellable as a property name** — it is a unary
expression, not a numeric literal token — so upstream keeps the written
bracketed form. A non-negative one still prints bare (`{ [1]: 1 }` is
`{ 1: number; }`, unchanged).

```
WRONG->RIGHT: 6   declarationEmitPartialReuseComputedProperty 4,
                  computedPropertiesNarrowed 2
no adverse transition of any kind
checker_types 5,909 -> 5,910 (+1 case)
```

### 27.1 The pattern in the last four landings

§553 opened a road, and §555 and §557 are both **its own residues**, each found
by probing shapes the new arm now reaches and comparing against a baseline:

| | | |
|---|---|---|
| §553 | the literal halves exist at all | +4 |
| §555 | a literal-named METHOD keeps method spelling | +3 |
| §557 | a NEGATIVE numeric name keeps brackets | +1 |

**+8 cases from one seam**, and the two follow-ups cost a probe each. That is
worth stating as method: *after landing an arm that makes new members exist,
probe the shapes it newly reaches and diff them against baselines* — the arm's
own conversions are where its residues hide, because those are the only places
the new code runs.

### 27.2 One open spelling question in §553's arm, with the evidence both ways

`{ ["1"]: 1 }` — a computed name whose type is the STRING literal `"1"` — prints
`{ "1": number; }` here. Whether that is right is **not settled by the corpus**,
and the two neighbouring facts point in opposite directions:

```
written string name        >{ "0": 1 } : { "0": number; }        QUOTED
computed NUMBER-literal    >t1 : { 1: number; }                  BARE
computed NEGATIVE number   >t6 : { [-1]: number; }               BRACKETED  (§557)
```

A late-bound member has no declaration name node, so the printer cannot be
re-emitting the written form; yet a written `"0"` keeps its quotes and a
computed `1` loses them. The three data points do not fit one rule, and **no
corpus line exercises a computed name whose type is a NUMERIC-LOOKING STRING**,
which is the case that would discriminate.

Left as it is, deliberately. §557 changed the negative-number spelling only
because two baselines demanded it and the pair measured `+6 W→R, 0 adverse`;
this one has no such witness, and guessing it would be exactly the move
`docs/conventions.md` warns about. **Falsifier if someone wants to settle it**:
find or write a fixture with `{ ["1"]: 1 }` and read upstream's `.types`.

---

## 28. §559 — the async road only ever accepted a BLOCK body (+3 cases, 47 favorable, 0 adverse)

Found by a broad probe sweep rather than a row dump — nine unrelated shapes in
one file, two of which gapped:

```ts
async function f() { return 1; }      // () => Promise<number>   works
class K { async m() { return 1; } }   // Promise<number>         works
var c = async () => 1;                // error
var g = { async m() { return 1; } };  // error
```

`return_type_from_body`'s async arm opened with
`let Body::Block(block) = body else { return None }`. **A concise arrow body is
not a block**, so `async () => 1` gapped while the identical declaration
worked — the asymmetry was that one line, not the async machinery.
`getReturnTypeFromBody`'s first arm is `!ast.IsBlock(body)`
(`checker.go:20135`), and the non-async road a few hundred lines below already
takes it through `concise_return_type`.

Modelled as a **one-element `returns` list** rather than by delegating to
`concise_return_type`: everything after that line — the `await` unwrap, the
never/bare-return handling, the `Promise<T>` wrap — is aggregation this arm
already does, and a concise body is exactly *one valued return and no bare
return*. The block became `Option`, and the two reachability questions
(`block_completes_normally`) answer *"the end is not reachable"* for a concise
body without consulting anything, which is true by construction.

```
GAP->RIGHT:    35   asyncMethodWithSuper_es6 20,
                    asyncUnParenthesizedArrowFunction_es2017 4, _es6 4
WRONG->RIGHT:  12   jsxElementType 7, asyncAwaitIsolatedModules_es2017 2, _es6 2
no adverse transition of any kind
checker_types 5,910 -> 5,913 (+3 cases), gradient 90.48% -> 90.49%
```

`jsxElementType`'s 7 lines were not aimed at and are the fifth unpredicted
reach of the session — an async arrow appears anywhere a callback does.

### 28.1 The residue, measured not guessed

`var g = { async m() { return 1; } }` **still gaps.** That is a different gate:
an object-literal method is not in `declaration_takes_no_contextual_return`'s
allowed set the way a `FunctionDeclaration` and a `MethodDeclaration` are
(`signatures.rs`), because upstream consults a contextual return type there
(`checker.go:20179`) and §14 of `checker-notes-callres.md` sized and fenced
exactly that slice. **Not touched**, and not to be confused with this one: this
landing changed which BODY SHAPES the async arm accepts, not which
DECLARATIONS it trusts.

### 28.2 Method note — the broad sweep earns its place beside the row dump

Every landing from §537 to §557 came from a deficit-1 row dump or from probing
an arm's own conversions. This one came from writing nine unrelated one-liners
and reading the output. It cost one probe and paid 47 lines, and the two gaps it
found were in a subsystem no dump had pointed at, because the failing lines are
spread thinly across cases with other blockers.

**Both instruments are needed**: the dump ranks what is already visible; the
sweep finds what nothing has pointed at yet.

---

## 29. §561 — §429's gate refined for uncontextual arrows (+1 case, 24 favorable, 0 adverse)

The second broad sweep's find, and the same asymmetry §559 had: **every**
destructured-parameter shape worked for a `function` declaration and gapped for
an arrow.

```ts
function r([x]) { return x; }   // ([x]: [any]) => any
var      s = ([x]) => x;        // error
function t({m}) { return m; }   // ({ m }: { m: any; }) => any
var      u = ({m}) => m;        // error
```

§429 gated the pattern-implied type to `FunctionDeclaration | MethodDeclaration`
containers, and its recorded reason is precise: *"expression/arrow parameters
may be contextually typed upstream and the implied `any` there was 78 G→W
(`coAndContraVariantInferences3`)"*. That is a claim about **contextually
typed** arrows, and `has_no_contextual_type` is the machinery already built to
decide it — the same refinement §169 made to the return-type gate. The gate now
admits an arrow or function expression exactly where that predicate can *show*
there is no contextual type.

### 29.1 The first draft measured 25 favorable : 29 ADVERSE, and the rows said why

```
emitArrowFunctionES6:0:30  want ([...a]: Iterable<any, void, undefined>) => void
                           got  ([...a]: any) => void
emitArrowFunctionES6:0:39  want ([a]: [number?]) => void
                           got  ([a]: any) => void
```

**§429's gate was hiding shapes the implied-type computation cannot spell.** A
REST-ONLY pattern and an OPTIONAL element both fall through its element guard
and land on a bare `any`. Widening the gate made those *render* where they used
to gap — a confident wrong answer replacing a silence, which is §549's rule
running in the other direction: **a computation that cannot answer must
DECLINE, not emit its fallback.**

Adding that decline — a newly-admitted arrow parameter whose implied type came
out as a bare `any` returns `None` — took the pair from 25:29 to:

```
WRONG->RIGHT: 16   unusedDestructuringParameters 14, destructuringInFunctionType 2
GAP->RIGHT:    8   arrowFunctionExpressions 4, emitArrowFunctionES6 4
no adverse transition of any kind
checker_types 5,913 -> 5,914 (+1 case) — and 62.00% is crossed
```

A **declaration** keeps its `any`: that is what §429 shipped and it is not this
landing's to change.

### 29.2 §429's falsifier, checked

§429's number is the bar, so the named falsifier was *"if
`coAndContraVariantInferences3` loses lines, the predicate is not showing what
it claims"*. It does not appear in the pair at all — zero movement, in either
direction. The 78 G→W were contextually-typed arrows, and
`has_no_contextual_type` excludes exactly those.

### 29.3 What the two sweeps have now established

§559 and §561 are the same defect wearing two hats: **a gate written for
declarations, excluding arrows wholesale, where the recorded reason only ever
justified excluding *contextually typed* ones.** Two subsystems, two sweeps,
one shape. Worth a third look elsewhere — any gate matching
`FunctionDeclaration | MethodDeclaration` is a candidate, and there is one more
at `signatures.rs`'s `declaration_takes_no_contextual_return` which §28.1
already fenced for its own reason.

---

## 30. §563 — the fourth and last gate of §29.3's shape (+2 lines, 0 adverse, +0 cases)

§29.3 predicted that any gate matching `FunctionDeclaration | MethodDeclaration`
is a candidate. Grepping found four; §561 fixed two, §28.1 fenced one for its
own reason, and this is the fourth — `destructure.rs`'s ELEMENT road, which
decides what `x` is inside `([x]) => x`.

Same recorded reason (*90 G→W on `coAndContraVariantInferences3`*), same claim
about contextually typed arrows, same refinement — and §561's lesson carried
across without having to be relearned: the implied type is consulted, and a bare
`any` means the computation could not spell the pattern, so §136's arm would
hand every element that `any`. The newly-admitted containers keep the gap there.

```
GAP->RIGHT: 2   arrowFunctionExpressions, emitArrowFunctionES6
no adverse transition of any kind
checker_types 5,914 (+0 cases), gradient 90.49%
```

**+0 cases and landed anyway**, on the same footing as §551: two lines moved
right, nothing moved wrong, and it removes an asymmetry that had no upstream
justification. The gate sweep §29.3 called for is now complete.

### 30.1 A gap this exposed, NOT fixed, and not to be confused with the gate

```ts
function r([x]) { return x; }   // x : any     works
function t({m}) { return m; }   // m : error   gaps — and this is a DECLARATION
```

The **object**-pattern element road gaps even for a `function` declaration,
where the array-pattern one answers. That is not §29.3's shape — the gate admits
both — so it is a separate defect in the element lookup, sized by nothing yet
and recorded here rather than guessed at. It is the natural next probe in this
file.

---

## 31. §565 — §30.1's object-element gap: §539's shape, a third time (+7 lines, 0 adverse)

§30.1 recorded the gap without guessing at it:

```ts
function r([x]) { return x; }   // x : any     works
function t({m}) { return m; }   // m : error   gaps — a DECLARATION
```

The cause is **§539's shape for the third time**. The pattern-implied object
type is minted as

```rust
self.store.new_named(TypeFlags::OBJECT, printed, None)
```

— **no symbol**, because a binding pattern declares none — so
`get_type_of_property_of_type` has no members table to read, and the members the
type's own printed form shows (`{ m: any; }`) are invisible to every consumer.
The array road never hit this because it reads its tuple positionally.

Same remedy as §539 and per ADR-0003: a side table,
`Checker::pattern_implied_members`, keyed by the minted type id. **Only the
names are stored** — every member of a pattern-implied object is `any` by
construction (`getTypeFromObjectBindingPattern`, `checker.go:17938`, with no
initializer to infer from), so the name being present *is* the answer, and a
name the pattern does not declare keeps the gap.

```
GAP->RIGHT: 7   asyncWithVarShadowing_es6 2, destructuringWithLiteralInitializers 2,
                noImplicitAnyDestructuringInPrivateMethod 1
no adverse transition of any kind
checker_types 5,914 (+0 cases), gradient 90.49% -> 90.50%
```

### 31.1 Three instances now, and the shape is worth naming

| § | what was printed but not consultable |
|---|---|
| 539 | an object literal's INDEX SIGNATURE — minted into the members list the type prints from, invisible to `get_index_infos_of_type` |
| 565 | a pattern-implied object's MEMBERS — minted into the printed text, invisible to `get_type_of_property_of_type` |
| (549) | the sibling failure: a decline returning the gap sentinel |

**The port renders types from baked TEXT and answers queries from SYMBOLS, and
anything minted without a symbol is visible to the first and invisible to the
second.** Every `new_named(..., None)` in the tree is a candidate for this bug,
and there are more of them than these two.

> **Next sweep, concretely**: `grep -n "new_named(.*None)"` and ask of each mint
> *"what would `get_type_of_property_of_type` answer here?"*. Two of the three
> found so far were worth 7 and 19 lines; the sweep costs one grep.

### 31.2 §31.1's sweep, RUN — and it terminates in a subsystem, not a seam

`grep -n "new_named(.*None)"` finds **17 mint sites**. Rather than read them,
the question §31.1 poses was asked directly — *what does member access answer on
each minted shape?*

```ts
type I = { x: number } & { y: string };   i.x  -> number    fine
type O = Omit<{s:number,t:string},"t">;   o.s  -> number    fine
type M = { [K in "a"|"b"]: number };      m.a  -> error
type R = Record<"p", number>;             r.p  -> error
type P = Partial<{ q: number }>;          p.q  -> error
```

The three failures are **not** the §539/§565 shape. The annotation itself does
not resolve — `declare var m: M` gives `m : error`, so there is no minted type
whose members are unreachable; there is no type at all. **Mapped types are
unported at the type-node level**, and `Record`/`Partial` fail for the same
reason while `Omit` happens to survive as a reference.

So the sweep's honest outcome is: the `new_named(..., None)` mints that remain
are not obviously carrying §31.1's bug, and the large gap adjacent to them is a
**subsystem** — mapped-type resolution — with its own cost that this page has
not sized and should not guess at.

**Recorded so the sweep is not re-run.** §31.1's rule stands and is still worth
applying to any NEW mint; what it does not do is point at more cheap wins today.

---

## 32. §567 — two halves of one sweep: the string spread and the enum index key (+2 cases, 38 favorable, 0 adverse)

The third broad sweep, thirteen unrelated one-liners, two gaps — and both turned
out to be **the same road split two ways**, where one entrance answers and the
other does not.

```ts
for (const ch of "ab") { ch }   // string    works
var c = [..."ab"];              // error
var b = E[0];                   // string    works
var a = E[E.A];                 // error
```

**The string spread.** `array_spread_element_type`'s iterator seam is gated on
`declares_symbol_iterator`, a **syntactic** presence test, and a primitive
declares nothing — `String`'s `[Symbol.iterator]` lives in the lib interface,
not on the operand. So the spread road could not reach `for_of_element_type`,
which handles a string perfectly well. One arm, widened not literal:
`[..."ab"]` is `string[]`, the characters and not the literal.

**The enum index key.** §262 synthesised the enum's reverse-mapping index
signature, and `E[0]` reads it. `E[E.A]` did not, because
`is_applicable_index_type` tests for the `number` intrinsic or a
`TypeData::NumberLiteral`, and this port mints an enum member as its own
`TypeFlags::ENUM` type. Upstream carries `NumberLiteral | EnumLiteral`
**together**, so `isTypeAssignableTo(E.A, numberType)` holds there and the
distinction never arises.

```
WRONG->RIGHT: 24   propertyAccess 9, noImplicitAnyIndexing 7,
                   noUncheckedIndexedAccess 5
GAP->RIGHT:    14  noUncheckedIndexAccess 8, enumMapBackIntoItself 3, enumBasics 3
no adverse transition of any kind
checker_types 5,914 -> 5,916 (+2 cases), gradient 90.50%
```

The string half alone measured **1 line**. The enum half took it to 38 — worth
recording, because the sweep surfaced both in one run and stopping at the first
would have looked like a dead end.

### 32.1 The shape these two share with §559 and §561

Four landings now, all the same sentence: **two entrances to one road, and only
one of them was gated correctly.**

| § | answers | gaps |
|---|---|---|
| 559 | `async function f() {…}` | `async () => 1` |
| 561 | `function r([x]) {…}` | `([x]) => x` |
| 567a | `for (const ch of "ab")` | `[..."ab"]` |
| 567b | `E[0]` | `E[E.A]` |

None was a missing subsystem; each was a gate or a test written for the entrance
its author had in hand. **The probe that finds them is always the same: write
the two spellings of one idea side by side and diff the output.** That is the
cheapest instrument in this file and it has now produced four landings and 111
lines.

---

## 33. §569 — the spread's iterator gate was SUFFICIENT, not necessary (+5 cases, 92 favorable, 0 adverse)

§32.1's instrument, run deliberately: pairs of one idea, side by side.

```ts
for (const x of g1()) { x }      // number   works
var c1 = [...g1()];              // error
var c2 = [...new Set([1])];      // error
```

`array_spread_element_type`'s iterator seam is gated on
`declares_symbol_iterator`, which asks whether the operand's **own
declarations** spell `[Symbol.iterator]`. A LIB type never does from the
operand's side — a `Generator`, a `Set`, a `Map` all carry it on the interface —
so the gate is a **sufficient condition standing in for a necessary one**, and
every lib iterable fell off the road that `for..of` walks happily.

The fix is to fall through to the same `for_of_element_type` the for-of road
calls. **Safe by construction**: it answers `Option` and declines where it
cannot decide, so this can only turn a gap into an answer the for-of road
already trusts. Placed before the decidable-failure arm so a real element type
wins over that arm's `any`.

```
WRONG->RIGHT: 76   genericRestParameters1 16, callWithSpread3 9,
                   readonlyRestParameters 8
GAP->RIGHT:    16  spreadsAndContextualTupleTypes 15, partiallyNamedTuples3 1
no adverse transition of any kind
checker_types 5,916 -> 5,921 (+5 cases), gradient 90.50% -> 90.52%
```

**92 favorable, zero adverse** — the best ratio since §553, and the reach is
almost entirely in *rest-parameter and call-spread* families that no probe
aimed at. §567a had already shaved the string case off this same gate one
landing earlier; the general form was worth 92 lines where the special case was
worth 1.

### 33.1 The lesson §567a nearly hid

§567 fixed **strings** by adding a special-case arm above the gate. That was
correct and it measured, but it treated a symptom: the gate itself was the
defect, and one more probe pair (`[...g1()]` against `for (const x of g1())`)
showed the same failure with nothing to do with primitives.

> **When a gate turns out to reject one thing it should admit, ask what ELSE it
> rejects before writing the special case.** §567a's string arm is now
> redundant-looking but harmless — it answers earlier and identically — and is
> kept because deleting it is a separate measurement, not because it is needed.

Five landings now share §32.1's sentence (§559, §561, §567a, §567b, §569), and
this is the first where fixing the *gate* rather than the *case* multiplied the
result by ninety.

---

## 34. §571 — the same gate, third site (+2 lines, 0 adverse)

§33.1 said: *when a gate turns out to reject one thing it should admit, ask what
ELSE it rejects.* Asked, and `declares_symbol_iterator` had a third caller —
the DESTRUCTURING positional read (`destructure.rs`), which decides what `a` is
in `var [a] = …`.

```ts
var [c1] = new SymbolIterator;   // symbol   works (§317)
var [c2] = g();                  // error
```

Same cause, same fix: the syntactic gate asks whether the receiver's own
declarations spell `[Symbol.iterator]`, and a lib iterable never does.
`for_of_element_type` declines safely, so dropping the gate can only turn a gap
into an answer the for-of road already trusts.

```
WRONG->RIGHT: 2   generatorAssignability
no adverse transition of any kind
checker_types 5,921 (+0 cases), gradient 90.52%
```

**+0 cases and landed**, on §551's and §563's footing. The three callers of that
gate are now consistent: §547 built the rest arm without it, §569 dropped it on
the spread road, §571 drops it here.

### 34.1 The sweep's remaining rows, recorded not guessed

The same probe pair surfaced two more, neither touched:

```ts
var a1 = Array.from(g());              // error   — a CALL, signature resolution
for (const d of new Set([1])) { d }    // any     — want number
```

`for_of_element_type` answers `any` for a `Set` where it answers `number` for a
`Generator`, so the iterated-type computation is partial in a way this landing
does not address and no probe here has sized. `Array.from` is a different road
again — overload resolution over an iterable argument. **Both are recorded so
the next reader does not mistake §571 for having covered them.**

---

## 35. §573 — `Set` was missing from the lib-iterable list (+9 lines, 0 adverse)

§34.1's second row. `for_of_element_type`'s §251 arm shortcuts *"the lib types
whose first type argument IS their iteration type"* and named three:
`Iterable`, `IterableIterator`, `Generator`. `Set<T>` satisfies the same
property — `interface Set<T> { [Symbol.iterator](): IterableIterator<T> }` — and
was simply absent, so `for (const d of new Set([1]))` answered `any` where the
identical `Generator` answered `number`.

Added `Set` and `ReadonlySet`.

**`Map` is deliberately NOT added.** `Map<K, V>` iterates `[K, V]`, a tuple of
both arguments, so reading `arguments[0]` would answer `K` and be confidently
wrong. The rule the list encodes is *the first argument is the iteration type*,
and `Map` does not satisfy it — the same reason `Array` has its own arm above
rather than a place in this list.

```
WRONG->RIGHT: 9   esNextWeakRefs_IterableWeakMap
no adverse transition of any kind
checker_types 5,921 (+0 cases), gradient 90.52%
```

### 35.1 The probe said nothing and the corpus said nine

Worth recording because it is §20.1's rule from the other side. The probe

```ts
for (const d of new Set([1])) { d }   // still `any` after the change
```

showed **no movement**, and on the evidence of §555 the instinct is *"the edit
did not apply"*. It had. The corpus moved nine lines in
`esNextWeakRefs_IterableWeakMap`, a case whose `Set` arrives through a different
construction than a bare `new Set([1])` in a probe file with the default lib
mount.

> **A probe that shows nothing is not a measurement either.** It is one fixture
> under one lib configuration; the corpus is 9,538. §20.1 said check whether the
> arm fired before believing a zero — the corpus IS that check, and it is the
> one that counts. Had this been reverted on the probe's word, nine correct
> lines would have gone with it.

---

## 36. §575 REFUSED — the `keyof` gate is load-bearing (177 R→W)

The paired sweep found `keyof` erroring in every form:

```ts
type K1 = keyof I;          // error
type K2 = keyof { x: 1 };   // error
type K3 = keyof C;          // error
```

and the machinery to answer it **already exists** — `keys_of` and
`literal_key_union`, with an arm in `get_type_from_type_node` gated on
`!self.alias_evaluation_bindings.is_empty()`. That reads exactly like §32.1's
shape (one entrance gated, the road built) and the ungated form does produce the
right answers on a probe:

```
keyof I        -> "x" | "y"
keyof { x: 1 } -> "x"
keyof C        -> "z"
```

**Measured, and it is catastrophic:**

```
RIGHT->WRONG: 177   mappedTypeRelationships 48, keyofAndIndexedAccessErrors 15,
                    controlFlowGenericTypes 14, spyComparisonChecking 10,
                    nonPrimitiveConstraintOfIndexAccessType 10, conditionalTypes1 8
WRONG->GAP:    17
WRONG->RIGHT:  23
```

Reverted; the tree re-measures `no transitions vs baseline`.

### 36.1 Why the gate is right, and the reopening condition

`keyof T` over a **generic or otherwise deferred** `T` must stay deferred and
**print as `keyof T`**. Eagerly evaluating it answers the keys of the
constraint — or of nothing — and the 177 lines are overwhelmingly generic
positions (`mappedTypeRelationships`, `controlFlowGenericTypes`,
`conditionalTypes1`) where upstream prints the operator and this port would
print an evaluated union. §35's deferred print is what the gate protects, and
the alias-evaluation window is the one place the evaluation is both wanted and
safe.

> **Reopen when the arm can distinguish a CONCRETE operand from a deferred
> one.** The probe's three shapes — an interface, a type literal, a class — are
> all concrete and all answered correctly, so the split is real and the win is
> the 23 W→R the ungated form also produced. The predicate needed is *"does
> `keys_of` see a complete members table that cannot change under
> instantiation"*, which is not the same question as *"did `keys_of` return
> `Some`"* — the ungated arm proves that, because `keys_of` answered `Some` for
> all 177 losers too.

**This is the second refusal of the session where the machinery existed and the
gate was the whole design** (§545 was the first). Both times the gate looked
like an oversight and both times it was measured, load-bearing, and cheaper to
respect than to re-derive.

### 36.2 §577 — the first attempt at that predicate, and it measured the SAME 177

The obvious reading of §36.1 was written and measured: *concrete* = the operand
is not itself a type parameter, and no type argument of it is.

```
RIGHT->WRONG: 177   — byte-identical to the ungated form
```

**The predicate excluded nothing.** The losers are lines like

```
k  : keyof T
f3 : <T, U extends T>(x: T, y: U, k: keyof T) => void
```

— unmistakably generic — so a `TypeFlags::TYPE_PARAMETER` test on
`get_type_from_type_node(T)` is not identifying them. Reverted; the tree
re-measures `no transitions vs baseline`.

**What that tells the next attempt**, and it is worth more than the attempt
itself: those lines are **RIGHT today**, printing `keyof T`. The gated arm
answers `error` for them, and the correct text is coming from **baked text**,
not from this arm at all. So ungating does not "evaluate a deferred type" — it
*replaces a correct printed form with an evaluated one*.

> **The reopening condition is therefore NOT a type-shape predicate.** It is:
> *the arm must not fire where the baked text is already the answer.* That is
> the same distinction §15 drew for slice 3 — baked text being richer than the
> computed name — arriving here from the other direction, and it means any
> future attempt should start by asking what the printed form is today rather
> than what the type is.

---

## 37. §579's candidate — `Array.prototype.concat`, narrowed to four probes and NOT built

A paired sweep found two failing array methods; one is priced territory and one
is not:

```ts
[1,2].reduce((p,c) => p+c)   // error — CALLBACK contextual typing, priced/refused
[1,2].concat([3])            // error — no callback anywhere
```

`concat` narrowed by probe, and each step eliminates a suspect:

| probe | result | eliminates |
|---|---|---|
| `[1,2].slice(0)`, `.indexOf(1)`, `.push(3)`, `.map(x=>x)` | all correct | member lookup on `Array<T>`; rest parameters; `T` from the instantiation |
| `declare var a: ConcatArray<number>` | `ConcatArray<number>` | the lib interface resolving |
| `a.length` | `number` | its members |
| `function f(...xs: ConcatArray<number>[])`, `f([1])` | `ConcatArray<number>[]` | a rest parameter OF that type, and assigning an array literal to it |

Everything `concat` is made of works in isolation. What is left, and it is a
**hypothesis not a measurement**: `concat` is the only OVERLOADED member tested,
and its two signatures

```ts
concat(...items: ConcatArray<T>[]): T[];
concat(...items: (T | ConcatArray<T>)[]): T[];
```

both carry the *enclosing interface's* type parameter in a rest position.
`push(...items: T[])` has the same `T` and works — but is not overloaded — so
the suspect is **overload resolution over signatures instantiated from the
receiver**, not the rest parameter and not `T`.

### 37.1 CONFIRMED — it is OVERLOADED *and* GENERIC, and the signature is uninstantiated

The hypothesis was tested directly with four user-written interfaces, and it is
no longer a hypothesis:

| shape | `x.m(1)` |
|---|---|
| `interface X<T> { m(a: T): T; m(a: T, b: T): T }` — overloaded + generic | **error**, and the member prints `>a : T` |
| `interface Y<T> { n(...xs: T[]): T }` — generic, ONE signature | `number` ✓ |
| `interface Z<T> { p(...xs: T[]): T; p(...xs: string[]): T }` — overloaded + generic | **error** |
| `interface W { q(a: number): number; q(a: string): string }` — overloaded, NOT generic | `number` ✓ |

**Either property alone is fine; together they fail.** And the failing member
prints `T` — the *uninstantiated* parameter — which localises it precisely:

> **The overload path reads a member's signatures WITHOUT the receiver's
> instantiation.** The single-signature road applies it (`Y` answers `number`),
> and the same seam is already documented one file over — *"on an instantiated
> reference the declared value types are the target's uninstantiated ones;
> `Array<string>`'s `[n: number]: T` must answer `string`, not `T`"* — so the
> mechanism exists and the multi-signature member does not thread it.

`Array.prototype.concat` is exactly this shape: two signatures, `T` from
`Array<T>`.

**Still not built.** The diagnosis is now a measured fact rather than a guess,
but the fix threads instantiation through the multi-signature member road, which
needs a bar registered before it and a full pair after. Recorded at the point
where the next session starts from a confirmed localisation and a four-row
falsifier table rather than from a failing lib method.

Also unresolved from the same sweep and NOT the same thing: `[1,2].reduce(…)`
and `[1,2].find(…)` / `.filter(…)` fail through the callback's contextual type,
which `checker-notes-fnexpr.md` §10 measured at 86% entangled and refused.
Do not bundle them with `concat`.

---

## 38. §579 — overload selection on an instantiated signature type (+307 lines, 0 adverse) — and SIX red tests found

§37.1 localised the `concat` defect to *"the overload path reads a member's
signatures without the receiver's instantiation"*. **Following it one step
further corrected that too**, and the truth is narrower and better:

`instantiate_signature_type` already loops over every signature — instantiation
was never the problem. The block in `resolve_call_signature` guarded by
`is_instantiated_signature_type` handles `[signature]` and gaps anything longer,
and its own comment explains why: *"an overload set stays a gap for the same
reason the symbol path's does."*

**The symbol path does not gap one.** `interface W { q(a: number): number;
q(a: string): string }` resolves `w.q(1)` to `number` today. So the two are not
the same reason, and what was unhandled was exactly **overloaded AND generic** —
the shape `Array<T>.concat` has.

The fix is to run the same `choose_overload` the symbol path runs, over the
signatures already read off the type. Every guard inside it applies unchanged.

```
WRONG->RIGHT: 307   promisePermutations2 130, promisePermutations 112,
                    promisePermutations3 32
no adverse transition of any kind
checker_types 5,921 (+0 cases), gradient 90.52% -> 90.59%
```

**+307 lines and the largest gradient move of the session**, in the promise
families — a subsystem §182's notes had priced as blocked on conditional-type
evaluation. It was partly blocked on this instead.

### 38.1 The gate report was wrong, and it was wrong for a mechanical reason

Running the crate suite properly found **six red tests**, the oldest red since
§533. Every one pins a refusal a measured landing in this session deliberately
overturned:

| test | overturned by | now |
|---|---|---|
| `a_module_named_by_two_aliases_is_a_gap` (×2 shapes) | §533 | `typeof one` / `typeof a` |
| `the_refused_legs_stay_gaps` — out-of-range element | §549 | `undefined` |
| `the_refused_legs_stay_gaps` — parameter pattern | §565 | `any` |
| `a_mixed_literal_still_gaps` | §551 | `{ a: number; [x: number]: number; }` |
| `a_member_this_port_cannot_type…` / `a_string_like_computed_name…` | §553 | `{ 1: number; }` |

Each was re-pointed at **upstream's baseline**, not at whatever the port now
prints — `destructuringArrayBindingPatternAndAssignment1ES5.types` records
`>c2 : undefined` for the out-of-range element, and `>p : any` appears verbatim
for the unannotated parameter pattern.

> **Why they went unnoticed: `cargo test -p tsr-checker` was TIMING OUT at 600 s
> and I read the partial log.** `grep -c '^test result: ok'` on a truncated log
> counts the suites that finished and sees no failure from the ones that never
> ran. The counts reported in earlier landings ("19 suites green") weretrue for
> the suites that ran and meaningless as a gate.
>
> **A test gate must assert on the RUN, not on a grep of its log.** The
> whole-crate suite is 64 suites; any report quoting fewer was partial. This is
> the same class as §555 (a scripted edit with no assertion) and §573 (a probe
> too narrow to see the change) — three different ways of believing an
> instrument that was not actually answering.

The corpus never regressed: every landing carried a full scorepair, and all six
tests were pinning refusals rather than behaviour the pairs measured.

### 38.2 What §579 did and did not reach, re-probed

The four-interface table from §37.1, re-run after the landing:

| shape | before | after |
|---|---|---|
| `interface X<T> { m(a:T):T; m(a:T,b:T):T }` | error | **`number`** ✓ |
| `interface Z<T> { p(...xs:T[]):T; p(...xs:string[]):T }` | error | error |
| `[1,2].concat([3])` | error | error |

**The discriminator between the fixed and unfixed halves is a REST parameter in
the overload set**, and that is not an oversight: `choose_overload` declines
outright on *"a rest or `this` parameter on any candidate"*, one of five
reductions it names, *"which changes what arity means"*.

`Array<T>.concat`'s two signatures are both rest, so it is blocked behind that
guard rather than behind anything §579 touched.

> **So `concat` is now a REFUSAL to reopen, not a defect to find.** The
> reopening condition is `choose_overload`'s rest guard, which needs upstream's
> arity treatment for spread/rest candidates (`hasCorrectArity`'s rest arm,
> `checker.go:9107`) before it can be relaxed — a bar-and-pair change of its
> own, and one that would also reach the `z.p` shape above.
>
> §37's four probe eliminations still stand and are not wasted: they are what
> proved the failure was in overload selection rather than in `ConcatArray`,
> rest parameters as such, or `T` from the instantiation. The +307 lines came
> from the half of that diagnosis which was reachable.

---

## 39. §581's candidate — `resolveJsonModule`, SIZED at 5 deficit-1 cases and not built

Re-dumping deficit-1 after §579 surfaced a family with an **identical wanted
text across five cases**, which is the strongest shared-shape signal this
instrument produces:

```
GAP  want { a: boolean; b: string; }  got error   × 5
  requireOfJsonFileWithModuleEmitUndefined
  requireOfJsonFileWithModuleNodeResolutionEmitEs2015
  requireOfJsonFileWithModuleNodeResolutionEmitEsNext
  requireOfJsonFileWithModuleNodeResolutionEmitUndefined
  requireOfJsonFileWithoutEsModuleInterop
```

All five import a `.json` file. The `resolveJsonModule` **option** is plumbed
(`tsr-execute`'s `compile.rs`, `help_all.rs`, `show_config.rs`) but nothing on
the **type** side turns a `.json` file into a module whose shape is its
contents, so every one of them gaps at `:0:0` — the import itself.

**Sized and not built** — and the sizing is now sharper than "two subsystems".
Checked: **the RESOLUTION side already exists.** `loader.rs` imports
`SUPPORTED_TS_EXTENSIONS_WITH_JSON_FLAT` and threads
`supported_extensions_with_json`, so a `.json` specifier already resolves to a
file. **CORRECTION, same session, before this paragraph was an hour old.** It first
said *"what is missing is upstream's `parseJsonText` — nothing in the tree
matches that name"*. **That was false and the grep proving it came back in the
same command that wrote it.** `Parser::parse_json_text` is
`crates/tsr-parser/src/json.rs:50`, reached from `lib.rs:159` and
`parsed_file.rs:113`, and `ScriptKind::Json` is routed in
`tsr-conformance/src/diagnostics_suite.rs` at five sites.

So **both** halves the original sizing named already exist: `.json` resolves
(`SUPPORTED_TS_EXTENSIONS_WITH_JSON_FLAT`) and `.json` parses
(`parse_json_text`). What is missing is only the **type**: a `.json` source file
becoming a module whose shape is its contents, which the `checker_types` suite's
five `requireOfJsonFile*` cases want and which the *diagnostics* suite already
routes around.

That is a materially smaller item than either version of this paragraph claimed
before, and it is now the best-priced unbuilt entry on this page: two
subsystems already built, one type synthesis missing, five deficit-1 cases
waiting.

> **Recorded as a correction rather than an edit** because the first claim was
> wrong in the direction that would have made the next session skip the item as
> too expensive — and because it is this session's fourth instance of an
> instrument answering after the conclusion was written (§541, §555, §573, this). What makes it worth
recording rather than leaving in the dump is the shape:

> **Five cases, one wanted text, one missing feature, and the option already
> parsed.** That is the cheapest-to-verify family left on this page — one
> `probefile` with a `.json` sibling confirms the whole diagnosis — and unlike
> the mapped-type and conditional-type items it is a *feature that does not
> exist* rather than a subsystem that is partially wrong.

Recorded with its population so the next session does not have to re-derive the
five case names from a fresh dump.

### 39.1 The `collisionThisExpression*` family — probed, does NOT reproduce

The other repeated shape in the re-dump was the `want any / got error` bucket
(15 cases). Its one coherent sub-family is
`collisionThisExpressionAndLocalVarIn{Constructor,Lambda,Method,Property}` plus
`AndParameter` — five cases sharing a fixture shape — whose first failing line is

```
collisionThisExpressionAndLocalVarInLambda:0:0
  want { (message?: any): void; (message?: any): void; }
  got  (message?: any) => void
```

i.e. a `declare function alert` merging with the lib's `alert` should print
**both** signatures. **The probe answers correctly:**

```ts
declare function alert(message?: any): void;
var a = alert;   // >a : { (message?: any): void; (message?: any): void; }
declare function f(x: number): void;
declare function f(x: string): void;
var b = f;       // >b : { (x: number): void; (x: string): void; }
```

So merged-declaration signature collection is **not** the defect, and whatever
these five share is environment- or fixture-specific rather than mechanical.

> **This is §35.1's inversion and the pair completes the rule.** There, a probe
> showed *nothing* while the corpus moved nine lines; here, a probe shows
> *correct* while the corpus is wrong. **A probe can only ever confirm the shape
> you wrote under the lib you mounted.** It is an excellent instrument for
> localising a defect you have already measured, and worthless as evidence that
> one does or does not exist.

Not built, and specifically **not** filed as "merged signatures are broken",
because they are not.
