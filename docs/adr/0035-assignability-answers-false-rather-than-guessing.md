# ADR-0035: Assignability answers `false` rather than guessing

**Status:** accepted, 2026-08-05
**Context:** `bd tsr-4sc`, `bd tsr-el3.2`
**Code:** `crates/tsr-checker/src/relater.rs`, `crates/tsr-checker/tests/relater.rs`

## The forcing constraint

`crates/tsr-checker` had no relation checking at all, and three separate items
were blocked on the same absence, by measurement rather than by assertion:

- **assignment narrowing.** `getAssignmentReducedType` is
  `filterType(declared, typeMaybeAssignableTo(...))`. `bd tsr-4sc.11` measured
  that condition narrowing over declared unions is worth ~190 lines and that the
  remaining ~9,350 live in `any` evolution and *the assignment reduction*, which
  "needs assignability".
- **overload resolution.** `resolveCall` picks among candidates by assignability;
  `calls.rs` gaps every overloaded call for exactly this reason.
- **contextual typing**, whose remaining arms need it.

## The decision

Port the **assignable relation only**, over the type shapes this port actually
has — intrinsics, literals, unions, intersections — and have **object types
relate only to themselves**.

That last clause is the decision. The alternative was to write a structural
comparison over the machinery that does exist: `get_property_of_type` can look a
name up on a `Named` type, so a check could enumerate a target's *own* members
and ask the source for each.

**That alternative was rejected because it is unsound in the permissive
direction.** Nothing in this crate can list a type's properties transitively
through its base types, so a check enumerating only the target's own members
would silently skip every inherited requirement and answer `true` for a source
that fails them. A wrong `true` produces a confident wrong type that propagates;
a wrong `false` produces a gap that shows up as a missing line. The two are not
symmetric, and the project's standing rule is that an unported form yields a gap.

**What would have to change for the rejected option to win:** a transitive
property enumeration on `Named` types — upstream's `getPropertiesOfType` over
`resolveStructuredTypeMembers` — plus variance markers on type references and
signature relation. At that point the structural arm is written against complete
information and the argument above evaporates.

`strictNullChecks` is likewise assumed **on**, for the same asymmetry: with it
off, `undefined` and `null` are assignable to nearly everything, so guessing
"off" is guessing in the permissive direction. No compiler options reach the
checker yet.

## Consequences accepted

- Two structurally identical interfaces are **not** assignable here. There is a
  characterisation test saying so, `two_structurally_identical_interfaces_are_a_gap`,
  which is meant to go red and be deleted when structural comparison lands.
- The relation cache is **per-call**, not per-checker, because `checker.rs`
  belongs to another workstream and could not take a field. Termination is
  unaffected; only the cross-call memo is lost, which is time rather than answers.
- **Nothing calls the relation yet.** It is enabling work, and this ADR says so
  rather than crediting it as lines. `bd tsr-4sc.11` records that this project
  has a documented habit of crediting enabling work as progress; the falsifier
  below is the guard against repeating it here.

## The recursion limits, and a correction

`bd tsr-el3.2` requires algorithmic limits to be ported with the code they guard
rather than retrofitted. This work was commissioned with the reasoning that the
loop *does* exist here, unlike the two cases where it was deliberately skipped.

**That reasoning was tested and is wrong, and the number is corrected here rather
than quietly.** Deleting the cycle guard and running
`a_recursive_type_terminates` leaves it green in milliseconds, because the only
cycles in a type graph run through an object type's *members* and this walk stops
at object types. `interface I { x: I }` is a cycle in the type graph and not a
cycle in this relation.

The depth cap and the cycle cache are ported **anyway**, because the loop appears
in the same commit that adds structural comparison and retrofitting a limit means
re-deriving which recursion it guards. What is not claimed is that they are
tested: they are recorded as unexercised, in the module docs and on the test.

## How you would know this was wrong

- **The gap is not where it is documented.** Mutating the trailing `false` in
  `structured_type_related_to` to `true` left the gap test green, which located
  the real gate one level up in `is_related_to`. Recorded because it is the kind
  of thing a reader would otherwise re-derive.
- **The relation is never called.** If the assignment reduction and overload
  resolution land without using `is_type_assignable_to`, this module was the
  wrong shape and the ADR should be superseded rather than amended.
- **A permissive answer appears.** Any `true` between two distinct object types
  is a defect by construction, not a feature; there is no arm that can produce
  one today, and if one appears the structural work landed without updating this
  record.
