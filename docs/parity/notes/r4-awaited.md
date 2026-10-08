# r4-awaited — the `getAwaitedType` family

Lane notes for round 4, lane `r4-awaited` (`tsr-2zk.10.1`, `tsr-2zk.10.5`).
Native source: `vendor/typescript-go` @ `5b1047d`, `internal/checker/checker.go`.
Owned code: `awaited_type`, `awaited_type_no_alias`, `awaited_type_no_alias_worker`
and the promised/awaited functions this lane adds, all in
`crates/tsr-checker/src/expressions.rs`.

## 1. A generic alias reference awaits its body (`tsr-2zk.10.1`)

**Forcing constraint.** Native `getTypeAliasInstantiation` returns the
instantiated *body* with an alias attached, so `PromiseOrValue<U>` (declared
`type PromiseOrValue<T> = Promise<T> | T`) is a union when
`getAwaitedTypeNoAliasEx` (`:31270`) sees it: the union arm maps
`Promise<U> → U` and `U → U`, and `createAwaitedTypeIfNeeded` wraps the
result, giving `Awaited<U>`. This port holds generic alias references as
`TypeData::Named` references keyed in `type_reference_targets`
(`docs/parity/notes/type-refs.md`); none of the worker's arms matched, so
`await callback()` answered `PromiseOrValue<U>` unchanged, and
`await y` for `y: PromiseOrValue<number>` answered `PromiseOrValue<number>`
instead of `number`. Reproduced with `examples/probefile` before the change.

**Decision.** In the worker, a reference whose target is a `TYPE_ALIAS`
symbol is projected to its body with the existing
`binding_type_alias_body` (`destructure.rs`, `evaluate_alias_body`'s
cached evaluation) and the body is awaited. When the awaited body comes back
as the body itself, the reference is returned instead: native returns `t`
there, alias intact, and the printed identity (`PromiseOrValue<T>`) must not
become the expanded union.

**Alternative rejected.** Expanding alias references at their producer (so the
worker sees a union, as native does) is the type-alias lane's model change
(`tsr-2zk.16.2`, `main`), not this lane's; every alias consumer would move.

**Consequences.** The `await` type of every generic-alias operand changes; at
the time of the change no judged corpus line moved (both dumps byte-identical),
because the cases that show it (`discriminateWithOptionalProperty2`, two
`@exactOptionalPropertyTypes` variants) are not judged by the types suite, and
the diagnostics they block need the async arm of `checkReturnStatement`
(§4).

**Falsifier.** A line where an awaited alias reference prints its expanded
body where native prints the alias name, or `Awaited<…>` where native keeps
the alias.

## 2. `checkAwaitedType` reports; TS1320 at `await` (`tsr-2zk.10.1`)

