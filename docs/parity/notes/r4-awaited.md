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
