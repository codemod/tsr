# r5-declemit3 — the declaration-emit SymbolTracker, round 5

Lane notes for round-5 box `r5-declemit3` (`bd tsr-2zk.1024`, covering
`tsr-2zk.1000` and `tsr-2zk.1001`), continuing r5-declemit2
(`docs/parity/notes/r5-declemit2.md` §4) and building on r5-modules' TS2883
tracker hook (`b24a56d`, `docs/parity/notes/r5-modules.md` §6). Pinned
upstream: `vendor/typescript-go` @ `5b1047d`.

Box baseline: the integration head `a1e453d` with
`claude/beautiful-shannon-ar5gh0-r5-modules` (`b24a56d`) merged, as the brief
asked; diagnostics 5,314 RIGHT / 5,576 EMPTY_RIGHT over 12,238 judged keys
(plain and configured).

## 1. TS4113/TS4114 for late-bound member names (`tsr-2zk.1001`)

`checkMemberForOverrideModifier` (`checker.go:4729`) looks the member up as
`getPropertyOfType(thisType, symbol.Name)` where `symbol` is
`getSymbolOfDeclaration(member)` — the **late-bound** symbol. A computed name
`lateBindMember` binds (`[foo]` for a `unique symbol` const, `[prop]` for a
literal-typed const) is therefore found under its bound name in the class's
own type and in the base type.

The port took the binder symbol's name, which is `__computed` for every
computed member, so neither lookup found anything and the member returned
before any report: no TS4113 for `override [foo]() {}` over a base without
it, no TS4114 for an unmarked `[prop]()` that overrides one. The name now
comes from `late_bound_members_of` (the same table `getPropertyOfType`
answers from here, and the one r4-heritage's `issueMemberSpecificError` fix
already reads), keyed by the member's declaration. A computed name that is
not late-bindable keeps `__computed` and finds no property, as natively.

The table is read only when `hasLateBindableName` holds — an entity-name
expression whose type is a string literal, number literal or `unique symbol`
(`TypeFlagsStringOrNumberLiteralOrUnique`). `late_bound_members_of` also
names a computed member keyed by a *plain* `symbol` entity (`[sym]` with
`sym: symbol`), because the printer shows those as index-info components;
native never late-binds them, so the member keeps `__computed`. The first
version read the table unconditionally and invented a TS4114 in
`overrideLateBindableIndexSignature1(noimplicitoverride=true)` (RIGHT →
WRONG on the full run); the gate removed it.

The TS4113 spelling suggestion stays on the bound name; for a unique-symbol
name native's `__@foo@N` never matches a candidate either.

Measured: +3 diagnostics rows (`override21`,
`overrideLateBindableName1` × 2 `noImplicitOverride` variants).
