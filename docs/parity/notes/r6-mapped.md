# r6-mapped — generic mapped instances, `.16.8`/`.16.91` classification (`tsr-2zk.16.8`, `.16.91`, `.16.100`, `.16.71`)

Round-6 cloud lane, successor to r5-mapped6 (`r5-mapped6.md`). Owns
`crates/tsr-checker/src/mapped.rs`, `intersections.rs`, `intrinsics.rs`
and this note. Other lanes' and main's files ship as measured diffs.
Native source is `vendor/typescript-go` @ `5b1047d`; every expectation
below was checked against a native probe built with
`scripts/offline-cargo/build-tsgo.sh`.

## 0. Baseline and setup

Frozen at `claude/beautiful-shannon-ar5gh0` `b18aec06` (main `17265fac`
plus bookkeeping), which carries r5-mapped6's route diff.

- types 549,853 RIGHT / 5,607 WRONG / 843 GAP (556,303 lines);
- diagnostics 5,530 RIGHT + 5,596 EMPTY_RIGHT (1,063 WRONG, 49
  EMPTY_WRONG) of 12,238;
- Ir (`valgrind --tool=callgrind`, release `tsr`, `--singleThreaded
  --pretty false --noEmit`): domain-model 1,091,471,488; generic-imports
  343,082,464.

Setup as r5-operators3 §4: PyPI answers 403, so `assemble.py`'s three
`tomlkit` calls ran against a stdlib-only stand-in kept outside the repo
(`parse` is `tomllib.loads`, `inline_table` a marked `dict`, `dumps` a
small table writer).

## 1. A generic mapped instance prints its mapped form (committed + diff)

**Forcing constraint.** `dependentDestructuredVariablesFromNestedPatterns`
calls `Promise.allSettled(fn())` with `fn: () => T` and
`T extends readonly unknown[]`. The lib signature returns
`{ -readonly [P in keyof T_1]: PromiseSettledResult<Awaited<T_1[P]>>; }`;
under `T_1 := T` native prints
`{ -readonly [P in keyof T]: PromiseSettledResult<Awaited<T[P]>>; }`. The
port printed `ReadonlyArray`'s members (`[x: number]: …; at: …; concat: …`).

**Native.** instantiateMappedType (`checker.go:22535`) maps the homomorphic
variable `T_1` to `T`, a type parameter, which is neither an array nor a
tuple, so it answers instantiateAnonymousType: a MappedType whose
constraint is `keyof T`. createTypeNodeFromObjectType
(`nodebuilderimpl.go:2690`) prints a type that `isGenericMappedType`
(`checker.go:24908`) holds for through createMappedTypeNodeFromType, from
its parts. Its members are resolved only when read.

**Cause.** `instantiate_mapped_type_worker` (`mapped.rs`) always answered
`resolved_mapped_object`, the member print, whether or not the instance
was still generic. `evaluate_mapped_type_node`, the node's own route,
already asks `is_generic_mapped_info` first.

**Ported (commit).** The worker asks `is_generic_mapped_info` before the
member print, and mints a generic instance from `mapped_type_text`, as
the node route does. The commit also records the instance's target and
mapper on `MappedTypeInfo::instance` (native's `MappedType.target` and
`.mapper`), and adds `apparent_mapped_instance` and
`is_generic_mapped_type`, which only the diff below reads (each carries an
`allow(dead_code)` naming the diff).

**Measured** (commit alone, both dumps unfiltered, against the frozen
base):
- types **+4 RIGHT**: `dependentDestructuredVariablesFromNestedPatterns:14/15/16/25`;
- diagnostics unchanged; zero losses on both dumps; no base-RIGHT key
  missing; slowcases clean on both dumps;
- Ir: domain-model 1,091,471,488 → 1,091,707,823 (+0.022%),
  generic-imports 343,082,464 → 343,063,422 (−0.006%);
- median child CPU new/old: domain-model 0.970 (41 samples);
  generic-imports 1.065 then 0.992 at 41 samples, with a base-vs-base
  control at 1.011, so the first reading was noise. `diagnostics_match:
  true` throughout.

### The diff: getResolvedApparentTypeOfMappedType for an instance

[`r6-mapped-apparent-instance.diff`](r6-mapped-apparent-instance.diff),
`mapped.rs` and `contextual.rs` (main's), on top of the commit.

**Forcing constraint.** The rest of the function reads `promises.map`.
Native's getApparentType (`checker.go:21729`) applies
getResolvedApparentTypeOfMappedType to a generic homomorphic mapped type:
when every constituent of the modifiers type's base constraint is an array
or tuple, the apparent type is `instantiateType(target,
prependTypeMapping(typeVariable, baseConstraint, mapper))`, here
`PromiseSettledResult<unknown>[]`. So `map`'s callback reads
`PromiseSettledResult<unknown>`, and the 11 lines under it follow. The
port's `apparent_mapped_type` had this only for an alias reference
(`type_reference_targets`); a signature's mapped return has none.

- **`mapped.rs` hunk:** `apparent_mapped_type` falls back to
  `apparent_mapped_instance`, which re-instantiates
  `MappedTypeInfo::instance`'s target (or the type itself, for a
  non-instance) with `typeVariable := base` prepended to its mapper. The
  type variable is the target's deferred `keyof` operand when it is a type
  parameter, and an `as` clause declines, as getHomomorphicTypeVariable and
  the `NameType == nil` test do.
