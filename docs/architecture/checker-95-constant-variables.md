# Constant-variable template evaluation

Baseline25215cc8:449,486/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

Template folding now evaluates AST values through semantic constant-variable
symbols (checker.go:24024). The declaration must be a variable with an
initializer and no written annotation; constant flags reuse isConstantVariable.
Namespace entity names follow export symbols. Recursive initializers use their
own declaration as the evaluation location, with an active-declaration guard.
Same-file forward references and references inside the declaration decline.
Mutable variables, arbitrary object properties and tagged templates decline.
Import alias/enum values, global Infinity/NaN and complete deferred-use ordering
remain outside this constant-variable slice (tsr-6.18).

A shared recursive evaluator supplies an entity callback to the existing literal,
arithmetic and string/template folding code. The symbol-free enum-name consumer
retains its preceding evaluator. A widened string type no longer hides a const
initializer value; a literal type is no longer treated as a constant value.

Removing the old type-based fold initially lost5 constAssertions answers. Their
nested substitutions revealed a pre-existing isConstContext bug:TemplateSpan
must recurse through its template parent (checker.go:13615). That parent rule is
now ported, so semantic template construction preserves those5 answers without
folding mutable literal-typed variables.

## Verification and limits

Checkpoint 3f8bade8:449,493/478,855 matching assertions (93.87%).
Complete cases:6,590/9,538 (69.09%). Another5,420 assertions are needed for95%.
Aligned verdicts:474,243 total;449,493 right;4,200 gap;20,550 wrong.
Against25215cc8:7 WRONG→RIGHT,zero RIGHT losses and no other type changes.
All7 repairs are templateLiteralConstantEvaluation. Denominator and oracle unchanged.

Sixteen pinned declaration outputs cover chained const arithmetic/strings,
namespace values,shadowed names,annotations,mutable/literal-typed variables,
deferred references,forward references,self/dependent cycles,tagged calls and
nested const templates. The ordinary source compiles in pinned tsgo (exit0).
The6 guard outputs come from a diagnostic probe with expected TS2448/TS2454/
TS7022 errors (exit2);its declaration recovery is compared explicitly.
Release workspace tests and clippy with warnings denied pass;3,374 upstream
anchors resolve. Checker snapshot and whitespace checks pass.

Code review: skipped (ce-code-review unavailable). Independent dispatch conflicts
with the sequential main-thread instruction. Manual review checked symbol scope,
constant flags,annotation/order/cycle guards,initializer locations,callback
recursion,pure-evaluator isolation and the template const-context parent rule.

Evidence:/tmp/tsr-95-constant-variables-context-verdict.{tsv,log},
/tmp/tsr-95-constant-variables-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-constant-variables-controls.log,
/tmp/tsr-95-oracle-constant-variables{.ts,-out/},
/tmp/tsr-95-oracle-constant-guards{.ts,-out/}.
Rejected initial draft:/tmp/tsr-95-constant-variables-initial-verdict.tsv.
Checker sources plus trace_case.rs SHA256:
2c44edc3313c4a3af9a92494e6a6dbc6317b9bc680f33ae27b3c228ced3a61c4.
