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
- **`@typedef {Object}` with nested `@property` tags.** Complete inline type
  literals now enter the normal alias-body checker seam, and sibling `@template`
  tags supply that alias's type parameters; inline `T` and `T[]` members are
  consequently instantiated by the ordinary alias mapper. Alias names are
  retained only for top-level JSDoc typedefs, matching
  `determineIfDeclarationIsVisible`'s JSDoc arm — this is lexical host
  visibility, not ordering by source offsets. Upstream instead gathers the
  property tags under a synthetic `JSTypeAliasDeclaration`; in this immutable
  AST they remain flat siblings with no lexical parent. A correct direct-port
  replacement must resolve every property annotation under the alias mapper so
  `T`, `T[]`, nested object types, and alias references all see the same scope.
  A member-lookup adapter for bare `T` would only conceal that missing ownership.
  Optional properties also need both exact and non-exact optional controls when
  this is built.
- **JSDoc template modifiers are parsed, not enforced as variance.** The parser
  follows `parseTemplateTagTypeParameter`/`parseModifiersEx`: `in`, `out`,
  `in out`, and `const` precede the actual parameter name. The checker currently
  consumes the parameter identity for typedef aliases; variance checking remains
  part of the broader generic relation implementation.
- ~~**`@import { Foo } from "./types"`** — the tag is not parsed at all …
  Do the reparse list first.~~ **BUILT — §269 (superseding §218), 2026-08-13,
  +22 cases / 228 W→R.** §218's finding that the parse alone converts zero
  cases was correct and is honoured: §269 landed the parse, the binder arm,
  the loader collection, and the checker arms as ONE commit. What §218 got
  wrong was transplanting upstream's *sequencing* — "do the reparse list
  first" priced the consumer as a 748-line reparser, but this port never had
  a reparser: `bind_jsdoc_declarations` binds JSDoc tags directly (`@typedef`
  was the precedent), so the consumer was three small arms, not a subsystem.
  Five pieces, each named in the commit: the `parse_tag` arm bridging to
  `module.rs`'s clause grammar; the scanner's `skipJSDocLeadingAsterisks`
  counter, which had been WRITE-ONLY since §110 (multi-line clauses and types
  never actually skipped the decoration `*`); the binder's alias
  declarations (namespace imports excluded — the §219 fence); the loader
  collecting the tag's specifier for module resolution; and the checker's
  `jsdoc_hosts` doc→host bridge, after a draft that parented JSDoc into the
  tree moved unrelated lines in three fixtures. `@import { Foo as F }`
  (renames) and `@import * as ns` remain declined — both are the alias-name
  printing wall (`bd tsr-e2u`).
- **`@callback` and `@overload` signatures** — `@callback` remains an unknown tag
  rather than being consumed through the typedef path, and `JSDocSignature` is
  not built. Callback parser/signature work therefore stays independent of the
  typedef alias-body bridge.
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
