# Lane notes: r4-index (tsr-2zk.905, tsr-2zk.16.119, tsr-2zk.16.51)

Judgment calls made by the `r4-index` parity box. Numbers are measured with
`verdictdump` / `diagverdictdump` against the baseline frozen at `b23dd3d`
(integration branch head at dispatch). Native behaviour is reproduced with a
`tsgo` built from the pinned submodule (`5b1047d`) by
`scripts/offline-cargo/build-tsgo.sh`.

## 1. Late-bound index signatures on classes and interfaces (tsr-2zk.16.51)

**Forcing constraint.** `getResolvedMembersOrExportsOfSymbol`
(`checker.go:15930`) late-binds a member whose computed name is an
entity-name expression (`isLateBindableAST`) of a type that is not usable as
a property name but is assignable to `string | number | symbol` into the
`__index` symbol (`lateBindIndexSignature`, `:16068`).
`getIndexInfosOfIndexSymbol` (`:19634`) turns those declarations into one
index info per key kind whose value is `getObjectLiteralIndexInfo` (`:19720`)
over the late-bound declarations **and every sibling of the members table**.
`index_infos_of_symbol` read only `IndexSignatureDeclaration` members, so
`class C { [k]: number }` with `k: string` had no index at all.

Native reproduction (`tsgo --declaration`):

```ts
declare const s: string; declare const n: number;
class C { [s]: number; foo!: string; }      // c["x"] : string | number
class D { constructor() {} [s]: number; }   // d["x"] : any
interface I { [n]: boolean; a: string }     // i[0]   : boolean
```

The value is the union of *all* applicable members, not the annotated type,
and a class with a constructor answers `any`: the binder files the
constructor as `__constructor` in `Members`, `getTypeOfSymbol` has no arm for
`SymbolFlagsConstructor` and answers `errorType` (`:16493`), and a union with
`errorType` is `errorType`. This binder does not put `__constructor`,
`__call` or `__new` in the members table, so `late_index_properties` adds that
contribution explicitly for a class with a constructor or an
interface/literal with call or construct signatures.

**Representation choice.** The constructor's contribution is spelled `any`,
not this port's `error`: here `errorType` is a native answer (it prints `any`
and is `IsTypeAny`), while this port's `error` means "not computed" and prints
as a gap. A sibling whose own type this port could not compute still yields
`error`.

**Port convention record** (`docs/conventions.md#checker-ports-preserve-ownership-and-work-boundaries`):

- Native operation: `getIndexInfosOfIndexSymbol` on `getMembersOfSymbol`
  (instance) / the static side's members (`resolveAnonymousTypeMembers`).
- No cache, side table, mapper or member image is added. The infos are
  computed on each `index_infos_of_symbol` call like the explicit arm, keyed
  by nothing; `IndexInfo.components` uses the existing `index_components`
  store exactly as the object-literal road does.
- Receiver context: the uninstantiated declaration; `get_index_infos_of_type`
  instantiates values afterwards, as for explicit signatures.
- Expensive work boundary: a syntactic member scan per declaration; the
  computed-name `check_expression`, sibling `get_type_of_symbol` and the
  subtype-reducing union run only when a member has an entity-name computed
  name. Perf at 21 samples, median child CPU new/old: domain-model 0.988,
  generic-imports 0.986.
- Declined (gap, `None`): the static side when the class is generic or has an
  `extends` clause. Its siblings include `prototype`
  (`getTypeOfPrototypeProperty`) and the base constructor's statics
  (`addInheritedMembers`), which this binder does not enumerate as symbols.
  Falsifier: a corpus case with a static late-bound index on a derived class.

