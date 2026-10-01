# Indexed-access prerequisites

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 43d36d92: 451,556/478,855 correct assertions (94.30%).

## Native rules and implementation

getTypeAliasInstantiation (checker.go:23641) instantiates the alias body.
A keyword body has no mapper-dependent structure: its instantiation is the
intrinsic type itself. The port now returns that type instead of minting an
OBJECT reference. The `intrinsic` keyword is a marker for compiler-provided
operations such as NoInfer and string mappings, and keeps its existing path.

Optional tuple elements contribute undefined to their numeric index signature.
The existing tuple mask now participates in array_or_tuple_element_access;
union construction retains the current strict-null behavior. Required tuples
and array element types keep their existing handling.

getPropertyTypeForIndexType (checker.go:27126) returns never for a never index
when no index signature applies. Both semantic indexed resolution and expression
indexing now implement that fallback after applicable index signatures. Thus
`{ a: string }[never]` resolves to never on this semantic path, while a string
index signature still supplies its value type. This does not replace the older
concrete annotation evaluator throughout the checker.

## Experiment and remaining prerequisites

Routing every concrete indexed annotation through the semantic resolver gained
72 assertions but lost31 previously correct rows. Twenty-three losses came from
recursive object member construction in normalizedIntersectionTooComplex; three
from recursive indexed alias evaluation in BuildTree; five from primitive alias
bodies exposed by newly resolved arguments. The broad route was removed. These
are shared semantic prerequisites, not grounds for fixture-specific gates.

The first keyword implementation also consumed intrinsic markers and lost49
RIGHT rows around NoInfer. Excluding the marker follows the native dispatch and
restores them. The final change is limited to supported keyword bodies.

Remaining work (tsr-6.30): lazy recursive object identities, indexed alias-body
instantiation, concrete union-key alias retention and mapped-value extraction.
A generic never-constrained key over a string index signature still retains an
indexed type locally where native tsgo resolves number; the native probe records
this existing limitation. No claim of complete indexed-access parity is made.

## Verification and judgment

Checkpoint afdc0a5b: 451,569/478,855 correct assertions (94.30%),
6,667/9,538 complete cases (69.90%).
Aligned verdicts:474,243 total;451,569 RIGHT;3,587 GAP;19,087 WRONG.
The95% threshold requires454,913 matches;3,344 remain.
Relative to43d36d92:5 WRONG→RIGHT,8 GAP→RIGHT,zero RIGHT losses and no other
changed verdicts. Gains: indexingTypesWithNever8,jsFileImportPreservedWhenUsed2,
genericTypeAliases2,noUncheckedIndexedAccess1. Denominator,oracle and expected
outputs are unchanged.

Pinned strict controls verify any/string/never alias instantiation,optional and
required numeric tuple reads,never-key reads and applicable string-index
precedence. NoInfer's parameter identity remains preserved. The native declaration
probe exits successfully. Focused Rust controls pass. Manual sequential review
checked marker dispatch,cache identity,optional-mask lookup and index fallback
ordering; no independent agent review was performed.

Evidence: /tmp/tsr-95-indexed-prerequisites-final-verdict.{tsv,log},
/tmp/tsr-95-indexed-prerequisites-final-transitions.txt,
/tmp/tsr-95-indexed-prerequisites-{controls,workspace-tests,clippy,anchors,coverage,depend}.log,
and /tmp/tsr-95-oracle-indexed-prerequisites.ts with emitted native declarations.
Checker sources plus trace_case.rs SHA256:
273a294f43909db3df4b8cf45e9ec91a6f002a572812f2a9e4df3f79a6e573a4.

Release workspace tests,clippy with warnings denied,all3,364 upstream anchors,
format and whitespace checks pass. Checker snapshot refreshed; binder retains
its prior verified result. Fresh depend walks4,607 lines,C1 fails with584 roots
that no longer gap,C2 reports297 cycles and zero depth hits,C3 balances,C4 still
quotes historical counts. Instrument repair remains tsr-6.29;these counts do not
establish reachable coverage.
