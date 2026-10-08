# Lane notes: r5-declared (`declared.rs` follow-ons)

Round-5 cloud lane on epic `tsr-2zk`. It takes the `declared.rs` follow-ons
r5-typeparams2 finished its lane before starting: `tsr-2zk.1034`
(conditional-type producers, from [`r5-relater4.md`](r5-relater4.md) §2/§3),
`tsr-2zk.1010` (alias references that enumerate as empty,
[`r5-jsx3.md`](r5-jsx3.md) §6), alias naming of single-constituent
intersections ([`r5-intersections.md`](r5-intersections.md) §3) and
`tsr-2zk.979` (qualified alias and enum references,
[`r5-triage2322.md`](r5-triage2322.md) X10). Owns
`crates/tsr-checker/src/declared.rs`, its own tests and this file. Native
anchors are `vendor/typescript-go` @ `5b1047d` (`internal/checker/checker.go`
unless noted).

Frozen base: `77db6b0` (integration head). `diagverdictdump`: RIGHT 5348,
EMPTY_RIGHT 5581, WRONG 1243, EMPTY_WRONG 66. `verdictdump`: RIGHT 543945,
WRONG 7572, GAP 1016.

**Setup note.** The offline bootstrap's `tomlkit` is unreachable (PyPI 403),
as r5-operators3 §4 recorded. A stdlib-only stand-in for `parse`,
`inline_table` and `dumps`, kept in the session scratchpad and not committed,
assembled the vendored tree. The base `diagverdictdump` was OOM-killed once
when a build and a filtered dump ran beside it (the full dump peaks above
8 GB within two minutes on a 15 GB box); every scoring run below ran alone.

## 1. `tsr-2zk.1034` — conditional-type producers

Committed: (a) the CONDITIONAL flags, (c) the inline distributive constraint
and (d) `forConstraint` in both captures. (b) and (e) are not done; §1.4 says
why. Measured on the final code against the frozen base, unfiltered:

| | before | after |
|---|---|---|
| diagnostics RIGHT + EMPTY_RIGHT | 10929 | 10932 (+3 cases) |
| type lines RIGHT | 543945 | 543954 (+9) |
| losses (diag / types) | | 0 / 0 |

