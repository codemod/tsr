# r5-relater6 — mapped indexed access, keyof Unknowns, TS2536 (`tsr-2zk.1035`)

Lane under `tsr-2zk.1035`, continuing r5-relater5 ([`r5-relater5.md`](r5-relater5.md) §5).
Owns `crates/tsr-checker/src/relater.rs`, `index_access_reports.rs` and the
tests for its items. Native anchors are `vendor/typescript-go` @ `5b1047d`.

## 0. Frozen base

`ccb48e7`, the integration head carrying r5-relater5's `fe31884` (merge
`e9cc3c8`).
- `diagverdictdump`: RIGHT 5368, EMPTY_RIGHT 5584, WRONG 1223, EMPTY_WRONG 63.
- `verdictdump`: RIGHT 544166, WRONG 7379, GAP 988.
- Callgrind `Ir` of the base `tsr` (`-p <project> --singleThreaded --pretty
  false`): generic-imports 343,420,025; domain-model 1,201,770,992.

Every `Ir` number below uses that command. The complete CLI output of each
run is compared with the base's (`cmp`).

## 1. IAM — getSimplifiedIndexedAccessType's mapped arm

**Forcing constraint.** Native's `isRelatedToEx` normalizes both sides
before anything else (relater.go:2619): source reading, target writing.
For an indexed access, `getSimplifiedIndexedAccessType` (checker.go:27915)
rewrites `{ [P in K]: E }[X]` to `E[P := X]` when the object is a generic
mapped type without a remapping `as` clause (:27963). It adds `undefined`
when the mapped type, or its modifiers type, has `?`
(`substituteIndexedMappedType`, :29291). So `Partial<T>[K]` relates as
`T[K] | undefined`, and `Readonly<U>[K]` as `U[K]`. The port related the
unsimplified indexed accesses. r5-relater5 made that pair answer `Unknown`
(its §2 decline), so the eight `mappedTypeRelationships` errors were missing.

**Ported** (`Relater::simplified_indexed_access`, called from
`is_related_to_with_flags` right after the `NoInfer` normalization; the
simplified pair re-enters the gate, as native's loop normalizes again):
- the object is simplified first (a nested indexed access);
- a union index over a generic mapped object distributes
  (`distributeObjectOverIndexType`): a union when reading, an intersection
  when writing;
- the substitution instantiates the template (`mapped_template_type`, which
  already carries the mapped type's own `?`) with `P := X`;
- `isOptional`: the type's own `+?`, else `getCombinedMappedTypeOptionality`
  of the modifiers type (`isGenericType(objectType)` always holds for a
  generic mapped type). A `-?` mapped type over a `?` modifiers type is
  therefore optional, as native's is;
- `mapType(.., getSimplifiedType)` over the result's indexed-access
  constituents, and `removeType(result, t)`.

**Alternatives.**
- The whole of `getSimplifiedIndexedAccessType`: distribution over a union
  index for every object, over union and intersection objects, and the
  generic-tuple arm. Rejected for this commit. Each of those changes what
  an existing pair relates through, so each needs its own loss measurement,
  and no census case asks for them. Stated as a divergence on the function.
- A memo, as native's `cachedTypes`. Not built: the arm reaches
  `instantiate_type` only for an indexed access whose object is a mapped
  type. Domain-model makes 15 substitutions, costing 212,730 `Ir` in all.
  Would win if a project made that number large.

**Accepted.**
- A remapping or filtering `as` clause is left unsimplified. The filtering
  kind needs `getMappedTypeNameTypeKind` (item 3).
- A step the port cannot compute leaves the whole side unsimplified, and
  the indexed-access pair arm keeps its decline for a generic mapped object.
  Such steps are: an unclassifiable mapped type, a failed instantiation, an
  unknown modifiers type, or 32 nested levels.
- The printed source of the TS2322 head is still the unsimplified
  `Partial<T>[keyof T]`, where native prints `T[keyof T] | undefined`
  (`reportRelationError` prints the normalized type). The suite compares
  position and code only. The head is `assignreport.rs`'s (r5-missingprop).

**Not converted, and why.**
- `conditionalTypes1` 114-117 (`f8`): the substitution happens.
  `FunctionPropertyNames<T>` becomes the template's conditional, `T[keyof
  T] extends Function ? keyof T : never`. That conditional is a mapped
  template conditional (`mapped_conditionals`), and r5-relater4's gate
  declines every such pair (`Unknown`, r5-relater4 §2). That decline is
  about `as`-clause filtering, so it moves with item 3.
- `mappedTypeConstraints2` 42 (`Mapped6<K>[keyof Mapped6<K>]`): a
  remapping `as` clause, which native does not substitute either. It
  belongs with item 3.

**Measured** against §0, both dumps unfiltered, both loss checks empty:
- diagnostics: case verdicts unchanged; 10 lines converted:
  - `mappedTypeRelationships` 30, 35, 40, 41, 45, 46, 61, 66 (missing
    TS2322 now reported);
  - `inferenceShouldFailOnEvolvingArrays` 5, 13 (extra TS2322 gone);
- types: unchanged.

**Perf.**
- `Ir`: generic-imports 343,420,025 → 343,418,605 (−0.0004%); domain-model
  1,201,770,992 → 1,203,251,922 (+0.12%).
- The gate's per-entry cost is one flags test per side; generic-imports
  shows none. Domain-model's increase is downstream: 212,730 `Ir` is the
  15 substitutions, and the rest is relating the simplified pairs, which
  used to stop at the `Unknown` decline. Examples are `(() => void)[] |
  undefined → never[]` and `never[] → T[]` from narrowing `{ [K in keyof
  Events]?: … }[K]`. Native does that work too. CLI output is identical.
- Median child CPU, new/old: 21 samples gave domain-model 1.027 and
  generic-imports 1.040. The 41-sample re-run gave 1.018 and 0.991.

**Falsifier.** A pair whose simplified form decides differently from
native's printed normalized type. Also a `-?` mapped type over a
non-optional modifiers type that gains `undefined`.

Test: `tests/generic_mapped_relations.rs`
`indexed_access_of_a_generic_mapped_type_is_substituted`.
