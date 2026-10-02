# Type-level spread folding

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline f2661fe5: 455,107/478,855 matching assertions (95.04%).
The active 99% target requires 474,067 matches.

## Native control flow

checkObjectLiteral flushes ordinary property groups before each spread and after
the final spread (checker.go:13283 and 13341). getSpreadType (13387) folds whole
types: top types, never, partial-union normalization, union distribution, generic
intersections, then concrete property merging. Flattening printed member vectors
cannot preserve union alternatives or generic operands.

The new spreads module follows this fold for ordinary property/shorthand groups.
The existing member checker checks each contiguous slice with the original node
identity, retaining contextual property typing and declaration provenance. Captured
batch properties are authoritative: the enclosing binder symbol contains all
properties in the literal, including those outside the current batch.

Generic object/index flags are distinguished as in getGenericObjectFlags (24880).
Mapped constraints and instantiated name types determine generic mapped types;
variadic tuples and instantiable nonprimitive types retain their generic identity.
An empty left operand preserves a generic right operand. A final concrete object
in a generic intersection merges with the next concrete group, so repeated and
trailing overrides use the same operation. Union cross-products reject sizes of
100,000 or more, matching checkCrossProductUnion (26648).

Concrete merging retains the surviving declaration origin, left presence for an
optional collision, and semantic value unions. Captured optional declarations need
their optional marker before merging: the first focused run caught its omission
because two optional sources lost undefined in the merged display. Native creates
a fresh property without Readonly for optional collisions, even in const context.
The native constMerge probe establishes that boundary.

## Deliberate remaining boundaries

The shared fold is not yet used for literals containing methods, accessors or
computed property batches. Their symbol flags and index declaration provenance
are incomplete in AnonymousProperty. The first candidate routed computed batches
through the new fold and lost two RIGHT assertions in
useBeforeDeclaration_propertyAssignment. Keeping those batches on the existing
collector restores both without broadening incomplete member data into a claim.
Source index infos remain a gap on the new path, instead of silently disappearing.
The existing spread extractor still limits instantiated references and class-member
visibility. These are remaining tsr-8 work, not completed spread support.

The normalization helper still lacks full structural-empty resolution and generic
constraint reduction. Three newly exposed wrong answers are recorded: arrowExpressionJs
prints T | {} instead of {}; commentsAfterSpread prints any instead of an index
object; nestedObjectRest prints any[] instead of a tuple. Seven changed-wrong rows
and three WRONG-to-GAP rows remain. The optional-symbol serialization difference
in spreadUnionPropOverride predates this unit. No corpus expectations were edited.

## Controls and frozen candidate

Two new pipeline tests cover eleven independently checked native outcomes:
generic identity, leading/trailing groups, repeated groups, required overrides,
generic mapped types, union distribution and cross-products, any, partial unions,
and const optional collisions. Existing optional/exact-optional, invalid spread,
mapped-key and declaration-order tests remain controls.
Native probes: /tmp/tsr-99-spread-fold.ts and
/tmp/tsr-99-spread-fold-native/tsr-99-spread-fold.d.ts.
The native overwrite diagnostic TS2783 is expected; declaration emission succeeds.

The frozen candidate /tmp/tsr-99-spread-fold2.tsv records 455,237 RIGHT,
2,879 GAP and 16,127 WRONG among 474,243 aligned rows. Relative to f2661fe5:
99 WRONG-to-RIGHT, 31 GAP-to-RIGHT, zero RIGHT losses, three GAP-to-WRONG,
three WRONG-to-GAP and seven changed-wrong rows. The full denominator is 478,855.
The final review restores native normalization order: the caller partially normalizes
spread operands, and the fold normalizes its right operand after left distribution.
The cross-product guard belongs inside each union arm, after that arm's normalization.
The isolated committed verdict must reproduce the candidate.

All 203 release workspace result blocks pass before that final control-flow ordering
adjustment; focused tests and clean-checkout gates verify the final source.
The sequential primary-thread review covers correctness, native ordering, resource
limits, tests, standards and maintainability. No independent or cross-model review
is claimed. Simplification retains the existing collector and renderer because the
remaining mixed-member path is still live; removing it would change behavior.

Final checker sources plus trace_case SHA-256: 7dd422e9a2b8ac0db60a1da77891b8af15c1ba2d14697d4d3e96ffe0af9ca870

Committed-checkout measurements, mutation results and final gates follow in the
evidence update. The broader 99% goal and tsr-8 remain active.
