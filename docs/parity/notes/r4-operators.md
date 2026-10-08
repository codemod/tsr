# Lane notes: r4-operators (`tsr-2zk.906`)

Round-4 cloud lane continuing [operators.md](operators.md) (the round-1/2
lane's record, which stays as written). Native source is
`vendor/typescript-go/internal/checker/checker.go` @ `5b1047d`. Baselines
are this box's frozen dumps at `b23dd3d`.

## 1. `!x` reads `getTypeFacts` (`tsr-2zk.16.85`)

**Forcing constraint.** `negated_truthiness_type` (`expressions.rs`) was a
hand-built table: unit literals, nullable, unions of decidable constituents,
primitives → `boolean`, objects/symbols → `false`, and `error` for
everything else — type parameters, indexed accesses, intersections,
`never`, and an `error` operand. Upstream's arm (`checker.go:10887`) is

```go
facts := c.getTypeFacts(operandType, TypeFactsTruthy|TypeFactsFalsy)
switch {
case facts == TypeFactsTruthy: return c.falseType
case facts == TypeFactsFalsy:  return c.trueType
default:                       return c.booleanType
}
```

and `getTypeFacts` (`:30974`) resolves instantiable types through their base
constraint and folds unions/intersections. The flow lane already ports it as
`Checker::get_type_facts` (`flow.rs`), so the arm now asks that function
instead of re-deriving a subset of it. No new state; nothing is cached.

**Consequences accepted.**

- An `error` operand answers `boolean`. Upstream's `errorType` is an
  intrinsic `any`, whose `AnyFacts` carry both bits; the old `error` answer
  was a gap, not a decision. Measured: no line went GAP → WRONG.
- Loose mode is now native for objects: `ObjectFacts` carry `Falsy` without
  `strictNullChecks`, so `!obj` is `boolean` there (it was `false`). The
  unit test `ordinary_and_runtime_this_false_branches_preserve_negation_limitation`
  (`flow_object_facts_tests.rs`) pinned the old `false` with a comment saying
  native returns `boolean`; its assertion now states the native answer per
  mode. That test file is not this lane's; the one-line edit is the
  consequence of this port and is reported to the integrator.
- Where `get_type_facts` itself is imprecise, `!` inherits it. Its
  undecidable default is "every bit", which prints `boolean`.

`checkTruthinessOfType` (TS1345 on a `void` operand) is not part of this
change.

**Measured** (against `b23dd3d`): types +24 lines (20 WRONG → RIGHT,
4 GAP → RIGHT), +5 whole cases in the verdict dump; coverage
`checker_types` 8075 → 8079; diagnostics unchanged (4232); zero losses in
either dump. Perf (median child CPU, 21 samples, new/base): domain-model
0.997, generic-imports 1.017.

**Falsifier.** A corpus line where upstream prints `true`/`false` for `!x`
and TSR prints `boolean` points at `get_type_facts`, not at this arm.
