# r5-iteration: iteration-types lane notes (round 5)

Lane `tsr-2zk.963` (iteration types) and `tsr-2zk.957` (`yield*` awaited
diagnostics). Pinned tsgo `5b1047d`; baseline frozen at `ac56208`
(diagnostics RIGHT 5070 / EMPTY_RIGHT 5551 / WRONG 1520+96 of 12238 rows).

## 1. The for-of operand reports through `checkNonNullExpression`

`checkRightHandSideOfForOf` (`checker.go:17678`) hands the iterated type
`checkNonNullExpression(statement.Expression())`, the *reporting*
`checkNonNullType` (`checker.go:7409`). `check_for_of_iteration` called the
silent `check_non_null_type` (`members.rs`), so `for (x of undefined)` lost
its TS18050 (`omittedExpressionForOfLoop`, r4-operators2 §3). It now calls
`check_non_null_type_reporting` (`nullable_operand.rs`, r4-operators'
reporter, called rather than copied): the same reporter that picks TS18050
for a `null`/`undefined` keyword operand and TS18048/TS2532 for a nullable
name or expression, and the same `errorType` answers for `unknown` and for
an all-nullable operand, after which the iteration check returns at
`IsTypeAny(errorType)` as upstream does.

**Not ported:** TS18046/TS2571 for an `unknown` operand. The shared
reporter declines it (the reason is in its own doc comment); the
iteration check still sees `errorType` and reports nothing, which is
native's behaviour apart from the missing TS18046.

**Measured** (against `ac56208`): diagnostics +1 (`omittedExpressionForOfLoop`
WRONG → RIGHT), types unchanged, zero losses in both checks.

**Falsifier.** A for-of case whose baseline lacks a TS18048/TS2532 that TSR
now reports on the operand: that points at the operand's *type* (narrowing,
or a declared type carrying `undefined` it should not), not at this call.

## 2. The iteration protocol carries native's error node (`tsr-2zk.957`)

