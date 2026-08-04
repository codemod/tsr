# The printer (`tsr-printer`)

**Status:** slice 3 of Phase 3.5 (`bd tsr-49v.4`), at **99.52%** on the round-trip
gate. Not yet an emit printer — see "What this is not" below.

> **Slice 4 pointed a byte comparison at this printer and found four defects the
> round trip could not see.** They are listed in
> [declaration-emit.md](declaration-emit.md#what-the-round-trip-gate-could-not-see)
> rather than duplicated here, but the shape is worth carrying in this document,
> because it is the sharpest available answer to "what does a structural gate miss?"
> — a leading blank line on every file, `{}` where upstream writes two lines,
> `export {  };`, and `typeof c1 .foo`. All four parse identically to the correct
> output. All four survived 11,726 cases. None is in the tree.

**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`.
**Upstream counterpart:** `internal/printer/printer.go` (6,280 lines; 14,827 for
the package with tests).

> **Corrected twice, so the history is worth stating.** This document first
> claimed the printer was "a faithful port under ADR-0001's default". Review found
> that false — it had been written against the AST node definitions with
> `printer.go` consulted only for its function list — and the claim was retracted.
> It has since been **made true**: the writer and the `ListFormat` table are ported,
> every child list goes through `emit_list`, and all 128 dispatch points name the
> upstream `emitX` they port. The deviations are enumerated in the crate docs
> rather than left implicit.

## What was ported, and what was not

| Upstream | Here |
|---|---|
| `textWriter` (`textwriter.go`) | `writer::TextWriter`, deferred indentation included |
| `ListFormat` `LF*` table | `list_format::ListFormat`, one-for-one |
| `emitList` / `emitListItems` / `writeDelimiter` | `Printer::emit_list` and friends |
| central `switch node.Kind` → 284 `emitX` | a `match` per category, each arm anchored |
| comments, source maps, precedence table | **not ported** — see the deviations below |

Four deviations, each deliberate: the separator guard (upstream places spaces by
hand, and porting that faithfully produced `1.toString()` for `1 .toString()`); no
comments or source maps; `PRESERVE_LINES`/`PREFER_NEW_LINE` degraded to single-line
because they consult original positions; and no precedence table, because
`ParenthesizedExpression` is in the tree.

## The gate, and why it needs no baselines

```
printer_round_trip   11,670/11,726   99.52%
```

Parse → print → reparse → compare trees. The printer is correct when the *tree*
survives, not when the text is reproduced. Two consequences, both deliberate:

- **The denominator is the whole corpus.** No baselines to acquire, and 11,726
  cases judged rather than the 1,289 that happen to carry emit output.
- **The suite can only sit at 100%**, like `scanner_termination`. The property is
  intrinsic; there is nothing external to disagree with.

Formatting is therefore free. Quote style, spacing and line breaks are not in the
tree, so the printer chooses them. Phase 5's emit baselines will constrain all
three; this does not.

## What this is not

It is **not** an emit printer. Upstream's removes redundant parentheses, preserves
comments, tracks source maps, and reproduces TypeScript's exact formatting. None of
that is needed to keep a tree intact, so none of it is here. In particular:

- **Comments are dropped.** They are trivia, not nodes.
- **Parentheses are preserved rather than recomputed.** TypeScript keeps
  `ParenthesizedExpression` in the tree, so printing children in order reproduces
  the grouping for free. The precedence table that decides where to *add*
  parentheses — a large part of upstream's printer — is Phase 5's problem.

## What "compare trees" means, and the gap that was measured

Full structural equality over 192 node types needs a generated comparator. The
suite compares a two-part fingerprint instead.

The first part is a pre-order walk emitting each node's kind plus the payload that
distinguishes nodes of the same kind — identifier and literal text, and the
`const`/`let` flags that are not in the tree at all.

The second part exists because **the walk alone is provably insufficient**, and
this is the most useful thing this document records. Of the 36 optional
`…_token` fields declared across the generated nodes, `walk_node` traverses
**zero**. A `?` on an optional property, a `!`, a `*` on a generator, a `?.` — each
is a node the parser allocated that the walk never reaches. A printer that turned
`a?.b` into `a.b` would produce an identical walk fingerprint, reparse cleanly, and
pass.

So the second part counts those token kinds across the whole `NodeTable`. It is
deliberately *not* a histogram of every kind: the table also holds JSDoc, which is
trivia, and counting it failed 455 cases for dropping identifiers that existed only
inside comments.

What still escapes both is a change preserving the kind sequence, every payload,
*and* the token counts — a reordering of same-kind siblings, say. Nothing in a
printer that walks children in order produces that. A generated structural
comparator would close it and belongs with Phase 5.

## Why a parse error in the input is a skip, not a failure

718 cases are skipped because their input does not parse cleanly. The corpus is a
*compiler* test suite, so it contains deliberately malformed syntax. Printing a
tree built by error recovery and demanding it reparse identically is not a property
the printer owes anyone — recovery invents nodes that have no source text.

The skip is applied to **every unit before any is printed**, which matters more
than it looks: doing it lazily made the denominator depend on the printer. A case
whose first unit was unsupported and whose second does not parse counted as judged
until the printer learned the first unit, then silently became a skip. A
denominator that moves when the component under test improves is the shape of a
measurement bug, not of progress.

## The traps, in the order they were found

Each cost real accuracy and none is guessable from the node definitions. The
percentage after each is the round-trip rate once it was fixed.

| Trap | Why it bites | Rate |
|---|---|---|
| Starting point | — | 87.09% |
| Several nodes carry a `kind` **field** that is a punctuation token, not the node's kind — `BindingPattern.kind` is the opening bracket. Dispatching on it printed every object pattern as an array. | Use `NodeTable::kind` instead. | 91.58% |
| `TemplateHead.raw_text` is the **whole token**, backtick and `${` included. Wrapping it again produced `` ``abc${${ ``. | The cooked `text` needs escaping; the raw text needs none. | 93.82% |
| `PrivateIdentifier.text` already carries its `#`, so prefixing gave `##a`. | — | 95.28% |
| JSX was unimplemented (329 cases). `JsxText` is a node, so JSX must not be reformatted at all. | — | 97.94% |
| `namespace A.B {}` nests two `ModuleDeclaration`s. Printing the inner one as a statement restated its header: `namespace A. export namespace B {}`. | Print only the inner *name*. | 98.69% |
| `declare global {}` carries **both** a `GlobalKeyword` and an identifier `global`; writing each gave `declare global global`. | — | 99.45% |
| Import attributes (`with { type: "json" }`) were dropped. Dropping them still produces valid syntax — just a smaller tree. | Only a round trip catches this class of bug. | 99.68%* |
| \* then the token histogram was added, which corrected the number down to **99.52%** by catching what the walk could not see. | | 99.52% |

The last row is worth reading twice. The rate went *down* when the gate got
stronger, and that number is the honest one. A gate that only ever moves upward is
not measuring itself.

## The separator guard, and the hole it had

`write` inserts a space when the last character written and the first about to be
written could scan as one token. That is the design's safety net, and it was
bypassed at **123 of its call sites**: an earlier `write_raw` skipped the check on
the grounds that a brace, a dot or a bracket "obviously" cannot merge.

`1 .toString()` printed as `1.toString()`, which does not parse — and
`would_merge` already had the digit-then-dot rule that would have caught it.

All non-JSX emission now goes through the checked writer; `write_raw` survives for
JSX alone, where the bypass is required rather than convenient because `JsxText` is
a node and an inserted space would change the tree. The corpus rate did not move,
which is the point worth recording: **11,726 cases contained no instance of the
bug**, and it was found by probing constructs by hand. A gate with a big
denominator is not the same as a gate that covers the space.

## Mutations

| Mutation | Rate |
|---|---|
| Token-merge separator removed (`a` `+` `+b` → `a++b`) | **27.01%** |
| `const`/`let` printed as `var` | 97.45% |
| Optional-property `?` dropped | 98.51% |
| — baseline — | 99.52% |

The third is the one that justifies the token histogram: before it existed, that
same mutation moved the rate by **zero**. It was a real defect the gate could not
see, and it was found by mutating rather than by reasoning about the fingerprint.

## What is left

56 failures, no bucket larger than eight, plus seven cases reporting an unsupported
`NoSubstitutionTemplateLiteral` reached through a path the expression printer does
not cover. The residue is filed under `bd tsr-49v.4`.

Separately, the emit gate (`dts_emit`) now constrains formatting that the round
trip leaves free, and its residue includes two printer choices that were correct
under this gate and are wrong under that one: module specifiers are re-quoted with
double quotes where upstream reproduces the source text through
`getLiteralTextOfNode`, and comments are not emitted at all. Both are recorded in
[declaration-emit.md](declaration-emit.md#known-approximations).
