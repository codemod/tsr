# Semantic adjusted type facts and non-null intersections

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 29828bdf: 454,134/478,855 correct assertions (94.84%).
The active 99% goal requires 474,067 correct assertions.

## Constraint and native behavior

Non-null generic narrowing previously stored an OBJECT-flagged printed name.
That preserved isolated spellings such as T & ({} | null), but gave subsequent
inference no intersection operands to substitute. A native probe applying two
generic guards successively returns T & {} and infers string and number at its
calls; the old port returned any. The focused test failed before implementation.

Native getTypeWithFacts (checker.go:31150) filters constituents using intersecting
fact bits. getAdjustedTypeWithFacts (checker.go:31159) separately expands unknown
into its distinct empty-object/null/undefined union, filters it, recombines an
unchanged expansion, and intersects surviving nullable constituents. The choice
between {} and {} plus the opposite nullable uses both whole-union and
constituent facts (removeNullableByIntersection, checker.go:31179).
getTypeFactsWorker first reads base constraints (checker.go:30982).

The port now follows these operations separately. Two empty intrinsic objects
preserve native identity, and the strict unknown union uses canonical member
order. Global NonNullable is evaluated through its actual alias body; an
intersection retains its alias print and reference arguments alongside semantic
operands. Without that global alias, the native fallback is T & {}. Three older
lib-less expectations described historical behavior; direct native probes now
justify their corrected expectations.

## Necessary consumer boundaries

Several existing shortcuts became observable once narrowed types carried their
real structure:

- checkNonNullType must adjust only when IsUndefined/IsNull facts require it.
  Unconstrained generic member receivers otherwise acquire unwanted intersections.
- The for-in path uses getNonNullableTypeIfNeeded, preserving a bare generic T.
- Generic predicates and assertions need a resolved, instantiated effects
  signature. The existing inference engine supplies the same substitution for
  predicate types as it does for ordinary signature slots.
- filterType discards an intersection origin after filtering its normalized
  union (checker.go:26568). Treating it as a union origin discarded all surviving
  members and answered never for consecutive guards.
- Flow joins recombine unknown. The empty-object strict-subtype exception and
  Object derivation rules distinguish {} from object and Object.
- In strict mode unknown cannot inhabit an object, including {}. Treating this
  negative relation as undecidable prevented common-supertype inference from
  choosing T over its narrowed subtype in overloaded array calls.

The old refinement-kind lattice and spelling cache are removed. A limited
reverse map remains for the port's union reducer: it records only semantic
intersections containing the base, including their distributed members. Shared
primitive and error results are excluded. This keeps flow joins reducible while
the broader native subtype reducer retains its documented capability boundaries.

## Alternatives and accepted limits

Keeping opaque names would retain the inference failures. Replacing them without
porting their consumers initially gained 197 matches but lost 307 previously
correct assertions. Boundary fixes reduced losses successively to 179, 126, 30,
13 and four; none of those experimental candidates is a delivered checkpoint.
The final four were the common-supertype relation described above. A member-access
ablation changed none of the remaining large-parser regressions, localizing them
to unresolved type carriers instead.

Unresolved names in this port use ANY-flagged types that also carry a reusable
written annotation. Non-null adjustment retains those carriers. Native declaration
emit returns any for an unresolved-name return, while expression baselines retain
the written name; changing the port's annotation-reuse model is outside this
unit. This is an explicit representation boundary, not a new semantic type kind.
The non-strict fact aggregates and fully general subtype reduction remain
incomplete. Native strictness is always passed explicitly to probes.

## Verification and falsifiers

Focused tests cover generic and constrained guards, opposite nullable values,
unknown's true/false branches, nested alias applications, strict false, absent
libraries, sequential checks, generic predicates/assertions, optional chains,
for-in, Object narrowing, written union identity, and independent string/number
instantiations. Native source/declaration probes are under /tmp/tsr-99-adjusted-*.
Full-corpus transitions compare against the committed dependent-bindings baseline.
A previously correct assertion lost, a primitive registered as its own refinement,
or a mismatched pinned source hash invalidates the corresponding claim.

Review and simplification run sequentially in the primary thread under the user's
AGENTS tool map. No independent or cross-model review is claimed. Final committed
measurement, quality gates and mutation evidence are recorded below when complete.

## Candidate checkpoint and review

The frozen candidate gains 262 correct assertions: 247 WRONG-to-RIGHT and 15
GAP-to-RIGHT, with zero RIGHT losses. Two GAP-to-WRONG callback tuple assertions
in narrowingByTypeofInSwitch now expose X & Function where native expects X.
One WRONG-to-GAP indexed read in typeVariableTypeGuards remains unresolved:
native NonNullable<T>[K], formerly T[K], now error. There are 79 changed wrong
answers. These are visible residuals, not coverage gains.

Review corrected two switch optional-chain paths to native's plain fact filter
(flow.go:1226). It also excluded callable, indexed and mapped shapes from the
empty-anonymous-object predicate (checker.go:26476-26482); empty property tables
alone cannot establish emptiness. Together these final corrections gain one more
match than the earlier 261-match candidate. Seven native-controlled tests pass.
The explicit written generic predicate/assertion probe returns string in native,
and the callable/indexed versus generic-empty-object probe returns never/T/never.

The isolated scratch mutation bypasses adjustment with the plain fact filter.
The generic narrowing test fails as intended; its separate Cargo target prevents
artifact reuse from the working checkout. Review and simplification retained the
existing inference engine and removed obsolete refinement storage and comments.
Clippy identified a redundant object-facts branch and a test rustdoc formatting
issue; both were corrected before committing. The source hash below includes the
branch simplification. The committed checkout must reproduce candidate verdicts.

Checker sources plus trace_case SHA-256:
3c0ee8c5da53acc5ea354f11199824486e0b8f635d8d719474364ed821be270a.

Remaining consumer and indexed-read work is tracked in tsr-6.34. The pre-existing
derivation helper also retains its early primitive/object capability boundary;
this unit does not claim the complete native subtype/derivation algorithm.

The final release workspace run passes 195 result blocks. Clippy
with warnings denied, formatting, whitespace checks and 3,343 upstream anchors
pass. Committed isolated measurement follows in the evidence checkpoint.
