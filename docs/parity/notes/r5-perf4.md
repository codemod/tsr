# Parity lane `r5-perf4` — receiver signature kinds, and the next native-cache gaps (round 5)

Lane: perf owner for round 5 of epic `tsr-2zk` (parent `tsr-2zk.17`, the
perf issue `tsr-2zk.943`). Box protocol: `docs/parity/box-protocol.md`.
Pinned upstream: `vendor/typescript-go` @ `5b1047d`. Predecessors:
[`r4-perf3.md`](r4-perf3.md) (publication under resolution, C2 sharing,
index infos), [`r4-perf2.md`](r4-perf2.md) (C3, C2, the wall/CPU split),
[`r4-perf.md`](r4-perf.md) (C7, C9, `memo_frames`). Release target
(CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on equivalent complete work.

Sections are numbered so code can cite them (`r5-perf4.md` §N). Every number
names the source state and measurement it came from.

## §1 Method

- Source base `f90fcef` (`integrate: merge origin/main`, the round-4
  wrap-up; `origin/main` at `ac56208` is the same tree plus the merge
  commit). 4-vCPU cloud container, Linux 6.18, `RUSTUP_TOOLCHAIN=stable`.
- **Ir**: `valgrind --tool=callgrind` total instructions of the release
  `tsr`, `--singleThreaded --pretty false`, on
  `python3 scripts/generate_perf_project.py --modules 100` ("p100") and on
  `benches/projects/domain-model` ("dm"). Deterministic; every variant's
  complete CLI output is `cmp`-identical to the base's on both.
- **Corpus**: both unfiltered dumps (`diagverdictdump`, 12,238 rows;
  `verdictdump`) against the base, frozen from the `f90fcef` build before
  any edit.
- **Wall/CPU**: `scripts/whole_project_perf.py`, median child CPU of 21
  samples (41 where a ratio exceeded 1.0) against the previous binary, and
  wall/CPU against native tsgo built from the pinned submodule
  (`scripts/offline-cargo/build-tsgo.sh`).

Base Ir at `f90fcef`: p100 **2,841,694,411**; dm **1,345,436,682**. (The
p100 figure is lower than r4-perf3 §1's 3.31 G because the integration
branch merged other lanes' work in between; it is not comparable with
r4-perf3's table.)

**Environment note.** The offline cargo bootstrap's assembler needs
`tomlkit`, which neither PyPI (blocked) nor the image provides. This box
ran `assemble.py` with a stdlib-only shim (`tomllib` to parse, a small
writer for the subset assemble emits) in its scratch directory; nothing
tracked changed. A future box hitting the same wall can do the same.

## §2 Receiver signature kinds (`PerfLinks::receiver_signature_kinds`)

**Forcing measurement** (p100 at base): `get_property_of_type_ex` was
14.34% inclusive (407.4 M). 85,782 calls missed the receiver's own members
and reached the §117 fallback, and each asked `signatures_of_type_kind`
for both kinds only to test non-emptiness: 171,564 calls, 146.4 M
(5.15%). Inside them, `intersection_has_never_discriminant` (58.5 M, see
§5), `signature_candidates_of_named_type` (52.8 M) and `apparent_type`
(45.2 M).

**Native operation.** `getPropertyOfTypeEx` (`checker.go:18918`) reads
`resolved.CallSignatures` and `resolved.ConstructSignatures` from the
receiver's resolved members (`resolveStructuredTypeMembers`), which native
computes once per type. The consumer is the fallback choice only:
`CallableFunction`/`NewableFunction`/`Function` before `Object`.

**Identity and owner.** Key: the receiver `TypeId` (the reference, not its
target: a type reference's identity already names its arguments). Value:
`(has_call, has_construct)`, each "the list `signatures_of_type_kind`
answers is `Some` and non-empty". Private `Checker`, Program lifetime.

**Admission.** `memo_frames` over the declarations of the receiver's
symbol (`Named { members }` owner, `Anonymous` symbol), the C7/C3 rule
(`r4-perf.md` §2): with no bound parameter's scope enclosing a declaration,
the computation runs with the alias frames taken. Ineligible, and computed
as before: a receiver with no symbol, a mapped type (`mapped_types`), an
instantiable type, and any receiver with a `signature_types` entry.

The last exclusion is what makes a hit safe against the two ways a
receiver's list changes after creation: `declared.rs` re-homes a resolved
type literal's list onto its reserved identity after
`TypeStore::complete_object` (r4-perf3 §6's staleness reason), and a few
producers (`jsx_intrinsic`, `decorators`, `inference`) attach a list to an
existing id. In both cases the entry appears in `signature_types`, the
read-side test sees it, and the memo is bypassed. Before such an entry
exists, the receiver's answer comes from its symbol's declarations
(interface / type-literal / class / function signatures), which no later
event changes.

**Publication.** Both lists decided (`Some`; a `None` is an unresolved
signature or return and is recomputed), `publishable_since(mark)`
(r4-perf3 §2), still no `signature_types` entry, and the receiver its own
apparent type (so the answer is not a mapped/constraint view of another
type).

**Context.** None beyond the receiver: the answer is two booleans; the
fallback's own lookups (`global_type_symbol_with_arity`,
`get_property_of_declared_symbol`) run per query as before. `class_static`
and the const-enum withhold stay per query.

**Work boundary.** p100: 85,278 admitted requests, **4,049** worker runs
(each the two `signatures_of_type_kind` calls); a hit is the admission
test, one hash probe and a `contains_key` on `signature_types`.

**Side effects skipped on a hit.** `signatures_of_type_kind` completes
signature returns (`complete_signature_return`) and resolves
interface/heritage signatures. A hit skips them for the second and later
misses on the same receiver; the first miss (the publishing one) ran them
to completion — a list with an incomplete return answers `None` and does
not publish — so later runs would have found them cached.

**Measured** (§1):

| | p100 Ir | dm Ir |
|---|---:|---:|
| base `f90fcef` | 2,841,694,411 | 1,345,436,682 |
| without `memo_frames` admission (published only with no alias frames) | 2,800,597,028 (−1.45%) | — |
| shipped | **2,728,739,632 (−3.97%)** | **1,299,109,431 (−3.44%)** |

generic-imports Ir 399,671,138 → 399,656,673 (its 3 checked files barely
reach this path; §7).

Whole-project, median child CPU against the base binary: domain-model
**0.924** (21 samples); generic-imports 1.055 at 21 samples, **0.988** at
41. Against native tsgo (default mode, 21 samples, base → this commit):
domain-model wall 0.866 → **0.838**, CPU 0.576 → 0.573; generic-imports
wall 1.117 → 1.169, CPU 0.625 → 0.663 — noise on a ~85 ms run whose Ir did
not move (§7 explains its wall). Corpus: both dumps `cmp`-identical to the
base; `cargo test --workspace --release` passes; coverage rows
`checker_types` 8179/9538, `diagnostics` 4398/5502 (the checked-in
snapshots were behind the base build; no dump line moved).

The unadmitted variant left about 50,000 of the 85,782 misses under an
alias-evaluation frame, which is why admission is needed. `memo_frames`
costs 12.0 M of the saving.

**How we would know it is wrong.** A §5-gate loss under the patch; or a
property miss whose fallback differs between a receiver's first miss and a
later one (a receiver gaining call or construct signatures without a
`signature_types` entry).

## §3 Shared name lists in `properties_related_to_with_optionals` (diff for the integrator)

**Forcing measurement.** r4-perf3 §4 left this as the next `into_vec`
consumer: every structural relation copied the target's memoised name list
(`get_property_names_of_type`), and re-copied the source's at each of up to
three uses.

**Change** (`r5-perf4-relater-shared-names.diff`, `relater.rs`, owned by
`r5-relater3`, so not committed here): the target's list and every
source-list read go through `get_property_names_of_type_shared`; the
intersection source's flattened constituent names are collected once into
an `Rc<[String]>` and handed out by reference count instead of a `Vec`
clone per use. A small helper, `Relater::source_property_names`, keeps the
old call pattern exactly: the source's own list is asked afresh at each
use (the enumeration has side effects, r4-perf3 §6), only the copy goes.

**Measured** (on top of §2): p100 Ir 2,728,739,632 → **2,706,630,494
(−0.81%)**; dm 1,299,109,431 → 1,290,266,137 (−0.68%). CLI output
`cmp`-identical on both.

**How we would know it is wrong.** It cannot change an answer: the names,
their order and the set of enumeration calls are unchanged.

## §4 The property walk's buffers (diff for the integrator)

**Forcing measurement** (after §2): `get_property_of_type_ex` freed 554,394
blocks (48.3 M Ir in `__rust_dealloc` alone): each call allocated the
`visiting` path of `get_property_of_declared_symbol` (one push = one
allocation) for the own-member walk and again for each fallback interface,
plus a `Vec<&str>` of fallback names (85,782 `grow_one`).

**Change** (`r5-perf4-property-walk-scratch.diff`, `members.rs` beyond the
hook lines this lane may touch, plus a `PerfLinks::visiting_scratch`
field): a `get_property_of_declared_symbol_fresh` wrapper runs the walk on
a reused path buffer (taken and put back, so a nested walk takes an empty
one and allocates as before); the fallback list is a fixed `[&str; 3]` (at
most `CallableFunction`/`NewableFunction`, `Function`, `Object`). Same
walk, same order.

**Measured** (on top of §2, §3 and §5's flow fix): p100 Ir 2,692,500,403 →
**2,617,494,614 (−2.79%)**; dm 1,285,036,420 → 1,255,632,414 (−2.29%).

## §5 Two allocation-only fixes in other lanes' files (diffs for the integrator)

1. **`Checker::intersection_has_never_discriminant`** (`flow.rs`) cloned the
   receiver's whole `TypeData` (`self.store.get(ty).data.clone()`) before
   testing whether it is an intersection: at base 216,269 calls, every one a
   clone (a `Named`'s text `String` included) and a drop, 58.5 M inclusive.
   It now borrows, and clones only an intersection's member list after the
   cache miss (`r5-perf4-never-discriminant-borrow.diff`). On top of §2+§3:
   p100 2,706,630,494 → **2,692,500,403 (−0.52%)**; dm 1,290,266,137 →
   1,285,036,420. (§2 had already removed most of the calls.)
2. **`Checker::is_generic_homomorphic_mapped_type`** (`mapped.rs`)
   allocated its cycle path for every query, although the inner walk
   answers `false` at once for a type that is not in `mapped_types` — the
   common case: `get_property_of_type_ex` asks it first on every call. It
   now tests `mapped_types.contains_key` first
   (`r5-perf4-mapped-early-return.diff`). On top of §2–§4 and §6: p100
   2,562,225,666 → **2,480,436,191 (−3.19%)**; dm 1,239,668,414 →
   1,206,769,880 (−2.65%).

Neither can change an answer: each removes an allocation in front of the
same test.

## §6 `resolve_name` per `(start, meaning, name)` (diff for the integrator)

**Forcing measurement** (after §4): name resolution was the largest
self-cost cluster in the checker: `BindResult::resolve_name` 108 M self /
162 M inclusive, `lookup_scoped` 52 M, the export-alias variant 64 M — about
1,250 Ir per call, with the same identifier resolved by several checks
(dotted-name flow reads, uninitialized-read and UMD checks, entity names).

**Native operation.** `getResolvedSymbol` caches `resolveName`'s answer in
the identifier's `links.resolvedSymbol` (per private checker); other
`resolveName` callers re-walk.

**Identity and owner.** Key `(start NodeId, meaning bits)`, value `(name,
Option<SymbolId>)`; a hit requires the same name, a different name at the
same key recomputes and does not overwrite. Private `Checker`, Program
lifetime, in a `RefCell` so that the many `&self` callers can use it.
**Exact:** `resolve_name` reads only the binder's tables, the node table and
the node map, all immutable for the checker's lifetime (`&'a` borrows), and
takes no callback. The export-alias variant takes a checker callback and is
not cached.

**Change** (`r5-perf4-resolve-name-memo.diff`): `Checker::resolve_name_memo`
in `perf_links.rs`, and the 132 `self.binder.resolve_name(self.nodes,
self.node_map, …)` call sites in 20 checker files routed through it. One
site is deliberately left alone: `check_used_before_its_declaration` asks
with `VALUE | EXPORT_VALUE`, a meaning nothing else asks, so its 38,179
queries are almost all first queries; routing it cost +5.7 M (p100
2,562,225,666 → 2,567,949,472) and was reverted.

**Measured** (on top of §2–§5.1, §4): p100 2,617,494,614 → **2,562,225,666
(−2.11%)**; dm 1,255,632,414 → 1,239,668,414 (−1.27%). 28,858 worker runs
remain for the routed sites.

**How we would know it is wrong.** A `resolve_name` that starts reading
mutable state (a checker-side table or a callback): the memo would then
need that state in its key.

## §7 Wall: where generic-imports' time goes (measurement, no code)

**Question** (brief): is generic-imports' wall (1.22x tsgo at `405b55ce`)
checker CPU, parse/bind, or a serial phase?

**Answer: none of the checker.** `--extendedDiagnostics` at `f90fcef`: 66
files parsed (3 checked, 63 lib `.d.ts`), Check 0.001 s, Program 0.110 s
(parse 0.070 s, bind 0.026 s, summed over loader workers). Callgrind
(single-threaded, 399.7 M Ir): `Program::from_root_files` 97.8%;
`parse_source_file` 76%; **`Parser::parse_leading_jsdoc` 148.9 M (37.3%)**;
binder 15%.

Medians of 31 fresh processes (ms; v2 binary = §2):

| | wall | user | sys |
|---|---:|---:|---:|
| TSR default | 82.9 | 69.3 | 25.3 |
| TSR `--singleThreaded` | 73.4 | 59.3 | 11.8 |
| tsgo default (4 checkers) | 71.6 | 91.7 | 46.3 |
| tsgo `--singleThreaded` | 68.4 | 73.6 | 28.6 |

Findings:

1. **TSR's parallel loader is a net loss on this project**: default mode
   is 9.5 ms *slower* than single-threaded, with +10 ms user and +13.5 ms
   sys, and the loader's reported time doubles (0.084 s against 0.044 s).
   The lib set is dominated by a few large files (`lib.dom.d.ts`), so the
   critical path is one file's parse and bind, and the extra workers add
   contention (sys time: allocator arenas and page faults) without
   shortening it.
2. **Eager JSDoc parsing is 37% of the work.** Native `withJSDoc`
   (`internal/parser/jsdoc.go:56`, pinned `5b1047d`) defers JSDoc in TS and
   `.d.ts` files to first access unless the comment has `@see`/`@link`;
   TSR parses every lib comment eagerly. That port is `tsr-2zk.17.1`
   ([`parser-lazy.md`](parser-lazy.md)), blocked on its four integration
   prerequisites. On generic-imports it is the largest single lever on
   wall: TSR single-threaded spends about as long parsing as tsgo does in
   parallel.
3. Checker work cannot move this project's ratio: Check is 1 ms.

**What would have to change** for generic-imports to reach 0.50: lazy TS
JSDoc (§7.2), and a loader that does not pay for parallelism it cannot use
(start serial, fan out only when several large files are pending, or size
the worker count by bytes queued). Neither is in this lane's files.

## §8 The stack, and what is left

**All five diffs together** on top of §2's commit (applied in the order
relater, flow, walk-scratch, resolve-name, mapped; each also applies alone;
`cargo fmt` clean; clippy reports nothing new in the touched files): p100
Ir **2,480,277,775 (−12.72% vs base)**, dm **1,207,545,034 (−10.25%)**, CLI
output `cmp`-identical; `cargo test --workspace --release` passes; both corpus dumps of the stack `cmp`-identical to
the base (12,238 and 552,533 rows).

Whole-project (21 samples; domain-model-large 11), stack against base
binary: domain-model CPU 0.960, generic-imports 0.947. Against native tsgo,
default mode:

| | base wall | base CPU | stack wall | stack CPU |
|---|---:|---:|---:|---:|
| domain-model | 0.866 | 0.576 | 0.860 | 0.573 |
| generic-imports | 1.117 | 0.625 | 1.115 | 0.621 |
| domain-model-large | 0.876 | 0.637 | **0.838** | **0.571** |

domain-model-large TSR CPU 1,468 → 1,340 ms (−8.7%), wall 628 → 600 ms.
Single-run wall ratios drift ±0.05 between identical binaries on this
4-vCPU box (box-protocol §5); CPU is the reliable column.

**Distance to 0.50.** r4-perf2 §6 estimated that domain-model-large needs
about 30% less checker CPU (plus better balance) to reach a 0.50 wall ratio.
The stack cuts TSR's whole-process CPU there by 8.7% (checker-only share
not separated), so it is a step, not the distance. The small projects are
dominated by program construction (§7), where checker work cannot reach the
target.

**Refused or deferred in this lane, with the number:**
- `memo_frames` admission (brief item 3): 30.9 M self (1.25% at the stack)
  over ~211,000 calls, ~146 Ir each. A span-containment enclosure test
  would replace the parent walk, but reparsed JSDoc nodes sit under hosts
  whose spans do not contain them, so a span test can miss an enclosing
  owner (a false *admit*, which changes answers); a generation counter on
  `alias_evaluation_bindings` needs its 75 writers in 17 files. Not tried.
- `is_pure_signature_type` (brief item 4): 69.8 M inclusive (2.7% at the
  stack). The signatures-first order stays refused (r4-perf3 §6). Moving
  mapped-member resolution to its readers is in `members.rs`/`relater.rs`;
  not attempted this round.
- `resolve_name` for `check_used_before_its_declaration`: +5.7 M, reverted
  (§6).

**Next candidates by measured size** (stack profile, p100):
1. Signature cloning: `Signature::clone` → `Vec<Parameter>::clone` 35.8 M
   (101,262 clones) and `drop_glue<Signature>` 29.2 M (302,467 drops);
   `signatures_of_type_kind` clones a `signature_types` list to filter it
   (22,766). A shared (`Rc`) parameter list, or kind-filtered views, is a
   cross-file representation change.
2. `TypeData::clone` 161,023 calls (17.7 M in `String` clones alone):
   `get_index_infos_of_type` 39,797, `generic_type_with_union_constraint`
   22,820, `narrowable_type_for_reference` 15,010,
   `get_regular_type_of_literal_type` 15,107 — each the §5.1 pattern
   (clone to match).
3. Program construction for small projects (§7): lazy TS JSDoc
   (`tsr-2zk.17.1`) and loader fan-out.