- **`contextual.rs` hunk:** that alone loses 4 lines
  (`typeParameterConstModifiersReverseMappedTypes:49/50/52/53`, `test5`'s
  `...args: { [K in keyof T]: T[K] }` with `const T extends readonly
  unknown[]`). `contextual_argument_type` read the rest type's property
  `"0"` through `contextual_type_for_element_expression`, which now sees
  `readonly unknown[]` and answers `unknown`, so the argument loses its
  const context. Native never reads that property:
  getContextualTypeForArgumentAtIndex (`checker.go:29772`) answers
  `getIndexedAccessType(restType, numberLiteral)`, which defers for a
  generic object, and isConstTypeVariable reaches `T` through the indexed
  access and isConstMappedType. The hunk sends a generic mapped rest type
  to `resolved_indexed_access_type`, the arm a type-parameter rest type
  already takes. It also removes the two `allow(dead_code)`s.

**Measured** with the diff (commit + diff, both dumps unfiltered, against
the frozen base):
- types **+19 RIGHT** (the commit's 4 and 15 more, all
  `dependentDestructuredVariablesFromNestedPatterns`: lines 23, 24, 26–29,
  31–33, 35–38, 40, 47); zero losses; diagnostics unchanged; slowcases
  clean.
- The lines that stay WRONG in that case (53, 54, 56–58) print
  `readonly [...]` for an `as const` tuple under a contextual mapped type,
  a different cause, not traced.

### Ownership and work boundaries (checker port convention)

- **Native operations:** instantiateMappedType / instantiateAnonymousType
  (`checker.go:22535`, `:22458`); createTypeNodeFromObjectType's
  isGenericMappedType arm (`nodebuilderimpl.go:2690`);
  getResolvedApparentTypeOfMappedType (`checker.go:21772`, diff); isConstTypeVariable (`:13645`).
- **Key identity and owner:** unchanged. An instance is cached in
  `instantiated_objects` under `(target, map)`, as before; its
  `MappedTypeInfo` is owned by `mapped_types` under the instance's id.
  `MappedTypeInfo::instance` is a value on that info (the target id and a
  copy of the mapper), not a new table.
- **Publication states:** a generic instance is published complete when
  minted, with no member table; members are resolved on first read by the
  existing `resolve_mapped_type_members`, as for a node-built generic
  mapped type. The apparent type is memoized in `mapped_apparent_types`,
  as the alias arm already was.
- **Receiver/alias context:** the instance's mapper is the one it was
  instantiated with; `apparent_mapped_instance` prepends the homomorphic
  variable to that mapper, so another instance of the same target has its
  own apparent type.
- **Expensive work boundary:** the commit removes work, the eager member
  resolution and print of a generic instance. The diff adds one
  instantiation of the target per generic homomorphic instance whose
  apparent type is asked for and whose constraint is array-like.

## 2. `.16.100`: a mapped node under alias bindings maps a tuple image elementwise (committed)

**Forcing constraint.** `mappedArrayTupleIntersections:11`:
`type Hmm<T extends any[]> = T extends number[] ? MustBeArray<{ [I in keyof T]: 1 }> : never`,
and `Hmm<[3, 4, 5]>`. Native prints `[1, 1, 1]`; the port printed the
tuple's members (`{ [x: number]: 1; 0: 1; 1: 1; 2: 1; concat: 1; … }`).

**Native.** getConditionalType instantiates the true branch with the
alias mapper (`checker.go:24300`), so the mapped node's declared type,
a MappedType with homomorphic variable `T`, goes through
instantiateMappedType (`:22535`). Its `mapTypeWithAlias` over `T`'s image
`[3, 4, 5]` reaches instantiateMappedTupleType, one template
instantiation per element.

**Cause.** The port has no declared generic type for the node. It
re-evaluates the node under the alias-evaluation frames (r5-mapped4 §5),
so `keyof T` resolves to the tuple's keys and the members print. The alias
road (`instantiate_mapped_alias_sequence`, for `Boxify<[…]>`) already
takes instantiateMappedType's sequence arms; a node written inline in a
branch did not.

**Ported.** `evaluate_mapped_type_node`, under alias frames and for a
homomorphic node, asks `instantiate_mapped_sequence` first, the same
function the alias road uses. Its `replace_source` re-evaluates the node
with the homomorphic variable rebound to one constituent, in a frame of its
own, which is the port's `prependTypeMapping(typeVariable, t, mapper)`.
That covers native's arms for a primitive image (answered as is), a union
image (distributed), an array or array intersection, and a tuple. Outside
alias frames the variable is the declared type parameter, which no arm
admits, so nothing else changes.

**Measured** (on top of §1's commit, both dumps unfiltered, against the
frozen base):
- types **+1 RIGHT** (`mappedArrayTupleIntersections:11`); no other
  line's text changes; diagnostics unchanged;
- zero losses; slowcases clean;
- Ir against §1's commit: domain-model 1,091,707,823 → 1,091,671,012
  (−0.003%), generic-imports 343,063,422 → 343,092,245 (+0.008%);
- median child CPU new/old against the frozen base (21 samples):
  domain-model 0.971, generic-imports 0.963. `diagnostics_match: true`.

### Ownership and work boundaries (checker port convention)

- **Native operation:** instantiateMappedType's `mapTypeWithAlias` arms
  (`checker.go:22535`), instantiateMappedTupleType and
  instantiateMappedArrayType.
- **Key identity and owner:** the result is published in
  `type_literal_types` under the node's `type_literal_key`, as a built
  mapped node already was. Each constituent's re-evaluation runs under a
  frame that binds the variable to that constituent, so it has its own key.
- **Publication states:** unchanged; an `error` constituent declines the
  node, and a declined node is not published (r5-mapped6 §1).
- **Receiver/alias context:** the frame pushed for a constituent sits on
  top of the caller's frames and is popped before the result is published.
- **Expensive work boundary:** one template instantiation per tuple
  element instead of the tuple's whole member table and its print.
