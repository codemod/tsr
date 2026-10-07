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
