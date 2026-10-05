# Property lane notes (tsr-2zk.4)

Judgment calls made while porting property-access resolution and its
diagnostics toward pinned tsgo (`vendor/typescript-go` @ `5b1047d`). Numbers are
measured with `diagverdictdump` against the frozen baseline at `0d996e8`.

## 1. TS2339 on a flow-narrowed identifier receiver

**Forcing constraint.** `crate::nonexistent_property` reports TS2339 only when
it can certify the miss. One certificate,
`receiver_type_is_the_declared_one`, declined every identifier receiver whose
flow type differed from its symbol's declared type, because when it was drawn
this port's narrowing produced wrong types (`controlFlowInstanceof`,
`narrowByClauseExpressionInSwitchTrue7`, `typePredicateInLoop`). Upstream has
no such decline: `checkPropertyAccessExpressionOrQualifiedName`
(`checker.go`) reports on whatever `checkNonNullExpression` answered.

**Measurement.** With the decline removed outright, the whole corpus gained 37
baseline diagnostics and 7 false ones. All 7 came from the same road:
`getNarrowableTypeForReference` (`checker.go:31491`) substituting the base
constraint of an **indexed-access** or **conditional** declared type —

- `deeplyNestedConstraints`: `Extract<M[K], ArrayLike<any>>` reads as
  `string | number | boolean | number[]`, because `crate::constraints`' conditional
  arm unions both branches instead of `getDefaultConstraintOfConditionalType`'s
  substituted true branch (`M[K] & ArrayLike<any>`);
- `dependentDestructuredVariables` (6 lines): a contextually typed parameter
  whose declared type still carries the uninstantiated `ClientEvents[K]`.

**Decision.** Keep the decline only for that road: trust the flow type when
`narrowable_type_for_reference(declared) == declared` (pure narrowing), or when
every substituted part is a plain type parameter (its base constraint is its
declared constraint). Indexed-access, conditional, substitution and
intersection parts still decline. Result: 37 baseline lines, 0 false, 0
losses; `enumPropertyAccess`, `narrowExceptionVariableInCatchClause`,
`narrowFromAnyWithInstanceof`, `typeGuardsWithAny` convert.

**Rejected alternative.** Removing the decline entirely (+4 cases, 1 loss).
It wins once `crate::constraints` ports the conditional true-branch
substitution and contextual parameters are instantiated before their bodies are
checked; then both residual false reports disappear and this gate can go.

**Falsifier.** A new false TS2339 on an identifier receiver whose declared type
is a plain type parameter or a non-generic type would show the flow walk, not
the constraint road, is wrong.
