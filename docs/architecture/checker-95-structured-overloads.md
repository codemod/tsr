# Structured overloads, effective constraints, and object freshness

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline a529367a: 451,884/478,855 correct assertions (94.37%).

## Native rules and implementation

getEffectiveConstraintOfIntersection (relater.go:2282) follows instantiable
constituents to their constraints. Wider target unions retain the original
variables alongside the constraints; disjoint-domain constituents participate in
the combined intersection. IntersectionFlagsNoConstraintReduction propagates
through nullable factoring, division and cross-product expansion, preserving
those variable identities. structuredTypeRelatedTo (relater.go:3216) retries the
combined constraint without interpreting an unknown original relation as false.
The port still lacks the complete native getNormalizedType step.

The overload subtype pass now asks the relation about structured arguments. The
previous primitive/class/type-variable gate hid two prerequisites. First,
signaturesRelatedTo (relater.go:4441) erases generic parameters for overload
matrices and comparable single signatures. Erasure reuses the existing signature
mapper. An uncomputed signature or pair remains unknown, while another source
signature can still prove the target compatible. Second, hasExcessProperties
(relater.go:2714) checks the names written on a fresh object literal, including
target union/intersection properties and applicable indexes. Spread-only names
are excluded. Assignability/comparability retain the Object/empty-object and JS
literal exemptions. Intersection constituents skip the already-completed outer
name check; their nested property checks still inspect fresh sources. Target
unions compare the regular source after the whole-target excess check.

getRegularTypeOfObjectLiteral (checker.go:28159) now caches a separate regular
identity and transforms nested member types. It preserves rendering metadata,
index infos and JS identity, and reads method/accessor-bearing literals through
their resolved symbols. The freshness part of getWidenedType runs at variable,
function-return, covariant inference and reverse-mapped inference boundaries.
Spread literals use captured semantic member types because their copied members
have no symbol in the new literal's binder table. Full widening of array/tuple
arguments and contextual object normalization remain separate work.

The namespace lookup arm now reads merged exports and merged symbol flags,
matching getMergedSymbol in binder/nameresolver.go:104. An unqualified annotation
inside a reopened namespace can resolve declarations from earlier blocks/files.
This restores the actual Intl return types exposed by the broader subtype pass.
The binder control verifies cross-file exports, local shadowing, non-exported
name isolation and type-versus-value meaning.

## Experiments and controls

Combined constraints alone changed no corpus verdicts, but the direct controls
prove wider-union identity and disjoint-domain reductions. Removing the subtype
gate initially gained 81 matches and lost 25. Signature erasure restored eight;
merged namespace exports restored eleven. An eager whole-set erasure introduced
16 DOM predicate-narrowing losses: one uncomputable generic overload had hidden
a usable partner. Pair-local unknown handling restored all 16.

Freshness checks restored the six original object-overload losses but exposed 12
missing widening-boundary cases. Covariant and reverse-mapped inference widening
restored them. Native controls additionally caught function-return freshness,
spread property lookup and nested objects in literals with methods. Fixing the
member transformation adds 14 spreadMethods matches. A final review corrected
unknown-pair aggregation so a failed alternative cannot turn an uncomputed one
into a proven rejection; this changed no full-corpus verdicts.

Pinned native declarations and Rust controls cover fresh excess rejection,
regular variables, spreads and own properties after spreads, returned objects,
nested fresh versus regular properties, indexed/union/intersection targets,
generic constrained inference, reverse-mapped inference and DOM predicate
narrowing. Invalid fresh calls retain the native never result and native argument
diagnostics. Corpus expectations and the oracle revision are unchanged.

One control also exposed an existing representation gap: an inline conditional
function signature with infer R remains unresolved where the equivalent
TypeFunction<infer R> alias resolves. The regression control uses the supported
alias form to isolate reverse-mapped widening. The inline form remains recorded
in tsr-6.30, not counted as working.

Simplification and review run sequentially in the main thread under the user's
tool mapping. No independent agent review is claimed. Review covers constraint
cycles, intersection flag propagation, overload unknowns, freshness identity,
nested transformations, namespace visibility and the full aligned comparison.

## Checkpoint and remaining work

CODE_CHECKPOINT: 452,054/478,855 correct assertions (94.40%).
6,687/9,538 complete cases (70.11%).
Aligned verdicts: 474,243 total; 452,054 RIGHT; 3,453 GAP; 18,736 WRONG.
Relative to a529367a: 144 WRONG→RIGHT, 26 GAP→RIGHT, zero RIGHT losses,
two GAP→WRONG and 70 changed wrong answers. The 95% threshold is 454,913;
2,859 additional correct assertions remain.

The two new wrong answers in declarationEmitGenericTypeParamerSerialization
resolve a formerly missing object but print its method as a function property.
Other changed wrong answers include qualified Intl display, spread method
rendering, object freezing, cross-declaration overload order and nested promise
results. They remain explicit failures. Full discriminant-sensitive excess
checks, computed-pattern/substitution targets, normalization, this-argument
constraint instantiation, mapped indexed relations and broader CheckMode
propagation remain incomplete. tsr-6.28 and tsr-6.30 retain the semantic work;
tsr-6.29 retains the depend instrument repair.

Evidence: /tmp/tsr-95-structured-overloads-final-verdict.{tsv,log},
/tmp/tsr-95-structured-overloads-final-transitions.txt,
/tmp/tsr-95-structured-overloads-{workspace-tests,clippy,anchors,coverage,depend}.log,
and /tmp/tsr-95-oracle-structured-overloads.ts with native declarations.
Release workspace tests, clippy across all targets with warnings denied, format
and whitespace checks pass. All 3,363 upstream references resolve. Checker and
binder snapshots re-verify; binder remains 8,497/8,497. The depend run walks
4,449 gap lines with 575 C1 roots no longer gapping, 285 cycles, zero depth-cap
hits and balanced C3 arithmetic; C4 still quotes a historical population.
The aligned comparison is authoritative.

Checker sources plus trace_case.rs SHA256:
1da8e74f308632dd4406742ce5a188f07adfe7f15d8e423e491f230bd48ca346.
Binder source files SHA256:
8f4850ae832eece29698bb94df99cdecc8c548ab3012918a4e8812c2e69b36ba.
Review receipt: /tmp/compound-engineering-501/ce-code-review/structured-overloads/review.json.

