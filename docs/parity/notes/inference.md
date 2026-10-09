# Lane notes: inference (box p2-inference)

Native: `vendor/typescript-go` @ `5b1047d`. Measured against the exact
full-corpus oracle (`docs/parity/notes/oracle.md`) and the legacy dumps.

## 1. Generic contextual signatures lend their type parameters (tsr-2zk.16.70)

`assignContextualParameterTypes` (`checker.go:10350`) gives a context-sensitive
function with no type parameters of its own the contextual signature's type
parameters. `var f: <T, U>(x: T, y: U) => T = (x, y) => x` therefore types the
arrow `<T, U>(x: T, y: U) => T`; TSR's signature builder already adopted them
(`signatures.rs`, signature construction), but the GROUNDED gate in
`get_type_of_function_expression` counted `T`/`U` as out-of-scope type
parameters and answered `error`.

`mentions_type_parameter_out_of_scope` now takes the contextual signature's
own type parameters as in scope. A callee's type parameter reached through a
non-generic contextual signature (`each<T>(cb: (x: T) => void)`) is not
adopted and keeps the gate's existing behaviour.

No new cache or traversal: the gate's walk is unchanged apart from the extra
exemption list (borrowed from the already-computed contextual signature).

Measured: EXACT +6 (`contextualTypingWithGenericSignature`,
`genericFunctionHasFreshTypeArgs`, `genericTypeAssertions3`,
`implicitAnyGenericTypeInference`, `maxConstraints`, `genericContextualTypes1`),
0 lost; legacy types +17 right, 0 lost; diagnostics unchanged.
