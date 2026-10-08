# ADR-0049: `SymbolTable` iterates in insertion order

- **Status:** Accepted. Built: `tsr_binder::SymbolTable`
  (`crates/tsr-binder/src/symbol_table.rs`), replacing
  `type SymbolTable<'a> = FxHashMap<&'a str, SymbolId>`.
- **Date:** 2026-10-08
- **Issue:** `bd tsr-2zk.1039` (r5-binperf §6.1's refused pre-sizing), lane
  r5-symtab.
- **Measurements:** [`docs/parity/notes/r5-symtab.md`](../parity/notes/r5-symtab.md).

## The forcing constraint

The binder's symbol tables (a symbol's `members` and `exports`, a
container's `locals`, `globals`, `global_exports`) were
`FxHashMap<&str, SymbolId>`. On generic-imports their inserts cost 7.6 M Ir,
4.7 M of it `reserve_rehash` — growth of 9,895 tables, most of them a small
member table's first allocations (r5-binperf §1, §6.1). Every table's final
size is cheaply known or bounded by the node that declares into it (a class
or interface's member list, a block's statements), so the obvious lever is
to pre-size. r5-binperf refused it without a build, for a reason that holds:

**A hash map's iteration order is a function of its capacity history.**
hashbrown places a key at the first free slot of its probe sequence, which
depends on the bucket count at insertion time and on which colliding keys
were already present; a rehash re-inserts in old-table order, not insertion
order. So a table pre-sized to its final bucket count and one grown to it
can iterate the same keys differently. `FxHash` is unseeded, so the order
was deterministic — and therefore *observable*: the checker iterates these
tables, and any site that kept the order let it into output. Choosing a
capacity was a behaviour change at every such site.

## What upstream does, and what that implies

Upstream's table is `ast.SymbolTable`, `map[string]*Symbol`
(`internal/ast/symbol.go:45` @ `5b1047d`). Go randomizes map iteration, so
tsgo can rely on no order, and where output is ordered it establishes the
order itself. Audited over `internal/checker` and `internal/binder`
(r5-symtab §2): **33 range-over-`SymbolTable` sites**; 7 sort afterwards
(`getNamedMembers` `checker.go:22049` and every other sort use
`sortSymbols`/`compareSymbols`, `checker/utilities.go:362-391`: first
declaration's position, then name, then symbol id), 20 are
order-insensitive (lookups, any/all, per-key merges, diagnostics — which
`DiagnosticsCollection` sorts on read), none uses an ordered collection, and
6 are order-sensitive and unsorted (language-service results, the TS2460
`findInMap` choice, `getInferTypeParameters`).

`compareSymbols` orders by first declaration, and the binder inserts a name
when it binds that name's first declaration. Within one file and one
container, **insertion order is declaration order**, i.e. the order
upstream's sorts reconstruct and the order TypeScript's own `Map`-based
`SymbolTable` iterated in. They differ only where a table's first insert is
not its earliest declaration in program order: cross-file merges into
`globals`, late-bound or checker-built members, and `mergeSymbolTable`
targets.

## The alternatives

**(a) An insertion-ordered table** — chosen. A `Vec` of `(name, symbol)` in
insertion order, plus an open-addressing index of `u32` slots (8-bit hash
tag, 24-bit position) built only above 8 entries; up to 8 a lookup compares
names linearly. Iteration order is a function of the insert sequence alone,
so capacity is free. `insert` of a present name replaces in place, `remove`
shifts (the binder removes rarely; JS `Map.delete` keeps the order of the
rest too).

**(b) Keep `FxHashMap`, pre-size it, and sort at every iteration site where
order is observable, as tsgo does.** Rejected for this port now:

- It is a behaviour change at every observable site, in checker files that
  other lanes are editing (`members.rs`, `declared.rs`, `relater.rs`,
  `symbols.rs`, `nonexistent_property.rs`, `index_signatures.rs`), and each
  sort must be the *right* one (`compareSymbols`'s keys, the
  class/interface own-members-first partition of `getNamedMembers`), i.e.
  a port of each site's upstream ordering rather than a mechanical edit.
- An unsorted site that is missed stays silently capacity-dependent, which
  is exactly the hazard `docs/conventions.md` records for object spread
  (an `FxHash` fixture that happened to hash into declaration order).
- Sorting costs `O(n log n)` per call at sites that are hot (property-name
  collection runs per structured type).

What would make (b) win: the checker porting `getNamedMembers` and the other
sorts faithfully at every site (they are upstream's real ordering points,
and (a) does not remove the need for them where insertion order and
declaration order differ). Once every observable site sorts, the table's
order stops mattering and either representation serves.

**(c) Other representations, considered.**

- *A sorted `Vec` keyed by name* (binary search): alphabetical order is not
  upstream's order at any site; it would move every unsorted site away from
  tsgo.
- *`indexmap`*: the same design as (a), but a new dependency is a
  `Cargo.lock` and offline-vendoring change (`scripts/offline-cargo`), and
  its `IndexMap` is three words plus a hasher; (a) is 32 bytes, the size of
  the map it replaced (asserted by a unit test).
- *`FxHashMap` plus a side `Vec` of insertion order*: two copies of every
  key and both costs of growth.

## Consequences

- **Order changes, measured.** Against the hash-ordered base, both
  conformance dumps: `diagverdictdump` byte-identical; `verdictdump` differs
  in 13 lines, every one a reordering of the same member set and every one
  toward tsgo's order — 3 lines WRONG → RIGHT (`mappedTypes1` ×2,
  `reverseMappedTypeAssignableToIndex`), 10 `mappedTypeRecursiveInference`
  lines still WRONG but with their in-order agreement with tsgo up from
  12/34 to 33/34, 56/271 to 225/271 and 36/187 to 128/187. No line was
  lost. CLI output on the bench projects is byte-identical (r5-symtab §4).
- The 5 checker sites that let table order into output
  (`nonexistent_property.rs` `collect_property_names`, `members.rs`
  `property_names_of_type` ×2, `symbols.rs` `report_missing_module_export`,
  `index_signatures.rs` `late_index_properties`) now follow declaration
  order instead of hash order. That is closer to upstream's
  `compareSymbols` order but still not a port of it: where a table's
  insertion order and its symbols' declaration order differ (merges
  across files), those sites remain wrong. They are recorded, not fixed
  here (r5-symtab §3).
- Insertion order must now be reproducible: a table's order is the binder's
  insert sequence, so the parallel bind (`publish_file`) must replay the
  serial sequence. `parallel_tests.rs` now compares member, export, local
  and global tables **in order** rather than as sets.
- A lookup in a table of more than 8 names hashes the name and probes an
  index behind one extra pointer (`Option<Box<Vec<u32>>>`), so that the
  table stays 32 bytes; at most 8 names it is a linear scan. Removal is
  `O(n)` and rebuilds the index.
- `PartialEq` stays order-insensitive, as the map's was.

## How we would know this was wrong

- **A table whose insertion order is not deterministic.** If any code path
  inserts into a binder table from concurrent work (checker-time insertion
  under the multi-threaded checker, or a parallel bind that publishes out
  of program order), outputs would vary run to run. The interleaved runs
  compare stdout on every sample, and `parallel_tests.rs` compares order.
- **A site where hash order happened to be tsgo's and insertion order is
  not.** The dumps found none on the corpus; a later dump that loses a line
  when only table order changed falsifies the "toward tsgo" claim.
- **Lookup cost.** If the linear scan or the boxed index makes the checker's
  name resolution measurably slower (callgrind `position` Ir, or
  domain-model's checker-bound wall), the threshold of 8 or the boxing is
  wrong, not the ordering decision.
