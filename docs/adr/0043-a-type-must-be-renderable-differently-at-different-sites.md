# ADR-0043: A type must be renderable differently at different sites

**Status:** Proposed — this record exists to make the decision, not to announce one.
It supersedes nothing yet; it names a limit in
[ADR-0003](0003-tree-plus-side-tables.md)'s side-table design that three separate
rows have now hit, and it prices the two ways out so a future session can choose
with the evidence in front of it rather than under time pressure.

## The forcing constraint

`TypeData::Named` carries a `text: String` — **the printed form, computed once at
creation from the declaration** (`crates/tsr-checker/src/types.rs`). The module
documents this as a deliberate divergence: upstream carries a `symbol` on the type
and its node builder renders the name at each site, applying scoping rules this
port has no equivalent of.

That was the right call for a long time and is still right for most types. It has
one consequence that has now stopped being theoretical: **a type can be printed
exactly one way, everywhere.** Upstream can print the same type differently at two
sites, because `TypeToTypeNode` takes the enclosing declaration as an argument.

Three rows, found independently, are all blocked on exactly that:

| row | cases | what upstream does that this port cannot |
|---|---:|---|
| type-alias names | 4 | prints `T` where `T` is accessible, `{}` where it is not |
| `typeof globalThis.X` | 12 | prints a qualified name whose qualification depends on the site |
| `export =` module objects (`bd tsr-e2u`) | ~7 | prints `typeof React`, not `typeof __React`, per site |

The alias row is the cleanest witness because upstream can be executed on it
directly. One probe, five positions:

```text
type A = {};                       >A : A
label: type B = {};                >B : {}
function f<U>() { type C = {}; }   >C : {}
function g()    { type D = {}; }   >D : {}
namespace N { type E = {}; }       >E : E
```

The discriminator is **accessibility from the enclosing declaration** — top-level
and namespace aliases are reachable by a symbol chain, function-local and labelled
ones are not. Deciding *that* is easy and needs no new architecture. Acting on it
is impossible today, because the port has nothing else to print:

```text
type T = {};                      get_declared_type_of_symbol prints "T"
function g() { type U = {}; }     get_declared_type_of_symbol prints "U"
```

The structure behind the name is not retained. **The predicate is the easy half;
the representation is the hard half.**

## Why this is a decision and not a backlog item

The instinct is to size it by the rows above — 23 cases, against touching every
consumer of the printed form, and the answer is obviously no.

**That sizing is wrong, and it will keep being wrong.** These three rows were not
selected for having this property; they are simply the ones reached so far. Every
future row whose answer depends on *where* a type is printed lands here, and the
port cannot converge on upstream's `.types` output without the capability, because
upstream's renderer is site-dependent by construction. Pricing it by today's rows
is the same error that priced `parseDelimitedList` at 6, then 7.

The counter-consideration is real and should not be minimised: this is
`ADR-0003`-scale surgery on a load-bearing representation, in a port that is 54%
converged and moving. Doing it badly is worse than not doing it.

## The options, with their costs

**Option 1 — retain the structural rendering alongside the name.** Widen
`TypeData::Named` (or add a side table) so a type carries both its name and the
form it would print without one, and let the writer choose. Faithful in shape to
upstream, which keeps the symbol and renders on demand.

- Touches every consumer of the printed form.
- Needs a rule for *when* the structural form is computed — eagerly at creation
  doubles the work for types that never need it; lazily needs the declaration to
  still be reachable.
- Does not by itself give site-dependence; it gives *two* forms. `typeof
  globalThis.X` needs qualification that varies per site, which is a third form.

**Option 2 — re-render from the declaration's type node at the writer.** When
accessibility fails, print the alias's written type node instead.

- Cheap, and would move the 4 alias cases today.
- **Wrong in general, and recorded as wrong rather than left to be rediscovered as
  attractive**: it is a syntactic stand-in for a semantic rule, and prints the
  *written* form where upstream prints the *computed* one. `type T = Array<string>`
  would print `Array<string>` where upstream prints `string[]`, and any alias whose
  right-hand side is not already normalised diverges.
- Does nothing for the other two rows, whose answers are not written anywhere.

**Option 3 — do nothing, and keep the rows refused.** Legitimate, and the current
state. The cost is that the refusals accumulate and each new one has to
re-establish the same diagnosis; this record exists so that it does not.

## The consequences accepted

Whichever is chosen, the port keeps a **rendering** divergence from upstream rather
than a data-model one — types are still identified per symbol, which is what
ADR-0003 actually bought. Nothing here reopens the tree-plus-side-tables decision.

If Option 1 is taken, the accepted cost is a wider `TypeData` and a slower type
store, in exchange for the ability to answer a question the port currently cannot
phrase.

## How we would know this is wrong

- If a fourth and fifth row do **not** appear over the next several sessions, the
  "cannot converge" argument is overstated and Option 3 is correct on the numbers.
- If the accessibility predicate turns out to answer identically for every type
  whose name is reachable at *any* site — i.e. if site-dependence collapses to a
  per-type property — then a single extra field suffices and Option 1's cost was
  overestimated.
- If upstream's own baselines can be matched by normalising the written form
  (Option 2 plus a normaliser), the semantic objection is weaker than stated. This
  is testable today with the probe runner and has not been tested.

  > **Tested 2026-08-12, and it does not hold — the objection is *stronger* than
  > stated.** Five aliases were declared inside a function body (where the name is
  > not accessible, so upstream must render structurally), each with a written form
  > differing from the computed one. Upstream's own runner records:
  >
  > | written | upstream prints |
  > |---|---|
  > | `Array<string>` | `string[]` |
  > | `1 \| 1` | `1` |
  > | `string & string` | `string` |
  > | `{ a: 1 } & { b: 2 }` | `{ a: 1; } & { b: 2; }` |
  > | `keyof { a: 1; b: 2 }` | `"a" \| "b"` |
  >
  > The first three are syntactic normalisations a rewriter could plausibly reach.
  > **The last one is not.** Turning `keyof { a: 1; b: 2 }` into `"a" | "b"`
  > requires *evaluating* the type — resolving the object's members and forming the
  > union of their keys — which is the checker's job and not a normal form of the
  > written syntax. A normaliser capable of that is a second type checker.
  >
  > So Option 2 cannot be rescued by normalisation, and the "syntactic stand-in for
  > a semantic rule" objection is not a matter of degree. Note also that `f4` shows
  > the written and computed forms *coinciding* — which is why Option 2 looks
  > workable on the fixtures anyone reaches for first.

## Evidence

- `crates/tsr-checker/src/types.rs` — `TypeData::Named`'s note on the divergence.
- `STATUS.md` §5, "The `getSymbolChain` accessibility walk" — the probe, the
  measurement, and the two options as first recorded.
- `docs/conventions.md` corollary 27 — the upstream probe runner used to settle the
  alias question, and why a written probe supplies controls the corpus cannot.
- checker-1's §226 — the same shape one level down: a function signature that could
  not express "upstream declines here". A representation making a necessary
  distinction unsayable is the recurring form.
