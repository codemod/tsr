# Return-mapper literal contexts for generic call arguments

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb. Unit: tsr-6.22
(contextual generic return inference). Baseline 7332f284 (code 2671c87c):
456,480/478,855 matching assertions.

## The missing step

`inferTypeArguments` (checker.go:9447-9458) builds `context.returnMapper` from a
separate return-type inference pass before any argument is checked. Each
argument is then typed by `checkExpressionWithContextualType`, which strips a
literal's freshness when it is a literal of
`instantiateContextualType(paramType, arg, ContextFlagsNone)`. With a return
mapper, that instantiation incorporates only return-type inferences and drops
the `true | false` pair (#48363); without one, the raw parameter type answers
through its base constraint. A regular literal does not widen in
`getCovariantInference`, so `function f(): Wrap<'foo'> { return wrap('foo') }`
infers `Wrap<"foo">`.

The port already computed the return-inference infos (`return_mapper` in
`check_generic_call_worker`) but checked arguments context-free and never
regularized them. `contextual_argument_literal_source` now applies the native
rule to each ordinary positional argument before it is recorded in
`argument_types`, so both the preceding-argument snapshot and the final
per-argument inference see the regular literal. The map follows
`cloneInferredPartOfContext`: only type parameters with return candidates are
mapped; the rest remain themselves and answer through their constraints.

## Outer return mappers are snapshots

The first draft gained 9 and lost one RIGHT row:
`baz(makeFoo(Enum.A), makeFoo(Enum.A))` printed `Func<Enum.A>` for the second
inner call. Native `createOuterReturnMapper` (inference.go:1423) caches the
outer context's mapper on first use. The first inner call snapshots `baz`
before any argument inference, so `U` maps to `unknown` for both inner calls.
The port rebuilt that mapper from live outer inferences on every request, so
the second call saw `U = Enum` and kept `Enum.A`. `InferenceContextSnapshot`
now carries `outer_return_map`, filled on the first non-`NoDefault` request
and reused afterwards. The `NoDefault` clone used for the weak `ReturnType`
inference remains uncached, as in native.

## Alternatives

Regularizing inside the resolver (treating return-mapper literals as a
primitive constraint) would also print `Wrap<"foo">`, but it would apply to
candidates from every source rather than to the argument expression whose
contextual type matched, and would diverge from native when the same literal
reaches the parameter through another argument. Keeping the live outer mapper
and special-casing enum literals was rejected because the measured loss is
exactly the native caching rule.

## Verification

At 7332f284 the full scorepair gate moved +9 (9 WRONG-to-RIGHT: 4
correlatedUnions, 3 inferFromGenericFunctionReturnTypes3, 2
extractInferenceImprovement), zero RIGHT losses, zero GAP-to-WRONG and zero
changed already-WRONG rows. Controls in
`crates/tsr-conformance/tests/contextual_generic_returns.rs` were checked
against pinned tsgo declaration output.

Remaining in this family: contextual return inference through
`instantiateContextualType`'s Signature arm (nonFixingMapper) is unchanged, and
binding-pattern return contexts are still not computed (`isFromBindingPattern`).
