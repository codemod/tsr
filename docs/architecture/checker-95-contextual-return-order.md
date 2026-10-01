# Contextual return inference order

Baseline0b84e435:449,665/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

inferTypeArguments now collects contextual return inferences before checking
argument expressions (checker.go:9390). Both weak ReturnType candidates and the
independent returnMapper are stored in the active inference snapshot first.
Nested generic argument calls can therefore instantiate their outer variables
from the written return context. Written type arguments skip this inference.
The existing candidate collector and mapper semantics are retained.

The trace of arrayize(wrap(value => value.length)) under Mapper<string,number[]>
previously entered wrap with Mapper<never,never>;only afterwards did arrayize
collect string/number return candidates. The reordered snapshot supplies those
candidates before entering wrap. This corrects the sequencing rather than
adding an alias-specific inference rule. Parameter identities are resolved once
and reused by the existing worker.

## Verification and limits

Checkpoint 936efea7:449,688/478,855 matching assertions (93.91%).
Complete cases:6,598/9,538 (69.18%).
Another5,225 assertions are needed for95%.
Aligned verdicts:474,243 total;449,688 right;4,158 gap;20,397 wrong.
Against0b84e435:23 WRONG→RIGHT,zero RIGHT losses,no GAP transitions,
and8 WRONG→WRONG type changes. Gains:9 inferFromGenericFunctionReturnTypes1,
14 inferFromGenericFunctionReturnTypes2. Denominator and oracle unchanged.

The pinned source compiles with exit0 and emits three annotated Mapper results.
The local control pins callback parameters/returns at zero,one and two levels
of nested arrayize calls. Release workspace tests and clippy with warnings denied
pass;3,374 upstream anchors resolve. Snapshot and whitespace checks pass.

Eight already-wrong answers still expose incomplete preceding-argument inference
and callback body returns. Three SetOf transforms now receive number input but
still return any instead of string;one nested Mapper still takes unknown instead
of number;two callback parameter reads remain any;two Mapper<string,number[]>
results retain any[] returns. General non-context argument inference ordering,
recursive generic signature relations and contextual candidate handling remain
in tsr-6.1/6.22. This checkpoint does not reach95%.

Code review: skipped (ce-code-review unavailable). Independent dispatch conflicts
with the sequential main-thread instruction. Manual review checked snapshot
publication before argument reads,written argument precedence,parameter identity
reuse,priority preservation,mapper save/restore and diagnostic early paths.

Evidence:/tmp/tsr-95-contextual-generic-returns-order-verdict.{tsv,log},
/tmp/tsr-95-contextual-generic-returns-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-contextual-generic-returns-order-controls.log,
/tmp/tsr-95-oracle-contextual-generic-returns{.ts,-out/}.
Diagnostic traces:/tmp/tsr-95-contextual-generic-returns-{debug,nested-debug}.log.
Temporary debug probes were removed before the full measurement.
Checker sources plus trace_case.rs SHA256:
c74c13ecc554c9b20cf810e57b04ccc9409573d42f10b154ccc8a7fda3fc730d.
