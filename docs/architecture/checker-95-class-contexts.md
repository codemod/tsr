# Single class constructor inference before defaults

Baseline5bc7cd73:450,038/478,855 matching assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

The class new-expression path filled declared defaults before considering its
constructor. Box<T=string> constructed with1 therefore returned Box<string>.
Callback defaults bypassed body inference,dependent partial arguments declined,
and const inference had no active new-expression context. Named constructor
interfaces already used the shared worker;this was the class-side ordering gap.

resolveNewExpression/inferTypeArguments select and infer before applying defaults
(checker.go). A single declared own class constructor now enters that worker
first. The existing class-parameter reference return is reused;the call carries
the actual NewExpression identity and retains its instantiated signature for
context reads. Written arguments take the shared explicit map. Defaults run after
inference. Unsupported classes and failed inference retain their prior fallback.

Keeping the old default arm first would preserve the demonstrated wrong answers.
Duplicating const/callback inference in the class path would split one upstream
algorithm across two implementations. The earlier single-constructor block is
moved and its call identity restored;no new candidate selection rule is added.
Overloaded/inherited/implicit constructors still require their broader selection
and synthesis paths in tsr-6.21.

## Verification and limits

Checkpoint 594ac488:450,110/478,855 matching assertions (94.00%).
Complete cases:6,608/9,538 (69.28%);4,803 assertions remain for95%.
Aligned verdicts:474,243 total;450,110 RIGHT;4,100 GAP;20,033 WRONG.
Against5bc7cd73:52 WRONG→RIGHT,20 GAP→RIGHT,zero RIGHT losses,no new GAP→WRONG,
and3 WRONG→WRONG changes. Gains include26
inferenceDoesntCompareAgainstUninstantiatedTypeParameter,20 callWithMissingVoid,
10 typeParameterConstModifiers and6 genericDefaultsJs. Denominator/oracle unchanged.

The3 changed wrong answers in inferFromAnnotatedReturn1 still infer unknown for
its callback-bound object return and NoInfer slot. General NoInfer/body return
handling and broader class constructor selection remain unfinished.

Pinned controls compile with exit0 and emit9 expected class instances plus the
written member. Local controls also pin inferred/written callback number→boolean
and deep const object/tuple literal retention. Release workspace tests,clippy with
warnings denied,all3,374 anchors,snapshot and whitespace checks pass.

Code review: skipped (ce-code-review unavailable). Sequential user instructions
prevent independent dispatch. Manual review checked constructor ownership,
parameter identity,explicit-map precedence,default ordering,memo persistence,
const source context and retained fallback behavior.

Evidence:/tmp/tsr-95-class-constructor-contexts-verdict.{tsv,log},
/tmp/tsr-95-class-constructor-contexts-controls.log,
/tmp/tsr-95-class-contexts-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-class-inference{.ts,-out/},
/tmp/tsr-95-class-inference-{before,after}-probe.log.
Checker sources plus trace_case.rs SHA256:
65620b94d6bf312ac03e59bc97468cb3f3e91402f5d3001f4ee44ca4dc47406e.
