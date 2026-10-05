# Parity lane `decls` — declaration, merge and heritage checks

Lane issue `tsr-2zk.3` (epic `tsr-2zk`). Case list:
`docs/parity/lanes/decls.txt`. Box protocol: `docs/parity/box-protocol.md`.
Pinned upstream: `vendor/typescript-go` @ `5b1047d`.

This file records the lane's judgment calls, numbered so code comments can cite
them (`docs/parity/notes/decls.md` §N).

## §1 TS2403: an unannotated, uninitialized `var` is a written `any`

`check_subsequent_declaration_type` (TS2403, `checkVariableLikeDeclaration`'s
secondary-declaration arm, `checker.go:5928`) declines a top-level `any` or
`unknown` operand unless the declaration *wrote* it, because in this port `any`
is often "no better answer" (§338 / §865 of `checker-notes-diag2.md`: admitting
`any` unconditionally measured −34 cases).

**Forcing constraint.** `var y = ""; var y;` is TS2403 upstream
(`duplicateVariablesWithAny`, `varBlock`): the second declaration's type is
`any` because `getTypeForVariableLikeDeclaration` has nothing else to read —
no annotation, no initializer. (Under `noImplicitAny` it answers `autoType`,
which the secondary arm turns back into `any` via `convertAutoToAny`,
`checker.go:5932`.) That `any` is as certain as a written one.

**Decision.** A `VariableDeclaration` with neither annotation nor initializer,
whose list belongs to a `VariableStatement`, counts as written `any`. A
`for (var x in …)` / `for (var x of …)` declaration also has neither but takes
its type from the loop, so it does not qualify.

**Measured.** Removing the trust gate entirely (an experiment, not shipped) at
`0d996e8` gained 3 cases and lost 3 (`variableDeclarationInStrictMode1`,
`constructorParameterProperties`, `objectLiteralGettersAndSetters`). Each loss
was a wrong `any` from another subsystem: an unmerged-merge symbol (fixed here,
§3), private-member access on an instantiated class, and setter-contextual
parameter typing. The narrow rule keeps the gains and none of the losses.

**Falsifier.** A corpus case where an unannotated, uninitialized
variable-statement `var` has a non-`any` type upstream would show up as a new
extra TS2403.

## §2 TS2403: a ported identity relation, structural only between annotations

`crate::identity` ports `isTypeIdenticalTo` (`relater.go:119`) as a
three-valued walk: flags equality and singletons (`isTypeRelatedTo`,
`isRelatedTo` under `identityRelation`), unions and intersections both ways,
same-target type references by type arguments (independent parameters skipped),
then `propertiesIdenticalTo` / `compareProperties`, `signaturesIdenticalTo` /
`compareSignaturesIdentical` for call and construct signatures, and
`indexSignaturesIdenticalTo`. It replaces the earlier "identity fragment",
which approximated the object arm by mutual assignability.

**Why a separate walk and not a `Relation::Identity` in `crate::relater`.**
The identity arm shares none of assignability's relaxations (apparent types,
optionality, excess properties, discriminants, the `any`-index rule); its
structural comparison is symmetric and exhaustive. Threading a fifth relation
through every arm of the relater is more code than the walk and touches a file
another lane owns. What would change this: a second consumer needing identity
inside relation recursion (e.g. `Unmeasurable` variance in
`typeArgumentsRelatedTo`, or `compareTypesIdentical` for overload checks),
where sharing the relater's cache would matter.

**Accepted limitation: structural identity only between written types.** The
first full-identity build at `0d996e8` gained 12 cases and lost 7, every loss a
negative between *inferred* initializer types that another subsystem builds
imprecisely:

| loss | operand built wrong | owner |
|---|---|---|
| `arrayLiteralWidened` | `[null, null]` not widened to `any[]` under `strict: false` | widening |
| `overloadBindingAcrossDeclarationBoundaries{,2}` | overload chosen `Opt3` vs `Opt1` | calls |
| `typeRelationships` | `[this, this.c]` not subtype-reduced to `C[]` | unions |
| `for-inStatements{,Invalid}` | flags-differ arm declined for conditional types (fixed by restoring the flag test) | — |
| `variableDeclarationInStrictMode1` | `var eval` merged into `lib`'s `eval` (§3) | binder |

So `check_subsequent_declaration_type` uses the structural arm only when both
declarations carry a type annotation; otherwise it keeps the fragment's
necessary condition (mutual assignability) via
`is_type_identical_to_by_assignability`. This is a trust rule of the same kind
as §1, not a port of anything upstream: upstream has one relation. **It goes
away** when those three subsystems produce upstream's types; the falsifier is
running the structural arm unconditionally and seeing no losses.

**Also declined:** a flags difference involving an enum (one enum has two
representations here; even object-vs-enum declines because a qualified
`M3.Color` annotation can resolve to an object-flagged type —
`instantiatedModule` measured a loss when it was allowed); distinct type
parameters that share a declaring symbol or were freshly minted by
`instantiate_signature_with_fresh_parameters`; type predicates
(`compareTypePredicatesIdentical` unported); index, template-literal and
string-mapping identity arms.

**Port boundary.** No cache or side table; an assumption stack plays
`maybeKeys` and a flat depth bound (40) declines. The only consumer runs per
secondary `var` declaration.

## §3 TS2403: a merge the excludes forbid leaves the declaration unmerged

Upstream's `mergeSymbol` reports `reportMergeSymbolError` and does **not**
merge (`checker.go:14199`), so `getSymbolOfDeclaration(var eval)` in a strict
script is the file's own symbol, whose only value declaration is itself. This
port's binder (`Binder::merge_globals`, `crates/tsr-binder/src/binder.rs:830`)
inserts `merged[source] = target` *before* testing the excludes, so the source
still resolves to `lib.d.ts`'s `function eval`, and TS2403 compared the `var`
against the function. The checker now treats a symbol recorded in
`merge_conflicts()` as unmerged for this rule.

**Needed outside this lane:** the binder should not record the `merged` edge
for a conflicting pair. That is the root cause; the check-side test is local to
TS2403 and every other consumer of `merged_symbol` still sees the bad edge.
