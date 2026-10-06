# ADR-0046: tsgo's JSDoc reparse is answered by checker queries, not built by the parser

- **Status:** Accepted — built for modifiers (`jsdoc_modifiers.rs`, lane js-3)
  and for the function-like `@param`/`@this`/`@template`/`@return`/`@type`
  arms (`jsdoc_params.rs::jsdoc_reparsed_function`, this commit). The other
  hosted arms are listed under *What is not built*.
- **Date:** 2026-10-06
- **Issue:** `bd tsr-2zk.34` (lane `tsr-2zk.5`)
- **Related:** [ADR-0003](0003-tree-plus-side-tables.md) (tree plus side
  tables), [ADR-0008](0008-jsdoc-parsed-eagerly.md) /
  [ADR-0010](0010-jsdoc-is-a-parse-option.md) (when JSDoc is parsed),
  [ADR-0012](0012-ast-is-sync.md) (nodes are immutable once allocated),
  [`docs/architecture/jsdoc.md`](../architecture/jsdoc.md).

## The forcing constraint

In a `.js` file typescript-go does not *interpret* JSDoc: its parser
**rewrites the tree**. `internal/parser/reparser.go` (748 lines at the pinned
`5b1047d`) runs at the end of every node that carries a comment
(`reparseTags`) and

- appends `NodeFlagsReparsed` keyword modifiers for `@readonly`/`@private`/
  `@public`/`@protected`/`@override` (`reparseHosted`, `:521`);
- sets `param.Type` and a reparsed `param.QuestionToken` from `@param`
  (`:479`, `makeQuestionIfOptional` `:597`), prefixes a `this` parameter for
  `@this`, sets the return type from `@return`, the type parameters from
  `@template`, and `FullSignature` from a `@type` on a function-like host;
- wraps initializers and expressions in `SatisfiesExpression` / `AsExpression`
  for `@satisfies` and a parenthesized `@type` (`makeNewCast`, `:684`);
- emits whole declarations (`@typedef`, `@callback`, `@import`, `@overload`)
  into a reparse list.

Every checker read is then an ordinary AST read. That is the behaviour to
match. The question is where this port computes it.

Three facts about *this* port decide it:

1. **The checker already answers the hosted arms from the side table, in
   nine files.** `jsdoc_entries` (host `NodeId` → comments) is read 19 times,
   and `JSDocTag::` is matched at 62 sites, across `checker.rs`,
   `declared.rs`, `heritage_conformance.rs`, `jsdoc_links.rs`,
   `jsdoc_modifiers.rs`, `jsdoc_params.rs`, `signatures.rs`, `symbols.rs` and
   `type_argument_arity.rs` (counted at `d109b0c`). Those files belong to at
   least five parity lanes. A parser that writes the reparsed nodes into the
   tree makes every one of those reads count the JSDoc twice — a bracketed
   `@param` would add `| undefined` once from the reparsed `?` and once from
   `symbols.rs`' own bracket test — so option (a) is a flag day across all of
   them, not an addition.
2. **Nodes cannot be mutated after allocation.** Upstream mutates the host
   (`AsMutable().SetType`, `SetModifiers`, `Parameters = …`). Here a node is
   arena-allocated and `Sync` (ADR-0012); its children are `&'a` slices already
   referenced by the parent. The comment is parsed *before* the host and
   attached after it (`parse_leading_jsdoc` / `attach_jsdoc`), so building the
   reparsed host means re-allocating it and every parameter it touches, with
   fresh `NodeId`s that the parent table, the binder and every side table
   must then agree on.
3. **The parser cannot tell a JavaScript file.** `ScriptKind` has
   `TypeScript`, `Tsx` and `Json` (`crates/tsr-parser/src/parser.rs:16`);
   `.js` parses as TypeScript. The JSDoc diagnostics gate already lives in the
   consumer for that reason (lane notes §1).

Upstream itself does not get "ordinary reads only": 25 sites in
`internal/checker` (`checker.go` 5, `grammarchecks.go` 20) test
`NodeFlagsReparsed` to tell a reparsed token from a written one. So either
design carries a written/reparsed distinction at the consumer; the question
is only whether it is a flag bit or a query.

## The alternatives

**(a) A JavaScript flag in `ParseOptions`; the parser builds the reparsed
nodes at parse time, as tsgo does.** Faithful by construction for every later
reader; no replay logic to keep in step with upstream's tag order.
*Cost:* a `ScriptKind::Js`/`Jsx` arm threaded from the compiler; host
re-allocation at `attach_jsdoc` for the arms that mutate; a
`NodeFlags::REPARSED` bit; and, in the same change, the removal of every
checker-side JSDoc read in the nine files above, owned by other lanes. On the
public bench projects (`benches/projects/domain-model`, `generic-imports`:
45 `.ts` files, **0** `.js` files, **0** `/**` comments in source) the arm never
fires, so its run-time cost there is one branch per attached comment, all of
which are in the bundled `lib.*.d.ts` (9,637 `/**` comments in
`_submodules/TypeScript/src/lib`). The cost that matters is the flag day.