**Forcing constraint.** Upstream threads `errorNode` and a
`diagnosticOutput` buffer through `getIterationTypesOfIterableWorker`
(`checker.go:6287`) into the slow path: `getIterationTypesOfIterableSlow`
(`:6460`), `getIterationTypesOfIteratorWorker` (`:6495`) and
`getIterationTypesOfMethod` (`:6541`), whose `resolveIterationType(t,
errorNode)` is the async resolver's `getAwaitedTypeEx(t, errorNode, Type of
'await' operand must …)` (`:1287`). This port's engine had no error node, so
every diagnostic raised *inside* the protocol was lost:

| Code | Raised by | Witness |
|---|---|---|
| TS1320 | async `resolveIterationType` of `next()`'s result (`then(){}` thenable) | `crashInYieldStarInAsyncFunction` |
| TS2490 / TS2547 | `mustHaveAValueDiagnostic`: the `next()` result has no `value` | `for-of15` |
| TS2767 / TS2768 | `mustBeAMethodDiagnostic`: `return = 0` | `for-of30` |
| TS2489 / TS2519 | `mustHaveANextMethodDiagnostic` | (none in the corpus) |

**What was ported.** `get_iteration_types_of_iterable_ex(ty, use, error_node)`
is the error-node form; the existing `get_iteration_types_of_iterable` is
its nil-node wrapper, so every type query is unchanged. Only
`check_iterated_type_or_element_type` (upstream's
`getIteratedTypeOrElementType` with `iterableExists`) passes a node. The
worker keeps one buffer across the async and sync slow attempts, as
upstream's local `diags` does, and emits it only when a slow attempt then
finds iteration types (`checker.go:6315`, `:6330`); on failure upstream
attaches the buffer as related information of `reportTypeNotIterableError`,
which this port's diagnostics do not carry, so it is dropped. Union
constituents are asked with a nil node (`checker.go:6291`). The fast paths
pass nil (`getResolvedIterationTypes`, `:6386`), and so does this port.

**Called, not copied.** The awaited resolution with a node is
`check_awaited_type` (`expressions.rs`, r4-awaited), the reporting form of
`getAwaitedTypeNoAliasEx`: its `errorType` answer is native's nil (already
reported), `None` is a step the awaited family cannot decide. Without a
node `awaited_type` cannot tell those apart, so the nil-node road still
declines native's nil; that is unchanged and is why the type of the
witness's `yield*` stays a gap.

**Accepted gaps.**

- The `Iterable` assignability elaboration that the slow path buffers when
  every `[Symbol.iterator]` signature needs arguments
  (`checkTypeAssignableToEx(t, getGlobalIterableTypeChecked(), …)`,
  `:6474`) is not built. The buffer is marked incomplete instead, and a
  success that would emit it declines (reports nothing) rather than emit
  part of the buffer. A failure drops the buffer anyway.
- The `getGlobalAwaitedSymbol()` probe of `getAsyncFromSyncIterationTypes`
  (`:6443`, TS2318 for a lib without `Awaited`) is not made.
- **No cache.** Upstream skips the walk, and therefore the diagnostics, when
  `iterationTypesCache` already holds a result *with* types from an earlier
  nil-node query (`checker.go:6273`). This port recomputes, so a reporting
  query always reports. Upstream's check sites (for-of head, spread,
  destructuring, `yield*`) are normally the first to ask for their
  operand, and the corpus agrees: after this change no case gained a
  diagnostic its baseline lacks. A case that reports a protocol diagnostic
  native suppresses would be the falsifier.

**Measured** (against `ac56208`, stacked on §1): diagnostics +3
(`crashInYieldStarInAsyncFunction`, `for-of15`, `for-of30` WRONG → RIGHT;
5071 → 5074), types unchanged, zero losses in both checks, and no other
case's diagnostic list changed.

## 3. Blocked outside this lane

- **`for-of58` (extra TS18048 ×2, three `X & Y` type lines).** The engine
  declines `X[] & Y[]`: its iterator `ArrayIterator<X> & ArrayIterator<Y>`
  has a computable `next` *type* but `get_property_of_type` answers no
  symbol for it, because `next` is inherited through `IteratorObject<T, …>`'s
  type-argument heritage (the gap `get_iteration_types_of_method`'s doc
  comment names). A missing symbol that is not decidably absent declines.
  The plain-name for-of variable then falls to `symbols.rs`'
  `for_of_yield_types`, which answers `(X & Y) | undefined`. Owner:
  `members.rs` (inherited-member symbol through a type-argument base).
  Probed: routing the plain-name arm through `for_of_iterated_type` first
  (as `destructure.rs` already does) changes nothing until the lookup is
  fixed. A second step will then be needed here: the two `next` return
  types are alias references (`IteratorResult<X, undefined>`), which this
  port's intersection does not distribute the way native's union body
  does. That step was written, changed nothing on `for-of58` or on a
  hand-written intersection probe, and was reverted, since nothing reaches
  it yet.
- **Tuple iteration (`ES5For-of30`, TS2488 on `for ([a = 1, b = ""] of
  tuple)`).** The engine declines `[number, string]` for the same reason:
  a tuple's `[Symbol.iterator]` is inherited from `Array<T>` through a
  type-argument base, and the lookup answers no symbol. The case also needs
  `checkForOfStatement`'s `checkDestructuringAssignment(varExpr,
  iteratedType)` source for an assignment-pattern initializer: a
  `ForOfStatement` arm in `destructuring_assignment_source` was written and
  measured (both dumps unfiltered against §5: no verdict, diagnostic list or
  type line changed), and reverted, since every source it reaches is the
  declined tuple. It is a few lines to restore once the lookup answers.
- **`restElementWithNullInitializer` (TS2488 ×3).** `function f([...r] =
  null)` and the `undefined`/`{}` twins: the pattern's parent type comes from
  `destructure.rs`' parameter road (`pad_binding_parameter_type` over the
  widened initializer), which answers no decidable type for these
  initializers, so the iteration check is never reached. Owner: destructure.
- **`asyncIteratorExtraParameters` (TS2504 ×2).** The for-await query
  declines on the sync fallback: `[Symbol.iterator]` is not *decidably*
  absent on an object-literal type whose members include a computed name
  (`object_literal_property_table`, `member_completeness.rs`, declines
  computed names by design). Native's lookup misses and reports TS2504.
  Owner: member completeness. Once the absence is decidable, §2's
  incomplete-buffer rule already gives TS2504 without the related TS2322.

## 4. `getIteratedTypeOrElementType`'s sent-type check (TS2763–TS2766)

With `checkAssignability` (every `checkIteratedTypeOrElementType` call),
upstream checks the caller's *sent* type against the protocol's *next*
type before reading the yield type (`checker.go:6118`):
`checkTypeAssignableTo(sentType, nextType, errorNode, head)`, where the
head is chosen by the use's flag in the order for-of, spread,
destructuring, `yield*`. `check_iterated_type_or_element_type` now takes
`sent_type` and makes that check through `report_relation_failure` with
the head; an undecidable relation reports nothing.

The sent type is `undefinedType` at every check site but `yield*`
(`checker.go:17680`, `:8064`, `:17749`, `:12648`, `:5876`). `yield*` sends
`checkYieldExpression`'s `signatureNextType` (`checker.go:10993`): the
annotated return type's next iteration type orElse `anyType`, which is the
existing `annotated_yield_next_type` (r4-arrays' port of the same lines);
without an annotation upstream has no iteration types and sends `anyType`.
An annotation this port cannot decide sends `errorType`, which skips the
check.

**Measured** (against §2): `generatorAssignability` gains 9 of its 11
lines (TS2764, TS2765 ×4, TS2763 ×2, TS2766 ×2); no other case's
diagnostic list changed; zero losses in both checks. The case stays WRONG
on its two async-over-sync lines (`for await (_ of g1)`, `yield* g1` in an
async generator): the async slow attempt declines because
`[Symbol.asyncIterator]` is not decidably absent on `Generator<…>`, whose
members come through type-argument heritage (§3's `members.rs` gap), so
the sync fast path that native reaches next is never asked.

## 5. `getIterationTypesOfIterable` iterates the reduced type

`getIterationTypesOfIterable` starts with `t = c.getReducedType(t)`
(`checker.go:6266`), so `{ a: "foo" } & { a: "bar" }` is iterated, and
reported, as `never`, and a union drops such constituents
(`getReducedUnionType`, `:21844`). The engine iterated the written
intersection, whose `[Symbol.iterator]` lookup is not decidably absent, and
declined. `iteration_reduced_type` is that step, built on the shared
`isDiscriminantWithNeverType` port (`intersection_has_never_discriminant`,
`flow.rs`); `reportTypeNotIterableError` names the reduced type, as the
worker's does.

The slow path's property lookup also takes `getPropertyOfType`'s
`getReducedApparentType` (`:21860`) for a type parameter: `T extends
{ a: "foo" } & { a: "bar" }` has no members, so `[Symbol.iterator]` is
absent and TS2488 names `T`. Only the `never` answer is taken from the
reduced apparent type; any other type parameter keeps the existing lookup.

**Not ported:** `isConflictingPrivateProperty`, the other half of
`isNeverReducedProperty`. The shared predicate does not cover it, so an
intersection of classes with conflicting private members stays unreduced
here.

**Measured** (against §4): diagnostics +1 (`iterableWithNeverAsUnionMember`
WRONG → RIGHT, all three TS2488), types unchanged, zero losses in both
checks, and no other case's diagnostic list changed.

## 6. The two "known failing" workspace tests

`iteration::optional_tuple_check_types_preserve_named_enum_identity_and_reads`
and `mapped_tuple_inference::tuple_slice_optional_arguments_follow_null_and_exact_optional_options`
(r4-perf, r4-config and r4-arrays record them failing) **pass** at this
round's base `ac56208`, and also at the round-4 wrap-up `f90fcef`, measured
in a separate worktree and target directory (`cargo test --release -p
tsr-conformance --test iteration --test mapped_tuple_inference`: 17/17 and
5/5). The workspace run is 3239 passed, 0 failed at every commit of this
lane. Something merged during round 4 fixed them; no root cause was left
to find here, and neither test's expectation was touched.

## 7. Round-5 totals

Against the frozen baseline `ac56208`, at `ef489c7`:

- Diagnostics: RIGHT 5070 → 5075 (+5: `omittedExpressionForOfLoop`,
  `crashInYieldStarInAsyncFunction`, `for-of15`, `for-of30`,
  `iterableWithNeverAsUnionMember`), plus 9 of `generatorAssignability`'s 11
  lines. EMPTY_RIGHT 5551 unchanged.
- Types: unchanged (543,119 RIGHT of 552,533 aligned lines).
- Both loss checks empty at every commit.

## OSS spread/iteration diagnosis (2026-10-13, tsr-2zk.1268.9)

Source baseline `adbc3c1ecb6c000bbbcc4007c250569dcfc2a78f`, native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. Fixture pins, complete project
diagnostics and install/setup receipts are in
`oss-projects-2026-10-13-fixed.json`. The three reported symptom families
do not establish a common spread or iteration cause.

**Zod:** checking the capsule's `packages/zod/src/v4/classic/schemas.ts`
with `--strict --target esnext --module nodenext --skipLibCheck --noEmit
--pretty false` reproduces ten source TS2556 diagnostics (268–274,
1470/1472/1473); the other ten in the full receipt come from the installed
zod@4.0.8 source copy. Temporary instrumentation at
`calls.rs::effective_call_arguments` confirms these operands are `any[]`,
not tuples with missing metadata. The initializer parameter has fallen
back to `T`/`ZodTrait` before the rest parameter is typed. Ordinary
contextually typed rest parameters and explicit optional tuple spreads
already work. Explicitly annotating `$constructor<Base, core.$ZodStringDef>`
instead of `$constructor<Base>` removes the entry's errors in baseline
TSR; native accepts both. The constructor lane traced the omitted
`D = T["_zod"]["def"]` default to inherited `_zod` resolution:
`members.rs::heritage_entity_symbol_worker` reads the barrel owner's raw
exports instead of its export-star-aware lookup. That producer belongs
to `tsr-2zk.1268.3`, not tuple-spread recognition. Its correction is not
yet verified on the real-import entry; `.3`/`.4` remain dependencies.
Redeclaring `_zod: T` on the entry's `Base` still fails in baseline TSR,
so bypassing that inherited property alone is insufficient. Explicit D
is a diagnostic discriminator, not an accepted source workaround.

This entry, placed beside the real classic schemas, reproduces the
constructor-context failure without an initializer inner call or binding
pattern:

```ts
import * as core from "../core/index.js";
declare function min(n: number, p?: string): void;
export interface Base<
  T extends core.$ZodStringInternals<unknown> = core.$ZodStringInternals<unknown>
