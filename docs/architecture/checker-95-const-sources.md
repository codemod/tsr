# Const literal inference source views

Baseline79500ed4:448,922/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

Const candidates now acquire their readonly view before inference, following the
literal AST origin. Direct const-variable and non-array rest candidates bypass the
blanket readonly image at the final mapper. Existing object/tuple variables retain
their mutability. Inline object and array children transform recursively; a
mutable array contextual constraint keeps its tuple mutable. Wrapped const
parameters can transform a child without making the enclosing property readonly.
Own expression-cache views remain unchanged.

The mutability predicate follows isMutableArrayLikeType (checker.go:23526),
including parameter/indexed/tuple constraints and derived Array bases. Heritage
arguments do not affect the proven relation to Array<any>. Other structural types
use the existing relation. Literal objects retain semantic property types in the
candidate view. Object spreads now capture semantic property metadata alongside
the existing displayed members; ordinary own-node metadata stays uninstantiated.
Fixed tuple spreads expand their existing elements. Copied variable children
remain mutable; literal children receive their own contextual transformation.
Last-write object origins distinguish a later spread/shorthand value from an
earlier inline literal. The code never derives semantic types from printed text.

getSpreadArgumentType (checker.go:29500–29568) now selects const/primitive literal
retention and tuple mutability while building non-array rest candidates. Tuple
rests use contextual element positions; other rests use indexed access positions.
Primitive retention uses maybeTypeOfKind, including union constituents. The
uninstantiated const identity walk is shared with the preceding checkpoint.

## Verification and limits

Checkpoint a973e8e8:448,945/478,855 correct assertions (93.75%).
Complete cases:6,578/9,538 (68.97%). Another5,968 assertions are needed for95%.
Aligned verdicts:474,243 total;448,945 right;4,405 gap;20,893 wrong.
Against79500ed4:23 WRONG→RIGHT,zero RIGHT losses and no other transitions.
The fixed denominator and pinned oracle are unchanged.

The rest draft lost one non-const variadic assertion by using indexed contexts for
a tuple rest; contextual element lookup repairs that. The direct-source draft lost
24 fixed-spread readonly assertions; AST spread expansion repairs all24. Manual
review also identified last-write origin and inherited-array mutability errors;
positive controls now cover both. Twenty-four pinned controls cover deep literals,
mixed slots, wrapper contexts, mutable constraints, existing variables/tuples,
readonly and mutable rests, variadic primitives, fixed spreads, parentheses,
last-write origins and derived arrays. Both pinned declaration compilations exit0.
The deliberate last-write override suppresses TS2783 in its control source; its
semantic declaration still comes from the pinned checker.

Release workspace tests and clippy with warnings denied pass;3,378 upstream
anchors resolve. The checker snapshot and whitespace checks are refreshed.

Callback/context-sensitive sources and other indirect candidates retain the
legacy readonly fallback. Composite expressions, constructors/no-call contexts,
variable/optional literal spreads, complete indexed/conditional base constraints,
computed-name child origins and readonly propagation across arbitrary referenced
containers remain incomplete (tsr-6.15 and related strategic issues). This is a
source-view continuation, not a completed const-inference subsystem.

Code review: skipped (ce-code-review unavailable). Independent review dispatch
conflicts with the sequential main-thread instruction. Manual review checked
source AST origins, cached own-node preservation, mutable constraints, candidate
collection before constraint fallback, parameter identity, tuple-rest context
selection, fixed spreads, semantic property capture and last-write replacement.
This is not independent review.

Evidence:/tmp/tsr-95-const-sources-derived-verdict.{tsv,log},
/tmp/tsr-95-const-sources-accepted-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-const-{sources,rest}{.ts,-out/}.
Checker sources plus trace_case.rs SHA256:
73e7644191bfdae65217ddd010a53e5797055f8f1fe52da81b45254c9dc60440.
The95% goal is not complete.
