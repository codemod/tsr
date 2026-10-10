# r7-reports — implicit-any, JSDoc, JSX and relation reports (`tsr-2zk.1276`)

Round-7 lane notes. The lane owns `relater.rs`, `assignreport.rs`,
`implicit_any.rs`, `jsdoc_*.rs`, `jsx_*.rs`, `js_case_data.rs` and
`assignment_declarations.rs` ([round7.md](../round7.md)). Native source is
`vendor/typescript-go` @ `5b1047d`, `internal/checker/checker.go` unless
noted.

## 0. Base

Frozen base `9020aa67` (origin/main at dispatch), `scripts/parity_gate.sh
freeze`:

- types: 556,357 aligned lines, 551,176 RIGHT / 4,508 WRONG / 673 GAP;
- diagnostics: 12,238 cases, 5,741 RIGHT / 5,610 EMPTY_RIGHT / 852 WRONG /
  35 EMPTY_WRONG.

The r6-triage case lists ([r6-triage-issues.json](r6-triage-issues.json),
cut at `e6eadf4`) were re-measured on this base before porting. Of the 20
`IMPLICIT-ANY-PARAMETER-REPORT` cases, seven were already RIGHT
(`contextualOverloadListFromUnionWithPrimitiveNoImplicitAny`,
`contextualSignatureInArrayElementLibEs5`/`Es2015`, `contextualTyping38`,
`jsxFragmentFactoryNoUnusedLocals`, `parserindenter`,
`typeSatisfaction_contextualTyping2`); the rest split into the root causes
below.

## 1. TS7006 in a declaration file

**Forcing fact.** `implicitAnyInAmbientDeclaration2.d.ts` expects TS7006 at
`declare function foo(x)`, the public `publicFunction(x)` and the public
constructor's `publicConsParam`, and nothing for the `private` members.
`check_implicit_any_parameters` skipped every parameter when
`file_is_ambient`.

**Upstream.** `widenTypeForVariableLikeDeclaration` (`checker.go:18242`)
reaches `reportImplicitAny` for a nil declared type unless
`declarationBelongsToPrivateAmbientMember` (`utilities.go:334`): the root
declaration's member (the parameter's parent) is `isPrivateWithinAmbient`.
There is no declaration-file exemption; a `.d.ts` is checked unless
`skipLibCheck` drops the whole file, which the program already does.

**Port.** The gate is `(ambient || file_is_ambient) && private_within_ambient`.
The TS7031 binding-pattern arm keeps its walk-threaded `!ambient` decline (no
corpus case distinguishes it, and the pattern road reports from a different
upstream site).

**Measured** (both dumps unfiltered against the base): WRONG → RIGHT
`implicitAnyInAmbientDeclaration2.d`; no other row changed.

**Falsifier.** A `.d.ts` case where TSR now reports a TS7006 the baseline
lacks would show a declaration-file arm upstream that this reading missed.

## 2. The retained `return`-position arm stays (routed)

`retained_return_position_report` (implicit-any-widening.md §3) keeps TS7006
extra in four cases: `asyncFunctionContextuallyTypedReturns` (19,10)(28,10),
`contextualTypeOnYield2` (3,13),
`inferPropertyWithContextSensitiveReturnStatement` (10,28) and
`invalidThisEmitInContextualObjectLiteral` (10,22). Removing it converts all
four and loses `subtypeReductionWithAnyFunctionType` (measured, unfiltered).

The loss is a producer answer, not this rule: for `return x => x.length > 0`
inside `useMemo(() => { … })` the port's contextual signature is
`(x: any) => boolean`. The `any` is the context-sensitive function's own
implicit parameter type, fed back through the inference of `T` from the
outer arrow's return expressions; tsgo has no usable signature there and
reports TS7006. Applying `within_generic_call_argument`'s decline to the
too-short source as well did not move it (the signature is present and long
enough). Routed to the contextual/inference owner: the inference of a
generic call's return type from a context-sensitive function's return
expressions must not see that function's own unfixed parameter types. When
it stops, delete the arm (+4 cases).
