# JSDoc

**Where:** `crates/tsr-scanner/src/jsdoc.rs` (tokenizer),
`crates/tsr-parser/src/jsdoc.rs` (parser).
**Upstream:** `internal/parser/jsdoc.go`, plus the JSDoc entry points in
`internal/scanner/scanner.go`.
**Decision record:** [ADR-0008](../adr/0008-jsdoc-parsed-eagerly.md) on why parsing
is eager here and lazy upstream.

## The shape of the problem

JSDoc is not in the grammar. A `/** … */` is a block comment, which the scanner
discards as trivia like any other, and TypeScript then reads structure out of the
discarded text. So there are two languages in one file, and the parser has to
switch between them mid-stream and switch back exactly.

Three things follow, and they drive the whole design.

**Detection has to be free.** The overwhelming majority of nodes have no JSDoc —
755 in the 4.5 MB `tests/cases/compiler` corpus. Any per-node work to find out
would cost more than the parsing itself. So the scanner, which is already walking
the trivia, sets `TokenFlags::PRECEDING_JSDOC_COMMENT` while it is there. The
parser's test is a bit test on the current token.

**The lexical rules differ.** Inside a comment, whitespace and newlines are
*tokens*, not trivia — JSDoc is layout-sensitive, and the parser has to see the
gaps to know where a tag ends and what a continuation line's margin is.
Identifiers admit `-`, so `@my-custom-tag` is one name. Punctuation is
single-character: `=>` is `=` then `>`, so that `{function(): void=}` can put the
`=` on its own. And anything that is not punctuation is prose, returned in runs as
`JSDocCommentTextToken`.

**But type expressions use the *ordinary* rules.** `@param {Record<string, T[]>} x`
contains a real TypeScript type, and reimplementing the type grammar for JSDoc
would be a second implementation to keep in sync. So the type parser is re-entered
in place.

## How the scanner is retargeted

Upstream slices `sourceText` down to the comment body and calls `SetText`. We
cannot: our `Scanner<'a>` borrows the source, and the `&'a str` tokens it produces
are handed straight into the tree, so re-pointing it at a subslice would either
break the lifetime or make token text point at a temporary.

Instead the scanner grows a `limit` field, and `Scanner::set_range(start, end)`
narrows the window. `rest()` slices up to `limit`, so every "is there a character
here?" test in the scanner treats the limit as end-of-file without knowing it
exists. The parser sets the window to `[comment.start + 3, comment.end - 2)` —
past the opening `/**`, short of the closing `*/` — and the closing delimiter is
then simply unreachable.

`ScannerState` captures the window along with the position, so the ordinary
`save`/`restore` pair puts everything back.

## Handing off to the type parser, and back

This is the part that was wrong first and is worth stating precisely.

Going *in*: the `{` must be consumed **by moving the scanner**, not by advancing
the JSDoc token cursor. `eat_jsdoc(OpenBraceToken)` would consume the brace and
also scan the *following* token under JSDoc rules — after which `next_token()`
scans a second one, and the type parser starts one token late.

That bug was invisible for a while. JSDoc diagnostics are discarded (below), and a
type parsed one token late is still *a* type, so `@typedef {number} Point` merely
came out with a slightly wrong tree and no complaint. What surfaced it was a test
asserting the parsed type's **source span** against the type as written; a test
asserting only "there is a `@typedef` tag with a type" passed throughout.

The correct sequence is: note whether a `{` is present, rewind the scanner to just
past it, then `next_token()` to rescan under ordinary rules.

Going *out*: the type parser stops on the token after the type, which it scanned
with ordinary rules. That token has to be re-read as a JSDoc token, so the parser
rewinds to its start and calls `scan_jsdoc_token`.

Leading `*` is suppressed for the duration, via a *counter*
(`set_skip_jsdoc_leading_asterisks`), because a type may span continuation lines
whose decoration is not part of it. A counter rather than a flag because the
regions nest.

## Diagnostics are discarded

Everything the parser objects to inside a comment is dropped:

```rust
self.diagnostics.truncate(saved_diagnostics);
```

In a `.ts` file the comment is not part of the program's syntax, so reporting on
its contents would turn a malformed comment into a compile error. Upstream routes
them into a separate `jsdocDiagnostics` list consumed only for `.js` files; we have
no consumer for that list yet, so they are dropped rather than collected. When the
`.js` path arrives, they should be collected instead of discarded — that is the
same line of code either way.

The cost of this is that JSDoc bugs are silent, which is exactly how the off-by-one
above survived. Tests here must assert against the parsed tree, not against the
absence of diagnostics.

