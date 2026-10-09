# r6-declared: `declared.rs`, round 6

Lane files: `declared.rs`, `instantiation_expressions.rs`, `unique_symbols.rs`.
Frozen base: `claude/beautiful-shannon-ar5gh0` `b18aec06` (main `17265fac`
plus bookkeeping). Vendor pinned at `5b1047d`.

## 0. Frozen base

Unfiltered, release build of `b18aec06`:
- `diagverdictdump`: RIGHT 5530, EMPTY_RIGHT 5596, WRONG 1063, EMPTY_WRONG 49
  (12,238 rows);
- `verdictdump`: RIGHT 549,853, WRONG 5,607, GAP 843 (556,303 rows).

Setup note: PyPI answers 403, so `assemble.py`'s three `tomlkit` calls ran on
a stdlib-only stand-in kept in the session scratchpad, not committed
(`r5-operators3.md` §4).

## 1. getObjectTypeInstantiation's referenced-parameter key

**Forcing constraint.** getObjectTypeInstantiation (checker.go:22304-22352)
computes an anonymous type's `outerTypeParameters` once per declaration. When
the type carries no alias type arguments (it is not the right-hand side of a
generic type alias) and its symbol is a `Method` or `TypeLiteral` (a type
literal, a mapped type, a function or constructor type node, a method), it
filters that list with isTypeParameterPossiblyReferenced (checker.go:22403).
An empty list answers the type itself, the uninstantiated literal. The port's
`type_literal_key` keyed every open alias-evaluation binding. So
`{ id?: number | string }` inside `type Column<T> = (keyof T extends never ?
{ id?: number | string } : { id: T }) & …`, evaluated under the frame
`T := T`, was a fresh literal, and its members printed as types
(`string | number`), where native keeps the written literal and prints
`number | string`. The written-text mint of a deferred conditional hid this.
r5-mapped6's typed conditional print exposed it (its four losses at
`controlFlowGenericTypes:298/300/301/303`, `r5-mapped6.md` §2).

