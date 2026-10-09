# Calls lane notes

Pinned oracle: typescript-go `5b1047d`. One entry per landed root cause.

## Overload passes use semantic minimum arity (tsr-2zk.16.66)

`chooseOverload` (`checker.go`) filters every candidate with `hasCorrectArity`,
whose minimum is `getMinArgumentCount`: trailing parameters accepting `void`
are optional. The survivor, subtype/assignable and argument-context passes in
`calls.rs` used the syntax-only free `has_correct_arity`, so
`f(value: string, unused: void)` was rejected for `f('x')`. They now call the
existing `overload_has_correct_arity` (`signature_min_argument_count` /
`signature_parameter_count` / effective rest). No new cache or traversal.
Native control: `f('value')` → `"string"`, `f(1)` → `"number"`; TSR before
dropped the first TS2322.

## Dynamic import argument checks (tsr-2zk.16.82)

`import_call.rs` now follows `checkImportCallExpression` (`checker.go:8267`):
check specifier, options and extra arguments (`checkExpressionCached`), then
TS7036, then options against `getGlobalImportCallOptionsTypeChecked()`
(arity 0) unioned with `undefined` (`getNullableType`), then TS2880 on the
first `assert` property. The `checkGrammarImportCallExpression` count/spread
arms (TS1324, TS1450, TS1325) run first, after check.rs's ES2015 arm (TS1323,
which returns). Not ported: the verbatimModuleSyntax arm (checker reads no such
option), the type-argument arm, and the trailing-comma arm (the parser records
no trailing comma on call arguments). No cache or traversal; each argument's
type is the memoised expression type.
Native control: TS2880, TS1325, TS1450, TS1324, TS2559 and TS7036 match
tsgo for esnext/commonjs. Remaining gap: `import(s, { with: 1 })` misses TS2322
because the relater does not answer NotRelated for `{ with: number }` to
`ImportCallOptions | undefined` (plain assignment of a non-fresh source shows
the same miss).

## Literal arguments re-checked under the picked overload (tsr-2zk.16.58)

`isSignatureApplicable` re-checks each argument under the tried candidate
(`checkExpressionWithContextualType`, uncached), and an argument's own type is
a check whose context is the resolved signature
(`getContextualTypeForArgumentAtIndex`). TSR cached the first check, made under
the first arity-matching candidate, so `foo([{ a: true }])` resolved to the
`{ a: boolean }[]` overload but printed `{ a: boolean; }[]` (widened under the
first candidate's `{ a: number }[]`). The subtype and assignable passes now
re-check array/object literal arguments under each candidate they try
(`check_literal_arguments_for_candidate`; `context` is the declaration they
were last checked under), through the existing per-call
`call_inference_signatures` memo, and relate those types; the pick leaves them
checked under the picked candidate. Literals containing a call, `new`, tagged
template, function, method, accessor or class are not re-checked: upstream
caches their nested resolutions from the first check, and `evict_subtree`
would drop TSR's. Converts `functionOverloads39`, `overloadResolutionTest1`.
Native control: `f4(bar: {a: string}[])` / `f4(bar: {a: 1|2}[])` with
`[{a:1}]` now selects the `number` overload, as tsgo does (its TS2322 names
`number`), and the literal prints `{ a: 1; }[]`.

## Union callees resolve through the composite list (tsr-2zk.16.58)

`check_resolve_call_arity` declined every union callee because composite
returns lacked `UnionReductionSubtype` (`getReturnTypeOfSignature`,
`checker.go:20013`). `combine_union_signature_returns` now reduces them with
the existing `union_with_subtype_reduction` (an undecidable reduction keeps the
literal-reduced union), and union callees run the ordinary arity/argument
rules. `non_generic_overload_candidates` orders candidates with the existing
`reorder_candidates` instead of declining composite lists whose signatures come
from different declarations (`(a: string, b?: number)` and `(a: string)` of
two constituents). Still declined: a union callee that could contain type
variables (`head_could_contain_type_variables`), because producers leave such
unions unreduced (`a ?? []` over a generic indexed access keeps `never[]`;
binary.rs/relater, out of lane) and the composite `push` then takes `never`.
No cache or traversal added; the composite list stays in
`composite_signature_types`. Converts `unionTypeCallSignatures`,
`unionTypeCallSignatures4`, `signatureCombiningRestParameters3/4/5`,
`functionCallOnConstrainedTypeVariable`. Native control: `u3('h', 'h')` on
`{(a: string, b?: number): string} | {(a: string): number}` reports TS2345 and
`u3('h', 1)` stays clean; `iterable[Symbol.asyncIterator]().return()` on
`AsyncGenerator | AsyncIterable` stays clean (the return reduces to
`AsyncIterator`). TS2345 message text still prints `'"h"'`/`'number |
undefined'` where tsgo prints `'string'`/`'number'` (relation reporting).

## Tagged templates run resolveCall's arity and argument reports (tsr-2zk.16.58)

`check_tagged_template_diagnostics` stopped at the head. A tag with call
signatures now runs `check_resolve_call_arity` over `getEffectiveCallArguments`'
tagged list (`written_call_arguments`: a synthetic `TemplateStringsArray`
argument located at the template, then the span expressions), reporting
`getArgumentArityError` at the tagged template (excess span from the first
surplus substitution). A sole non-generic candidate and a non-generic overload
set then run the existing argument walks with the synthetic argument first
(`getGlobalTemplateStringsArrayType` via `global_template_strings_array_type`).
Not ported: generic tags' argument errors, and `callIsIncomplete` (a template
without its tail only occurs in files with parse errors, where no call
diagnostic runs). No cache or traversal; the span list is collected per
diagnosed tagged template. Converts
`taggedTemplateStringsWithIncompatibleTypedTags(ES6)`. Native control:
`` foo `${1}${2}${3}` `` → TS2554 at the third substitution, `` foo `${1}${true}` ``
→ TS2769 at `true`, `` foo `${1}${"2"}` `` stays clean, as tsgo.

