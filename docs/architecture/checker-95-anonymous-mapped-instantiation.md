# Anonymous mapped type instantiation

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 996bb885: 451,056/478,855 matching assertions (94.19%).

## The missing path

Named mapped aliases already instantiate their source and transform array and
tuple elements. Anonymous mapped return types retained their original generic
constraint/template or declined. instantiateTypeWorker now dispatches captured
mapped objects to getObjectTypeInstantiation/instantiateMappedType's corresponding
path (checker.go:22535–22603). The existing object-instantiation cache guards
re-entry. Constraint, template, modifier source and intersection constraints are
substituted by identity; concrete captured members render an anonymous object.

The alias and anonymous paths share the homomorphic sequence transformation.
Primitives pass through, unions distribute, tuples and arrays transform elements,
and intersections of sequences recurse. Alias distribution retains alias display;
anonymous distribution does not introduce one. Union distribution precedes the
failed-template check because an undivided nullable source may decline while
each constituent maps successfully. Retaining the originating declaration lets
generic images render modifier tokens and the mapped parameter correctly.

The alternative was another alias-shaped wrapper around anonymous types. That
would invent alias identity and duplicate the sequence rules. Shared semantic
metadata instead preserves the existing array, tuple and per-key machinery.
Key remapping, readonly index metadata and fully lazy recursive mapped types
remain incomplete; this is not a claim of the entire native instantiation model.

## Context and ordering prerequisites

The initial full run gained48 net matches but lost11 RIGHT assertions. Native
instantiateContextualType (checker.go:30817–30872) leaves mapped object contexts
uninstantiated. The port's eager substitution had previously declined, implicitly
preserving those contexts; the new support made the incorrect substitution
observable. Keeping mapped contexts generic in both argument-context paths
restores key-specific template reads. Written call type arguments still instantiate.

A destructured callback parameter subsequently recomputed its consumed context.
Native assignContextualParameterTypes preserves checked parameter types; this
port now reads the completed function signature for a checked parameter lacking
a binder symbol. It uses existing node/signature caches and their invalidation.
The computed-key corpus case then passes49/49 assertions.

getPropertiesOfType/getNamedMembers retain declaration order (checker.go:22049,
utilities.go:362). Complete captured property lists now supply that order for
original objects as well as instantiated images. This replaces the reverse
mapper's local order workaround; uncaptured binder objects retain their existing
enumeration path. Capture producers were checked for completeness before using
their lists as authoritative.

## Measurement and boundaries

Checkpoint CODE_CHECKPOINT: 451,117/478,855 correct assertions (94.21%);
6,652/9,538 complete cases (69.74%). The95% target needs3,796 more matches.
Aligned verdicts:474,243 total;451,117 RIGHT;3,719 GAP;19,407 WRONG.
Against996bb885:31 WRONG→RIGHT,30 GAP→RIGHT,zero RIGHT losses,
19 GAP→WRONG and37 changed wrong answers. Denominator and oracle are unchanged.

Newly exposed wrong answers comprise six primitive mapped-member rows, six const
readonly propagation rows, five nested destructuring rows and two conditional
mapped display rows. Changed wrong answers include twelve partially inferable
callback contexts, sixteen conditional/alias display rows, four destructuring
rows and five reverse-mapped constraint rows. No corpus expectations or case-name
filters changed. General context mapper behavior beyond mapped objects remains
outside this unit.

Pinned strict declaration controls verify object member order, readonly arrays
and tuples, nullable union distribution, primitive passthrough, optional/readonly
modifier removal and substitution of an additional type parameter. A separate
all-callback control reports native unknown (and the expected TS2322 when assigned
to string); the local callback parameter still exposes an unresolved indexed
type. That discrepancy remains open in tsr-6.9. The regression test also checks a
computed-key destructured parameter against the pinned corpus behavior.

Manual sequential review covered metadata containment, cache/re-entry behavior,
sequence sharing, mapper ordering, source order and completed-signature reuse.
No independent agent review was performed. Any previously correct assertion
changing or a claimed control disagreeing with native rejects the unit.

Evidence: /tmp/tsr-95-anon-cached-verdict.{tsv,log},
/tmp/tsr-95-anon-cached-focus.{tsv,log},
/tmp/tsr-95-anon-{mapped-controls,workspace-tests,clippy,anchors,coverage}.log,
and /tmp/tsr-95-oracle-anon-mapped.ts with emitted strict declarations.
Checker sources plus trace_case.rs SHA256:
40e735f8c187bd3abfe29e40f134917394fab998177da887d460d9b14258cae3.

Release workspace tests, clippy with warnings denied, all3,366 upstream anchors,
format and whitespace checks pass. Checker snapshot refreshed; binder retains
its preceding verified100% result. The95% goal remains active.
