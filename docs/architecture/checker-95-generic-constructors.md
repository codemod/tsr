# Generic constructor inference and contexts

Baseline3f8bade8:449,493/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

Named construct signatures now use the existing supported overload walk before
filling generic defaults (resolveNewExpression,checker.go:8603). A single
candidate needs no selection; unsupported sets retain their preceding road.
Generic inference receives the new expression's identity,arguments,written type
arguments and contextual return. Empty candidate lists do not enter selection.
The generic written-argument road now recognizes NewExpression and can return
an instantiated signature even when its return is independent of the parameters.
Resolved signatures are retained for later constructor callback context reads.

NewExpression argument context reads the active uninstantiated signature for
const queries and the inference/resolved signature memos for ordinary queries.
The preceding class-annotation road remains the fallback. Const literal carriers
can reach an active constructor inference context;they share the ordinary call's
semantic target walk. Deep inline readonly tuples/objects preserve literals;
existing mutable values keep their source types.

AwaitExpression operands now receive the awaited contextual type and its
PromiseLike form (getContextualTypeForAwaitOperand,checker.go:29750). This supplies
missing return inference to new Promise callbacks inside awaits. The awaited
query preserves parameter identities and existing known-promise/recursion rules;
structural thenables remain incomplete.

## Verification and limits

Checkpoint 0b84e435:449,665/478,855 matching assertions (93.90%).
Complete cases:6,598/9,538 (69.18%). Another5,248 assertions are needed for95%.
Aligned verdicts:474,243 total;449,665 right;4,158 gap;20,420 wrong.
Against3f8bade8:136 WRONG→RIGHT,36 GAP→RIGHT,zero RIGHT losses,
6 GAP→WRONG and35 WRONG→WRONG type changes. Denominator and oracle unchanged.

Four controls compare16 declaration results, two written callback signatures and
three awaited constructor expressions against pinned declaration/corpus behavior.
They cover defaults,inferred and partial written arguments,dependent defaults,
overloaded array/iterable signatures,contextual returns,callback parameters,deep
const literals and await/async return contexts. All four pinned source programs
compile with exit0. Release workspace tests and clippy with warnings denied pass;
3,374 upstream anchors resolve. Checker snapshot and whitespace checks pass.

The initial single-candidate draft ignored written new type arguments and lost6
RIGHT answers. Recognizing NewExpression in the written query restores all6.
The first overload draft panicked on an empty candidate list;the guarded full
run completes. Const and written-callback controls exposed missing constructor
context/memo reads;these are repaired before acceptance.

Six former gaps still produce wrong answers:arrayLiteralInference widens enum
members in Map candidates;inferenceLimit has an unresolved async callback body
return;contextuallyTypeAsyncFunctionReturnType has2 void-context outputs;
jsDeclarationsTypedefFunction has2 JS any-default outputs. These remain deficits
in tsr-6.15/6.21. Broader class constructor selection,unsupported overload sets,
const sources without active constructor contexts and structural thenables remain
incomplete. This unit does not reach95%.

Code review: skipped (ce-code-review unavailable). Independent dispatch conflicts
with the sequential main-thread instruction. Manual review checked empty sets,
selection/inference order,written-argument precedence,default substitution,
context/memo restoration,const source ownership and awaited generic identity.

Evidence:/tmp/tsr-95-generic-constructors-await-verdict.{tsv,log},
/tmp/tsr-95-generic-constructors-accepted-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-generic-constructors-await-controls.log,
/tmp/tsr-95-oracle-generic-constructors{.ts,-out/},
/tmp/tsr-95-oracle-generic-constructors-{extra,written-callback}{.ts,-out/},
/tmp/tsr-95-oracle-await-constructor{.ts,-out/}.
Drafts:/tmp/tsr-95-generic-constructors-{initial,written,overloads,
overloads-guarded,context,const,final}-verdict.{tsv,log}.
Checker sources plus trace_case.rs SHA256:
464565ac94dfc96be3749353957b4a7e32e2b3868e4a147e04f62b1fa8920e00.