**(b) Shared checker queries over the side table, one per reparsed fact,
used by every checker caller.** `has_effective_modifier` /
`jsdoc_reparsed_modifiers` (modifiers, js-3) and
`jsdoc_reparsed_function` (this ADR: per-parameter reparsed `?` and type
tag, plus `FullSignature`) each replay the hosted arm **in upstream's tag
order over the host's last comment** — the order matters: a `@type` before
`@param` becomes the full signature and the `@param` is then skipped; after
it, the reverse. *Cost:* each query is an `in_js_file` walk to the root and
then nothing for a TypeScript file; in a JS file, one hash lookup and one
comment's tags. No cache: the callers are once-per-declaration checks or are
memoized above (the symbol's type).

**(c) Status quo: each caller re-derives what it needs.** Rejected — it is
how the port got three different approximations of "is this JS parameter
optional" (`signatures.rs:1512`, `symbols.rs:6077`, `optionality.rs:252`),
none of which applies `findMatchingParameter`'s positional rule, the
`FullSignature` gate or the last-comment rule.

## Decision

**(b).** The reparse is answered by checker queries that are each a port of
one `reparseHosted` arm, replayed in tag order, living in the JSDoc-owned
checker files. A new consumer of a reparsed fact calls the query; it does not
read `jsdoc_entries` itself. The function-like arms are one replay
(`jsdoc_reparsed_function`) so that the order-dependent state — full
signature, typed parameters, `this` prefix, type parameters, return type — is
computed once, the same way, for every reader; `jsdoc_full_signature.rs` now
reads its `FullSignature` from it.

What would make (a) win: the number of checker-side JSDoc readers falling to
the few queries named here (so the flag day is small), **and** a consumer
needing a reparsed node *as a node* — a `NodeId` for the binder to hang a
symbol on, or a span the checker reports at — that a query cannot fake. The
declaration-emitting arms (`@typedef`, `@callback`, `@import`, `@overload`)
are already of that kind and are already built as nodes by the parser
(lanes js-2, §269); that split is deliberate, not drift.

## Consequences

- Converted by this commit: `checkJsdocOptionalParamOrder`,
  `jsdocParseBackquotedParamName` (TS1016 through `checkGrammarParameterList`
  reading a reparsed `?`). The modifier clusters named by `tsr-2zk.34`
  (`privateNamesIncompatibleModifiersJs`, `jsdocAccessibilityTags`,
  `override_js2`/`4`, `jsdocOverrideTag1`) were converted by js-3 with the
  same design before this ADR was written; this ADR records the choice they
  made.
- The replay is a second implementation of upstream's ordering rules, so it
  can drift from `reparser.go`. It names its anchors line by line; a change
  to `reparseHosted`'s arms in a submodule bump must be mirrored in
  `jsdoc_params.rs`.
- A reparsed token has no node. Where upstream reports *at* the reparsed
  `?` (TS1047 on a rest parameter), this port reports at the `@param` tag,
  which is where `makeQuestionIfOptional` puts the token's location.
- The three existing "is this JS parameter optional" approximations in
  `signatures.rs`, `symbols.rs` and `optionality.rs` are not yet moved onto
  the query; they are other lanes' files. Moving them is the next step and is
  reported to the integrator (lane notes §3).

## What is not built

- `@satisfies` (`KindJSDocSatisfiesTag` arm): no checker query yet;
  `checkJsdocSatisfiesTag*` stay wrong. The query shape it needs is "the
  satisfies type wrapping this expression", read where `checkExpression`
  dispatches `SatisfiesExpression`.
- The parenthesized-`@type` cast is answered by `flow.rs`'
  `is_jsdoc_type_assertion`; it is not routed through a shared query.
- `@this` / `@return` / `@template` read their own way in `signatures.rs`;
  the replay tracks them only for the order state.

## How we would know we were wrong

- A case where a `@param`, `@type` and `@template` mix on one comment gives a
  different signature through `jsdoc_reparsed_function` than through the
  older per-caller roads. That is the order rule drifting, and the fix is to
  move the caller onto the query, never to special-case the comment.
- Profiling a JS-heavy project (none is in `benches/` today) showing the
  replay above a few percent of check time: then the answer is a per-host
  memo keyed by the function `NodeId`, owned by the checker, filled on first
  query — not option (a).
- The checker-side JSDoc reads growing rather than shrinking as lanes land.
  That means the query is not being used and the flag day for (a) is getting
  more expensive; revisit before it does.
