# Instantiated literal members through alias narrowing

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 4bdedb15: 454,396/478,855 correct assertions (94.89%).
The 99% target requires 474,067 correct assertions.

## Why the existing member boundary was necessary

The dependent-binding port retained a refusal for literals containing methods,
accessors, computed properties, call/construct signatures or indexes. These
members were rendered under generic alias bindings, then later read from their
uninstantiated declaration symbols. Removing the refusal alone leaked T where
native returned number. The first new focused test fails on that baseline:
methodRead returns any instead of readonly [number, number].

Native resolveAnonymousTypeMembers (checker.go:20650) instantiates properties,
call signatures, construct signatures and index infos together, then stores them
on the resulting type. The port now keeps those same semantic member categories
while its alias frame is active. Method overloads share a captured signature set;
accessors carry their resolved read type; indexes retain key/value/readonly
information. Completing a reserved recursive literal moves all these tables to
its final identity, including index infos. Member types therefore outlive the
binding frame without falling back to an uninstantiated T.

Ordinary discriminant flow also needs the instantiated alias body. Previously
only the dependent-binding consumer exposed that union, so a.kind === "a" could
read the discriminator without narrowing the named carrier. narrowType now uses
the existing cached alias-body expansion with cycle detection. If narrowing does
not change the body, it returns the original alias identity. This preserves
written aliases instead of gratuitously expanding their display.

## Verification and boundaries

Native probes cover distinct number/string instantiations, destructured methods,
ordinary discriminants, method overloads, getter/setter pairs, call signatures,
construct signatures, numeric indexes and unique-symbol properties. Follow-up
controls exercise optional methods, generic methods, quoted computed names,
unchanged alias identity and unchecked indexed access. Full-corpus transitions
and a committed isolated checkout determine the delivered checkpoint.

The type-literal builder still refuses member forms it cannot resolve or render;
removing the old blanket alias refusal does not make those forms supported.
Native declaration probes explicitly pass strictness. This work does not claim
complete accessor-write diagnostics or symbol-identity lookup for every computed
expression; it preserves the port's existing lookup representation.

A leaked outer parameter, an overload lost during capture, a changed written
alias without narrowing, a source-hash mismatch or a previously correct corpus
assertion lost falsifies the corresponding claim. Review runs sequentially in
the primary thread under the user's AGENTS tool map; no independent or cross-model
review is claimed. Work is tracked in tsr-6.32.

## Integration findings

The first complete capture candidate gained 22 matches but lost 96 previously
correct assertions. Ninety-four came from the callable-expando printer rewriting
methods as function-valued properties or omitting index declarations. The literal
renderer already preserves those distinctions, so captured callable literals keep
that rendering, including when their reserved identity is completed. The other
two came from expanding a non-union mapped Record before predicate narrowing.
The new flow bridge is for union alias carriers; non-union types retain their
existing narrowing path. With these corrections, the second candidate gains 21
matches with zero RIGHT losses, but exposes 40 GAP-to-WRONG assertions, including
index values that still carry outer type parameters.

A native returned-literal probe then identified a second instantiation boundary:
source<T> returns a literal union containing methods, a call signature and an
index; source(1) must support all three reads as number. Existing instantiation
rebuilt only properties and could not carry these members. The first experiment
retained the source node and bindings, then re-evaluated the literal under a
composed map. The rejection is recorded below; the final implementation substitutes
captured semantic members. Index keys and values also participate in
mentions_type_parameter; otherwise an index-only literal silently skips
substitution. This follows instantiateAnonymousType and its native member
resolution while accommodating the port's eager AST evaluation.

Quoted computed method names exposed a separate lookup/display distinction:
"x y" is the displayed spelling, while x y is the property key. A shared semantic
key helper handles literal names for both methods and properties. Unique-symbol
chains keep the port's existing bracketed lookup representation.