Diagnostics gains: `distributiveConditionalTypeConstraints`,
`inlineConditionalHasSimilarAssignability`, `propTypeValidatorInference`
(r5-relater4 §2a's three). Type gains: `distributiveConditionalTypeConstraints`
×7, `conditionalTypes1` ×2. Ir (callgrind, `--singleThreaded --pretty false`):
generic-imports 373,064,328 → 373,066,812 (+0.0007%); domain-model
1,233,634,197 → 1,233,607,085 (−0.002%). Median child CPU vs the base binary:
generic-imports 1.009 (21 samples); domain-model 1.038 at 21 samples, **0.9997**
at 41. `diagnostics_match: true` on both.

### 1.1 (a) Inline deferred conditional mints carry CONDITIONAL

Native's deferred conditional (`getConditionalType`, `:24300`) is a
`TypeFlagsConditional` type wherever it is created. The §906 print-only mint
gave it OBJECT flags outside a mapped template. That is r5-relater4's
[`r5-relater4-inline-conditional-flags.diff`](r5-relater4-inline-conditional-flags.diff),
applied as written. Alone it costs `distributiveConditionalTypeConstraints:0:71`:
`typeof y == 'string'` narrowing of `y: T extends B ? string : number`
(`T extends A`) printed `T extends B ? string : number & string` where native
prints `never`.

**The loss was not in `flow.rs`.** `narrowTypeByTypeFacts` (`flow.go:685`)
keeps a constituent whose facts allow the `typeof`, and `getTypeFacts` of an
instantiable type reads its base constraint. `flow.rs`'s `get_type_facts`
already does that for INSTANTIABLE flags. With OBJECT flags the mint answered
object facts, which deny `typeof == 'string'`: `never`, right by coincidence.
With CONDITIONAL flags it answered the base constraint, and the inline mint had
none, so every fact was allowed. Native's constraint here is the distributive
one, `number` (§1.2), whose facts deny `string`. So (a) is landable only
together with (c), and then it is lossless. No `flow.rs` diff is needed.

### 1.2 (c) The inline distributive constraint

`capture_inline_conditional_constraint` ports
`getConstraintOfDistributiveConditionalType` (`:17286`) for the inline mint and
publishes it into `conditional_constraint_branches`, which
`base_constraint_of_type` (`constraints.rs`) already reads for alias
references. A distributive root (naked type-parameter check) whose check type
has a constraint is evaluated with the check bound to that constraint, one
union constituent at a time, as getConditionalTypeInstantiation distributes
before getConditionalType runs. A `never` result is native's
`noConstraintType`.

- **Ownership/identity.** The key is the mint's `TypeId`. Mints are fresh per
  type node evaluation, private to one `Checker`, and the value is computed
  under the alias frames active when the mint is made, the same frames the
  mint's `conditional_inference_nodes` entry records. It is no new table: the
  existing alias capture already writes the same map.
- **Publication.** Written once, at mint time, as a completed answer. An
  undecided evaluation or relation publishes nothing, which leaves the mint as
  it was before (no constraint).
- **Work boundary.** One `evaluate_conditional_node` per constraint
  constituent, under the existing instantiation-depth guard (declined at 99).
  Inline deferred conditionals are rare on the bench projects (Ir above).
- **Stated divergence: the constraint read.** Native maps the check to
  getConstraintOfType, which for `T extends U` is `U`, and instantiates a
  conditional that is still deferred. This port's evaluator cannot instantiate
  a deferred root under a mapper, so it reads the base constraint, as
  `capture_conditional_alias_branches` already did.

### 1.3 Refused for now: the inline default constraint

getConstraintOfConditionalType falls back to
getDefaultConstraintOfConditionalType (`:17262`), the union of the branches.
Publishing it for inline mints was tried. Filtered on the 11 conditional
cases, against item 1's own binaries:

- with the default branches alone:
  `conditionalTypeAssignabilityWhenDeferred:0:99` RIGHT→WRONG. In
  `const x1: [T] extends [number] ? … = x`, `x` printed `never`.
  getNarrowableTypeForReference substituted `x`'s new union constraint,
  because `context_type_is_generic` (`constraints.rs`) does not count a
  CONDITIONAL type as generic, which native's getGenericObjectFlags does.
- adding that `constraints.rs` arm
  ([`r5-declared-inline-default-constraint.diff`](r5-declared-inline-default-constraint.diff)):
  the same type line still lost, and `distributiveConditionalTypeConstraints`
  went RIGHT→WRONG on diagnostics.

So the diff is −1 type line and −1 case. It is kept as a record, not as a
landable patch. What would change the answer: the narrowing substitution
reading native's `isGenericType` in full (`constraints.rs`, and the
reference's flow type in `flow.rs`), then a re-measure.

### 1.4 Not done: (b) and (e)

- **(b) getConditionalFlowTypeOfType** (`:24952`). Native types a reference
  to the check type inside the true branch as a *substitution type*
  (`getSubstitutionType(T, extends)`). It prints as `T` and relates as
  `T & extends`. This port has no substitution type kind. Adding one changes
  printing and relation everywhere a conditional's true branch mentions its
  check type, so it is an ADR-level change and not a `declared.rs` producer.
  Rewriting `yes` to `T & extends` only inside the constraint capture would
  answer a different question than native's constraint (the default
  constraint, not the branch type), so it was not built either.
- **(e) operands when only the extends type is generic.**
  `conditional_inference_operands` reads the extends operand only for a
  generic check. Its own comment says why: for a concrete check, reading the
  extends type can resolve a recursive `infer` target a second time. Gating
  on roots without `infer` would be a new decline needing its own measurement.
  It is not dispatched in this session; the relater's arms still decline
  (`Unknown`) for such roots, as before.

### 1.5 (d) `forConstraint` in `capture_conditional_alias_branches`

getConditionalType's `forConstraint` arm (`:24383`): when the check type
(the parameter's constraint) is definitely not assignable to the extends type,
the true branch is still added if some constituent of the extends type is
assignable to the check. `for_constraint_includes_true_branch` decides it with
the relater. The permissive instantiations are the types themselves, because a
generic check or extends type has already deferred. Undecided (`Unknown`)
relations publish no constraint. `with_for_constraint_extras` applies it per
distributed constituent to the alias capture's existing evaluator result.
Inline mints use the same helper (§1.2).

- **Stated divergence: `infer` roots.** Native tests the *inferred* extends
  type, which only the evaluator's inference road builds. No extra is added for
  an `infer` root, which is what the port did before. The first draft declined
  the whole capture for `infer` roots instead. That lost three
  `genericConditionalConstrainedToUnknownNotAssignableToConcreteObject` lines,
  because `ReturnType<T[M]>` lost its `unknown` constraint.
- **Stated divergence: chained false branches.** Native's loop applies
  `forConstraint` at each level of a `A ? X : B ? Y : Z` chain. The helper
  tests the outer root only.
- r5-relater4's `for_constraint_extra` (`relater.rs`) adds the same branch on
  the relater's side. With the capture doing it, the relater's union is
  idempotent. Retiring it is r5-relater5's call.
