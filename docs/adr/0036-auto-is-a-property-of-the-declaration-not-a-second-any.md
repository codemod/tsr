# 0036 — `auto` is a property of the declaration, not a second `any`

**Status:** accepted
**Date:** 2026-08-05
**Scope:** `crates/tsr-checker/src/flow.rs`, `bd tsr-4sc.11`
**Upstream:** `vendor/typescript-go` @ `5b1047d10`

## The forcing constraint

`let x; x = 1; x` prints `number` in a `.types` baseline. The declaration is
`any`; the *reference* is what the assignment reaching it put there. Upstream
calls this the automatic type, and it is the second-largest single arm in this
port's defect table: **4,140 lines, 0.88% of aligned output**, measured at
`f99072c` — lines where this checker prints `any` and upstream prints something
else.

The count is a ceiling, not a forecast. It includes lines upstream types from an
initialiser rather than by evolution, and evolution only pays where an
assignment reaching the reference has a type this checker can compute.

**A number that has been wrong twice, recorded so it stops being wrong.** An
earlier figure of 5,696 summed this population with 1,556 lines of *unannotated
parameter* references. Upstream does not evolve a parameter: `autoType` is
reached only from `getTypeForVariableLikeDeclaration` (`checker.go:16697`) for a
variable declaration with no annotation and no initialiser, and a parameter gets
`any` or a contextual type instead. Those 1,556 lines belong to contextual
typing. Expecting 5,696 from this work would read a correct result as a failure.

## How upstream distinguishes automatic from real `any`

By pointer identity. `c.autoType = c.newIntrinsicTypeEx(TypeFlagsAny, "any",
ObjectFlagsNonInferrableType)` (`checker.go:976`) is a *distinct intrinsic that
also prints `any`*, and `getTypeAtFlowAssignment` asks `f.declaredType ==
c.autoType` (`flow.go:232`). Two types, indistinguishable in output, separated
only by which object they are.

## The decision

**Ask the declaration, not the type.** `Checker::is_auto_typed_declaration`
answers from the symbol's `value_declaration`: a `VariableDeclaration`, name not
a binding pattern, no annotation, no initialiser, not `const`.

### The alternative, taken seriously

Add a second `any` to `crate::intrinsics` and return it from
`get_widened_type_for_variable_like_declaration`, exactly as upstream does. This
is the more faithful port and it is what should happen eventually.

It was not done now because the question is asked in exactly one place — the
assignment arm of the flow walk — while the change would touch `intrinsics.rs`
and `symbols.rs`, both of which other slices were editing concurrently, and it
would put a type that prints `any` into every consumer of `get_type_of_symbol`
without any of them being audited for `== intrinsics.any` comparisons.

**What would have to change for the intrinsic to win:** a second consumer of
automatic-ness. Upstream has several — `checkIdentifier`'s implicit-any
diagnostics (`checker.go:11183`), `getTypeOfPropertyOfType` for automatic
properties (`checker.go:11403`), `getTypeFacts` (`checker.go:31218`). The first
of those to land should carry the intrinsic and delete the predicate.

### The consequence accepted

A caller that hands `get_flow_type_of_reference` a declared type *not* derived
from the symbol's declaration gets the automatic treatment anyway. There is one
caller, `crate::expressions`, and it passes `get_type_of_symbol(symbol)`. That
is a real coupling and it is why the predicate is private to `flow.rs`.

## `noImplicitAny` is assumed on, and that is the risk in this commit

Upstream gates the whole automatic arm on `c.noImplicitAny`
(`checker.go:16697`). With it **off**, `let x; x = 1; x` is `any` at every use
and nothing evolves.

Nothing in `tsr-checker` reads a `CompilerOptions` — there is no options
plumbing in the crate at all — so the flag cannot be consulted. It is assumed
on, matching `strict`.

**How you would know this was wrong:** conformance lines *regressing* from `any`
to a narrowed type in fixtures compiled without `noImplicitAny`. That population
is invisible from inside the checker crate and was not measured. If the coverage
delta from this commit is materially below the arm's measured population, or
negative, this assumption is the first thing to look at. `bd tsr-4sc.11` tracks
plumbing options through.

## What the automatic arm made visible elsewhere

`getTypeAtFlowAssignment` has a second reduction — a *union* declared type keeps
only the constituents the assigned type could be
(`getAssignmentReducedType`, `flow.go:2399`). It was previously an admitted gap
because this port had no assignability; ADR 0035 landed that, so both reductions
ship together.

The union half is not confined to `x = 1` statements. **An initialiser is an
assignment**: the binder records a flow node against the declaration
(`bind_initialized_variable_flow`), so `let x: string | undefined = "a"` reduces
to `string` *at the declaration*. Four narrowing tests written before this
commit asserted `string | undefined` for fixtures of exactly that shape. They
were asserting the absence of the reduction, not the behaviour of the guard they
named; their fixtures dropped the initialiser and one — the test that existed to
fail the day assignability landed — was inverted.

## Deliberately not ported

- **`autoArrayType` and the evolving-array machinery**
  (`ObjectFlagsEvolvingArray`, `getEvolvingArrayType`,
  `addEvolvingArrayElementType`, `finalizeEvolvingArrayType`). A second
  mechanism layered on this one: `let x = [];` gets `autoArrayType` and every
  `x.push(e)` widens the element type through an `ARRAY_MUTATION` flow node the
  walk currently skips. Porting the scalar half alone is safe because the two
  are selected by the *declaration* — an empty-array initialiser is an
  initialiser, so `is_auto_typed_declaration` answers `false` and such a
  variable keeps today's answer rather than a half-evolved one.
- **A `null` or `undefined` initialiser**, which upstream also treats as
  automatic. Answering `false` leaves today's answer.
- **`export`ed and ambient declarations**, which upstream *excludes* and this
  does not, because modifier flags are not reachable from `flow.rs`. This is the
  one exclusion whose absence can produce a wrong answer rather than a stale
  one: an exported `let x;` evolves here where upstream leaves it `any`.
- **Compound assignment** (`x += 1`), and the `for..in` / `for..of` /
  destructuring arms of `getAssignedType`.

## The initial type is `undefined`, and that is not a detail

`checkIdentifier` sets the initial type of an automatic declaration to
`undefinedType`, not to the declared type (`checker.go:11165`). So `let x; x;`
answers **`undefined`**, and `let x; if (c) { x = 1; } x;` answers
`number | undefined` — a union the branch label builds out of the assignment on
one path and the initial type on the other. Nothing special-cases that shape.
A port that set the initial type to the declared `any` would print `any` for
both and look plausible while being wrong on the second.

`convertAutoToAny` (`checker.go:11186`) needs no code here: it maps `autoType`
to `anyType`, and an automatic declaration's declared type in this port already
*is* `anyType`.
