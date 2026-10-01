# Empty-array identity and initializer widening

Baseline6837bbf9:450,757/478,855 matching assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

## The identity prerequisite

getAssignmentDeclarationInitializerType (checker.go) recovers inferred empty
arrays to any[] unless the containing function initializer has an annotated
parent declaration. Testing an array's printed never[] or undefined[] would also
change explicitly written arrays, which native isEmptyLiteralType excludes.
The port now creates distinct implicitNeverType and undefinedWideningType
identities and uses them for empty array literals. Annotation types keep the
existing regular never/undefined identities. The helper compares these identities,
not flags or printed text, and hasParentWithTypeAnnotation reads the function
initializer's containing declaration.

The first full run lost179 correct assertions: introducing a widening undefined
identity made non-strict for-of variables bypass the existing scalar widening
branch. Teaching that branch the new identity recovered every loss. The assignment
slice then measured zero corpus gains, while new pinned controls demonstrated
previously unsupported behavior. It is retained as native functionality, not
claimed as coverage progress.

widenTypeInferredFromInitializer (checker.go) is the declaration-side companion:
JavaScript empty literal/array inference yields any/any[] after ordinary literal
widening. Both identifier and destructuring initializer callers now use it.
getWidenedTypeWithContext's array argument rule also widens the inferred
non-strict empty-array element at variable-like declaration boundaries. These
identity-based rules avoid changing explicitly written undefined[] and never[].
The JavaScript slice adds8 matches; the array widening slice adds54 more.
General recursive widening through arbitrary object/union structures and
implicit-any diagnostics remain outside this change.

## Verification and limits

Checkpoint EMPTY_CHECKPOINT: **450,819/478,855 (94.15%)**;
6,639/9,538 complete cases (69.61%). The95% goal needs4,094 more matches.
Aligned verdicts:474,243 total;450,819 RIGHT;3,885 GAP;19,539 WRONG.
Against6837bbf9:62 WRONG→RIGHT,zero RIGHT losses,no new GAP→WRONG,
and one changed wrong type. Denominator and pinned oracle unchanged.
Gains include37 parserRealSource4,8 typeFromJSInitializer4,5 parserRealSource1
and5 functionSubtypingOfVarArgs. trailingCommasInBindingPatterns still answers
undefined[] instead of any[] for a rest binding (previously any); the binding
result does not yet reach the widening rule. tsr-6.26 retains this boundary and
constructor/method this-property flow,nullable object/tuple widening and diagnostics.

Pinned controls pass in strict and non-strict mode with explicit CLI options.
They verify unannotated callable array properties, annotated callable parents,
written never[]/undefined[] preservation, JS variables/class fields/default
parameters and JSDoc annotation preservation. Release workspace tests,clippy with
warnings denied,3,366 upstream anchors and whitespace checks pass; checker snapshot
refreshed. Binder retains its earlier100% result.

Code review: manual sequential review; no independent agent review. Checked
intrinsic identity separation, array target identity, parent annotation ownership,
JS-only initializer recovery, non-strict widening and preserved annotation paths.

Evidence:/tmp/tsr-95-empty-final-verdict.{tsv,log},
/tmp/tsr-95-empty-final-transitions.txt,
/tmp/tsr-95-empty-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-empty-js-controls.log,
/tmp/tsr-95-oracle-empty-assignment.ts and strict/loose declarations,
/tmp/tsr-95-oracle-empty-initializers.js and emitted declarations.
Checker sources plus trace_case.rs SHA256:
45f79e8148d5d68009783f3b7f4a7bc3381c738ada54693a4cdee14c6b72df70.
