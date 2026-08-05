# ADR-0037: Object types are compared structurally, over base symbols

**Status:** accepted, 2026-08-05
**Context:** `bd tsr-4sc`, supersedes in part
[ADR-0035](0035-assignability-answers-false-rather-than-guessing.md)
**Code:** `crates/tsr-checker/src/relater.rs`,
`crates/tsr-checker/src/members.rs`, `crates/tsr-checker/tests/relater.rs`

## The forcing constraint

ADR-0035 gapped structural comparison for one stated reason, and it was a real
one: *nothing could enumerate a target's properties through its base types*.
`get_property_of_type` could look a name **up**, but a structural check needs to
iterate — and one that iterated only a target's *own* members would silently drop
every inherited requirement and answer `true` for a source that fails them. A
wrong `true` produces a confident wrong type.

That blocker is gone. `base_symbols_of` (`crates/tsr-checker/src/members.rs`,
commit `efae007`) walks the `extends` graph and, critically, answers `None`
rather than an empty list for a base it cannot follow. `None` is what makes
enumeration safe: a target whose inherited requirements cannot be listed is
simply not satisfiable here.

## The decision

`propertiesRelatedTo` (`internal/checker/relater.go`) is ported over that walk.
For every property name of the target — own first, then each base in declaration
order, first hit winning — the source must have a property of that name, and the
two property types must be related.

**Names are enumerated; symbols are not.** `property_names_of` collects names
only, and the symbol for a name is then taken from `get_property_of_type`. The
alternative — collecting `(name, symbol)` pairs during the walk — was rejected
because it decides shadowing in a *second* place, and the two places would drift
the day either walk changes. One extra hash lookup per property is the price.

**The walk is over symbols, not resolved types**, for the reason
`get_property_of_type` already documents: there is no resolved-members table to
layer bases into. This inherits that shape's falsifier verbatim — the moment
`getDeclaredTypeOfClassOrInterface` grows instantiated members for
`class C extends B<number>`, both walks must move to the type level together.

## The consequences accepted

Three kinds, and they are not symmetric:

- **Missing rejections, i.e. too permissive.** `readonly`, `private`/`protected`
  identity and the property-vs-method distinction are not compared. A source
  differing *only* in one of those relates here and would not upstream. This is
  the one place this port can now produce a wrong `true`, and it is bounded to
  those modifiers: names and types are fully checked. ADR-0035's blanket
  falsifier — "any `true` between two distinct object types is a defect" — is
  replaced by this bounded one.
- **Missing acceptances, i.e. gaps.** Optionality is not read, so a target with
  an optional property the source lacks fails. Signatures, index signatures and
  variance markers contribute nothing.
- **Cost.** Enumeration allocates a `Vec<String>` per target visited. Measured
  against the alternative of interning, this was not worth optimising before a
  profile exists on a real corpus.

## The recursion limits stopped being theoretical

This is the substantive change ADR-0035's addendum records: structural
comparison is what creates the loop. Both guards were re-measured under mutation
and both now bite — the relation cache buys the *answer* on
`interface A { x: B }` / `interface B { x: A }`, the depth cap buys *stack
safety* on a long chain, where raising it aborts the process. `bd tsr-el3.2`'s
rule — port the limit with the code it guards — paid here exactly as intended.

## How you would know this was wrong

- **A wrong `true` outside the modifier list above.** If a source relates to a
  target while missing a property or carrying an unrelated property type, the
  enumeration is under-reporting and `base_symbols_of`'s `None` contract is the
  first place to look.
- **`base_symbols_of` starts returning an empty list for an unfollowable base.**
  That single change turns every gap here into an unsound `true`. The two
  functions are coupled by that contract and the coupling is not enforced by the
  type system.
- **Conformance falls rather than rises.** This unblocks assignment reduction
  and overload resolution; if those land and the assertion-line share does not
  move, the shape was wrong rather than merely incomplete.