Re-evaluating source syntax under composed binding frames was tested and rejected:
it gained 81 matches but lost 49 previously correct assertions. Cached function
type nodes and mapped-parameter clones retain semantic identities that syntax
resolution cannot recover; correlated unions leaked P instead of K. The replacement
follows native member-wise substitution directly: instantiate captured property
and signature types and index keys/values, caching the result before recursive
members. Source nodes supply only method-versus-property rendering and index
parameter names. They do not supply re-instantiated member types. This preserves
semantic identity across mapped and higher-order inference.

The semantic substitution candidate gains 95 matches with zero RIGHT losses:
55 WRONG-to-RIGHT and 40 GAP-to-RIGHT. Fourteen GAP-to-WRONG, one WRONG-to-GAP
and 28 changed wrong answers remain visible; higher-order name serialization,
recursive presentation and divergent accessor rendering are still incomplete.
A final review probe separates a computed method from a computed function-valued
property and checks a method's shadowed type parameter. It also confirms native
noUncheckedIndexedAccess adds undefined to an index fallback, not to a known
unique-symbol property. The latter fixes an existing consumer error newly exposed
by these captured members. A scratch mutation omitting index capture must fail
the generic member test; it runs under its own Cargo target directory.

The final candidate adds 96 matches, with zero RIGHT losses: 56 WRONG-to-RIGHT,
40 GAP-to-RIGHT, 14 GAP-to-WRONG, one WRONG-to-GAP and 28 changed wrong answers.
All four new native-controlled tests and four dependent-binding tests pass.
The isolated mutation fails on indexed returning T instead of number, as intended.
The release workspace passes; clippy and 3,342 upstream references also pass.
The simplification pass removes unused captured bindings from origin metadata:
only the source NodeId is retained for rendering. Member types remain semantic.
Final checks and committed isolated measurement follow that representation cleanup.

Residuals are tracked in tsr-6.35. The 14 newly reachable wrong answers occur in
objectFromEntries (4), accessorsOverrideProperty8 (3),
declarationEmitHigherOrderRetainedGenerics (2),
genericCallWithOverloadedFunctionTypedArguments (2), genericFunctionParameters
(2), and multiSignatureTypeInference (1). The subscribe member in symbolProperty61
changes from wrong to gap. These are not counted as conversions.

Checker sources plus trace_case SHA-256:
19c77d328971b5c11866d74c4b0ddb74c2b46736f91a1caf57a70810b3e6f776

After metadata simplification, release workspace tests pass 196 result blocks;
clippy with warnings denied, formatting, whitespace and 3,342 anchors pass.

## Committed checkpoint

At fe8f3a22, the isolated checkout's eight focused tests pass and its checker
source hash matches. The full verdict is byte-identical to the final candidate:
454,492 RIGHT, 2,963 GAP and 16,788 WRONG among 474,243 aligned assertions.
Against all 478,855 expected assertions this is 94.91%. The 99% target still
requires 19,575 additional correct assertions. Denominator and pinned oracle are
unchanged; the overall goal remains active.

Evidence:
- /tmp/tsr-99-alias-members-verified.tsv
- /tmp/tsr-99-alias-members-verified-transitions.txt
- /tmp/tsr-99-alias-members-verified-tests.log
- /tmp/tsr-99-alias-members-workspace-final.log
- /tmp/tsr-99-alias-members-clippy.log
- /tmp/tsr-99-alias-members-anchors.log
- /tmp/tsr-99-alias-members-mutation.log
- /tmp/tsr-99-alias-members-sourcehash.txt
- /tmp/compound-engineering-501/ce-code-review/alias-members/review.json

This unit is tracked in tsr-6.32; the residual consumers remain in tsr-6.35.

The checker_types run at fe8f3a22 reports 6,808/9,538 complete cases (71.38%),
five more than the baseline. Its snapshot is refreshed from the isolated checkout.
Coverage output: /tmp/tsr-99-alias-members-coverage.log.

The depend instrument exits 0 but retains stale controls: C1 reports 521
nongapping roots, C2 reports 225 cycles and zero depth-cap hits, C3 balances
3,830 walked gap lines, and C4 still quotes a historical checkpoint. These are
not coverage evidence; tsr-6.29 remains open. Output:
/tmp/tsr-99-alias-members-depend.log.
