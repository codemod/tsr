# ADR-0044: A name is rendered from its symbol at the site

**Status:** Accepted. **Supersedes**
[ADR-0043](0043-a-type-must-be-renderable-differently-at-different-sites.md),
which framed this question, priced three options and deliberately decided
none. ADR-0043 stays exactly as written — it is the record of the open
question and of two probes that still stand.

The transcription evidence is `docs/architecture/checker-notes-sitename.md`;
this record carries the decision and its consequences.

## The forcing constraint

`TypeData::Named { text, .. }` and `TypeData::Anonymous { text, .. }` compute
a type's printed form once, at creation. Upstream's node builder renders a
name at each site by walking symbol containers for an accessible spelling
(`getAccessibleSymbolChain`, `symbolaccessibility.go:373`, reached from
`getSymbolChain`, `nodebuilderimpl.go:1088`). Sixteen recorded refusals
depend on the difference; `checker-notes-sitename.md` §3 lists each with the
number that measured it.

## What ADR-0043 got wrong, and it is the whole reason this record can decide

ADR-0043 wrote that acting on site-dependence is *"impossible today, because
the port has nothing else to print"*, and priced Option 1 as widening
`TypeData` and touching every consumer of the printed form.

**Both halves are now false, and neither was false when it was written.**

1. **The port has a per-site rendering layer.** `type_to_string_at` carries
   four composite arms (§95, §97, §99, §10.13) that decompose a type through
   a side table and re-render every slot at the reference. They were built
   one shape at a time, by sessions working around this wall from the inside.
2. **The symbol is already retained.** `Named` carries
   `members: Option<SymbolId>`, `Anonymous` carries `symbol: SymbolId`, and
   `reference_text_at` (`checker.rs:1558`) already renders a generic
   reference's name **from that symbol** at the site — `best_name`, else
   `symbol_chain` + the bare name — never consulting the baked text.

So the capability ADR-0043 priced as architectural surgery exists and is
load-bearing on one road. What is missing is not a representation; it is
**reach**: the render is wired only to types recorded in
`type_reference_targets`, and every other type falls to `qualified_name_at`,
which string-patches the baked text at an offset `split_around_name`
(`checker.rs:1829`) can find in exactly three spellings — `typeof C`, bare
`C`, `C<…>`.

## The decision

**A type that carries a symbol renders its NAME from that symbol at the
reference site. The baked text becomes a fallback, not the identity.**

Concretely, and in this order (`checker-notes-sitename.md` §5):

1. the accessibility walk gains upstream's missing arms — the `globalThis`
   globals-table fallback (`symbolaccessibility.go:588-591`) and
   `compareSymbolChains`' shortest-chain selection (`:582-586`) in place of
   the port's ambiguity decline;
2. the name-from-symbol render generalises off the reference road to every
   symbol-carrying `Named`/`Anonymous`;
3. symbol-less mints — tuples, the §29 alias placeholder, generic-alias
   texts, minted composites — keep the baked text, permanently and by design.

`TypeData` does **not** widen. No consumer of `type_to_string` changes. The
site-rendered name is produced inside `type_to_string_at`, which is already
the single entry every rendered corpus line goes through
(`types_producer.rs:1355`).

## The alternatives, taken seriously

**ADR-0043's Option 1 (store a second, structural form) — rejected as
unnecessary rather than as wrong.** It answers a question the port no longer
has to ask: the structural form is either reconstructible from a side table
(the four composite arms) or the type has a symbol and needs a name, not a
structure. What would revive it: a row whose answer is a *structural*
rendering of a type that has a symbol and no side-table decomposition. The
type-alias row in ADR-0043's own table (`type T = {}` printing `{}` where the
name is inaccessible) is exactly that shape, and it is the one debt this
decision does **not** pay — recorded here rather than quietly dropped.

**ADR-0043's Option 2 (re-render from the written type node) — stays
rejected, on ADR-0043's own measurement.** Its probe found upstream printing
`"a" | "b"` for a written `keyof { a: 1; b: 2 }`; a normaliser that reaches
that is a second type checker. Nothing here disturbs that finding.

**Option 3 (do nothing) — rejected on the ledger.** Sixteen refusals, four of
them re-derived independently more than once (`umd-augmentation-1`'s 14 lines
have now been measured against three separate arms).

## The consequences accepted

- **The name and the structure diverge in one visible way**: a symbol-carrying
  type prints a site-dependent name over a creation-time structure. For
  `typeof C` and `C<…>` that is the whole printed form and the divergence is
  invisible; for a baked composite embedding a name (`{ a: C; }`) the name
  inside it stays baked until that shape gets a decomposition arm. This is a
  *narrowing* of today's divergence, not a new one.
- **Two roads for a while.** Until slice 3 lands, `reference_text_at` renders
  from symbols and `qualified_name_at` patches strings. That asymmetry is the
  current bug, and the ADR's own success condition is its removal.
- **The port keeps a rendering divergence, not a data-model one** — ADR-0003
  is untouched, exactly as ADR-0043 said of every option.

## How we would know this is wrong

- **Slice 1's falsifier, registered before its code**: if the `globalThis`
  arm fires on more than a handful of sites, the globals-reachability gate is
  not doing its work and the build is SS197 (measured **−66 cases**) under a
  new name.
- If slice 3 measures net-negative on wrongs across the corpus, the
  name-from-symbol render is answering a *different* question than the baked
  text answered, and the two-road asymmetry was load-bearing rather than
  accidental.
- If the type-alias structural row (above) is joined by two more of its
  shape, Option 1 was deferred wrongly and `TypeData` does need to widen.
- If `compareSymbolChains`' shortest-chain rule measures worse than the
  port's ambiguity decline, then upstream's total order depends on a
  `compareSymbols` tie-break this port cannot reproduce, and the decline is
  the honest answer after all.

## Evidence

- `docs/architecture/checker-notes-sitename.md` — the transcription, the
  ledger, the build order and slice 1's bar.
- `symbolaccessibility.go:373-744`, `nodebuilderimpl.go:644-851`, `:1061-1151`
  — the upstream walk and its chain-to-text branch, at the pinned commit.
- `internal/testutil/tsbaseline/type_symbol_baseline.go:395` — the fact that
  reframed M2: the `.types` baseline calls `TypeToTypeNode` directly, so the
  declaration-emit node-reuse machinery is **not** on the corpus path.
