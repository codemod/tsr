# Semantic construction of inline mapped templates

Pinned tsgo:5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline48e1e6d4:450,978/478,855 matching assertions (94.18%).

## The construction boundary

An inline mapped parameter such as `{ [K in keyof T]: (value: T[K]) => void }`
failed before inference: the bounded written-type renderer has no function-type
arm, so getTypeFromTypeNode declined before capturing mapped metadata. Named
mapped aliases could already carry the same semantic template.

The new fallback constructs the constraint and template through the existing
semantic capture path and renders their types, following getTypeFromMappedTypeNode
(checker.go:24255) and createMappedTypeNodeFromType(nodebuilderimpl.go:1458).
The latter preserves the top-level keyof operator from the modifiers source even
when its constraint resolved to a concrete key union. Readonly and optional
modifier tokens remain as written. The mapped key parameter keeps its identity.

Expanding the static written-node renderer was an alternative. That would require
another function-signature renderer and would enlarge annotation reuse throughout
the checker. Reusing the semantic template keeps the change at the mapped-type
construction boundary. Existing written forms and alias handling retain their
previous paths. Key remapping and modifier-preserving wrappers remain unsupported;
failed constraint/template capture still declines.

## Prerequisites exposed by the change

The first corpus run gained74 correct assertions but lost9 previously correct
readonly-array narrowing rows. Better mapped construction made a structural
library-method comparison reach a formerly hidden failure. Native
structuredTypeRelatedTo (relater.go:3841) compares a mutable array with a readonly
array through their numeric index types, without comparing each method. That
missing path is now ported using global array symbol identities. Same-target
arrays still use variance; readonly-to-mutable assignment is unchanged.
The focused narrowing and reverse-mapped cases then pass106/106 assertions.

A pinned two-property reverse-mapping control also exposed reordered properties.
resolveReverseMappedTypeMembers should retain the source order, but this port
enumerated its binder table. When a source has captured literal properties,
reverse mapping now uses that complete ordered list. Other source shapes keep
the previous member enumeration. This is a semantic ordering fix, not a spelling
change to the control's expected result.

## Verification and limits

Checkpoint 996bb885:451,056/478,855 correct assertions (94.19%).
The95% target needs3,857 more matches. Aligned verdicts:
474,243 total;451,056 RIGHT;3,768 GAP;19,419 WRONG.
Against48e1e6d4:47 WRONG→RIGHT,31 GAP→RIGHT,zero RIGHT losses,
9 GAP→WRONG and21 changed wrong answers. Denominator and oracle unchanged.
The nine newly exposed wrong answers are five const return/yield forms,
two single-quote annotation displays and two concrete anonymous mapped displays.
Changed wrong answers still expose symbol qualification, mapped indexing,
isomorphic inference and higher-order prerequisites. These are retained as
limits; corpus baselines and case filters are unchanged.

Pinned strict declarations verify an all-callback source returning unknown,
reverse-mapped callback return values with source property order, concrete mapped
function properties and readonly-array narrowing. The native array guard returns
any[] | undefined; its true-branch local is any[]. The all-callback control's
parameter context remains an unresolved indexed type locally even though its
call result now matches. Anonymous concrete mapped display and more general
mapped instantiation remain in tsr-6.9.

Manual sequential review checked shared capture behavior, semantic template
construction, keyof preservation, global array identity/direction, source order
and unchanged failure paths. No independent agent review was performed. Any
previously correct assertion changing or a pinned control disagreeing with native
rejects the unit even if the aggregate gain is positive.

Evidence:/tmp/tsr-95-inline-array-verdict.{tsv,log},
/tmp/tsr-95-inline-final-transitions.txt,
/tmp/tsr-95-inline-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-inline-array-focus.{tsv,log},
/tmp/tsr-95-oracle-inline-mapped.ts and emitted strict declarations.
Checker sources plus trace_case.rs SHA256: 4513f1c30841fcac8f0e79088b9fbaa11bf1a86f97aacf82b9fe2d0426b48ec7.

Complete cases:6,648/9,538 (69.70%). Release workspace tests,clippy with warnings
denied,all3,366 upstream anchors,format and whitespace checks pass. Checker
snapshot refreshed; binder retains its prior100% result. The earlier progress
message rounded94.19469% incorrectly to94.20%; the corrected rate is94.19%.
