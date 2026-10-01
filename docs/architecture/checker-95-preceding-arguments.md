# Preceding argument inference and callable alias substitution

Baseline936efea7:449,688/478,855 matching assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

inferTypeArguments checks positional arguments and infers each before checking
its successor (internal/checker/checker.go:9485). createOuterReturnMapper
(inference.go:1423) combines the outer return mapper with a snapshot of preceding
argument candidates. The port previously checked every ordinary argument before
collecting any of those candidates. Nested calls therefore missed earlier inputs.

The ordinary, non-spread positional path now publishes a separate prefix snapshot
after each argument. Final per-argument buckets remain separate to preserve their
candidate order and avoid collecting the same candidate twice. Const source views
use the existing source materialization. Written type arguments retain precedence;
spread/rest and context-sensitive argument sequencing retain their existing roads.
Replacing the whole bucket collector with interleaved checks would also alter
context-sensitive member ordering, whose older experiments lost610 assertions.
This change supplies the missing snapshot without claiming that broader port.

The prefix-only draft gained79 RIGHT with zero RIGHT losses. Its combined callback
control still failed. Traces showed that first wrap had T=string, and its partial
return mapper supplied U=unknown. This was not a missing inference candidate:
Mapper<string,unknown> had lost its callable signature during reference rebuilding.
The source reference carried a callable body, but create_type_reference_with_display
only retained its printed name. contextual_call_signature consequently declined.

instantiateTypeWorker/getObjectTypeInstantiation/instantiateAnonymousType preserve
an object's signature target and mapper (checker.go:22220,22458). The reference
substitution road now instantiates an existing named alias signature table when a
rebuilt reference has none. Its alias spelling and signature kind survive; already
resolved tables remain authoritative. If any signature substitution declines, the
new table stays absent. Broadly resolving every alias body here would introduce
unrelated evaluation and recursion; this uses only an already resolved callable
source. No callback-name or fixture-specific inference rule is added.

## Verification and limits

Checkpoint bde0210e:449,865/478,855 matching assertions
(93.95%);6,598/9,538 complete cases (69.18%). Another5,048 assertions are needed
for95%. Aligned verdicts:474,243 total;449,865 RIGHT;4,151 GAP;20,227 WRONG.
Against936efea7:169 WRONG→RIGHT,8 GAP→RIGHT,zero RIGHT losses,2 WRONG→GAP,
1 GAP→WRONG and22 WRONG→WRONG type changes. Gains include71
inferFromGenericFunctionReturnTypes2,32 inferFromGenericFunctionReturnTypes1,
26 bindingPatternCannotBeOnlyInferenceSource and14 contravariantInferenceAndTypeGuard.
Denominator and pinned oracle are unchanged.

The pinned combine/wrap source compiles with exit0 under explicit strict=true.
The local control checks first callback string→number,second callback number→boolean
and the resulting Mapper<number,boolean>,alongside the earlier nested arrayize
controls. Release workspace tests,clippy with warnings denied,and all3,374 upstream
anchors pass. Checker snapshot and whitespace checks pass.

The single GAP→WRONG is jsFileImportPreservedWhenUsed:the callback now takes any
where the oracle takes never. NonInferrableTypePropagation1/3 each have one
WRONG→GAP.22 changed wrong answers retain incomplete body return types,non-inferrable
propagation and contextual mapper behavior. General rest/spread preceding snapshots,
recursive generic signature relations and binding-pattern-only return contexts
remain in tsr-6.1/6.22. This checkpoint does not reach95%.

Code review: skipped (ce-code-review unavailable). Independent dispatch conflicts
with the sequential main-thread instruction. Manual review checked candidate
nonduplication,priority preservation,written argument precedence,const source
normalization,named alias display,signature kinds and failed substitution behavior.

Evidence:/tmp/tsr-95-contextual-generic-prefix-callable-verdict.{tsv,log},
/tmp/tsr-95-prefix-callable-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-contextual-generic-prefix{.ts,-out/}.
Prefix-only measurement:/tmp/tsr-95-contextual-generic-returns-prefix-verdict.tsv.
Diagnostic traces:/tmp/tsr-95-contextual-generic-prefix-debug{2,4,5}.log.
Temporary probes were removed before final measurement.
Checker sources plus trace_case.rs SHA256:
cae4c9177f743d431d36f8b0c102c252230df05c6bc33e3b179d9a1df51cf129.
