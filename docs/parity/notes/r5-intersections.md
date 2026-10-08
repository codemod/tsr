# r5-intersections — getIntersectionType fidelity (`tsr-2zk.971`)

Lane under `tsr-2zk.971`. Owns `crates/tsr-checker/src/intersections.rs`.
Native anchors are `vendor/typescript-go` @ `5b1047d`.

Frozen base: `935972e` (integration branch). `diagverdictdump`: RIGHT 5075,
EMPTY_RIGHT 5554, WRONG 1516, EMPTY_WRONG 93. `verdictdump`: RIGHT 543125,
WRONG 8295, GAP 1113. Perf is a self-comparison against the base binary
(median child CPU, new/old) plus callgrind `Ir` of `--singleThreaded true`
runs, which repeat exactly.

## 1. `removeConstrainedTypeVariables` and the identity of `{}`

Witness: `unknownControlFlow` `fx4`, `value: T & ({} | null)` with
`T extends {} | null`. Native prints `value : T`; the port kept
`T & ({} | null)`, and the `value === 42` comparison then overlapped.

Native path. `getIntersectionTypeEx` distributes over the union operand
(`checker.go:26203`): `T & {}` and `T & null`. Each is the two-constituent
type-variable case (`:26130`); neither reduces, and both are marked
`ObjectFlagsIsConstrainedTypeVariable`. `getUnionType` then sees
`IncludesConstrainedTypeVariable` and runs `removeConstrainedTypeVariables`
(`:25881`): the primitives `{}` and `null` cover every constituent of T's
base constraint `{} | null` (`containsType`, identity), so both intersections
collapse to `T`.

The port already had every one of these pieces (`reduce_constrained_intersection`,
the `constrained_type_variables` side table, `unions.rs`'s
`remove_constrained_type_variables`). The miss is identity: native resolves
every member-less, unaliased type literal to the one shared
`emptyTypeLiteralType` (`checker.go:22939`), while this port gives each
written `{}` its own `TypeId` and answers "is this `emptyTypeLiteralType`"
with `is_unaliased_empty_type_literal`. The annotation's `{}` and the
constraint's `{}` are two ids, so `containsType` found nothing.

Fix, in the owned file: when the side table records a constraining
intersection whose primitive side is an unaliased empty type literal, it
records the variable's base constraint's own `{}`
(`constraint_identity_of_empty_type_literal`). That is the identity native's
membership test sees. Rejected alternatives:

- **Unify `{}` at its producer** (`get_type_from_type_literal`, `declared.rs`).
  The faithful fix, and the one that would also dedupe `G<{}> & G<{}>`
  (`mixinAccessModifiers`). Not in this lane's files; shipped as a measured
  diff (§4).
- **Compare by `is_unaliased_empty_type_literal` inside
  `remove_constrained_type_variables`.** Same effect, but `unions.rs` is
  read-only for this lane, and the side-table canonicalization keeps the
  equality test native's.

Falsifier: once `{}` is one identity, `constraint_identity_of_empty_type_literal`
returns its input on every call and can be deleted.

Side table (`constrained_type_variables`, Checker-owned, whole-check lifetime):
key is the intersection `TypeId` minted by `create_intersection`; value is
`(variable, primitive)`; published once, when the intersection is first
created with the constrained-variable mark. Only the recorded primitive
changes here. The base-constraint read happens only for an `{}` operand of a
constrained pair, after the constraint reduction already computed it (cached
in `base_constraint_cache`).

Measured against the frozen base: types +4 lines (`unknownControlFlow`
`0:419`, `0:421`, `0:423`, `0:424`), and the case's missing TS2367 at line 341
is now reported (the case stays WRONG on unrelated TS2322/TS2345/TS2536). Both
loss checks empty. Perf (21 samples, median child CPU new/old): domain-model
0.996, generic-imports 1.013. `Ir`: 1,345,819,958 → 1,345,815,899 and
399,682,984 → 399,678,193; CLI output identical.
