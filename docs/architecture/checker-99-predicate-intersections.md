# Predicate narrowing and intersection members

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code 2671c87c, evidence 9cdd1038: 456,480/478,855 matching assertions.

## Preserve generic identity during narrowing

Native getNarrowedTypeWorker (internal/checker/flow.go:859) maps both candidate
and input constituents. Predicate calls use strict-subtype and subtype ordering;
instanceof, including a custom Symbol.hasInstance predicate, uses derivation.
If no constituent relates directly, a generic whose constraint relates to the
candidate becomes an intersection. The all-never tail tries subtype and both
assignability directions before intersecting. The false predicate branch filters
against the true result; unknown first expands to the native unknown union.
The port now shares this worker between both callers and preserves unchanged
union identities and origins through its local mapType equivalent. Unimplemented
relations still return an undecidable answer rather than a fabricated negative.
The native key-property index remains an omitted optimization.

The old member-name heuristic discarded structural supersets for every predicate.
Removing it exposed the actual distinction: hasInstance must select the derived
flavor. Reverse comparisons also need to decide that an object does not derive
from a primitive target. Without that negative answer the whole map declined,
leaving string in object unions. Ordinary predicates retain structural subtypes.
Assertions now share getTypePredicateArgument with ordinary calls and use the
existing discriminant-aware predicate narrowing, including asserts this is T.

Synthetic polymorphic this carries its class owner for constraint lookup, but
is still a type parameter rather than an object member table. The relater follows
its source constraint and rejects concrete values against an arbitrary target
parameter. Two prerequisites became observable: fresh higher-order parameter
renaming must retain its instantiation mapper for recursive constraints, and
constraining intersections that cover a variable's complete primitive constraint
must recombine to that variable. The latter uses a TypeId side table corresponding
to ObjectFlagsIsConstrainedTypeVariable and native removeConstrainedTypeVariables
(checker.go:25881). Existing limitations of constraint reduction remain.

## Intersection property identity

Native createUnionOrIntersectionProperty combines every contributing member and
substitutes the entire intersection for polymorphic this. The previous port
refused multiple typed contributions and some this-bearing declarations. It now
intersects their types, skips constituents reduced to never in union projection,
and recognizes required conflicting literal discriminants and distinct private
property declarations. Reduced views remain separate from written identities.
Readonly flags combine with AND for intersections, while unions retain OR.
Accessibility diagnostics do not replace a readable property's type.

getPropertyOfTypeEx (checker.go:18900) first searches intersection constituents
without global Object/Function augmentation. Only an empty result retries with
augmentation. Alias and Omit traversal must preserve this flag: resetting it
repeatedly expanded recursive lookup, confirmed by a stack sample and an aborted
full run. Preserving it restores normal corpus completion. Conflict reduction
also avoids forcing a property contributed by only one constituent; such a
property cannot conflict, and evaluating it prematurely perturbs recursive
alias resolution. Different instantiations of one declaration still count as
multiple contributions.

Identity mapped members now read their instantiated source, then apply optionality.
Reading the reused symbol alone lost generic arguments and class-static prototype
identity. This fixes Readonly<typeof Model> inside a constructor intersection and
Readonly<Box<number>>. Deferred generic mapped objects retain their existing
limitations; this is not a complete native mapped-member implementation.

## Alternatives and adverse transitions

The predicate-only draft gained five matches and lost three, exposing alias and
polymorphic-this identity. Completing assertion receivers, constraints and member
projection revealed readonly/private and hasInstance prerequisites. An attempted
accessibility-to-any shortcut lost 240 RIGHT rows and was removed: native performs
that diagnostic separately. The first global-augmentation pass ran for more than
seven minutes with repeated recursive property lookup; its incomplete verdict is
not measurement evidence. A corrected pass restored all prior RIGHT assertions.

The last measured candidate adds 275 matches: 217 WRONG-to-RIGHT and 58
GAP-to-RIGHT, with zero RIGHT losses. Nine previously gapping rows now produce
wrong types: seven in reverseMappedIntersectionInference2 and two in
nonNullParameterExtendingStringAssignableToString. These expose incomplete
reverse-mapped tuple/contextual inference and generic-union constraint reduction.
They are tracked in bd tsr-9 and are accepted, explicitly recorded limitations of this prerequisite port;
no case-specific refusal or baseline change hides them. Forty-four already-WRONG
rows change but remain wrong. The denominator and upstream pin are unchanged.

## Validation and review boundaries

Five new tests extend contextual_this_objects from 19 to 24. Native controls
cover generic and this predicates, negative unknown/candidate unions, recursive
parameter renaming, constraint coverage, whole-intersection receivers, derived
versus structural narrowing, readonly writes, conflicting members, global
augmentation and instantiated mapped sources. Native negative controls intentionally
report TS2540 and TS2339. The port still exposes its internal error sentinel for
two invalid member reads where native prints errorType as any; those assertions
check refusal rather than claiming native type-printing parity.

Review is sequential in the primary thread under the user's AGENTS override;
there is no independent or external review. Reuse and maintainability review
removed duplicated narrowing workers, obsolete member heuristics and a redundant
relater guard. Correctness/adversarial review traced predicate flavor, union
identity, recursive alias entry, generic mapper identity and diagnostic/type
separation. Performance review reproduced and fixed augmentation-flag loss.
The documented remaining inference/reduction limitations stay open; this batch
does not complete the 99% goal.

Main validation passes all 217 release workspace result blocks, all 24 focused
tests, clippy, formatting and 3,317 upstream anchors. Aligned verdict evidence is
/tmp/tsr-99-predicate-eleventh.tsv: 456,755 RIGHT, 2,626 GAP, 14,862 WRONG across
474,243 rows. Final source hash (sorted checker/src/*.rs paths and bytes, then
conformance/src/trace_case.rs):
b9c9055f07bdf2970ec8384efafa2b3db644b5f0fefe359163b3bbe875ab9bef.
Only comment/dead-local cleanup followed that verdict. The isolated committed
verdict at 85ce61f6 matches it byte-for-byte, and the production source hash
matches the main checkout. Review receipt:
/tmp/compound-engineering-501/ce-code-review/predicate-intersections/review.json.

## Isolated committed verification

Commit 85ce61f6 reproduces 456,755/478,855 matching assertions (95.38%),
with 6,971/9,538 complete cases (73.09%): 275 additional assertions and 18 cases.
The target of 474,067 matches leaves 17,312 to 99%. All 217 isolated release
workspace result blocks, clippy, formatting and 3,317 upstream anchors pass.
Logs use the /tmp/tsr-99-predicate-verified prefix.

Fresh depend reports 459 non-gapping roots, 209 cycles, zero depth caps and
3,371 walked gaps. C3 balances; C1/C4 remain stale under tsr-6.29. This instrument
is not evidence that all remaining gaps or wrong answers have valid roots.

## Mutation checks

In the isolated checkout, restoring all eight changed production files to
9cdd1038 while retaining the new tests produces four assertion failures and
20 passing controls. A narrower mutation switches only Symbol.hasInstance
narrowing from derivation to structural predicates; it fails the fifth added
test, retaining Point3D on the true branch and dropping it on the false branch.
Restoring 85ce61f6 restores all 24 passing tests and the original production
source hash. Logs use /tmp/tsr-99-predicate-mutation and -restored-tests prefixes.
