# Generic function values in overload-failure inference

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb. Unit:
tsr-6.1. Baseline: cec7cef5, 457,641/478,855 matching assertions.

## The missing check mode

When every overload candidate fails, `inferSignatureInstantiationForOverloadFailure`
(checker.go:9575) makes one recovery instantiation with both
`CheckModeSkipContextSensitive` and `CheckModeSkipGenericFunctions`. The latter
is narrower than “the argument's type is generic.”
`instantiateTypeWithSingleGenericCallSignature` (checker.go:7599) requires one
generic call or construct signature on the source and one non-generic signature
of the same kind on its contextual parameter type. It then returns
`anyFunctionType`, whose `ObjectFlagsNonInferrableType` prevents it from
contributing an inference candidate.

The port already detected the applicability failure and retried in an
`overload_failure` mode, but that mode skipped only syntactically
context-sensitive function expressions. A generic function-valued identifier
was checked and inferred again. In `promisePermutations2`, an argument
`<T>(x: T, cb: (a: T) => T) => IPromise<T>` is inapplicable to a one-parameter
callback, yet recovery inferred `U = IPromise<unknown>` and returned
`Promise<IPromise<unknown>>`; native skips that candidate and defaults `U` to
`unknown`.

## Port shape

`overload_failure_skips_generic_argument` reuses the port's shared
`single_call_or_construct_signature`, which transcribes `getSingleSignature`:
exactly one signature of one kind, none of the opposite kind, and—on the
contextual side—no properties or index signatures. This matters for a value
with both call and construct signatures and for a member-bearing contextual
callable; native skips neither. The contextual type alone is made non-nullable,
matching checker.go:7613. The argument loop still checks the expression, as
native does, but records a no-candidate placeholder and excludes that position
from the later inference walk. The exclusion represents `anyFunctionType`'s
non-inferrable flag; treating the placeholder itself as `undefined` would
incorrectly infer `undefined`.

The guard runs only on the overload-failure retry. A successful generic
function-valued argument therefore retains ordinary inference, and non-generic
callbacks are unchanged. A pinned control distinguishes all three paths plus
the mixed call/construct and member-bearing contextual-callable refusals.

## Verification

Full scorepair against cec7cef5: +16 assertions, all WRONG-to-RIGHT (12
`promisePermutations2`, 4 `promisePermutations3`), zero RIGHT losses, zero
GAP-to-WRONG, and zero changed already-WRONG rows. The first draft skipped the
generic value but allowed its `undefined` placeholder to infer; it gained the
same 16 but lost two RIGHT rows in `functionConstraintSatisfaction2`. Excluding
the position from inference restores native's non-inferrable behavior.

Remaining `promisePermutations2` rows are generic function-expression display
(`<U>(x: U) => U` versus `(x: U) => U`), outside this recovery inference unit.
