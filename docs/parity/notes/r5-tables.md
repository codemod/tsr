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

## 2. Measured

§1 alone, against the frozen baseline `f4ae684`: diagnostics +1 case
(`asyncIteratorExtraParameters` WRONG → RIGHT), zero losses on both dumps.

## 3. Remaining, with the blocker each was traced to

See the lane's final report.
