# Assignment declaration types and descriptor prerequisites

Baseline2b92b583:450,304/478,855 matching assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

Binary and call declarations were already attached to property symbols by the
binder, but the variable/property checker worker declined both declaration kinds.
getWidenedTypeForAssignmentDeclaration/getAssignmentDeclarationInitializerType
(checker.go) now collect distinct assigned types, preserve regular CommonJS
literals, ignore an initial undefined export followed by other assignments,
and recover JS all-nullable values to any. getTypeFromPropertyDescriptor prefers
value, then the single getter return, then the single setter's first parameter;
a zero-parameter setter supplies never upstream. Unknown source computations
retain the error gap. Constructor/method this-property flow remains declined.

The value-only draft gained47 net but lost13 correct assertions. Three upstream
prerequisites recover all13. isReadonlyAssignmentDeclaration checks literal false
writable values, missing writable values, and missing setters through the shared
readonly-symbol predicate. getTargetOfModuleDefault gives a synthetic CommonJS
namespace priority over exports.default without an __esModule marker.
The binder's deferred expando lookup now records its nearest IsContainer,
including object literals, through a separate save/restore cursor: the existing
lexical cursor tracks only HasLocals and incorrectly extended an outer class.
Changing that lexical cursor globally would disturb unrelated local binding.

Pinned setter controls also revealed missing JSDoc attachment on object-literal
members. parseObjectLiteralElement now attaches leading docs to every resulting
property/method/accessor/spread, as parser.go does. Existing checker doc readers
then supply the setter's boolean parameter without a descriptor-specific shortcut.
A setter-only type inferred as any would hide this missing prerequisite.

## Verification and limits

Checkpoint 5d47e663:450,417/478,855 assertions (94.06%).
Complete cases:6,616/9,538 (69.36%);4,496 assertions remain for95%.
Aligned verdicts:474,243 total;450,417 RIGHT;4,044 GAP;19,782 WRONG.
Against2b92b583:111 WRONG→RIGHT,2 GAP→RIGHT,zero RIGHT losses,
zero new GAP→WRONG and no changed wrong types. Denominator/oracle unchanged.
Gains include52 checkExportsObjectAssignProperty,13 jsDeclarationsGetterSetter,
11 modulePreserve4 and6 jsdocTemplateTag2. The guarded assignment slice alone
adds71; object-member docs add42 more without adverse transitions.

Pinned controls emit9 declaration types and report exactly2 expected TS2540
readonly writes. Local controls pin descriptor precedence, setter parameters,
CommonJS literal unions/initial undefined, nullable values, readonly writes,
synthetic-default namespace selection and the nested object-literal scope boundary.
The synthetic namespace's missing bar query locally declines to error; general
missing-property recovery to tsgo any remains incomplete and is explicit in the test.
Full this-property flow/inherited precedence, empty-array assignment inference,
object/tuple nullable widening and full callable expando rendering remain outside
this slice. Module-mode-specific synthetic default rules remain broader than the
existing source-file predicate used here.

Release workspace tests,clippy with warnings denied,all3,367 anchors and
whitespace checks pass. Checker and binder snapshots refreshed; binder result 8,497/8,497 (100%).

Code review: skipped (ce-code-review unavailable). Sequential user instructions
prevent independent dispatch. Manual review checked declaration dispatch/circularity,
value/getter/setter precedence,readonly boundaries,literal freshness,initial
undefined ordering,synthetic-default priority and expando cursor save/restore.

Evidence:/tmp/tsr-95-assignment-doc-hosts-verdict.{tsv,log},
/tmp/tsr-95-assignment-final-{tests,anchors,coverage}.log,
/tmp/tsr-95-assignment-final-clippy-pass.log,
/tmp/tsr-95-assignment-declarations-controls-final.log,
/tmp/tsr-95-assignment-controls-final-probe.log,
/tmp/tsr-95-oracle-assignment-controls/{mod.js,validator.ts,out/,../tsr-95-oracle-assignment-controls.log}.
Checker sources plus trace_case.rs SHA256:
5ae0335864264358b7b8e17bad928b11aa34c116f3117e8214b6bcb9f78dde84.
