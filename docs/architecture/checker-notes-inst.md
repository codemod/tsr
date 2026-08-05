# Members on an instantiated generic type: why the slice is not one edit

Status: designed 2026-08-05, **no production code changed**. Written for the
item `bd tsr-el3.2` names and that
[`checker-notes-recv.md`](checker-notes-recv.md) demonstrated is the blocker for
the 557 typed-receiver calls. The finding below is why this agent did not build
it, stated so the next one starts from the seam rather than from the symptom.

## The one-line finding

`create_type_reference` passing `members: None` is **not** the blocker; it is the
only *safe* answer given where members currently live. Making `C<number>` carry
members is a two-line change that produces confidently wrong types at three call
sites unless a substitution seam is introduced first — and there is no such seam,
because **member lookup in this port returns a `SymbolId`, and a symbol has
nowhere to hold an instantiation.**

That is not a new discovery. It is the falsifier `crates/tsr-checker/src/members.rs`
wrote down for its own design, verbatim:

> **The consequence accepted:** the moment `getDeclaredTypeOfClassOrInterface`
> grows real member resolution — the instantiated members of `class C extends
> B<number>` — this must move to the type level, because a symbol has no place
> to hold an instantiated table. That is the falsifier for this shape.

**The falsifier has fired.** This page is the record of that, which is the thing
`docs/conventions.md` asks for and which no later diff will recover.

## What upstream does, verified against the pinned submodule

`grep -n`, `vendor/typescript-go/internal/checker/checker.go`:

| function | line |
|---|---|
| `getTypeOfPropertyOfType` | `18951` |
| `resolveTypeReferenceMembers` | `19095` |
| `instantiateType` | `22100` |
| `createTypeReference` / `createTypeReferenceEx` | `25103` / `25107` |

`resolveTypeReferenceMembers` (`:19095`) reads the target's `allTypeParameters`
and the reference's `typeArguments` and hands both to `resolveObjectTypeMembers`,
which builds a `TypeMapper` and **eagerly instantiates a whole new
`ast.SymbolTable`** onto the reference type. Note the fast path at `:19113`: when
`slices.Equal(typeParameters, typeArguments)` it reuses `declaredMembers`
untouched — upstream itself treats "no substitution needed" as a distinct,
cheaper case.

So upstream's answer is *a member table hanging off the type*. Ours has no such
thing: `TypeData::Named { members: Some(owner) }` holds a `SymbolId`, and
`get_property_of_type` walks symbol → base symbol → symbol
(`crates/tsr-checker/src/members.rs:163`). Two references to the same generic
share one symbol, so any instantiated table written there would be wrong for the
other. This is the ADR-0003 shape argument arriving at members: the instantiated
table is id-keyed by `TypeId`, not by `SymbolId`.

## Why the obvious two-line version is unsafe

The obvious change is `declared.rs`'s `create_type_reference` passing
`Some(symbol)` instead of `None` to `new_named`, then substituting at the
property-access site with the existing `Checker::instantiate_type`
(`crates/tsr-checker/src/inference.rs:262`) and the reverse index
`Checker::type_reference_targets` (`crates/tsr-checker/src/checker.rs:108`). The
substitution half genuinely is already built: arm 1 maps `T := number`, arm 2 is
the identity on a member that mentions no type parameter, and everything else
answers `errorType`. That is exactly fixtures `A`/`B` of `checker-notes-recv.md`.

The problem is that **`get_property_of_type` has four production callers, and
only one of them is the property-access site**:

```
crates/tsr-checker/src/members.rs:108     property access  a.b
crates/tsr-checker/src/indexed.rs:117     element access   a["b"]
crates/tsr-checker/src/relater.rs:389-390 structural relate, both sides
crates/tsr-checker/src/calls.rs:390       classification counter only
```

Each of the first three turns the returned `SymbolId` into a type on its own. A
member typed `string` on `C<number>` is fine everywhere; a member typed `T` would
answer **`T`** at `indexed.rs:117` and at both `relater.rs` sites — a confident
wrong type, and at the relater it is the dangerous direction described under
"A conservative `false` is safe for one kind of consumer and unsafe for the
other" in `docs/conventions.md`: the relater acts on a negative. Instantiating
only at `members.rs:108` leaves the other two wrong, which is strictly worse than
today's uniform `error`.

## The slice, stated so it can be built

1. **Introduce the seam.** `Checker::get_type_of_property_of_type(receiver: TypeId,
   name: &str) -> Option<TypeId>`, anchored to `getTypeOfPropertyOfType`
   (`checker.go:18951`) — upstream has exactly this function and it is the one
   that returns a *type*. Body: `get_property_of_type`, then `get_type_of_symbol`,
   then instantiate against the receiver.
2. **Route all three type-taking callers through it** (`members.rs:108`,
   `indexed.rs:117`, `relater.rs:389-390`). `calls.rs:390` only asks existence and
   stays. This step changes no answer on a non-generic receiver and is the one
   that makes step 3 safe; it is worth committing separately for that reason.
3. **Instantiate inside the seam.** Look the receiver up in
   `type_reference_targets`. On a miss, the type is not a reference and the
   member's type is returned unchanged — today's behaviour exactly. On a hit,
   build the map from the *target's* type parameters to the arguments and call
   `instantiate_type`; `errorType` out means `errorType` for the member.
4. **Only then** flip `create_type_reference` to `Some(symbol)`.

Step 3 needs one thing that does not exist: the type parameters of a **class or
interface** declaration as `TypeId`s with names.
`Checker::type_parameter_types` (`inference.rs:314`) does this for function-like
declarations only — its `match` covers eight function-like node kinds and returns
`None` otherwise. A sibling over `InterfaceDeclaration` / `ClassDeclaration` /
`TypeAliasDeclaration` is the missing piece, and going through each type
parameter's *symbol* (as that function already does) rather than matching by
printed name is what keeps two `T`s apart.

Deliberately **out** of the slice: `fillMissingTypeArguments` and defaults
(`get_instantiated_type_reference` already answers `errorType` on an arity
mismatch and should keep doing so), the `len(typeArguments) == len(typeParameters)-1`
self-padding at `checker.go:19099`, instantiated call/construct signatures and
index infos, and instantiated *base* types (`class C extends B<number>`) — the
symbol walk in `get_property_of_declared_symbol` reaches the base symbol, not a
base reference, so an inherited member of a generic base stays `errorType`.

## Coverage, honestly

Measured share is unchanged from `checker-notes-recv.md` and this page adds none:
`promiseType` + `promiseTypeStrictNull` are **97 of the 557 (17.4%)** and are this
shape. `typedArrays` (54, 9.7%) is not — its receiver `Int8ArrayConstructor` is
non-generic. **The split of the 557 by "does the receiver carry type arguments"
is still not measured**; it is one counter at `crates/tsr-checker/src/calls.rs:416`
and it should be run before step 4, because it is what says whether this is worth
17% of the row or most of it.

Note that steps 1–3 move **nothing** on their own — they are refactoring plus
dead machinery. Only step 4 moves a line. An agent that builds 1–3 and reports
movement has measured noise.

## How you would know this page is wrong

- If `get_type_of_symbol` on a member of a generic already answers `errorType`
  for a `T`-typed member and the *declared* type for a `string`-typed one, then
  routing is unnecessary for the relater and `indexed.rs`, and the two-line
  version is safe after all. This page asserts it does not; that assertion is one
  fixture away from being checked and was **not** checked here.
- If the 557's receiver-carries-type-arguments counter comes back near 97, the
  slice is worth 17% of a 557-node row and the ranking that put it first should
  be revisited before step 4 is built.