**Symbol keys and `SymbolConstructor` (decline, upstream piece missing).**
`widenTypeForVariableLikeDeclaration` (`checker.go:18246`,
typescript-go#1212) turns a `symbol`-typed member merged into the global
`SymbolConstructor` into that member's `unique symbol`; this port's widening
lacks the arm (`check.rs` already declines TS2717 on it). With the arm missing,
`const observable: typeof Symbol.obs` is plain `symbol` here and
`[observable]()` became a symbol index where native late-binds a member, which
turned `symbolProperty61` from EMPTY_RIGHT to a false TS2345. A symbol-kind
late-bound key is therefore skipped while any `SymbolConstructor` declaration
writes a `symbol`-typed property signature. It lifts when the producer in
`get_widened_type_for_variable_like_declaration` (`symbols.rs`, not owned) is
ported; that change is in the lane report.

## 2. An index signature without a value annotation is `any` (tsr-2zk.16.119)

`getIndexInfosOfIndexSymbol` defaults `valueType` to `anyType` when the
signature has no type (`checker.go:19649`); `index_infos_of_declaration`
returned no info. Exposed by §1: `intTypeCheck`'s `interface i4 { [p];
[p1: string]; }` gained the late-bound number index (key `errorType` from the
unresolved `p` is assignable to `number`) but lost the explicit `any` string
index, and `var obj35: i4 = new Object()` reported a false TS2322. Native:
`i4` has `[x: string]: any` and `[x: number]: any`.

The `value == error` decline in the same function is kept: it is not native
(an unresolved value annotation is `errorType`, still an info), and removing
it is a separate measurement.

The type-literal half of `.16.119` (`var foo: { [index: any]; }` prints `{}`
natively; this port prints `any`) is in `declared.rs`'s
`get_type_from_type_literal`, which declines the whole literal when
`index_signature_member` answers `None` (no value annotation, invalid key).
Not owned; see the lane report.

## 3. TS7053 / TS7015 for a key that names no property (tsr-2zk.905)

**Forcing constraint.** `getPropertyTypeForIndexType`'s access-expression arm
(`checker.go:27129-27184`) reports, under `noImplicitAny`, an element access
whose key has no applicable index info and no string fallback (`:27085`):
TS7015 at the index when the object has a number index
(`getIndexTypeOfType(objectType, numberType)`), else TS7053 at the access.
This port reported only the literal-name half (`nonexistent_property.rs`,
argument written as a string literal); `obj[k]` with `k: string` over
`{}` (`noImplicitAnyForIn`) or `any[] | Record<string, any>`
(`narrowingMutualSubtypes`) was silent. Native messages were compared line
by line with the pinned `tsgo` on `indexSignatures1` (21 of 23 reported
lines, identical text; the two left are a literal key and a type-literal
receiver).

**Where it lives and why.** `check_element_access_index_type` already hosts
the TS2538 arm of the same function as a check-site reporter, so the new arm
sits beside it rather than inside `indexed.rs`' lookup (not owned). It does
not re-derive the lookup's answer: it reports only when the element access
itself failed (`check_expression` of the access is `error`), so the for-in
numeric substitution, tuple reads and mapped indexes the lookup answers are
never reported over. The literal-name arm stays where it is; a union key
mixing literal and non-literal constituents is declined whole so the two
arms cannot both fire.

**Certification.** "No applicable index info" is believed only for receivers
whose infos `get_index_infos_of_type` reads completely
(`index_infos_are_declared`): classes, interfaces, type literals, class/enum
objects, primitives' apparent interfaces, and mapped/alias instantiations
that published a non-empty index set. Measured before that gate: tuples
(`unionsOfTupleTypes1`, `avoidNarrowingUsingConstVariable...`: this port has
no `Array` base number index on a tuple) and `{ [P in keyof any]: V }`
(`mappedTypeWithAny`: key set unresolved, no index published) produced false
TS7053s. Declined, with the upstream piece each waits for: nullable receivers
(`checkNonNullExpression` precedes), generic object or key
(`shouldDeferIndexedAccessType`), JS literal (`isJSLiteralType` answers
`any`), object literal (`:27135` answers the property union), `typeof
globalThis` (its own TS7017/TS2339 road), and a receiver with a `get`/`set`
member (TS7052's `getSuggestionForNonexistentIndexSignature` is not ported).

Port convention record: no cache, side table, mapper or traversal is added;
the arm reads `check_expression` and `get_index_infos_of_type`, both already
cached or computed for the access, once per checked element access with
`noImplicitAny` on. Perf at 21 samples, median child CPU new/old:
domain-model 1.004, generic-imports 1.007.

Measured: missing TS7053/TS7015 lines 51 -> 25, extra 0; cases converted
`noImplicitAnyForIn`, `narrowingMutualSubtypes`, `for-inStatementsArrayErrors`.
The 25 left are literal keys from non-string-literal nodes (number literal,
unique symbol, enum member, `const` string; they need the property-absence
certification `nonexistent_property.rs` owns), tuple receivers, and
type-literal receivers whose index infos live in `declared.rs`.
