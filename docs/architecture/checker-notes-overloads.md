# Checker notes — `overloads` workstream

Working notes from one workstream, kept out of `docs/architecture/checker.md`
because that file cannot be shared: `git commit -- <path>` commits the
*working-tree* state of a path, so a shared dirty file is swept by whoever
commits second no matter how carefully either party stages. These are for the
lead to fold into `checker.md` once, at the end of the cycle.

Earlier sections from this workstream are already *in* `checker.md` — overload
sets printing as a type literal, `typeof X` resolving from `exports`, and the
enum-member table correction. This file starts from the enum widening arm.

## An enum member widens to its enum, and the back-edge that needed a side table

`var e = E.A` is `E`, not `E.A`:

```text
enum E { A }
var e = E.A;
>e : E           conformance/enumAssignability.types:15
>E.A : E         conformance/enumAssignability.types:16
```

while `const v = E.A` keeps `E.A` (`validEnumAssignments.types:31`), because
`getWidenedLiteralTypeForInitializer` returns early for a constant
(`checker.go:16898`).

What identifies this as *widening* rather than a property of the enum is that
the same file prints the same symbol both ways: `>A : E.A` at the declaration
(`enumAssignability.types:8`), `>A : E` at the use site. One symbol, two printed
types, so something happens between declaring and using it.

### It was not the one-arm fix it looked like

Two of the three preconditions were already satisfied, which is exactly why it
looked like one arm in `crate::literals`. Probed rather than assumed:

```text
MEMBER  text="E.A"  flags=TypeFlags(ENUM)  fresh=true
MEMBER  data=Named { text: "E.A", members: None }
```

`fresh` is already true (`crate::declared` freshens the member type, with a
comment saying it was unobservable — it is observable now), and
`TypeFlags::ENUM_LIKE` already existed. The **body** was the problem:
`getBaseTypeOfEnumLikeType` (`checker.go:25470-25475`) reaches the enum through
`t.symbol`, and `TypeData::Named` is `{ text, members: Option<SymbolId> }` with
no symbol at all — `crate::declared` passes `None` for `members`, correctly,
because an enum member has no properties.

So there was no route from the `E.A` type to `E`. Writing the guard alone, with
a body that returned the type unchanged, would have been an arm that matches and
does nothing — the decoration this project deletes elsewhere.

### The rejected alternative, and why the ADR decided it

Two ways to give the type its enum back:

1. **Add a `symbol` field to `TypeData::Named`**, set where the member type is
   created. Most faithful to upstream, whose types carry a symbol.
2. **An id-keyed side table**, `TypeId -> SymbolId`, on the checker.

(2) was taken, and not merely because it is smaller. ADR-0003 is titled *"The
tree is a tree; everything cyclic lives in id-keyed side tables"*, and a
type → symbol back-edge is precisely the shape it puts in a side table. Option
(1) would re-add a field that ADR removed on purpose. What would have to change
for (1) to win: enough distinct consumers wanting a type's symbol that threading
one table per question becomes worse than one field — at which point the ADR
itself is what should be revisited, not this call.

The parent hop is collapsed. Upstream stores the *member's* symbol and calls
`getParentOfSymbol`; the table stores the enum symbol directly, because that is
all any reader wants. Accepted consequence: a future caller wanting the member
symbol cannot get it from this table.

### A comment that was wrong, and the probe that caught it

The control asserting the enum type itself does not widen was first documented
as being held back by the missing table entry — the natural guess, since `E` is
`EnumLike` and the flags test alone would match it. Measured, that is false:

```text
ENUM TYPE fresh=false flags=TypeFlags(ENUM_LITERAL | UNION) in_table=false
```

`E` is **not fresh**, so `get_widened_literal_type` returns at its first line and
the enum arm never sees it. The table check is the second line of defence, not
the first. The test comment now says so, and describes the control as pinning
the freshness gate rather than more than it covers.

This is the second time in this workstream that probing rather than reasoning
changed a documented claim, and both times the wrong version was the plausible
one. Recorded because a reader who trusts the wrong reason would put the next
guard in the wrong place.

### How you would know this is wrong

An `.types` line where `var x = E.A` prints `E.A`, or where `const x = E.A`
prints `E`, means the freshness gate has moved. A line where a *string* or
*number* literal stops widening means the enum arm — which sits first, as
upstream has it — is matching more than `EnumLike`. The ordering is currently
unobservable here because this port's member type carries `ENUM` alone, where
upstream's carries `EnumLiteral | NumberLiteral`; it is written upstream's way
so that it stays correct if the member type ever gains that flag.