**Port.** `type_literal_key` keeps only the bindings for which
`is_type_parameter_possibly_referenced` holds, for exactly native's node
kinds and only when `type_alias_host_for_type_node` names no generic alias.
`is_type_parameter_possibly_referenced` ports the pinned function:
- a symbol with other than one declaration answers `true`;
- walking from the node up to the parameter's declaration container, a
  `Block`, a conditional whose `extends` clause references the parameter, or
  running off the tree (the node is outside the parameter's scope) answers
  `true`;
- otherwise `containsReference`: an argument-less type reference whose
  identifier resolves (type meaning) to the parameter; a `typeof` whose first
  identifier resolves to a declaration inside the parameter's scope, or whose
  type arguments reference it (`this`, or an unresolvable shape, answers
  `true`); a method with a body and no return annotation answers `true`, and
  otherwise only its type parameters, parameters and return type are walked.

**The members follow the key.** The instance's members resolve under the
mapper of the parameters it keys on. With none, it is the declared literal,
resolved with no mapper at all. The port's member road reads the open frames
directly: `reused_annotation_text` (`node_reuse.rs`) declines while any frame
is open. So a literal keyed on no binding, but first built inside a frame,
still printed its member as a type: `C<"a">` for
`type C<T> = T extends string ? { id?: number | string } : never` printed
`{ id?: string | number; }`. `get_type_from_type_literal` now builds the
literal under exactly its key's bindings: one frame of the kept bindings, or
none. The caller's stack is restored afterwards. The native tsgo probe
(`--declaration`) prints `{ id?: number | string; }` for that alias and for
`W<boolean>`'s `inner`, and `{ id?: string | number | undefined; }` for a
literal that names the bound parameter.

**Divergence kept.** A bound symbol that is not a type parameter answers
`true` (the old, unfiltered key). No frame binds one today; native's
`this`-type arm has no counterpart in the frames.

**Checker port convention.**
- Native operation: `typeNodeLinks.outerTypeParameters`, written once per
  declaration by getObjectTypeInstantiation.
- Key identity and owner: `(declaration NodeId, type-parameter SymbolId) ->
  bool` in `InstantiationExpressionLinks::possibly_referenced`. That links
  struct is this lane's; `Checker`'s field list is main's, so the memo lives
  there rather than in a new field. It is a `RefCell` because
  `type_literal_key` takes `&self` at about twenty call sites in other lanes'
  files.
- Publication: written once per key, never invalidated. The answer is syntax
  plus name resolution, neither of which depends on a frame.
- Receiver/alias context: none. The frames decide which symbols are asked
  about, never the answer.
- Work boundary: one ancestor walk and at most one subtree walk per key. A
  subtree walk resolves a name only when the identifier's text equals the
  parameter's name.

**Measured** (unfiltered, both dumps, against §0): diagnostics and types
unchanged, zero losses, slowcases clean. Callgrind Ir (`tsr -p <project>
--singleThreaded --pretty false --noEmit`): domain-model 1,090,817,385 ->
1,090,253,218 (-0.05%), generic-imports 343,083,431 -> 343,055,471 (-0.01%). The port changes identities only
where a literal names none of the open bindings, and today's printers render
the merged identities the same way. Its effect is the diff it unblocks (§1.1).

Tests: `tests/r6_declared.rs`:
- `a_resolved_branch_naming_no_parameter_is_the_written_literal` fails on the
  base;
- `a_literal_naming_no_bound_parameter_is_the_written_literal` and
  `a_literal_naming_the_bound_parameter_is_instantiated` pin the neighbours,
  which already passed.

**Falsifier.** A literal whose port evaluation reads a frame through a path
`containsReference` does not see. That would be a dynamic-scope leak in the
alias road, for example a non-generic alias body evaluated under the caller's
frames. Such a literal would merge two different instances, and a print
would show the first instance's members.

### 1.1 r5-mapped6's conditional-typed-print diff, re-measured (sent to r6-mapped)

[`r5-mapped6-conditional-typed-print.diff`](r5-mapped6-conditional-typed-print.diff)
applies unchanged on top of §1's commit `f9339d0` (only the line offsets
move). Unfiltered against `f9339d0`:
- types: **+5 RIGHT, 0 losses**:
  - `complicatedIndexesOfIntersectionsAreInferencable:0:5`;
  - `simplifyingConditionalWithInteriorConditionalIsRelated:0:17/19`;
  - `wideningWithTopLevelTypeParameter:0:47`;
  - `mappedTypeAsClauses:0:106`.

  r5-mapped6's four held losses (`controlFlowGenericTypes:298/300/301/303`)
  are gone.
- diagnostics unchanged; slowcases clean on both dumps.
- Ir: domain-model 1,090,253,218 -> 1,092,009,836 (+0.16%, the same +0.15%
  r5-mapped6 §2 measured), generic-imports +0.002%.

It touches `declared.rs` and `node_reuse.rs`, so it stays a diff. It was sent
to r6-mapped as the round-6 brief asks; the integrator orders it after
`f9339d0`.

## 2. `tsr-2zk.1123`: ramdaToolsNoInfinite2's 54 MB type text

**Forcing constraint.** With r5-relater7's binder diff
([`r5-relater7-binder-declare-module-imports.diff`](r5-relater7-binder-declare-module-imports.diff),
main's `binder.rs`, so it stays a diff), ramdaToolsNoInfinite2's imports
resolve. The dump then aborted: `type_reference_text` formatted a 54,525,882-byte
argument list (2.5 GiB peak, `memory allocation … failed`). Instrumented, the
port was at instantiation **depth ~90 with a count of ~4,600**; the texts
doubled at every level (`Overwrite`, `Required`, `Naked`, `Length`,
`__Reverse`). The native tsgo probe checks the same file in 0.4 s with
**48,750 instantiations, no TS2589**. Native's two bounds (depth 100, count
5,000,000, checker.go:22111) are never reached, so bounding the port's count
would not be faithful. The port recursed where native does not.

The cause is `__Reverse`:

```ts
type __Reverse<L, LO, I = IterationOf<'0'>> = {
    0: __Reverse<L, Prepend<LO, L[Pos<I>]>, Next<I>>;
    1: LO;
}[Extends<Pos<I>, Length<L>>];
```

Native's getIndexedAccessType reads the one property the index names
(getPropertyTypeForIndexType -> getTypeOfSymbol). An anonymous type's members
are resolved on demand, so arm 0 is instantiated only while the index selects
it, and the recursion ends when `Extends<…>` turns to 1. The port's
indexed-access road built the object literal first, both arms included, so
arm 0 recursed at every level until the depth guard. Each level's arguments
contain the previous level's, so the texts doubled.

