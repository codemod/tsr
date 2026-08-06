# A `Named` callee and its signatures — `bd tsr-4sa`

Sixth session. Sized by counterfactual at `c757d16`, **before any checker code
existed**, which is the point: this item carries a *measured* wrong-manufacturing
risk and the whole question is whether the arm converts more than it breaks.

Companion pages: `checker-notes-callres.md` §5 (where the risk was first
measured), §8.1 (the correction to the record this item must respect), §13.2
(the row this item was scored from), §14–§15 (the `new C<T>()` build whose
method this page copies).

---

## 1. The mechanism, and the correction it has to respect

`resolve_call_signature` (`crates/tsr-checker/src/calls.rs:644`) destructures
`TypeData::Anonymous` and returns `None` for everything else.
`check_new_expression` (`crates/tsr-checker/src/expressions.rs:790`) does the
same. So a callee whose type prints as a **name** — a lib constructor interface
(`SymbolConstructor`, `DateConstructor`, `ErrorConstructor`, `ArrayConstructor`,
`MapConstructor`, `SetConstructor`) or any interface with a call signature —
cannot be called at all. `callgate.rs` attributes **2,791 assertion lines** to
the two gates: 1,663 on the `new` side, 1,128 on the call side.

**The natural citation is wrong and must not be used.** `bd tsr-qk9` says
`signature_parts_of` has no `CallSignatureDeclaration` /
`ConstructSignatureDeclaration` arm. It has both today
(`crates/tsr-checker/src/signatures.rs`). The missing link is the **route from a
`Named` type to its members' signatures** — upstream's `getSignaturesOfType`
(`checker.go:18959`) over `resolveStructuredTypeMembers` — not the declaration
reader. `checker-notes-callres.md` §8.1 records the same correction.

`get_signatures_of_symbol` (`signatures.rs:237`) reads a *symbol's declarations*,
and an interface declaration is not signature-shaped, so it answers an empty
list for `DateConstructor`. What is needed is the other direction: the
interface's **members**, filtered to the signature elements.

---

## 2. Why the sizing had to compute the answer, not count the row

`docs/conventions.md` requires the match test rather than the shape test, and
this item is the one where the difference is not academic.
`checker-notes-callres.md` §5 measured, a session before any of this:

> Its two largest answers are `unique symbol` (264) and `symbol` (150) —
> `Symbol()` calls. `unique symbol` passes a *shape* test because it contains no
> punctuation, but this port has no fresh-unique-symbol rule, so resolving
> `Symbol()` would print `symbol` where upstream prints `unique symbol`: **a gap
> turned into a wrong line, 264 times.**

A row count would have said 2,791. A spellability count would have said the same
264 lines were convertible. Only computing the string separates them.

And because the mechanism fires on a **position** — every call whose callee is a
`Named` type — `docs/conventions.md`'s rule applies in full:

> When a mechanism fires on a position rather than on a defect […] compute the
> at-risk population in the same pass that computes the target one.

`examples/namedcallee.rs` does that: one loop, three columns.

### The at-risk population is gap→wrong, and that is established rather than assumed

A line that is **right today** cannot be broken by this arm. The arm only fires
where the callee's call currently answers `errorType`, and every consumer of an
`errorType` answers `errorType` too — so no currently-right line reads a value
this arm changes. Control **C1** is what turns that from an argument into a
measurement: every classified line answers `errorType` today, and it reads **0**
violations. The at-risk column is therefore *gap → wrong*, which is what the
probe's `WOULD PRINT WRONG` bucket counts, and leg 2 of the bar watches the
residual cascade that C1 cannot see.

---

## 3. The counterfactual — `examples/namedcallee.rs`, at `c757d16`

Unit: assertion lines. Population: a gap line whose blocking call (the same
two-route rule `callgate.rs` uses, copied verbatim so the two populations stay
comparable) has a callee typing as `TypeData::Named`.

The forecast is the signature's **written return annotation** resolved through
`get_type_from_type_node` and printed with `type_to_string` — computed
independently of the plumbing being sized.

```
classified (gap line, callee types as a `Named`): 2,352

   CONVERTS                                                     625
   WOULD PRINT WRONG                                              2
   STAYS A GAP                                                1,725
```

The gap column, in full, because *why* it declines is the item's real shape:

