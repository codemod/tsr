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
