# r6-relater3 — single-code relation rows, the `||` type-parameter arm, MAIN diffs (`tsr-2zk.1148`)

Lane under epic `tsr-2zk`, round 6, successor to r6-relater2
([`r6-relater2.md`](r6-relater2.md) on its branch; §7 and §8 there are the
hand-off this lane works from). Owns `relater.rs`, `relation_cache.rs`,
`variances.rs`, `identity.rs`, `index_access_reports.rs`, `assignreport.rs`,
`binary.rs`, its tests under `crates/tsr-checker/tests/` and this file.
Native anchors are `vendor/typescript-go` @ `5b1047d`; every test
expectation was checked against a native tsgo built from that submodule
(`scripts/offline-cargo/build-tsgo.sh`).

## 0. Frozen base

Batch BR (r6-relater2) had not landed on `claude/beautiful-shannon-ar5gh0`
when this lane started; the base is that branch's tip, `5e4d21b` (after
batch BM).

- `diagverdictdump`: RIGHT 5614, EMPTY_RIGHT 5606, WRONG 979, EMPTY_WRONG 39;
- `verdictdump`: RIGHT 550280, WRONG 5266, GAP 757;
- callgrind `Ir` of the base `tsr` (`-p <project> --singleThreaded --pretty
  false`): generic-imports 343,108,876; domain-model 1,091,067,960.

Every `Ir` below uses that command, and each run's complete CLI output is
compared with the base's (`cmp`). Loss checks are `box-protocol.md` §5's, on
`cut -f1,2`, unfiltered; slowcases runs on both dumps.

## 2. Diff: the `||`/`??` type-parameter arm (`logicalOrOperatorWithTypeParameters`)

[`r6-relater3-logical-or.diff`](r6-relater3-logical-or.diff), against
`5e4d21b`.

**Forcing constraint.** checkBinaryLikeExpression's `||` and `??` arms
(checker.go:12509, :12518) answer `getUnionTypeEx([filtered left, right],
UnionReductionSubtype)` for every operand pair. `check_logical_or_coalescing`
(`binary.rs`) answered the error type for any pair with a type-parameter or
`unknown` constituent (or a reference with such an argument), so `t || u`
was untyped and `var r4: {} = t || u` (lines 5 and 14) reported nothing.
Natively it is `U | NonNullable<T>`, and `U -> {}` fails.

**Ported (`binary.rs`).** The decline is gone: a pair that is neither
identical, `any`, nor a known refinement goes to getUnionType's subtype
reduction (`union_with_subtype_reduction`, removeSubtypes), whose own
undecidable answer stays the error type as before.

**Why the bare lift overflowed the stack.** `discriminatedUnionJsxElement`:
`const v = data.menuItemsVariant ?? ListItemVariant.OneLine` is now
`MenuItemVariant | ListItemVariant.OneLine`, a type generic with a union
constraint. In `<ListItem variant={v} />` the reference `v` then asks for
its contextual type (getNarrowableTypeForReference's
hasContextualTypeWithNoGenericTypes), which discriminates the attributes
(discriminateContextualTypeByJSXAttributes), which checks `v` again, which
asks for its contextual type, and so on. Native ends this cycle in
getContextFreeTypeOfExpression (checker.go:7542): it pushes `anyType` as the
initializer's contextual type before checking it, and getContextualType
answers a pushed node first (findContextualNode, :29350). The port had no
contextual-type stack, so nothing ended it. No depth guard: the cause is the
missing push.

**Ported (MAIN and r6-errorsplit2 files).**
- `Checker::contextual_infos` (`checker.rs`), native's `contextualInfos`:
  a stack of `(node, type)`, pushed and popped around one check;
- `get_contextual_type` (`contextual.rs`) answers a pushed node first
  (findContextualNode);
- `jsx_discriminant_value_type` (`jsx_intrinsic.rs`) pushes `any` for the
  attribute initializer around its context-free check, as
  getContextFreeTypeOfExpression does. Its template-expression decline,
  which waited for exactly this scoped any-context, is lifted.

**Convention record** (`contextual_infos`): native operation
pushContextualType/findContextualNode; key the expression `NodeId` (node
identity, as natively); owned by the checker; published by push and removed
by the matching pop, so no entry outlives the check that pushed it; no
receiver or alias context; no expensive work of its own. Only
getContextFreeTypeOfExpression's push is ported; native's other pushes
(checkExpressionWithContextualType, the inference caches) are not, and the
`isCache` flag is not needed until one of them is.

**Measured** against §0, both loss checks empty, slowcases clean:
- diagnostics: `logicalOrOperatorWithTypeParameters` WRONG → RIGHT (RIGHT
  5615, WRONG 978);
- types: +18 lines (RIGHT 550298, WRONG 5260, GAP 745):
  `logicalOrOperatorWithTypeParameters` 8, `nonNullableTypes1` 4,
  `discriminatedUnionJsxElement` 2, `neverType` 2,
  `nullishCoalescingOperator_not_strict` 2;
