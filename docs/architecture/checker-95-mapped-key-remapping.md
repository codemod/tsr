# Mapped key remapping and conditional templates

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline95f55180:451,117/478,855 matching assertions (94.21%).

## Native behavior and implementation

Mapped types with an `as` clause previously lost their captured semantic
metadata. resolveMappedTypeMembers (checker.go:20894) instantiates the name type
for each source key, visits every constituent of a resulting name union, and
merges source keys that produce the same property name. The template is then
instantiated with that combined key union. Modifier flags come from the first
source property. Index signatures differ: each source key instantiates its own
value, then appendIndexInfo unions the values. A nondistributive conditional
control distinguishes these two rules.

The port now captures and substitutes the name type alongside its constraint
and template. The member enumerator retains both the combined key and first key;
literal names render as identifiers, numeric names or quoted strings. Arbitrary
source key domains, such as unions of objects with name/type properties, feed
the same mapper. Empty names produced by never are skipped. Unresolved generic
name types still decline member materialization.

getIndexTypeForMappedType (checker.go:26871) returns the constraint directly for
an unremapped type and otherwise maps each property key. Concrete string index
results include number in keyof. Written keyof now reaches this semantic path
for mapped operands. Generic homomorphic remapping defers its key calculation.
The existing sequence helper enumerates tuple/array properties when remapping:
an as clause disables tuple/array transformation, tuple contextual inference
(checker.go:8030), and reverse inference (inference.go:708). Primitive passthrough
and union distribution remain homomorphic behavior.

The alternative of retaining only printed `as` clauses cannot support indexed
reads, filtering or key collisions. Reusing semantic instantiation and the
existing member machinery avoids a second textual substitution implementation.

## Conditional prerequisite and contextual ordering

Conditional mapped templates previously retained only their true/false branches
for reverse inference. Key filters need to test the condition after substitution.
Captured conditional roots now retain the declaration, four semantic operands
and outer symbol bindings. instantiateType composes bindings and invokes the
existing conditional evaluator; an unresolved result captures a new deferred
image. Containment checks follow semantic operands and the type has CONDITIONAL
flags. The existing instantiation depth/count limits remain authoritative.
This extends the current alias evaluator; it does not claim full native lazy
conditional types, cache behavior or recursion support.

Filtering versus remapping uses conditional default constraints, including
native any-branch elision (checker.go:17263). The conditional/intersection
isExcludedMappedPropertyName rule is also ported (checker.go:30624).

The first conditional/context run gained64 net assertions but lost2 RIGHT
parameter rows. A property removed by `T[K] extends string ? K : never` still
received the raw generic callback context. As clauses disable reverse inference;
their argument contexts must consume candidates from preceding data arguments.
The port's existing partial mapper now instantiates these contexts while keeping
the prior unremapped reverse-inference path. Both lost rows recover. Pinned tsgo
reports TS2353 and TS7006 for that invalid property, and its callback parameters
are any. The remaining three wrong rows in that case are callback/object display
recovery, not the parameter contexts (47/50 correct).

## Verification and limits

Checkpoint 00c67552:451,183/478,855 correct assertions (94.22%).
The95% target needs3,730 more matches. Aligned verdicts:474,243 total;
451,183 RIGHT;3,701 GAP;19,359 WRONG. Against95f55180:59 WRONG→RIGHT,
7 GAP→RIGHT,zero RIGHT losses,11 GAP→WRONG and34 changed wrong answers.
Denominator,oracle,corpus expectations and case filters are unchanged.

Pinned strict controls verify getters, multiple names, property and index
collisions, inherited optional/readonly modifiers, object-valued key domains,
quoted names, primitive passthrough, a remapped tuple's length, remapped keyof,
never-domain constant remapping, Exclude filters, an additional outer type
parameter, conditional value templates and accepted/rejected callback keys.
The tuple control checks the length read, not full tuple-to-object rendering.
Unique-symbol members, readonly index metadata, deep readonly propagation,
recursive remapping and full generic key reduction remain incomplete.

The11 newly exposed wrong answers are4 correlated union rows,4 late-bound
property rows,2 isomorphic inference rows and1 recursive remap row. The34
changed wrong answers are13 thisless callback rows,8 generic remapped-key rows,
4 nested destructuring rows,3 late-bound property rows,2 nested excess-property
rows,2 indexed-access constraint rows and2 recursive mapped rows. These remain
tsr-6.9 work rather than being hidden by expected-output changes.

Manual sequential review checked mapper composition, conditional containment,
property/index collision differences, first-key modifier selection, tuple and
reverse-inference gates, contextual invalid-property recovery and failure paths.
No independent agent review was performed. Any RIGHT loss or disagreement in a
claimed pinned control rejects the unit.

Evidence:/tmp/tsr-95-remap-context-verdict.{tsv,log},
/tmp/tsr-95-remap-final-transitions.txt,
/tmp/tsr-95-remap-context-focus.{tsv,log},
/tmp/tsr-95-remap-{controls,workspace-tests,clippy,anchors,coverage}.log,
and /tmp/tsr-95-oracle-key-remap.ts with strict declarations and expected
invalid-property diagnostics. Checker sources plus trace_case.rs SHA256:
b8220dbd3621f83b8067ec6a2f4af3ee9ae97cf93701f67627e203967cc7dc87.

Complete cases:6,656/9,538 (69.78%). Release workspace tests,
clippy with warnings denied,all3,366 anchors,format and whitespace checks pass.
Checker snapshot refreshed; binder retains its prior verified100% result.
The95% goal remains active.
