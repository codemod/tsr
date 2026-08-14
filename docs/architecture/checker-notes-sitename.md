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

**M2 — the NODE-REUSE rule.** Whether the printed form starts from the
**written syntax node** or is built fresh from the computed type. Input: a
position in the tree. Owns nothing about names.

§517 and §519 are the proof that M2 exists and is per-position: the *same*
carriage (keep a written union's constituent order) measured **+80/0** at
type-literal member slots and **356 R→W** at declaration names. No naming rule
explains that split; a node-reuse rule does.

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

*(§4.1–§4.3 are filled from the reads in progress; the port-side analysis
above stands independent of them.)*

---

## 5. Build order

*(Written after §4 lands.)*

---

## 6. The bar, registered before the code

*(Written after §5.)*