- `Ir`: generic-imports 343,108,876 → 343,043,689 (−0.02%); domain-model
  1,091,067,960 → 1,090,813,230 (−0.02%). CLI output identical.

**Falsifier.** A context-free JSX discriminant that natively keeps a
contextual type it does not get from the pushed `any` (the port pushes for
the initializer as written, natively too); or a `||` pair whose subtype
reduction the port decides differently from removeSubtypes, now typed where
it was the error type.

Tests (in the diff, `crates/tsr-checker/tests/r6_relater3_logical_or.rs`):
`a_type_parameter_operand_of_or_reduces_by_subtype`,
`a_context_free_jsx_discriminant_meets_its_pushed_any_context` (overflows
the stack without the push).

## 3. Diff: the generic-call object-literal argument gates (`excessPropertyCheckWithEmptyObject`, `reverseMappedTypeLimitedConstraint`, `indexedAccessRelation`)

[`r6-relater3-call-excess.diff`](r6-relater3-call-excess.diff), against
`5e4d21b`; independent of §2's diff (no shared file).

**Forcing constraint.** `check_instantiated_candidate_arguments`
(`calls.rs`, MAIN) is the port's isSignatureApplicable with `reportErrors`
for a generic candidate (checker.go:9256). Natively each argument is
re-checked under the instantiated parameter (checkExpressionWithContextualType,
:7484); the port reuses the argument's cached type, so its object-literal
arm declined whenever the target mentions a literal type
(`type_mentions_literal`: a literal member may be preserved under the
instantiated context and widened in the cache) or either side could contain
type variables (`head_could_contain_type_variables`). Both gates stood in
front of the excess-property report too:
- `Object.defineProperty(window, "prop", { value, readonly: false })`: the
  target `PropertyDescriptor & ThisType<any>` mentions `boolean`
  (`true | false`), so TS2353 on `readonly` was never reported;
- `k({ x: 1, y: 2 })` with `k<T extends number>(p: { x: T })`: the target
  `{ x: 1 }` is a literal;
- `this.setState({ a: a })` (`indexedAccessRelation`): the target
  `Pick<S & State<T>, "a">` contains type variables.

**Ported.**
- hasExcessProperties (relater.go:2714) runs on a fresh literal before any
  structural comparison, and against a non-union target it reads only the
  literal's property *names*, which literal preservation does not change.
  The arm now asks `report_fresh_literal_excess_property` (`assignreport.rs`,
  the existing hasExcessProperties port, which handles intersection targets
  where the older `check_excess_properties` declined them) before the
  literal gate. A union target stays behind the gate: its discriminant
  reduction (findMatchingDiscriminantType) reads the members' *types*.
- The type-variable gates are lifted on this arm. The relation is asked
  first, and it answers `Unknown` for a pair it cannot decide; the gates
  were the arm's older stand-in for that. Measured: no verdict or line moves
  on the base, and `indexedAccessRelation` converts once r6-relater2 §5's
  relation (batch BR) is in.

**Alternative.** A real checkExpressionWithContextualType (re-checking the
literal under the instantiated parameter, with a contextual-type push like
§2's) would lift `type_mentions_literal` entirely. That is the faithful end
state, but it needs a per-context type for an expression the port caches
once per node (`node_types`); not attempted here.

**Measured** against §0, both loss checks empty, slowcases clean:
- diagnostics: `excessPropertyCheckWithEmptyObject`,
  `reverseMappedTypeLimitedConstraint` WRONG → RIGHT (RIGHT 5616, WRONG 977);
- with batch BR merged (`origin/…-r6-relater2` at `587bc84`), against BR
  with only the excess reorder: also `indexedAccessRelation` WRONG → RIGHT,
  nothing else moves in either dump. (Measured on the diff before the union
  restriction below, which does not touch that case's non-union target.)
- types unchanged;
- `Ir`: generic-imports 343,068,763 → 343,072,504 (+0.001%); domain-model
  1,090,870,214 → 1,090,114,812 (−0.07%), each against a re-run of the base
  binary (base runs vary by ~0.02%: 343,108,876 and 1,091,067,960 on the
  first). CLI output identical.
- The first draft also ran the early report for union targets: domain-model
  +0.43% `Ir` (160 `find_matching_discriminant_type` calls, 5.7M `Ir`) for no
  verdict change. That is the reason for the union restriction.

**Falsifier.** An excess property the cached literal has but the literal
re-checked under the instantiated parameter would not have (a computed
name whose type depends on context); or a pair with type variables that the
relation now decides NotRelated wrongly and the arm reports.

Test (in the diff, `crates/tsr-checker/tests/r6_relater3_call_excess.rs`):
`an_instantiated_parameter_reports_a_literal_excess_property_first`.