## Attachment

Parsed comments go into a side table keyed by `NodeId`
(`JSDocTable`), per [ADR-0003](../adr/0003-tree-plus-side-tables.md) — not a field
on each node, which would cost every node a pointer for something a small minority
uses.

The read happens at the head of a construct and the attachment after it, because
the comments precede the node that does not exist yet:

```rust
let docs = self.parse_leading_jsdoc();
let statement = self.parse_statement_worker();
self.attach_jsdoc(statement.into(), docs);
```

Four constructs do this: statements, class members, type members, and parameters.
That is where TypeScript allows documentation.

Reaching a `NodeId` through the `Node` union needs a match over all 192 variants,
so `Node::node_id()` is generated (`xtask/src/gen_nodes.rs`) rather than
hand-written — a hand-written one would rot the next time `ast.json` grows a node.

Adding it turned up that `Token` nodes were being pushed into the `NodeTable` and
their ids thrown away, so a token in the tree could not be found in the side
tables. `Token` now carries a `node_id` cell like every other node, and is
deliberately no longer `Copy`: a `Copy` token would silently duplicate an id that
identifies one position.

## Where we differ from upstream

**`/** * @type */`.** Upstream's comment says the initial `SawAsterisk` state
exists "so that `/** * @type */` doesn't parse". Its code does not do that: the
second asterisk becomes prose, but control still reaches the `@` in the ordinary
way and builds the tag. We follow the code, per
[ADR-0006](../adr/0006-conformance-oracle.md) — asserting the comment's claim would
be asserting documentation rather than behaviour. The test says so explicitly.

## Not built

- **The reparser** (`internal/parser/reparser.go`, 748 lines). In a `.js` file,
  `@type` and `@param` are *promoted* into real type annotations on the tree, so
  the checker sees an ordinary typed program. That is the whole of JSDoc's
  semantics for JavaScript and belongs with the checker; parsing is a prerequisite,
  not a substitute. Filed as a `bd` issue under the parser epic.
- **`@typedef` with nested `@property` tags**, where following tags synthesise a
  type literal. Needs the reparser's machinery for the same reason.
- **`@import { Foo } from "./types"`** — the tag is not parsed at all; there is no
  `"import"` arm in `parse_tag`'s dispatch. **§218.** Worth ~7 `checker_types`
  cases, all shaped `want Foo, got any` on a `@param { Foo } foo` that the tag was
  supposed to bring into scope (`conformance/importTag1` and `importTag3` opened;
  the first names the type, the second imports a default).

  The measurement that matters is **which half is missing**, because the parse
  half looks like the job and is not. `parseImportTag` (`parser/jsdoc.go:940`)
  needs four things, and `module.rs` already has three of them:
  `parse_module_specifier` (`module.rs:587`), `parse_import_attributes`
  (`module.rs:492`), and the `ImportClause` construction inlined at
  `module.rs:61-76` — which is `tryParseImportClause` under another name and
  wants extracting rather than writing. So the parse is an afternoon.

  The cost is entirely downstream: the tag does nothing until the **reparser**
  turns it into a synthetic `JSImportDeclaration` and appends that to the
  statement list (`reparser.go:119-133` — deep-clone the clause, force
  `PhaseModifier = KindTypeKeyword`, push to `reparseList`), which is the
  first entry in this list. Neither `binder.go` nor `checker.go` mentions
  `KindJSDocImportTag`; they only ever see the reparsed declaration. Both
  witnesses are additionally **two-file** fixtures, so cross-file resolution is
  a second prerequisite.

  Building the parse alone therefore converts **zero** cases while adding a node
  the binder ignores and the printer must now emit — a strictly negative trade.
  Do the reparse list first.
- **`@callback` and `@overload` signatures** — the tags parse, but
  `JSDocSignature` is not built.
- **JSDoc-only type syntax**: `*` (`JSDocAllType`), `?T`, `!T`, `T=`, `...T`. These
  are `.js` conveniences; the nodes are generated and unused.

## What to check when it breaks

`cargo run -p tsr-parser --example jsdoc_probe -- <file>` dumps every documented
node with its tags, and prints each type by its **source span** — so a
mis-positioned handoff shows up as the wrong text rather than as a plausible-looking
tree.

`cargo run --release -p tsr-parser --example jsdoc_cost -- <dir>` measures what
eager parsing costs on a given corpus, which is the falsifier named in
[ADR-0008](../adr/0008-jsdoc-parsed-eagerly.md).