| lines | why the arm declines |
|---:|---|
| 835 | `new`: a **generic** construct signature — that is inference (`callgate.rs`'s own largest gate) |
| 291 | **REFUSED (unique symbol)** — see §4.1 |
| 187 | C3 `new`: the `Named`'s members symbol has no interface declaration |
| 91 | call: a generic call signature |
| 87 | C3 call: same |
| 75 | **REFUSED (namespace-qualified)** — see §4.2 |
| 30 | call: the interface declares no call signature |
| 25 + 23 | an overload set whose candidates **disagree** about the return type |
| 20 + 7 | **REFUSED (heritage clause)** — see §4.3 |
| 16 + 11 | the name carries no members symbol |
| 13 | `new`: the interface declares no construct signature |
| 11 | the return annotation itself gaps |
| 2 + 1 | **REFUSED (return is a type parameter)** — see §4.4 |

Controls: **C1** classified-but-not-gap **0**. **C2** buckets sum 2,352 = 2,352.
**C3** is not a violation count but a named bucket — a `Named` whose members
symbol has no interface declaration is a class instance type, which the data
model says lands here, so it is printed rather than folded into "no signature
member" where a wrong reading of `TypeData::Named` would have hidden.

Conversion is **625 of 2,791 = 22.4%** of the row, inside the observed 15–57%
band and at its low end. The 2,791 was a ceiling and this is the estimate.

### What converts

| lines | callee |
|---:|---|
| 150 | `SymbolConstructor` (the *non*-const positions — §4.1) |
| 128 | `DateConstructor` (four construct signatures, all returning `Date`) |
| 127 | `ErrorConstructor` |
| 31 | `ObjectConstructor` |
| 27 | `BigIntConstructor` |
| 23 | `StringConstructor` |
| 22 | `RegExpConstructor` |
| 17 + 14 | `SharedArrayBufferConstructor`, `ArrayBufferConstructor` |
| the rest | user interfaces with a call or construct signature |

**625 is own-node and direct-initialiser lines only.** A read of a variable two
hops downstream is not forecast here, so it is upside and it is deliberately
**not** in the floor — the same rule §14.2 of `checker-notes-callres.md` set for
the `new C<T>()` arm, whose conversion then came in at 413% of its forecast for
exactly that reason.

---

## 4. The four refusals, each measured both ways

`docs/conventions.md`: a gap beats a wrong answer. Each refusal below was
measured with its cost *and* its benefit in the same run, so the trade is
visible rather than asserted.

### 4.1 `unique symbol` — 291 refused, **0 conversions lost**

Upstream, `resolveCallExpression` (`checker.go:8348`):

```go
returnType := c.getReturnTypeOfSignature(signature)
// Treat any call to the global 'Symbol' function that is part of a const variable or readonly property
// as a fresh unique symbol literal type.
if returnType.flags&TypeFlagsESSymbolLike != 0 && c.isSymbolOrSymbolForCall(node) {
    return c.getESSymbolLikeTypeForNode(ast.WalkUpParenthesizedExpressions(node.Parent))
}
```

`getESSymbolLikeTypeForNode` (`checker.go:22982`) makes a fresh
`UniqueESSymbolType` **when the position is a valid ES symbol declaration**
(`isValidESSymbolDeclaration`, `utilities.go:961` — a `const` variable
declaration with an identifier name, a `readonly static` property declaration,
or a `readonly` property signature) and returns plain `esSymbolType` otherwise.

This port has `TypeFlags::UNIQUE_ES_SYMBOL` as a flag and no type carrying it,
so the unique half cannot be answered. **The refusal is therefore positional and
not on the return type** — which is what makes it free: outside a valid ES
symbol declaration upstream answers `symbol` too, and those 150 lines convert.
Refusing on "the return is `symbol`" instead would have cost all 150 to save the
same 291.

Predicted at 264 by §5 a session earlier; measured at **291**. The prediction was
low by 10%, and the direction is worth recording — §5 counted the `unique symbol`
*answers*, and the refusal also catches lines whose answer differs for the same
reason.

### 4.2 Namespace-qualified naming — 75 refused, **0 conversions lost**

`new Intl.NumberFormat()` wants `Intl.NumberFormat`; this port's
`TypeData::Named` bakes the symbol's own name and would print `NumberFormat`.
That is `STATUS.md` §5's standing refusal (qualified naming, **2.7 wrong per
right**, `bd tsr-93f`), reached through a new door. The test is syntactic: any
declaration of the answer type's members symbol has a `ModuleDeclaration`
ancestor.

The family is `Intl` (50) and `Temporal` (25) and it costs nothing to decline.

### 4.3 An interface with a heritage clause — 27 refused, **20 conversions lost**

This is the one refusal that costs more lines than it saves, and it is kept.

Upstream's `resolveDeclaredMembers` folds the **base types'** call and construct
signatures into the resolved list. A walk over the interface's own members sees
only part of the candidate set, so a set that looks like a single candidate here
may be one arm of an inherited overload — which is exactly what
`compiler/inheritedOverloadedSpecializedSignatures` is: seven lines wanting
`void`, `boolean`, `boolean[]`, `number`, `number[]`, `string[]` off a callee
whose *direct* call signature returns `string`.

The trade is −20 converted for −7 wrong, i.e. **net −13 lines**. Kept anyway,
because the two are not the same currency:

> A loss here is not "the arm was unprofitable on this slice"; it is **the arm
> answering from a candidate set it knows is incomplete**. `docs/conventions.md`
> distinguishes a bad trade from a wrong rule, and answering off partial data is
> the second. Thirteen lines is the price of the mechanism being *true* rather
> than usually-true, and the alternative leaves a defect that fires whenever a
> base interface is added anywhere in the corpus.

Revisit when base-type members are resolved — that is the same
`resolveStructuredTypeMembers` work this item declines to build in full, and it
returns these 27 lines plus whatever the 48 disagreeing-overload lines become.