## `super(...)` calls resolve against the instantiated base constructors (tsr-2zk.16.58)

A `super` callee answered `CallHead::Unknown`, so no arity or argument error
was reported. `check_super_call_diagnostics` follows `resolveCallExpression`'s
super arm: `checkSuperExpression`'s type (the existing `check_super_expression`;
`any`/`errorType` report nothing), then
`getInstantiatedConstructorsForTypeArguments` (`instantiated_constructors_for_type_arguments`:
the type-argument window filter, `getSignatureInstantiation` with written
arguments and instantiated defaults, `unknown` without one), then the shared
candidate arity and argument walks (`check_candidates_arity`, split out of
`check_resolve_call_arity`; error node the `super` keyword). JS files and a
generic surviving candidate report nothing. No cache or traversal; the
instantiation runs once per diagnosed super call. Converts `baseCheck`,
`superCallArgsMustMatch`. Native control: `super(1)` against
`(x: number, y: number)` → TS2554 at `super`, `super("a")` against
`G<number>`'s `(x: T)` → TS2345, `super(true)` against overloads
`(x: number)`/`(x: string, y: number)` → TS2345 (one arity survivor),
`super(0, x)` stays clean.

## getArgumentArityError's missing-argument note (tsr-2zk.16.58)

`getArgumentArityError` (`checker.go:9705`) adds related information to a
too-few-arguments TS2554/TS2555: the closest signature (the first with the
smallest `getMinArgumentCount`) names its declaration parameter at
`len(args)` (plus one for a `this` parameter) as TS6211 (binding pattern),
TS6236 (rest) or TS6210. `report_argument_arity_error` now builds it
(`missing_argument_related`). The syntactic fallbacks in `call_arity.rs`
still emit the head alone.

Related notes need the declaration file's image (`Diagnostic.File()`):
`Checker::related_diagnostic` is `NewDiagnosticForNode` for a note.
- Native: `NewDiagnosticForNode` + `AddRelatedInfo` (5b1047d).
- Key/owner: `diagnostic_files`, `SourceFile` NodeId of this checker's node
  table → `Option<Arc<DiagnosticFile>>`, private to the checker; built from
  `ModuleHost::file_path`/`source_text` (which witnesses the node table).
- Publication: absent = not built; `Some(None)` = host cannot name the file
  (note dropped); `Some(Some)` = completed image. Never certifies a type.
- Work: one text copy + line map per file that receives a note, on report
  paths only.

Span: `error_span` (`GetErrorRangeForNode`). It maps a `MethodSignature` to
its name where native keeps the whole node (scanner.go:2598 lists no
`MethodSignature`), and a constructor keeps its node span where native scans
to the `constructor` keyword: notes at those nodes differ (check.rs, out of
lane). Converts 37 EXACT cases (`functionCall11/12/13/16/17/18`,
`arityErrorRelatedSpanBindingPattern`, `callWithMissingVoid`,
`genericRestArity(Strict)`, …). Native control: `function foo(a:string,
b?:number){}; foo()` → TS2554 at `foo` with TS6210 `a:string`; `foo('x')`
stays clean; `bar(a, b, [c])` with two arguments → TS6211 at `[c]`.

## reportCallResolutionErrors' overload chain and notes (tsr-2zk.16.58)

The `candidatesForArgumentError` arm (`checker.go:9651`) rewrote the last
candidate's report head to TS2769 and dropped the rest. It now chains each
report under `The_last_overload_gave_the_following_error` and
`No_overload_matches_this_call` when several candidates failed (the child
keeps its file and location: `diagnostic_file_image` attaches the reporting
file's image before `NewDiagnosticChain`), notes
`The_last_overload_is_declared_here` at the candidate's declaration, and
ports `addImplementationSuccessElaboration` (`checker.go:9686`): TS2793 at
the implementation when `chooseOverload` accepts its signature. Single
candidates (`report_single_candidate_arguments`) get the TS2793 note too.
The implementation check runs for a non-generic implementation over
arguments whose checked type is independent of context (the refusal of
`check_overload_candidates_arguments`); otherwise no note. No cache: one
relation per argument on report paths. Converts `functionOverloads2/27`,
`excessPropertiesInOverloads`, `specializedSignatureAsCallbackParameter1`,
`genericCallWithOverloadedConstructorTypedArguments`. Native control: `f(true)`
over `(string)`/`(number)` with a `string | number | boolean` implementation
→ TS2769 + TS2771 + TS2793; with a `string | number` implementation, no
TS2793; one overload + `any` implementation → TS2345 + TS2793.
Remaining: the declared-here span at a method signature or constructor
(`error_span`, check.rs), and chains under elaborated relation children
(assignreport).
