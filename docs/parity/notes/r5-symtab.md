# Parity lane `r5-symtab` — the binder's symbol-table representation (round 5)

Lane: perf box r5-symtab on epic `tsr-2zk`, picking up r5-binperf's refused
item ([`r5-binperf.md`](r5-binperf.md) §6.1, `tsr-2zk.1039`): symbol-table
growth, the largest binder cost left on generic-imports. Box protocol:
`docs/parity/box-protocol.md`. Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Decision record: [ADR-0049](../../adr/0049-symbol-table-insertion-order.md).
Release target (CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on equivalent
complete work.

Sections are numbered so code can cite them (`r5-symtab.md` §N). Every
number names the source state and the measurement it came from.

## §1 Method

- Source base `f2ca0b2`: this branch after merging
  `claude/beautiful-shannon-ar5gh0` (`f4fb673`) and r5-binperf's branch
  (`064eba1`, carrying `0e06bef`/`484a6e4`/`339d279`, not yet on the
  integration branch at dispatch). 4-vCPU cloud container, Linux 6.18, glibc
  `malloc`, `RUSTUP_TOOLCHAIN=stable`; native tsgo built from the pinned
  submodule (`scripts/offline-cargo/build-tsgo.sh`).
- **Ir**: `valgrind --tool=callgrind`, release `tsr`,
  `-p benches/projects/<p>/tsconfig.json --singleThreaded --pretty false`.
- **Wall/CPU/RSS**: fresh processes, base / new / tsgo interleaved with the
  order rotated each round, one warmup round dropped, medians of wall,
  `wait4` user+sys and `ru_maxrss`, with `whole_project_perf.py`'s flags
  (`--noEmit --incremental false --composite false --pretty false`,
  default multi-threaded mode). Base and new stdout are compared on every
  run. Wall noise on this box: two copies of the *same* base binary in one
  interleaved session differed by 1.9% (domain-model, single-threaded, 31
  rounds), so a wall ratio inside ±3% is not a signal.
- **Dumps**: `diagverdictdump` and `verdictdump`, unfiltered, compared with
  `cmp` and the protocol's loss joins.

## §2 Where tsgo establishes symbol order

`ast.SymbolTable` is `map[string]*Symbol` (`internal/ast/symbol.go:45`); Go
randomizes map iteration. Inventory of every `for … range` over a
`SymbolTable` in `internal/checker` and `internal/binder` (non-test):
**33 sites**.

| class | count | sites |
|---|---:|---|
| sorted afterwards (`sortSymbols`/`compareSymbols`, or a chain sort) | 7 | `getNamedMembers` ×2 (`checker.go:22059`, `:22066`), `expandModuleDecl` (`nodebuilder_hover.go:415`), export-alias naming (`nodebuilderimpl.go:785`), `getContainersOfSymbol` (`symbolaccessibility.go:146`), `getAliasForSymbolInContainer` (`:361`), `getSymbolTableAliases` → `trySymbolTable`'s `compareSymbolChains` sort (`:521`, `:584`) |
| order-insensitive (lookup, any/all, per-key merge, diagnostics) | 20 | e.g. `initializeChecker`'s global merges (`checker.go:1308`, `:1322`), `mergeSymbolTable` (`:14110`), `getExportsOfModuleWorker` (`:16159`–`:16236`), `checkUnusedLocalsAndParameters` (`:7143`) |
| ordered collection | 0 | none; `collections.OrderedMap`/`OrderedSet` appear only for widening contexts and union properties (`checker.go:18464`, `:21455`) |
| order-sensitive, unsorted | 6 | `getInferTypeParameters` (`:23796`), `GetAmbientModules` (`:15547`), `findInMap` for TS2460 (`utilities.go:40` via `checker.go:14923`), `symbolsToArray` (`utilities.go:1637`), `ForEachExportAndPropertyOfModule` (`services.go:126`, `:147`) |

Diagnostics never depend on report order: `DiagnosticsCollection`
(`ast/diagnostic.go:172`) sorts them on read. The ordering point for
properties is `getNamedMembers` (`checker.go:22049`): own members of a class
or interface first, then the rest, each partition sorted by
`compareSymbols` (`checker/utilities.go:366`): first declaration's position
(`compareNodes`), then name, then symbol id. `getSpellingSuggestionForName`
ties break on `compareSymbols` too (`core/core.go:567`), so it is
order-free.

## §3 The port's iteration sites

Counted by the compiler, not by grep: with every iteration API of the new
type temporarily `#[deprecated]` and both `IntoIterator` impls disabled,
`cargo check --workspace --all-targets` lists **59 sites in
`crates/tsr-checker/src`** that iterate a binder table (r5-binperf's "97"
was a pattern grep that also matched checker-owned maps; corrected here).
Classified by reading each:

- **53 order-insensitive**: sorted by `compare_symbols`/`compare_symbols_key`
  (`checker.rs:3893`) or a `(declaration, name)` key, `any`/`all`, unique-key
  hits, spelling suggestions with a fixed tie-break, diagnostics (sorted by
  `compare_diagnostics`, `tsr-compiler/src/program_diagnostics.rs:234`).
- **5 order-observable** (output followed table order, i.e. hash order before
  this lane):
  1. `nonexistent_property.rs:1958` `collect_property_names` (via
     `property_names_of`): member order of homomorphic mapped types over a
     named interface (`mapped.rs:550`), of object types built in
     `objects.rs:544` and `widening.rs:238`.
  2. `members.rs:3435` `property_names_of_type`, namespace/enum/expando
     exports: relater walk order (first failing property, "missing the
     following properties" lists).
  3. `members.rs:3412`, the same for CommonJS `module.exports`.
  4. `symbols.rs:2352` `report_missing_module_export`: the `exported as`
     name in TS2460 when one local has several export names (upstream:
     `findInMap`, itself map-ordered).
  5. `index_signatures.rs:878` `late_index_properties`: printed order of
     index-info components.
- **1 unsure**, `declared.rs:8286` `evaluate_conditional_inference`: infer
  type parameters in `locals` order (upstream's `getInferTypeParameters`,
  `checker.go:23796`, also map-ordered); the result should not depend on it.

Sites 1–3 are where upstream sorts (`getNamedMembers`). Insertion order is
declaration order within one container, so the new table moves them toward
upstream without porting the sort; a faithful port is still owed where
declarations merge across files (not done here: those files belong to
active lanes).

Table sizes at bind end (scratch probe, not committed), generic-imports /
domain-model:

| table | tables | of which <= 8 names | entries | largest |
|---|---:|---:|---:|---|
| members | 3,603 / 4,333 | 3,316 / 4,007 | 12,377 / 15,016 | 5 tables over 256 |
| locals | 6,928 / 8,777 | 6,875 / 8,683 | 14,313 / 17,673 | one table of 2,022 names |
| exports | 17 / 99 | 10 / 51 | 266 / 832 | |
| globals | 1 | | 2,229 | |

92% of member tables and 99% of locals tables hold at most 8 names (empty
ones included), which is what the linear-scan range covers.

## §4 Step 1: the insertion-ordered table (`SymbolTable`)

**Change.** `type SymbolTable = FxHashMap<&str, SymbolId>` becomes a struct
(`crates/tsr-binder/src/symbol_table.rs`): `Vec<(&str, SymbolId)>` in
insertion order plus, above 8 names, an open-addressing index of `u32`
slots (8-bit hash tag, 24-bit position, load <= 1/2) behind
`Option<Box<Vec<u32>>>`. The API keeps `HashMap`'s method names and item
types, so **no checker file changed**; outside `tsr-binder` one test helper
changed (`tests/bind.rs`, `HashMap::len` → `SymbolTable::len`).
`parallel_tests.rs` now compares tables in order.

**Outputs** (against base `f2ca0b2`):

- `diagverdictdump`: `cmp`-identical (12,238 rows).
- `verdictdump`: 13 of 552,533 lines differ; summary right 544,047 →
  **544,050**, wrong 7,492 → 7,489. Every changed line has the same member
  multiset and a new order closer to tsgo's (longest common subsequence of
  top-level member names with the expected printing):

  | line | verdict | in tsgo order |
  |---|---|---|
  | `conformance/mappedTypes1:0:40`, `:41` | WRONG → RIGHT | 3/6 → 6/6 |
  | `compiler/reverseMappedTypeAssignableToIndex:0:11` | WRONG → RIGHT | 1/2 → 2/2 |
  | `compiler/mappedTypeRecursiveInference:0:60,61,65,71` | WRONG | 12/34 → 33/34 |
  | `compiler/mappedTypeRecursiveInference:0:64,66,70,72` | WRONG | 56/271 → 225/271 |
  | `compiler/mappedTypeRecursiveInference:0:69,73` | WRONG | 36/187 → 128/187 |

  Both loss joins print nothing. The remaining disorder in
  `mappedTypeRecursiveInference` is `lib.dom.d.ts` interfaces merged from
  several declarations: insertion order follows the first declaration of
  each name per merge, upstream's `compareSymbols` the global declaration
  position (§3, sites 1–3).
- CLI on domain-model, domain-model-large and generic-imports:
  `cmp`-identical.

**Ir** (callgrind):

| project | base | step 1 | Δ |
|---|---:|---:|---:|
| generic-imports | 343,366,092 | 342,843,469 | −0.15% |
| domain-model | 1,199,540,698 | 1,190,788,008 | −0.73% |

Binder inserts on generic-imports: `HashMap::insert` 7.63 M (4.69 M of it
`reserve_rehash`) became `push_new` 5.74 M inclusive, of which 2.95 M is
`Vec` growth (`grow_one`, 8,941 calls) — the growth §5 removes — plus
`position` lookups (3.0 M self, all callers).

The first two builds (8-byte, then 4-byte index slots, the index an
unboxed `Vec`) measured about the same Ir but +3.1% and +3.2% RSS on
generic-imports (5 and 7 interleaved rounds): the table had grown from 32 to 48
bytes and every symbol carries two. The boxed index restores 32 bytes
(asserted by `a_table_is_the_size_of_the_map_it_replaced`).

**Wall/CPU/RSS** (interleaved, §1):

| project | rounds | wall new/base | CPU new/base | RSS new/base | wall/tsgo base → new |
|---|---:|---:|---:|---:|---|
| generic-imports | 31 | 0.974 | 1.005 | 0.995 | 0.729 → 0.710 |
| domain-model | 31 | 1.012 | 1.014 | 1.002 | 0.690 → 0.698 |
| domain-model | 41 | 1.028 | 1.010 | 0.998 | 0.666 → 0.685 |
| domain-model, `--singleThreaded` | 31 | 1.007 | 0.996 | 0.995 | — |

Inside the box's noise. Cachegrind on domain-model (single-threaded)
showed D1 misses 7.72 M → 7.60 M, LLd 666 K → 660 K, branch mispredicts
16.19 M → 16.39 M (+1.2%, the linear scans): no cache regression to explain
a wall loss, and the same-binary control in the single-threaded session
spread 1.9%. Step 1 is the correctness precondition for §5, not a speed win.

**Full parity run** (`coverage`, step 1): `checker_types` 8,234/9,538,
`diagnostics` 4,517/5,502 (snapshots not committed; the base was not run
through `coverage`, the dumps above are the base comparison).