**Forcing constraint.** `getAwaitedTypeNoAliasEx` answers `nil` in two
decided situations — a recursive fulfillment type (TS1062, `:31289`/`:31341`)
and a non-promise thenable (the caller's message, `:31372`, chained under
TS2684 when a `then` signature's `this` rejected the receiver) — and its
callers report them through `checkAwaitedType` (`:31232`). The port's worker
answered `None` for those *and* for undecidable steps, so no caller could
report without risking a diagnostic on a gap.

**Decision.** The worker returns `Option<Native>`: `Some(Native::Type(t))`
native's type, `Some(Native::Nil)` native's `nil`, `None` a gap. The public
`awaited_type_no_alias` flattens it (callers unchanged). The new
`check_awaited_type` collects the walk's reports and emits them only when the
whole walk is decidable — a report from one union constituent is not emitted
if a later constituent gaps. Arms now follow native exactly where they had
diverged:

- the union arm is `mapType`: a `nil` constituent is dropped, all-`nil` is
  `nil`, an unchanged union is returned as itself (it used to gap the whole
  union on any `None`);
- a union already on the stack is TS1062 `nil` (it used to gap);
- `getPromisedTypeOfPromiseEx` is a separate step (`promised_type_of_promise_worker`)
  run before `isThenableType`, as native orders them; its `nil` exits
  (no `then`, no call signatures, every signature's `this` rejecting the
  receiver, an `any` or `never` fulfillment callback, a callback with no call
  signatures) are now `nil` instead of gaps. `never` is listed because
  `getSignaturesOfType(never)` is empty natively while
  `signatures_of_type_kind` declines it;
- the primitive exclusion is one helper,
  `all_types_assignable_to_primitive`, shared by both steps.

The `PromiseLike<T>` short-circuit (pre-existing) is kept: native reaches the
same `T` through the `then` walk, since `onfulfilled`'s first parameter is
declared `value: T`.

`checkAwaitExpression`'s report runs from the diagnostics walk
(`check.rs`, `Node::AwaitExpression` arm, one added call) through
`check_await_operand_awaited`, so it is made once per node; the expression's
type stays `check_await_expression`'s. The "`await` has no effect"
suggestion is not ported (a suggestion, not judged).

**Converted.** `conformance/await_incorrectThisType` (TS1320 chained under
TS2684). `asyncFunctionDeclaration15_es6` gains its line-23 TS1320 (still
WRONG on TS1064/TS1058, §3).

**Not here.** `crashInYieldStarInAsyncFunction`'s TS1320 is
`getIterationTypesOfIterable`'s `getAwaitedTypeEx(…, errorNode, nil)` for
`yield*` (`:6446`), in `iteration.rs` (r4-arrays' file).

**Falsifier.** A TS1062/TS1320 that native does not report on a decided
walk, or a union await whose dropped `nil` constituent native keeps.

## 3. `checkAsyncFunctionReturnType`: TS1064 and TS1058 (`tsr-2zk.10.1`)

**Forcing constraint.** `checkSignatureDeclaration` (`:2764`) runs
`checkAsyncFunctionReturnType` (`:2776`) for a function whose flags are
exactly `Async` (not a generator; a bodiless declaration is `Invalid`) and
that has a written return type: TS1064 at the annotation unless the type is a
reference to the global `Promise` — its argument is
`typeToString(getAwaitedTypeNoAlias(returnType) ?? voidType)` — otherwise
`checkAwaitedType(returnType, false, node, TS1058)` at the function. Neither
had a producer.

**Decision.** `check_async_function_return_type`, called once per function
node from the diagnostics walk (`check.rs`, the arm that already runs
`check_all_code_paths_return_or_throw`). The TS1064 argument uses the
worker's tri-state: native `nil` prints `void`, a gap reports nothing.
`getGlobalPromiseTypeChecked() == emptyGenericType` (no global `Promise<T>`)
returns without reporting; native's TS2318 for the missing global is not
this function's.

**Measured wrong turn.** The first build tested `isReferenceToType` on the
held reference and reported TS1064 on `async function f(): PromiseAlias<void>`
(`type PromiseAlias<T> = Promise<T>`) — five EMPTY_RIGHT → EMPTY_WRONG losses
(`asyncAliasReturnType_es5/_es6`, `asyncAwait_es5/_es6/_es2017`). Native's
alias instantiation *is* `Promise<void>`; the test now reads the alias body
(`binding_type_alias_body`), the same projection as §1.

**Converted.** `conformance/asyncQualifiedReturnType_es6`.
`asyncFunctionDeclaration15_es6` now has every TS1064; it still lacks
line 17's TS1058, which is the *inferred* return type's `checkAwaitedType`
(`getReturnTypeFromBody`, `:20149`/`:20287`, `signatures.rs`/`flow.rs`).
`asyncImportedPromise_es6` has its TS1064 and lacks the TS2322 of
`checkReturnStatement`'s async arm (§5).

**Falsifier.** A TS1064 on an annotation native's `isReferenceToType`
accepts, or a TS1058 on a function whose return type native awaits.

## 4. A self-referential alias's placeholder awaits as the alias (`tsr-2zk.10.1`)

**Forcing constraint.** In `type T1 = 1 | Promise<T1> | T1[]`, native's
`Promise<T1>` argument is the alias's own (deferred) type, the `T1` union;
`getAwaitedTypeNoAliasEx` on `T1` pushes the union, finds `Promise<T1>`'s
promised type already on the stack, reports TS1062 and drops the
constituent (`mapType`), answering `1 | T1[]`
(`compiler/unresolvableSelfReferencingAwaitedUnion`). This port's mention is
the memoized NAME placeholder (`alias_placeholders`,
`get_declared_type_of_type_alias` §29), a distinct `Named` type with no
members, so the stack test never matched and the placeholder was its own
awaited type: `1 | T1[] | T1`, no TS1062.

**Decision.** `completed_alias_placeholder_type` maps a placeholder to its
owner's declared type, and the worker awaits that, keeping the placeholder
when it comes back unchanged. Checker port convention record:

- *Native operation:* the deferred alias mention resolving to the alias's
  declared type (`getDeclaredTypeOfTypeAlias`, `:23837`), read inside
  `getAwaitedTypeNoAliasEx`.
- *Key identity and owner:* the placeholder `TypeId`, owned by
  `alias_placeholders`[owner symbol]; exactly one owner or decline (the same
  scan as `members.rs` `completed_array_placeholder_length_body`).
- *Publication state:* only a published `declared_types`[owner] that is not
  `errorType` and whose `DeclaredType` resolution is not on the stack; no
  evaluation, insertion or cache.
- *Expensive work boundary:* one scan of `alias_placeholders` per
  member-less `Named` type reaching the worker, skipped when the map is
  empty.

The general "already on the stack → gap" entry test is narrowed to what
native has: a union on the stack is TS1062; an alias reference or
placeholder re-entered while its own projection is open is still a gap (no
native counterpart — native has no projection step); every other repeat is
caught at the promised type, as native catches it. That is what lets
`EffectResult`'s narrowed `Promise<EffectResult>` reach TS1062 instead of a
gap when it meets itself inside the union.

**Converted.** Types `unresolvableSelfReferencingAwaitedUnion` lines 9, 10,
29 (`1 | T1[]`, `() => EffectResult`). Its diagnostics now carry both TS1062;
the case stays WRONG on an extra TS2349 at `result()` after
`result instanceof Function` (narrowing, not this lane).

**Falsifier.** A placeholder awaited as its declared type where native keeps
the deferred reference, or a gap on a repeat native resolves.
