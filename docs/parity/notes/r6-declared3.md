# r6-declared3: `declared.rs`, `mapped.rs` and neighbours, round 6

Lane files: `declared.rs`, `instantiation_expressions.rs`, `unique_symbols.rs`,
`mapped.rs`, `intersections.rs`, `intrinsics.rs`. Epic `tsr-2zk`; items
`tsr-2zk.1255`, `.1154`, `.1184`, `.1190`, `.1209`, `.1240`. Successor of
r6-declared2 (`r6-declared2.md`). Vendor pinned at `5b1047d`.

## 0. Frozen base

Batch BY (r6-declared2) had not landed on `claude/beautiful-shannon-ar5gh0`
when this box started (tip `c3c42d0`, batch BR's snapshot refresh). The
brief's fallback applies: the current tip, with BY merged when it lands.
Because every item here builds on r6-declared2's §1, its branch
(`claude/beautiful-shannon-ar5gh0-r6-declared2` at `3b516be`, the content BY
lands) was merged into this branch first, with no conflicts, and the base was
frozen on that merge. Unfiltered release build:

- `diagverdictdump`: RIGHT 5642, EMPTY_RIGHT 5606, WRONG 951, EMPTY_WRONG 39
  (12,238 rows);
- `verdictdump`: RIGHT 550,430, WRONG 5,126, GAP 747 (556,303 rows);
- Callgrind Ir (`tsr -p <project> --singleThreaded --pretty false
  --noEmit`): domain-model 1,092,375,842; generic-imports 343,256,832.

Setup: PyPI answers 403, so `assemble.py`'s three `tomlkit` calls ran on a
stdlib-only stand-in kept outside the repository (`r5-operators3.md` §4).
The native oracle is `scripts/offline-cargo/build-tsgo.sh`'s tsgo.

## 1. getPermissiveInstantiation / getRestrictiveInstantiation without a registry search (`tsr-2zk.1255`)

**Forcing constraint.** r6-declared2 §5 measured `ramdaToolsNoInfinite2` with
r5-relater7's held binder diff at 7.7 s (diagnostics) / 6.3 s (types)
against 2.4 s / 1.5 s before its §1, and put 42% of the case's callgrind in
`collect_mentioned_type_parameters`: r6-declared §2.2's bisection over the
whole type-parameter registry, run once per conditional extends type to list
the parameters a permissive and a restrictive instantiation must map.

Re-measured here (base + binder diff, `RAYON_NUM_THREADS=1`, verdictdump
filtered to the case): **45,355,019,265 Ir**. Removing the bisection was
not the whole story; three native shapes were missing, and each was measured
in turn on the same case:

| step | native | case wall (types) |
|---|---|---|
| base | | 7.6 s |
| (a) one walk instead of the bisection | the mappers are functions over every type parameter; nothing searches | 6.3 s |
| (b) one restrictive clone per parameter | getRestrictiveTypeParameter caches `CachedTypeKindRestrictiveTypeParameter` | 2.5 s |
| (c) restrictive instantiation only after the permissive relation holds; each cached separately | checker.go:24377 / :24415, `CachedTypeKindPermissiveInstantiation` / `RestrictiveInstantiation` | 2.0 s |

(Those walls were taken one by one on a loaded box; the final interleaved
measurement is below.)

**(a) The walk.** `collect_registered_type_parameters` gathers the registered
type parameters the extends type mentions in one traversal. It follows
exactly the edges of `mentions_type_parameter_inner` (`inference.rs`, MAIN),
table for table and in the same first-match order, with a hash set for
`visited` instead of the walker's linear scan, and never stops early. The
bisection answered "which candidates does some walk reach", and a candidate
is reached iff the full traversal reaches it, so the set is identical; the
list keeps registry order, so the restrictive clones are minted in the same
order as before. A function-mapper instantiation (native's actual road) was
not taken: `instantiate_type` takes an explicit `(from, to)` map and its
`parameters` slice drives its own `mentions_type_parameter` early-outs; a
function mapper would be a change to `inference.rs` (MAIN). The walk cannot
share the MAIN walker's code: its closure entry point is private to
`inference.rs`, and the public wrappers stop at the first hit.

**(b) The clone cache.** With (a) in place the restrictive instantiation was
80% of what remained (an experiment that skipped it took the case from 6.1 s
to 1.3 s). Each extends type minted its own unconstrained clones, so every
restrictive instantiation built fresh type identities, and every cache keyed
by type identity downstream (alias instantiations, relations, base
constraints) missed. Native caches the clone on the parameter
(getRestrictiveTypeParameter, checker.go:24514). `restrictive_type_parameter`
does the same. Native also returns the parameter itself when it has no
written constraint; that was built, measured as no gain (2.46 s → 2.67 s,
noise), and not kept, so a constraint-free parameter still maps to its clone.

**(c) Laziness.** getConditionalType asks the permissive relation first and
instantiates the restrictive form only when it did not reject
(`||` short-circuit, checker.go:24415). The port built both up front. The
pair cache became `ExtendsInstantiations` (parameters and names gathered
once; each instantiation slot filled when first asked for), matching
native's two separately keyed caches. One behaviour moved with it, toward
native: a restrictive instantiation that fails (`error`) used to defer a
check the permissive relation had already rejected; now the false branch is
taken, as native's has no such dependency. No dump row moved.

**Checker port convention** (`docs/conventions.md`):
- `conditional_extends` (`instantiation_expressions.rs`): native operation
  getPermissiveInstantiation / getRestrictiveInstantiation (checker.go:24479,
  :24492); key the extends type's identity; owner the instantiation-expression
  links; states: absent → `None` (mentions no parameter) or an entry whose
  two slots are each written once when first asked for; never invalidated.
- `restrictive_type_parameters`: getRestrictiveTypeParameter (:24514); key the
  parameter's identity; written once on first use; never invalidated.
- The walk stores nothing. Expensive work: one traversal per extends type,
  bounded by the extends type's reachable graph, not by the registry.

**Measured** (unfiltered, both dumps, against §0): **byte-identical** on
both dumps (`cut -f1-4` of every row, sorted, compares equal); slowcases
clean on both. Callgrind Ir: domain-model 1,092,375,842 → 1,092,429,017
(+0.005%), generic-imports 343,256,832 → 343,288,718 (+0.009%): flat, since
neither project ever reaches a conditional whose extends type mentions a
type parameter (no `extends_instantiation` frame in either profile). The
measured drop is on the case that pays the path, `ramdaToolsNoInfinite2`
with the held binder diff, filtered verdictdump, `RAYON_NUM_THREADS=1`:

- Ir **45,355,019,265 → 12,880,763,821 (−71.6%)**;
- wall, three interleaved runs each: types 4.9/5.3/4.8 s → 1.43/1.45/1.40 s,
  diagnostics 6.5/6.3/6.2 s → 1.83/1.79/1.98 s. That is below r6-declared2's
  pre-§1 figures (2.4 s / 1.5 s on its box); native is 0.4 s.
- Verdicts on the case unchanged (`TOTAL 493 right 470 gap 2 wrong 21`).

What remains on the case (profile of step (b)): `resolved_indexed_access_type`
45% inclusive and `is_string_index_signature_only_type` 22% (`indexed.rs`,
r6-errorsplit3), and `type_parameter_constraint`'s cache key, which collects
every active alias-binding frame into a fresh hash map on each call (29% in
the step-(a) profile, `members.rs`, MAIN). Not this lane's.

**Falsifier.** A dump row that moves with this commit (none did), or a
conditional whose restrictive instantiation fails after its permissive one
rejected the check, where native defers: native has no failing restrictive
instantiation, so that would point at the instantiation, not here.
