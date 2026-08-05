# Members on an instantiated generic type: why the slice is not one edit

> **Correction, 2026-08-05, and it moves the item.** The section "Coverage,
> honestly" below asked for the 557 `member types as error` nodes to be split by
> whether the receiver carries type arguments. **That split is zero by
> construction and asks the wrong row.** A receiver carrying type arguments has
> no member table, so `get_property_of_type` returns `None` before it reads the
> name and the node lands in `no such member` — never in `member types as
> error`. The population instantiated members acts on is inside the **1,985**,
> and almost certainly inside the **1,011** `Named`-without-members sub-row that
> `checker-notes-calls.md` already measured as 43.4% `Promise`. See
> [the counter section](#the-counter-bd-tsr-fua-and-why-it-had-to-move-rows).
> The same wrong claim is in `checker-notes-recv.md` ("97 of the 557, 17.4%");
> that page is another agent's and has not been edited here.

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

## The counter, `bd tsr-fua`, and why it had to move rows

**Superseded by measurement below: the paragraph in "Coverage, honestly" that
asked for a split of the 557.** It is left in place because the wrong turn is the
useful part of the record.

### The forcing fact, pinned by a test rather than asserted

`create_type_reference` (`crates/tsr-checker/src/declared.rs`) builds every
instantiated reference with `members: None`. `get_property_of_type`
(`crates/tsr-checker/src/members.rs`) matches only
`TypeData::Named { members: Some(..) }` and `TypeData::Anonymous`, and returns
`None` for everything else **before it reads the name**. Compose the two:

> A call whose receiver carries type arguments *cannot* reach
> `of which: member types as error`. It lands in `of which: no such member`,
> sub-row `receiver is Named without members`.

That is a two-hop code reading, which is exactly the kind this project has been
wrong about eight times, so it is a test:
`calls::tests::a_generic_receiver_has_no_member_table_so_the_lookup_never_runs`
asserts `get_property_of_type(P<number>, "get") == None` and that the same
interface *without* the type parameter answers `Some` — the A/A' discrimination
from `checker-notes-recv.md`, one token apart. Its named mutation is step 4
itself (`new_named(.., Some(symbol))` in `create_type_reference`), so the test is
simultaneously the falsifier for this finding and the tripwire for the change
that ends it.

The consequence for the two pages that sized this item: **`checker-notes-recv.md`
fixture A — `interface P<T> { get(): string; } declare var p: P<number>; p.get()`
— is not a member of the 557.** The demonstration is still sound; the population
it was attached to was the wrong one. The "97 of the 557" figure was arrived at
by summing `promiseType` (51) and `promiseTypeStrictNull` (46) out of the *case*
concentration of the 557, which says which files those nodes are in and nothing
about their mechanism. The Promise nodes that *are* this shape are the 122 + 122
of the **1,011**, not the 51 + 46 of the 557. This is the
"a row named after one case is about that case" error one level down, and it is
recorded here rather than quietly fixed.

### What was built instead

A second, **orthogonal** partition of `property access: receiver is typed`
(2,562), keyed on the receiver's *provenance* rather than on the member lookup's
outcome. `Checker::receiver_carries_type_arguments` asks the reverse index
`type_reference_targets` (`docs/architecture/checker-notes-subst.md`) — upstream
asks `objectFlags & ObjectFlagsReference` and reads `target` off the type
(`createTypeReference`, `checker.go:25103`, verified at `5b1047d10`), which this
port cannot, because the `(symbol, arguments)` pair is the intern map's key.

```text
      by receiver: type arguments, no member found          ?   <- the population
      by receiver: type arguments, member found (CONTROL: 0) ?  <- must read 0
      by receiver: no type arguments                        ?
```

The three sum to 2,562, which the reader can check off the printed rows; that is
the partition control. The **zero control** is the middle row, and it is not a
residual — it is reachable in principle and empty in fact for the reason above.
A zero there over 2,562 samples re-derives the invariant on every run instead of
trusting the two-hop reading, exactly as `classify_unresolved_callee`'s existing
control buckets do. It stops being a control the moment step 4 lands, at which
point it becomes the measurement of how many generic receivers newly resolve a
member — the same counter answering the before and after question.

`receiver_generic_member_absent` is also predicted to be **≤ 1,011**, since
carrying type arguments implies `Named`-without-members. If it exceeds that,
`create_type_reference` is not the only producer of `type_reference_targets`
entries and this page's model of the type store is wrong.

### How to run it

```bash
TSR_OVERLOAD_COUNTERS=1 cargo run -p tsr-conformance --example overload_funnel --release
```

No change to `crates/tsr-conformance/` was needed: the `define_counters!` macro
generates `Snapshot::rows()` in declaration order and the example prints every
row, so new counters appear without editing a file this workstream does not own.

### The decision this gates

- If `by receiver: type arguments, no member found` comes back **near the 439
  Promise nodes**, this is the same case-gate item `checker-notes-calls.md`
  already ranked: ~4 nodes per case, half in ten files, and worth building for
  cases rather than for the gradient.
- If it comes back **materially above 1,000**, it is the largest single
  confirmed mechanism on the call path and step 4 is the highest-value build item
  on this board.
- If it comes back **near zero**, the receivers in the 1,011 are `Named` types
  with no member table for reasons unrelated to instantiation — a different item
  — and steps 1–4 should not be built at all. The demonstration in
  `checker-notes-recv.md` would still be correct and still be about a shape the
  corpus does not contain in quantity, which is the outcome this counter exists
  to be able to report.

## Coverage, honestly

*Superseded by the section above; retained per "never delete a decision record".*

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
