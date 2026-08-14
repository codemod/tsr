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
