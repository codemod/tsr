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
