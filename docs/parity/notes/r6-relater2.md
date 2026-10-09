# r6-relater2 — held lifts, unreached rows, relater-owned TS2322 (`tsr-2zk.1139`)

Lane under epic `tsr-2zk`, round 6, successor to r6-relater
([`r6-relater.md`](r6-relater.md); its §8.1 is the re-triage table this
lane works from). Owns `relater.rs`, `relation_cache.rs`, `variances.rs`,
`identity.rs`, `index_access_reports.rs`, `assignreport.rs`, the tests
`crates/tsr-checker/tests/r6_relater2.rs` and this file. Native anchors are
`vendor/typescript-go` @ `5b1047d`; every test expectation was checked
against a native tsgo built from that submodule
(`scripts/offline-cargo/build-tsgo.sh`).

## 0. Frozen base

Batch BG (r6-relater) had not landed on `claude/beautiful-shannon-ar5gh0`
when this lane started (tip `9f24647`, after batch BE). Every item builds
on r6-relater's code, so the base is that tip with r6-relater's branch
(`0d52780`) merged in: `da4974d`, a clean merge.

- `diagverdictdump`: RIGHT 5579, EMPTY_RIGHT 5603, WRONG 1014, EMPTY_WRONG 42.
- `verdictdump`: RIGHT 549932, WRONG 5544, GAP 827.
- Callgrind `Ir` of the base `tsr` (`-p <project> --singleThreaded --pretty
  false`): generic-imports 343,013,497; domain-model 1,090,808,216.

Every `Ir` below uses that command, and each run's complete CLI output is
compared with the base's (`cmp`). Loss checks are `box-protocol.md` §5's, on
`cut -f1,2`, unfiltered; slowcases runs on both dumps.

## 1. Lift: the union-walk decline (r6-relater §5(3))

**What it was.** A failed union or intersection target walk for an
`is_mapped_type_generic_indexed_access` source (`{ [P in K]: E }[X]`, `X`
generic) answered `Unknown`, because the walk's other fallthroughs (the
conditional, string-like and type-parameter ones) do not take an
indexed-access source. r6-relater measured the bare lift: `correlatedUnions`
EMPTY_RIGHT → EMPTY_WRONG (TS2322 at 181:5 and 299:3), no gains.

**The two causes, traced.**
- 181 is `x = y` in `f4`: `Funcs[K] -> Funcs[keyof ArgMap]`, a target the
  port holds as the union `Func<"a"> | Func<"b">`. Natively the walk fails,
  unionOrIntersectionRelatedTo falls through for the instantiable source
  (relater.go:3380) to structuredTypeRelatedToWorker's type-variable arm:
  the substitution `Func<K>` fails, and then isMappedTypeGenericIndexedAccess
  (relater.go:3681) explores `Funcs[constraint of K]` = `Funcs[keyof
  ArgMap]`, the target itself. The port's type-variable arm for an
  indexed-access source stopped at the constraint and had no such step.
- 299 is `return o[k]`: `Partial<Foo1>[K] -> Foo1[K] | undefined`. The
  substitution (getConstraintFromIndexedAccess, checker.go:17227) is `Foo1[K]
  | undefined`; the port computes it already (`constraints.rs` through
  `mapped_indexed_access_constraint`, once `ensure_mapped_type_info` has
  captured the resolved `Partial<Foo1>` instance). What was missing was the
  fallthrough that reaches it.

So "the substitution for a resolved instance" (r6-relater §5(3)) was in
place for both rows; the missing pieces were the walk's fallthrough and the
`[constraint of X]` step.

**Ported.**
- The type-variable arm for an indexed-access source
  (`type_variable_source_related_to`) keeps its constraint steps
  (`indexed_access_source_constraint_related_to`, unchanged) and, when they
  fail and the source is a mapped-type generic indexed access, relates
  `getIndexedAccessType(object, constraint of X)` (`resolved_indexed_access_type`)
  to the target. A nil index constraint skips the step (native's `if
  indexConstraint != nil`); an undecided one is `Unknown`.
- Both target walks (union and intersection) fall through to that arm for
  such a source in place of the decline.

**Alternative.** The fallthrough for every indexed-access source, as native
has it: refused in r5-relater5 §3 (`quickinfoTypeAtReturnPositionsInaccurate`
loses 6 lines while `flow.rs` reads an undecided narrowing relation as a
decision). That stated divergence stays for the other indexed accesses.

**Measured** against §0, both loss checks empty, slowcases clean:
- no verdict or type line moves in either dump: the base kept
  `correlatedUnions` EMPTY_RIGHT through the decline, and the port now
  reaches native's answer through native's road;
- `Ir`: generic-imports 343,013,497 → 343,039,210 (+0.007%); domain-model
  1,090,808,216 → 1,091,006,021 (+0.018%). CLI output identical.

**Falsifier.** A mapped-type generic indexed access whose `[constraint of
X]` read the port builds differently from getIndexedAccessType (likeliest
an `as`-clause or `-?` mapped object, which `is_mapped_type_generic_indexed_access`
excludes), now decided False where the decline kept `Unknown`.

Test: `tests/r6_relater2.rs`
`a_mapped_generic_indexed_access_meets_a_union_through_its_index_constraint`.

## 2. The write-constraint step asks isGenericObjectType (§8.1 R-indexed)

**Forcing constraint.** `undefinedAssignableToGenericMappedIntersection` 5
(`obj[x] = undefined`, `obj: Errors<T>`, `Errors<T> = { [P in keyof T]:
string | undefined } & { all: string | undefined }`, `x: keyof T`) is
TS2322 natively. The indexed-access target arm (relater.go:3443-3488) takes
the write constraint only when `!isGenericObjectType(baseObjectType) &&
!isGenericIndexType(baseIndexType)`; the intersection's base is itself, and
it holds a generic mapped type, so the step is skipped and the worker ends
False. The port tested only `INSTANTIABLE_NON_PRIMITIVE` on the base object,
went on to `indexed_access_write_constraint`, which answers an intersection
object `Undecided`, and the pair stayed `Unknown`.

**Ported.** `Relater::is_generic_object_type` (getGenericObjectFlags'
IsGenericObjectType): an instantiable non-primitive type, a generic mapped
type (`is_generic_mapped_type`), a generic tuple (a variadic element that is
not an array), or a union/intersection with such a constituent; `None` when
a constituent's mapped classification is undecided. The write-constraint
step answers False for a generic base object and `Unknown` for an undecided
one. `indexed_access_pair_after_components` keeps its own test (it already
answers `Unknown` for an index it cannot classify).

**Measured** against §1's commit (and §0), both loss checks empty,
slowcases clean:
- diagnostics: `undefinedAssignableToGenericMappedIntersection` WRONG →
  RIGHT (RIGHT 5580, WRONG 1013);
- types unchanged;
- `Ir`: generic-imports 343,039,210 → 343,038,224 (−0.0003%); domain-model
  1,091,006,021 → 1,090,403,545 (−0.06%). CLI output identical.

**Falsifier.** A union or intersection base native does not classify as
generic but the port does (a mapped constituent `is_generic_mapped_type`
over-reads), now False where native relates through the write constraint.

Test: `tests/r6_relater2.rs`
`a_write_to_a_generic_intersection_object_is_not_related_through_a_constraint`.
