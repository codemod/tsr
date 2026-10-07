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