### 4.4 A return annotation that resolves to a type parameter — 3 refused, 1 conversion lost

`callable2<number>` has a call signature returning `T`. This arm does not
substitute — `bd tsr-4qx`'s seam instantiates *properties*, not signatures — so
it would print `T` where `number` is wanted. Refused on the **return type** and
not on "the callee is instantiated", which is the cheaper cut: measured both
ways, refusing the instantiated callee cost 11 conversions to save 2, refusing
the type-parameter return costs 1 to save 2.

---

## 5. The residual, named in advance

The forecast leaves exactly **two** wrong lines, and both are named here so that
anything else in the measured residual indicts the arm rather than being
absorbed:

```
  1  call  want `void`, forecast `any`   [ObjectConstructor]  compiler/classUpdateTests
  1  new   want `server`, forecast `Date` [DateConstructor]   compiler/exportAssignValueAndType
```

---

## 6. The bar — registered before any checker code

> **KEEP** if **net ≥ +450 lines**, **lost ≤ 10 with every loss diagnosed as a
> cascade**, **fewer cases regress than finish**, and **new wrong ≤ 40 lines
> absolute** by `wrongdelta`.
> **REVERT** otherwise.

**Leg 1 — the floor is 72% of the forecast 625, not of the row's 2,791.** The
same fraction §14.4 used, for the same reason: the cascade is upside and a floor
resting on unforecast lines is a floor resting on a guess.

**Leg 2 — the denominator is not empty.** C1 establishes that no currently-right
line reads a type this arm changes, so a *loss* can only be a cascade: something
downstream that was gapping now computes confidently and wrongly, and the wrong
value displaces a right line elsewhere. That is a real mechanism (§15.1 of
`checker-notes-callres.md` measured it on the `new C<T>()` arm), which is why the
leg is here and why it is not zero.

**Leg 4 is an absolute, and the choice is the load-bearing one.**
`docs/conventions.md` says *"when a change can only add or only remove, the
honest bar is an absolute zero on the other direction, not a ratio"* — and that
entry does not settle this case, because this arm can do both. The entry's
*deciding question* does settle it:

> Is a loss here evidence of a bad trade, or evidence of a wrong rule?

Evidence of a wrong rule. The counterfactual refuses four whole families up
front and names the two survivors. A wrong line beyond that is not the price of
a win; it is a family the forecast did not see, and the honest response is to
find it, not to net it off. **A ratio bar would have shipped the version of this
arm that prints `symbol` for `unique symbol`**: at 646 gained and 377 wrong that
is 1.7×, which fails a 3× ratio — but the intermediate design measured at 646/23
passes 3× at 28×, and 646/9 passes at 71×, and every one of those residuals is a
family that should have been refused rather than priced. A ratio prices what has
no price here.

Why 40 and not 0. A hard zero is a bar whose denominator the change cannot
control: the forecast covers own-node and direct-initialiser lines, and the
cascade beyond them can go wrong for reasons no forecast over this population
could see — the `new C<T>()` build's residual was 68 new wrong on +572 (**12%**)
and 40 of those turned out to be a *printer* defect older than the build. 40 on a
forecast 625 is **6%**, half that rate, which is the tightening the four refusals
are supposed to have bought. If the arm needs more than 40, the refusals did not
work and that is the finding.

**FALSIFIER:** more than **40% of the gain in a single case**. The
counterfactual's top converting case is `conformance/uniqueSymbols` at 24 of 625
= **3.8%**, and the top ten sum to 130 = 21%, so concentration near half means
the mechanism reached something this page does not describe.

**If a leg fires: the first hypothesis is that the build is wrong, the second is
that this page's premise is wrong, and there is no third.**

---

## 7. What will be built

In `crates/tsr-checker/src/signatures.rs`, a new
`get_signatures_of_named_type` — upstream's `getSignaturesOfType`
(`checker.go:18959`) reduced to the interface case of
`resolveStructuredTypeMembers`:

- read the callee type's `TypeData::Named { members }`;
- over every `InterfaceDeclaration` of that symbol, collect the
  `CallSignatureDeclaration` (for a call) or `ConstructSignatureDeclaration`
  (for a `new`) members;
- decline for a heritage clause (§4.3), for a generic candidate, for a set whose
  candidates disagree about the return type, and for a return resolving to a
  type parameter (§4.4);
- otherwise build the signature through the existing
  `get_signature_from_declaration`, which `signature_parts_of` already handles
  for both element kinds (§1).

Called from `resolve_call_signature` (`calls.rs`) and `check_new_expression`
(`expressions.rs`) as the arm that runs when the callee is not `Anonymous`, with
the `unique symbol` refusal (§4.1) and the namespace refusal (§4.2) applied at
the call sites where the position is known.

Each refusal keeps its `bd` reference so the lines come back when the blocking
subsystem lands: inference (`bd tsr-4sa`'s largest gap column, 926 lines),
qualified naming (`bd tsr-93f`), base-type members, and unique symbols.
