# Declaration provenance for spread properties

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 8e234201: 455,064/478,855 matching assertions (95.03%).
The active 99% target requires 474,067 matches.

## Why vector order cannot model spreads

Native getNamedMembers (checker.go:22047) sorts the resulting symbols using
compareSymbolsWorker (utilities.go:366). Its primary key is the first declaration,
not the left-to-right spread insertion order and not the property's printed name.
getSpreadType copies the right symbol for a required override. Optional collisions
concatenate left declarations before right declarations, retaining the left origin.
The port's upsert_member preserves the replaced vector position, so it cannot
produce the native ordering when a spread changes the surviving declaration.

AnonymousProperty now carries its original symbol solely as declaration provenance.
Its semantic TypeId remains authoritative; the origin is not used to recompute a
merged or instantiated member's type. Capture, regularization, mapped and reverse
mapped images, instantiation, partial spreads and widening preserve that provenance.
Complete captured member sets are sorted with the existing compare_symbols helper.
Uncaptured methods/accessors/index mixes retain their current path and limitations.

The first full candidate gained 38 matches but lost 95. Shorthand properties had
no origin because the initializer-cache node id deliberately excludes shorthand
members. Taking provenance from the actual property node restores 94 of those
matches without changing that cache policy. Source order for {x,y} therefore does
not depend on the order in which x and y were declared as variables.

## Mapped source declarations

The last regression was Pick<Props, 'foo'>. Native getModifiersTypeFromMappedType
(checker.go:28127) reads the declared constraint of the mapped key parameter,
extends it once when it is itself a type parameter, and retains the operand of
keyof before applying the mapper. The port previously retained a modifier source
only for a directly written keyof operand. With K instantiated to a literal key,
that erases the original property's declaration and modifiers.

The indirect path now evaluates the declared constraint without the active alias
bindings, recovers the deferred keyof operand, restores the bindings and instantiates
that operand. The existing constraint cache is keyed by the mapper, so declaration
and concrete constraints remain distinct. A modifier source is not automatically
a printed keyof constraint; the node builder preserves the written constraint form.
Native remapped names intentionally have no copied declaration. The existing
name-type assignability test distinguishes filtering from remapping before origins
are attached (resolveMappedTypeMembers and getMappedTypeNameTypeKind).

Native controls cover required/optional replacement, properties after spreads,
repeated spreads, const copies, partial unions, plain literal ordering, shorthand
ordering, generic selected keys and renamed keys. Probes live in
/tmp/tsr-99-spread-origin-native and /tmp/tsr-99-spread-origin-mapped-native.
The first contains expected TS2783 diagnostics for required overwrites; native
still emits the declarations used for comparison.

Remaining distribution, complete semantic members and original optional-symbol
serialization are tracked in tsr-8. Full measurement and gates follow.

The indirect modifier source must stay separate from key enumeration. Native
resolveMappedTypeMembers enumerates the source's keys only for a syntactic keyof
constraint; Pick<T,K> enumerates K. Reusing the old modifiers-source test admitted
unrequested properties and lost seven RIGHT assertions (keyofIsLiteralContexualType
and mappedTypeContextualTypesApplied). A dedicated syntactic-constraint flag now
keeps those native choices distinct. The same flag controls mapped constraint
printing. Spread extraction also resolves mapped members before inspecting the
captured table, matching getPropertiesOfType's lazy resolution boundary.

## Frozen candidate

/tmp/tsr-99-spread-origin4.tsv records 455,107 RIGHT, 2,910 GAP and 16,226 WRONG
among 474,243 aligned rows. Relative to 8e234201 this adds 43 matches: 41 WRONG
to RIGHT and two GAP to RIGHT. There are zero RIGHT losses, zero GAP to WRONG
and 13 changed-wrong answers. The full denominator remains 478,855, giving 95.04%;
the 99% target still needs 18,960 matches.

Ten changed-wrong rows are spreadMethods, whose order improves but whose method
syntax still differs. Three are generic mapped spreads/rest: eager capture prints
a constrained member table where native retains the generic mapped type. Those
require the generic-object spread/intersection path, not a printed-text correction.
They remain in tsr-8 alongside union distribution and complete semantic extraction.

The two focused tests cover eleven native outputs. A selected-only spread is a
negative control against accidentally enumerating all modifier-source keys.
A RIGHT loss, a selected-key leak, changed shorthand source order, or missing
origin propagation through repeated spreads falsifies the relevant claim.
Sequential primary-thread review compares every production hunk to the pinned
native functions; no independent or cross-model review is claimed. The existing
symbol comparator and captured TypeId semantics are reused.

Checker sources plus trace_case SHA-256: 36bb916090d007c2003a9a2bcbcb8fc8893d73990113c6a6b0cfbf9a36659a12

The workspace caught a legacy assertion in members_object_spread.rs claiming
that overwritten properties retain their vector position. An exact native control
(/tmp/tsr-99-spread-order-legacy-native) disproves it: `{...o,a:"x"}` prints `b,a`
when o declares `a,b`, while `{a:"x",...o}` prints `a,b`. The test and its source
comment are corrected to the pinned native result rather than retaining an old
port-specific invariant. The shared property_members renderer replaces a duplicate
conversion after sorting.

Release workspace gates pass 202 result blocks after the legacy expectation is
corrected. Clippy, formatting and whitespace checks pass. The final source uses
the shared member renderer; the committed-checkout verdict will verify that this
simplification preserves the frozen candidate exactly.
