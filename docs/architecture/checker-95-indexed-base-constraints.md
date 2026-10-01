# Indexed-access base constraints and contextual narrowing

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 00c67552: 451,183/478,855 correct assertions (94.22%).

## Native rules and representation

getBaseConstraintOfType/getResolvedBaseConstraint/computeBaseConstraint in
checker.go recursively resolve type-parameter, union/intersection, indexed,
keyof, template-literal, string-mapping and conditional-default constraints.
The port retains semantic object/index identities for written generic indexed
accesses and uses those constraints for apparent-type and flow reads. The
cache includes effective alias bindings because the current node evaluator
uses mutable outer mapper frames. In-progress entries stop identity cycles;
the native 50-level final safety bound is retained. Native recursion-identity
heuristics and all distributed conditional constraints are not yet implemented.

substituteIndexedMappedType (checker.go:29291) substitutes the index into a
mapped template. Optionality follows explicit modifiers, recursively combined
generic modifier sources, or accessible optional concrete properties. Legacy
identity-mapped aliases also contribute their modifiers. Literal numeric keys
retain their type when checking whether an optional property is accessible.
Generic expression indexing validates the semantic keyof operand identity or
a concrete key constraint against the receiver's keys; equal printed names
are no longer evidence that two parameter scopes are the same.

getNarrowableTypeForReference (checker.go:31491) substitutes union constraints
before flow analysis in constraint positions or under a context without generic
types. Generic receiver/index pairs remain symbolic. Genericity follows retained
keyof, indexed, non-null, template and alias-body metadata; Array<T> does not
become a top-level generic merely because its element is generic. Active
inference contexts suppress substitution. Full CheckMode propagation and rest
binding context flags remain follow-up work.

## Prerequisites exposed by the measurement

The first flow build gained 347 assertions but lost 22 previously correct rows.
Twenty-one losses came from generic contexts represented as ordinary named
objects; semantic contextual classification restores them. One loss came from
capturing interface this[K], which exposed missing polymorphic-this member
instantiation. Interface this now has its native type-parameter identity and
constraint. resolveTypeReferenceMembers pads the member mapper with the
receiver for its target's this parameter. The direct S[K] to T[J] relation
compares objects and indexes inside the existing recursive pair cache
(relater.go:3443). The inferenceErasedSignatures fixture now passes 47/47.

Preserving printed indexed annotations would hide the missing behavior and
prevent constraint reads. Replacing generic references with their constraints
unconditionally would lose generic return and assignment identities. The
implementation follows the native position/context decision instead.

## Verification and remaining work

Checkpoint 43d36d92:451,556/478,855 correct assertions (94.30%),
6,667/9,538 complete cases (69.90%). The95% goal needs3,357 more matches.
Aligned verdicts:474,243 total;451,556 RIGHT;3,595 GAP;19,092 WRONG.
Relative to00c67552:282 WRONG→RIGHT,91 GAP→RIGHT,zero RIGHT losses,
20 GAP→WRONG,52 changed wrong answers and5 WRONG→GAP.

The20 newly exposed wrong answers are3 contextual-index display rows,
5 correlated-union rows,1 variadic tuple row,4 generic mapped-index rows,
3 readonly-this-index rows and4 generic lookup narrowing rows. The5 new gaps
were already wrong:4 destructuring rows and1 JS-import callback row. All77
non-RIGHT changes are retained in the comparison report; no expectations changed.

A broader attempt to switch every written keyof parameter to an INDEX mint
exposed template/context regressions. Ordinary written parameters retain their
existing operand metadata; polymorphic this uses the semantic index constructor.
Tuple inherited members explicitly pass the tuple as this while using Array's
ordinary element mapper. The single-method tuple control passes; selection among
merged slice overloads remains an existing limitation.

Pinned strict controls cover numeric member access through generic keys,
optional mapped keys, chained parameter constraints, optional generic receivers,
symbolic T[K], invalid keyof U indexing of T, explicit and inherited numeric
optionality, nested generic mapped values, concrete versus generic contexts,
and conditional inference over an interface with a generic this-indexed method.
The two intentionally invalid controls produce native TS2536 and TS2322.

Manual sequential review covers mapper-aware caching, cycle guards, constraint
position exclusions, generic context classification, numeric optional keys,
interface-this substitution and recursive indexed relations. No independent
agent review was performed. A RIGHT loss or a mismatch in a claimed pinned
control rejects the unit. The corpus denominator, pinned oracle and expected
outputs remain unchanged; the checker snapshot only records actual results.

Evidence:/tmp/tsr-95-constraints-final-verdict.{tsv,log},
/tmp/tsr-95-constraints-final-transitions.txt,
/tmp/tsr-95-constraints-{controls,workspace-tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-indexed-constraints.ts and its native declarations/diagnostics,
and /tmp/tsr-95-oracle-tuple-this.ts with native declarations.
Checker sources plus trace_case.rs SHA256:
f6bd81b6ee4e1cf292bb0db6664f19fea465b948aa1348338c4020d5b85faad5.

Release workspace tests,clippy with warnings denied,all3,364 anchors,format and
whitespace checks pass. Checker snapshot refreshed; binder retains its prior
verified100% result. The fresh depend run's C1 fails with584 roots no longer
gapping among4,615 walked lines (297cycles,no depth-cap hits,C3 balanced,C4
stale). It is recorded as an instrument limitation,not used as a reachable score.

The 95% goal remains active. Full conditional constraints, generic key reduction,
recursive mapped types, CheckMode parity and complete indexed relations remain
tracked under tsr-6.28 and tsr-6.9.
