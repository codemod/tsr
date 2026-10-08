# r5-tables — member tables that declined where native resolves (`tsr-2zk.1014`)

Round-5 cloud lane. Native source is `vendor/typescript-go` @ `5b1047d`,
`internal/checker/checker.go` unless noted. Owned files:
`member_completeness.rs`, `destructure.rs`.

## 1. Object-literal tables read late-bound computed names

**Forcing constraint.** `asyncIteratorExtraParameters` (TS2504 ×2):

```ts
const iter = { async *[Symbol.asyncIterator](_: number) { yield 0; } };
for await (const _ of iter);
```

The async slow attempt finds `[Symbol.asyncIterator]` but its signature
needs an argument, so native falls back to the sync protocol, misses
`[Symbol.iterator]`, and reports TS2504. This port's iteration engine
(`iteration_member_decidably_absent`, `iteration.rs`) only accepts a miss
that a complete table certifies, and `declared_property_table` answered
`None` for the widened literal type of `iter`, for two reasons:

1. it never read an object literal at all (only `relation_property_table`
   did, through `object_literal_property_table`);
2. `object_literal_property_table` declined any computed name.

**What native does.** `checkObjectLiteral` gives a computed member whose
name type is `isTypeUsableAsPropertyName` (a string/number literal or a
unique symbol) a named property, the same name `lateBindMember` gives a
class or interface member. Any other computed name contributes an index
signature (or nothing, after an error). The widened literal's member list
is the literal's property list. Widening changes member types but not
names.

**What is ported.**

- `object_literal_property_table` admits a computed name when
  `computed_member_index_key` (`objects.rs`, upstream's
  `StringOrNumberLiteralOrUnique` dispatch, `checker.go:13317`) answers
  `LateBound`. Its name is the one `late_bound_members_of` (`members.rs`,
  `lateBindMember`) records for the literal's symbol. If any computed member
  does not late-bind, or the late-bound list is shorter than the computed
  member count, the table still declines. A written name and a late-bound
  name that collide are one row.
- `declared_property_table` falls back to `object_literal_property_table`.
  Its consumers ask whether a name is decidably absent from a complete
  table: the iteration protocol, the for-of yield resolver (`symbols.rs`),
  array spread, unique-symbol index reports, the primitive-source negative
  and the property-name candidates. A widened literal's table answers that
  truthfully. The TS2339 predicate (`declared_members_are_complete`) is
  separate and unchanged. Its object-literal hazard (a union's
  `createUnionOrIntersectionProperty` reads a member a literal constituent
  lacks as `undefined`) does not apply to a single complete table.

**Rejected.** Changing `iteration_member_decidably_absent` to read
`relation_property_table` (`iteration.rs`, not owned). It would convert the
witness too, but the for-of yield resolver and the spread check ask the same
question through `declared_property_table`, and they would keep declining.

**Falsifier.** A consumer of `declared_property_table` that reports a missing
member on a widened object literal that native finds: through an expando
assignment (JS literals stay declined), a spread (declined) or a computed
name this port late-binds under a different spelling.

## 2. A parameter's binding parent is the unwidened initializer type

**Forcing constraint.** `restElementWithNullInitializer(target=es2015)`
(TS2488 ×3) is `@strict: false`:

```ts
function foo1([...r] = null) {}       // TS2488 'null', r: any[]
function foo3([...r] = {}) {}         // TS2488 '{}',   r: any[]
```

`getTypeForBindingElementParent` (`checker.go:17695`) reads
`getTypeForVariableLikeDeclaration` with `includeOptionality` false. For an
uncontextual parameter with an initializer, that is
`widenTypeInferredFromInitializer(checkDeclarationInitializer(..))`
(`:16749`): literal widening only, never `getWidenedType`. So the parent is
`null`, which is not iterable. This port's parameter road answered
`get_widened_type_for_variable_like_declaration`, the declared type, and
under non-strict that is `any`. `any` short-circuits the iteration check and
makes `r` `any`.

**What is ported** (`get_type_for_binding_element_parent`, the admitted
parameter arm). An initializer is checked, padded
(`pad_binding_pattern_initializer`, `checkDeclarationInitializer`'s
`padObjectLiteral`/`padTupleType`), and widened with
`widen_type_inferred_from_initializer`. That helper also folds in
`getWidenedTypeWithContext`'s nullable arm, which this road must not run, so
a purely nullable type returns as is under non-strict.

Two consequences follow, both upstream's:

- **The rest element on a failed protocol.** `checkIteratedTypeOrElementType`
  answers `anyType` when iteration fails (`checker.go:6103`), and the rest
  element is `createArrayType` of it (`:17766`). The array arm now builds
  `any[]` when `get_iteration_types_of_iterable` answers an empty, decided
  result.
- **The symbol road widens.** Now that a parent can hold widening
  nullables, the element's declared type needs
  `getWidenedTypeForVariableLikeDeclaration`'s `getWidenedType`
  (`checker.go:16647`). With `function foo4([...r] = [])`, `r` is
  `undefinedWidening[]` → `any[]`. `get_type_for_binding_element` (the
  symbol entry) applies `widen_object_literal_freshness` (this port's
  `getWidenedTypeWithContext`). The nested-pattern parent road calls the
  unwidened worker, as `getTypeForBindingElementParent` does.

**Falsifier.** A pattern parameter whose element type now prints a fresh
or widening type where native prints the widened one. That would mean the
symbol entry is bypassed somewhere.

## 3. Measured

Against the frozen baseline `f4ae684`, all measurements unfiltered:

| | diagnostics | types | losses |
|---|---|---|---|
| §1 alone | +1 case (`asyncIteratorExtraParameters`) | ±0 | none on either dump |
| §1 + §2 | +2 cases (adds `restElementWithNullInitializer(target=es2015)`) | +7 lines | none on either dump |

No other case's diagnostic list changed. Median child CPU for §1 + §2
against the baseline binary, at 21 samples: domain-model 1.014,
generic-imports 1.006, and diagnostics match. callgrind Ir
(`tsr -p tsconfig.json`): domain-model 1,318,764,032 → 1,319,533,864
(+0.06%), generic-imports 400,842,815 → 400,845,447 (+0.0007%).

## 4. Remaining, with the blocker each was traced to

- **Binding-pattern props (`tsxStatelessFunctionComponents1` 29:15,
  31:15).** `function Meet({name = 'world'})`: the props type is
  `binding_pattern_object` (`binding_patterns.rs`) with `owner: None`. Its
  `anonymous_properties` list is complete, but nothing distinguishes it
  from the other producers that publish `(properties, true)`; the `bool`
  means "synthetic lookup", not completeness. Certifying it needs a marker
  at the producer, which is not this lane's file. Even with the marker,
  29:15 (`name={42}`) also needs the relater's structural arm to relate
  against a members-less `Named` (`relater.rs`).
- **Spread and binding-initializer object literals.**
  `object_literal_property_table` still declines a literal with a spread
  (its properties live only in the `getSpreadType` result) and the
  initializer of an object binding pattern (padding adds members). Neither
  was traced to a converting case in this lane.
