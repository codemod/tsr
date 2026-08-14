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
