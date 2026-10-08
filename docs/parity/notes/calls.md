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