**Port.** `indexed_type_literal_member` (declared.rs), first in the
`IndexedAccessTypeNode` arm: for a written type literal whose members are all
plain property signatures (written annotation, no `?`, static name) and an
index that evaluates to a string or number literal naming exactly one of
them, it answers that member's annotation type and builds nothing else. Every
other shape declines to the eager road unchanged: index signatures, methods,
accessors, optional members, and generic or union indexes.

**A cycle keeps the eager road.** `limitDeepInstantiations`' `type Foo<T, B> =
{ "true": Foo<T, Foo<T, B>> }[T]` re-enters its own instantiation key: the
selected member's first argument is the instance being computed. Native
recurses to the depth guard and caches errorType for that key, which
collapses the recursion (27,369 instantiations, TS2589). This port's alias
road declines at the guard, answering the named reference rather than
errorType (`r5-spans.md` §2.3). So a lazily read member that re-enters never
collapses. The first draft OOMed at 6 GiB, and a draft that handed only the
re-entrant read back went from 30 MiB to 975 MiB (slowcases `SLOWER`). The
arm now records the literal's publication state:
- `lazy_member_reads`: keys whose selected member is resolving;
- `eager_indexed_literals`: literal nodes that re-entered. Such a node takes
  the eager road from then on, whose reserved literal identity closed the
  cycle before this arm existed.

A self-re-entrant literal is a static property of its alias. Reading its outer
levels lazily would add one eager chain per level.

**`keyof T` is one identity.** The deferred `keyof T` mint
(`get_type_from_type_node_worker`'s keyof arm) made a new named type per
evaluation. Native's getIndexType caches one index type per generic type
(`resolvedIndexType`). The eager literal had hidden this: its member was
cached inside the cached literal. Read lazily, `x is { a: keyof T }["a"]`
evaluated `keyof T` twice, and pseudoReturnTypeMatchesPredicate's identity
test failed (`type_predicates::predicates_retain_resolved_mapped_and_indexed_types`).
The mint is now memoized per `(operand type, printed text)`.

**Checker port convention.**
- Native operations: getPropertyTypeForIndexType -> getTypeOfSymbol, with an
  anonymous type's lazy members; getIndexType's `resolvedIndexType`.
- Keys, owner and publication, all in `InstantiationExpressionLinks` (this
  lane's links struct; `Checker`'s fields are main's):
  - `lazy_member_reads`: `TypeLiteralKey`, inserted before and removed after
    the member read;
  - `eager_indexed_literals`: the literal `NodeId`, inserted once and never
    removed;
  - `deferred_keyof_mints`: `(operand TypeId, text) -> TypeId`, written once
    when minted.
- Receiver/alias context: the open alias frames, through `type_literal_key`
  (§1).
- Work boundary: one annotation evaluation per selected read, instead of the
  whole literal.

**Divergence kept.** At the guard, native answers errorType and reports
TS2589. Here a re-entrant cycle takes the eager road, as on the base. That
waits on the alias road answering errorType at the guard, which r5-spans §2.3
showed is not safe until this port stops reaching depth 100 where native does
not.

Tests: `tests/r6_declared.rs`
`an_indexed_type_literal_resolves_only_the_selected_member` (`Count<3>` is
`3`; an unresolvable sibling does not poison `["a"]`). It passes on the base
too. A doubling fixture that OOMs the base does not exercise the arm in a
lib-less unit test: its index evaluates to `error` there. The falsifier is
the conformance case itself: ramdaToolsNoInfinite2 with the binder diff
OOMs without this arm.

**Measured** (unfiltered, both dumps, against §0): diagnostics and types
unchanged, zero losses, slowcases clean on both dumps
(`limitDeepInstantiations` 45 ms / 30 MiB at the base; the rejected
hand-back draft took it to 863 ms / 973 MiB). Ir against §1's `f9339d0`:
domain-model 1,090,253,218 -> 1,089,528,215 (-0.07%), generic-imports
343,055,471 -> 343,061,332 (+0.002%). With the binder diff on top,
ramdaToolsNoInfinite2 completes (5.8 s, 249 MiB) instead of aborting; §2.1
has that diff's unfiltered numbers.
