# Generic call inference: what was measured, and what was built

Notes on `crates/tsr-checker/src/inference.rs`. Upstream is
`Checker.inferTypeArguments` (`vendor/typescript-go/internal/checker/checker.go:9390`)
and `Checker.inferTypes` (`internal/checker/inference.go:53`), at the pinned
commit `5b1047d10`.

## Written type arguments, and a correction to this document

`f<string>(x)` is now answered. It needs no inference — `checkTypeArguments`
(`checker.go:9269`) validates what the caller wrote and
`getSignatureInstantiation` (`checker.go:19293`) substitutes — so it shares this
module's *second* half and is bounded by exactly the same limit. Of the 40
explicit-type-argument calls in the corpus whose callee carries a written return
annotation: 14 return a bare type parameter and are answered, 5 mention no type
parameter and are free, **21 return a type that merely contains one** and are
gaps. (The other ~53 of the 93 have an inferred return type, which is a
different machine.) So this slice pays about 19 declarations, not 93 — small,
and worth saying plainly.

**Correction.** An earlier version of this document said substitution into
`T[]` and `C<T>` needs a change to the type representation, on the grounds that
those are a `TypeData::Named` whose text is baked at creation. **That is wrong,
and it makes the next slice much cheaper than recorded here.** The
`(target symbol, type arguments)` pair *does* exist — `create_type_reference`
(`crates/tsr-checker/src/declared.rs:485`) interns on exactly that key in
`Checker::instantiations`. It is simply not reachable from a `TypeId`, because
the pair is the map's key rather than the type's payload. A **reverse index**,
`TypeId -> (SymbolId, Vec<TypeId>)`, populated in the same function, makes it
reachable — which is the ordinary ADR-0003 side-table move, not a model change.

With that index, `instantiate_type(id, map)` is short: identity if the map has
it; a rebuilt reference via `create_type_reference` if the reverse index has it,
recursing on the arguments; a rebuilt union or intersection via
`get_union_type` (`unions.rs:288`); otherwise unchanged. That closes `T[]` and
`C<T>` for **both** the written-type-argument path and the inference path at
once, which is the shared dependency that makes it the right next item.

What it does *not* close: tuples and function types, which have no equivalent
key to reverse, and which upstream builds through `instantiateType`'s object-type
arm. Those need the structural rebuild the earlier version of this document
described, and for them that description was right.

The edit is in two files this module does not own — a field on `Checker`
(`checker.rs`) and three lines in `create_type_reference` (`declared.rs`) — which
is why it was not taken in the same commit as the slice above.

## Status: built but not yet reached

**Superseded.** This section recorded that the module was built but unwired.
It was wired in the same commit that first landed it: `check_call_expression`
routes to `check_generic_call`, and `crates/tsr-checker/tests/generic_calls.rs`
proves the routing end-to-end — reverting the call site makes its positive
assertion fail while its gap assertion still passes, which is why both are
there. The unit tests inside `inference.rs` still call `check_generic_call`
directly, and still prove inference rather than wiring.

## The forcing constraint

`CallExpression` is the largest verified-open row on the board at 12,289 gap
lines. Of 1,496 corpus declarations initialised by a call to a locally declared
function, 884 (59%) are stopped by one line: the generic test in
`check_call_expression`.

### Concentration: it is not a one-file artefact

Three rows measured in the previous cycle collapsed on inspection — one
12,376-line row was ~10,000 lines in a single file. This one does not. Counting
declarations initialised by a call to a generic function declared in the same
file, over the source carried in every `.types` baseline under
`vendor/typescript-go/testdata/baselines/reference/submodule`:

| measure | value |
|---|---|
| declarations | 1,223 |
| distinct files | 316 |
| top-10 files' share | 22.1% |
| top-50 files' share | 54.5% |
| largest single file | 42 (`compiler/genericFunctionInference1.types`) |

So generic calls are a genuine workstream spread across the corpus, and a slice
of them pays proportionally rather than paying once.

## The decomposition that chose the slice

The question that matters is not "how many type parameters" or "is there a
constraint" — it is **how the candidate becomes knowable**. Of 1,145
classifiable calls:

| shape | count | share |
|---|---|---|
| no type parameter written bare in a parameter position | 605 | 53% |
| every type parameter written bare in a parameter position | 370 | 32% |
| explicit type arguments, `f<string>(x)` | 93 | 8% |
| some, but not all, written bare | 77 | 7% |

The 53% row — `T[]`, `(x: T) => U`, `Partial<T>`, mapped and conditional types,
return-type inference — is what upstream's inference *engine* is for: the
candidate has to be dug out of the argument's type by a structural walk with a
priority lattice and contravariant tracking. The 32% row needs no engine at all.
`f<T>(x: T): T` called as `f(1)`: the candidate **is** the argument type, read
off position 0. That is a lookup wearing an inference's name, and it is what was
built.

