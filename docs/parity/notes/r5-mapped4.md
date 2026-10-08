# r5-mapped4 — mapped printing, node reuse, mapped iteration (`tsr-2zk.1033`)

Round-5 cloud lane, successor to r5-mapped3 (`r5-mapped3.md`). Owns
`crates/tsr-checker/src/mapped.rs`, `intersections.rs` (printing paren rules),
`node_reuse.rs` and `printing::prints_as_a_single_token`. `declared.rs`
(r5-declared) and `relater.rs` (r5-relater6) changes ship as measured diffs.
Native source is `vendor/typescript-go` @ `5b1047d`.

Baseline frozen at `ccb48e7` (`integrate: merge …-r5-vardecl`), before batch
W's enum-keys diff reached the integration branch: types 544,166 RIGHT /
988 GAP / 7,379 WRONG of 552,533 aligned lines; diagnostics 5,368 RIGHT +
5,584 EMPTY_RIGHT of 12,238 rows.

## 1. Intersection constituents parenthesised by node precedence (committed)

`emitTypeNode(node, TypePrecedenceIntersection)` (`printer.go:2274`) wraps a
constituent whose node binds below `Intersection` (`ast/precedence.go`:
`Conditional < JSDoc < Function < Union < Intersection`). `create_intersection`
decided by type kind instead: a union without an alias symbol, or a
signature type. Two kinds came out wrong:

- **A union printing a non-union origin.** getIndexType attaches
  `origin = newIndexType(t)` to a key union; typeToTypeNode prints the origin,
  a `TypeOperator` node, which is never wrapped. The port wrapped it:
  `P & (keyof NameMap)`. The same holds for an alias-spelled distributed
  union (mapTypeWithAlias, a `TypeReference`) and an intersection origin.
- **A conditional constituent** binds below `Intersection` and printed
  unwrapped: `T extends C ? number : string & string`.

The constituent's node kind exists in this port only as its printed text (a
union's text *is* its origin's print, `unions.rs` `create_union_with_text`), so
the precedence is read off the text by `node_reuse::text_precedence`, the
reader the node-reuse fallback already uses for the same question
(`binds_below_intersection`).

**Alternative rejected.** Recording the origin's kind on the union at mint
time is more direct, but the `keyof` origins are minted in `declared.rs`
(`keyof_origin_applies` callers) and `unions.rs`' `union_with_origin_text`
takes only text; both are outside this lane. If the origin ever becomes a
type rather than text (`tsr-2zk.16.99`'s operand work), the kind should be
read from it and the text reader retired here.

**Measured** against the frozen base, both dumps unfiltered: types +3 RIGHT
(`distributiveConditionalTypeConstraints:82/93`, `mappedTypeAsClauses:103`),
diagnostics unchanged, zero losses on both. On top of r5-mapped3's
declared-route diff it clears that diff's 13 `(keyof X)` losses
(`reverseMappedTypeIntersectionConstraint` ×10, `mappedTypeAsClauses:101/102`,
`reverseMappedTupleContext:45`).

**Falsifier.** A printed constituent whose text the reader misclassifies:
a text with a top-level `|`, `&`, `=>` or ` extends … ?` that is not that
node kind. Type texts this port prints put such tokens at depth zero only in
the node kinds they name.

Full parity run (`coverage`) after: checker_types 8,236 of 9,538
(configured 1,643 of 1,928); diagnostics 4,531 of 5,502 (configured 837 of
1,089). Perf, median child CPU new/old: domain-model 0.982 (21 samples),
generic-imports 1.033 at 21 samples, 0.989 at 41; `diagnostics_match: true`.
Callgrind Ir (`--singleThreaded --pretty false`): domain-model 1,201,918,429
→ 1,201,832,316, generic-imports 343,420,545 → 343,407,345 (both −0.01%).