> extends core.$ZodType<string, unknown, T> {
  min(n: number, p?: string): this;
}
export const Base: core.$constructor<Base> = core.$constructor("Base", (inst, def) => {
  inst.min = (...args) => min(...args) as any;
});
```

Baseline TSR reports TS2339 (`min` on `ZodTrait`) and TS2556. The original
entry with `core.$ZodString.init(inst, def)` instead reports TS2345 for
`inst: ZodTrait` plus TS2556, while native reports no errors. A schema
declaring its own `_zod: { def: unknown }` and the same `min` method in
the same entry passes. A standalone simplified constructor and schema
also pass; that is not a reproducer for the actual import/inheritance
contract. These entry checks are reductions, not complete OSS-project
validation.

**Effect:** the sink/stream `never` diagnostics occur when destructuring
`pipe(input, Chunk.splitAt(...))`. Direct `never` iteration is also
rejected by native; it must not be suppressed in `iteration.rs`.
Generic-inference ownership must correct the pipe result first. The
other Effect diagnostics (`{} | null` spreading the queue returned by
`tRef.unsafeGet`) are separate leads, not evidence of the same cause.

**Excalidraw:** this reduced contract under `--strict --target esnext`
has two TS2504 reports in baseline TSR, but only the unguarded loop is
rejected by native:

```ts
declare const result: AsyncGenerator<number, void, any> | Promise<void>;
async function consume() {
  if (Symbol.asyncIterator in result) {
    for await (const value of result) {}
  }
  for await (const value of result) {}
}
```

`flow.rs`'s `InKeyword` dispatch uses `property_name_from_index`, which
lacks a unique-symbol arm, so the guarded operand still contains
`Promise<void>`. Flow and computed-key owners must coordinate semantic
property keys; the iteration engine correctly rejects the unguarded
union. No compiler change, cache, mapper, member image or traversal is
introduced by this lane; existing iterator publication, concrete receiver
and diagnostic-buffer contracts remain unchanged.

**Safety contract:**
`iteration::tuple_spreads_and_iteration_keep_valid_and_invalid_contracts_distinct`
pins native's exact eight-diagnostic multiset through the public
conformance diagnostic collector. It accepts contextual/readonly optional
tuples, ordinary arrays passed to array-rest parameters, sync iteration,
async iteration, async-from-sync iteration and `Iterable<never>`. It
rejects a reversed tuple's element type, fixed-parameter spreads of both
`number[]` and `any[]`, non-iterable objects, direct `never`, and an async
iterable used in sync iteration. This guards against fixing upstream
operand defects by suppressing valid spread/iteration errors.

Complementary validation uses a disposable worktree at the source baseline;
no copied compiler changes enter this lane's commit. TSR runs set
`TSR_LIB_PATH=/home/user/workspace/repo/vendor/typescript-go/internal/bundled/libs`;
native uses the same pin's bundled libs. The source capsule
has no installed package dependencies; the reduced Excalidraw/controls
checks use only native lib declarations, and the Zod entry uses the pinned
capsule's internal source imports. Complete frozen-install project checks
remain the parent's combined-integration gate.

The computed-key owner's `target/computed-key-preview.patch` from
`tsr-2zk.1268.2` was transferred, applied at the source baseline and built
with `cargo build -p tsr --locked --target-dir ../spread-validation-build`.
This is preview-snapshot evidence, not the owner's final verified commit.
The combined 19-line spread/iteration fixture (SHA below), checked with
`--strict --target esnext --noEmit --pretty false`, produces these exact
position/code multisets:

| Run | Diagnostics (line, column, code) |
| --- | --- |
| Unchanged TSR | `(8,5,2556) (10,17,2488) (12,17,2488) (16,31,2504) (18,27,2504)` |
| Computed-key preview | `(8,5,2556) (10,17,2488) (12,17,2488) (18,27,2504)` |
| Pinned native | `(8,5,2556) (10,17,2488) (12,17,2488) (18,27,2504)` |

Native and preview CLI output is byte-identical, including the remaining
unguarded union message. The unchanged TSR binary was copied before any
instrumentation; the preview binary was built separately. The preview also
passes all 38 targeted conformance tests, including the eight-diagnostic
safety contract. Zod's ten-line real-import entry still produces
`(9,8,2339) (9,31,2556)` in baseline and preview; native reports nothing.
No full OSS spread parity or Effect inference repair is claimed here.

Validation identities (SHA-256):

| Input | Identity |
| --- | --- |
| Applied computed-key preview patch | `aa32c0f9ec4234fbad5347d696f82f6af03bc3a184a55832fbe0ea4fd53e3161` |
| Unchanged TSR binary | `e4cf5c81046a4f7671be4d630a8906d6e486a9f77a438c0e45678c46757f9bf5` |
| Computed-key preview TSR binary | `090ce27a8d8365e5041f0d599d200eb9447caed165af6e21cab3b7c5d20d9746` |
| Pinned native binary | `fc7d6c8713c70bafce83e53bd401fd8191e2f0c3f6024e6d891b10da730b4437` |
| Source capsule | `a54970d2e32a684a18d464ff9268a81d85113b43061c91c5f9e83e8193eb5525` |
| Complete OSS receipt | `1cc607d56c746b461516b9078a52b941c7f807e864006294e837e29a100cb8f5` |
| Zod source pin | `3b946107b6c94b2ac8ff9fb451160c34dc4dd794` (Git commit) |
| Zod capsule pnpm lock | `4c44e700f62629199e8b874ab523a71f92a97b3d50ec8a3615599abcb04f03c7` |
| Zod capsule tsconfig | `79ff8445180c036872e7c6ada7c49993f7b4dc9523bea9060731773c4a9cced2` |
| Ten-line Zod entry above | `58581e2c5babfc0f8cf4ea242f66d09d9efaa5f80fd6590b43bdb9896ed52570` |
| Explicit-D entry | `8f076a98feed2e8028b3eed4f9a6ce2f0e926950e4711f26e315ecb406bcc76e` |
| Combined 19-line spread/iteration fixture | `97df55e260d588d502d9065aa3607e81a3631d8473a1f39e480f456526e6ed4f` |
| Kept 31-line safety source (without harness directives) | `fa98b375290a1ec7a75ab9de9ab524706fab53833d71756acae4563b1cc9c28e` |

The safety source is checked with `--strict --target esnext --noEmit
--pretty false`: native and baseline TSR have identical eight diagnostics.
All 187 targeted checker/conformance tests pass; `cargo fmt --all --
--check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`
pass. A disposable mutation suppressing direct-never iteration reporting
makes the safety test fail by losing its TS2488 and TS2504; restoring the
original branch passes again. This lane makes no compiler behavior change,
so it cannot change previously RIGHT compiler outputs.

The targeted commands (149 checker tests and 38 conformance tests) are:

```sh
cargo test -p tsr-checker --test call_reports --test generic_calls --test contextual --test contextual_overload_arity --locked
cargo test -p tsr-conformance --test iteration --test async_iteration --test destructuring_iteration --test spread_inference --test spread_batches --test implied_arity --locked
```

The second command also passes on the transferred preview with
`--target-dir ../spread-validation-build`. The disposable worktree and
uploaded controls remain available for final-snapshot validation. Parent
owns the final `.2` snapshot, unresolved real-Zod `.3`/`.4`, Effect's `.4`
inference work, full-project regression checks, and main delivery; this
handoff does not close those compiler dependencies.