### Alternatives, and what would make them win

- **Port `inferTypes` properly.** Rejected for this session, not on principle:
  it is a multi-thousand-line recursive walk whose correctness depends on
  inference priorities that only show up in the cases it exists to handle, and
  half of it — recursion without upstream's depth guards — is the kind of port
  that passes the corpus and hangs on a real program (the same objection
  `get_type_from_type_reference` records for instantiation). It wins as soon as
  someone can spend a whole cycle on it, and the 605-line row is the reason to.
- **Answer the uninstantiated return type.** This is the tempting shortcut: no
  inference, prints something. It prints `T` where upstream prints `number`, on
  every one of the 884 lines. Rejected under the `errorType`-not-`anyType`
  discipline — a wrong answer that looks like an answer is worse than a gap
  because it is invisible in the aggregate.
- **Instantiate explicit type arguments first** (the 93-line row). Genuinely
  independent of inference and probably the next cheapest thing; it needs a
  substitution over the return type, which this module deliberately does not
  have (see below). Not done here.

## Two things this gets right that a plausible port gets wrong

**1. Nothing is widened.** `getCovariantInference` (`inference.go`) widens an
inferred literal only when the type parameter "was fixed during inference or
does not occur at top level in the return type". Both shapes this module answers
have the type parameter *as* the return type, so upstream does not widen. The
oracle is direct —
`baselines/reference/submodule/conformance/callGenericFunctionWithZeroTypeArguments.types:10`:

```text
var r = f(1);
>r : number
>f(1) : 1
>f : <T>(x: T) => T
```

The call is `1`. The `number` on `r` comes from the declaration site, where
`get_widened_literal_type` already runs. A port that widens the candidate is
wrong on the call line and *right on the variable*, which is why it would
survive a careless test — so `a_bare_type_parameter_is_the_argument_type_unwidened`
asserts on the call.

The other half of the rule is visible at
`conformance/genericCallWithConstraintsTypeArgumentInference2.types:17`, where
`<T, U extends T>(t: T) => U` applied to `1` prints `number`: `T` does not occur
at top level in the return type there, so upstream widens it, and `U` falls back
to its constraint. That call has a type parameter with no bare parameter
position and is a gap here — which is why this module never has to decide about
widening at all. **If the slice ever grows past "the return type is a type
parameter", widening becomes mandatory and this is the first thing to port.**

**2. Type parameter identity comes from the declaration, not from the name.**
`Signature::type_parameters` carries names, and matching a parameter's type to a
type parameter by printed name would work on every fixture. It is wrong: a
nested generic function can declare a second `T`, and the two print alike while
being different types. `type_parameter_types` walks the signature's declaration
node to each type parameter's symbol and asks `get_declared_type_of_symbol`,
which is already interned per symbol, so the comparison is `TypeId` equality.

## Consequences accepted

- **No substitution.** The return type is answered only when it *is* a type
  parameter, or when it mentions none. `T[]`, `T | string`, `C<T>` and
  `(x: T) => T` are all gaps even when every candidate is known. Substitution
  needs a structural rebuild that re-interns unions in `CompareTypes` order and
  recomputes every printed form, which is a separate piece of work.
- **The "mentions a type parameter" test is textual, and deliberately
  over-eager.** A type that merely *contains* a type parameter carries no
  structural evidence of it here — `T[]`, `C<T>` and `(x: T) => void` are a
  `Named` or `Anonymous` whose payload is a string. So after checking identity
  and union/intersection constituents, `mentions_type_parameter` scans the
  *printed* form for a type parameter's name as a whole identifier. A false
  positive costs a gap; a false negative costs a wrong answer, so the bias is
  chosen rather than incidental. It over-gaps a signature returning the string
  literal type `"T"`. That is the price.
- **Two candidates for one type parameter gap** rather than unioning, because
  the union upstream builds is subtype-reduced and this port cannot decide that.
- **A rest parameter gaps the whole signature**, not just its position, because
  positional argument mapping stops being positional.

## How I would know this is wrong

- If the conformance pair does not move when `calls.rs:64` is wired, either the
  bare-parameter row is smaller in *assertion lines* than in declarations, or
  these calls are gapped earlier — most likely by the callee resolving to
  something without exactly one call signature.
- If `checker_types` **falls**, the `mentions_type_parameter` scan has a false
  negative and something is being answered uninstantiated. The scan is the only
  place in this module that can produce a wrong answer rather than a gap; the
  identity and position logic can only fail closed.
- If a line prints `number` where the baseline says `1`, widening crept in.
