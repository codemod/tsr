# Lane notes: misc-checks (`tsr-2zk.15`)

Judgment calls made while porting the remaining per-code checker diagnostics.
Upstream anchors are `vendor/typescript-go` @ `5b1047d`.

## §1 TS2344 at call sites: handed off to the calls box (`tsr-2zk.9`)

**Status: not built here.** Commit `5a517c2` added
`check_call_type_argument_constraints` (constraints.rs), a side pass that
re-derived `resolveCall`'s candidate walk to report TS2344 at a call's type
arguments. The integrator declined it and it was reverted: the faithful home
is `chooseOverload`'s `checkTypeArguments(candidate, typeArguments, false)`
(`checker.go:9049`) recording `candidateForTypeArgumentError`, reported by
`reportCallResolutionErrors` (`checker.go:9671`). That is `choose_overload`
in `calls.rs`, whose port of upstream's `CallState` (the candidate-for-error
fields) is the calls box's work. A side pass with its own arity filter and
undecidable declines would be an approximation to delete once that lands.

**Measured, for whoever ports it.** The side pass converted
`genericConstraint1`, `invalidConstraint1`, `primitiveConstraints1`,
`genericConstraint2` and 18 further correct lines (`primitiveConstraints2`,
`nonPrimitiveInGeneric`, `overloadResolution*`, `restTupleElements1`,
`typeArgumentInferenceWithConstraints`) with no wrong lines. Two traps found:
`reportRelationError` (`relater.go:4751`) drops the TS2344 head when the chain
ends in a missing-property message, so `compare<ComparableString>(a, b)` is
TS2741 (`genericConstraint2`); and resolving the callee's type from inside its
own initializer (`declarationsWithRecursiveInternalTypesProduceUniqueTypeParams`)
surfaced two spurious TS7024 lines.

## §2 TS2313: type parameters on a naked constraint cycle

**Forcing constraint.** `check_circular_type_parameter_constraint` reported
only `T extends T`. Upstream reports from `getResolvedBaseConstraint`
(`checker.go:27448`) when `popTypeResolution` fails, which is every type
parameter whose resolution sits on the cycle the stack closes; the result is
cached, so each is reported once. `<U extends T, T extends V, V extends T>`
reports `T` and `V`, not `U` (which only leads into the cycle).

**What was ported.** Each `TypeParameterDeclaration` follows its constraint
while the constraint is a bare reference to another type parameter with a
single declaration, and reports itself iff the walk returns to it. That set
equals upstream's (each cycle member is reported exactly once, whichever is
checked first).

**Declines.** Any other constraint form ends the walk silently: `Array<T>` is
legal, but a union, intersection, indexed-access or conditional constraint
can still close a cycle upstream through `computeBaseConstraint`'s other arms
(`circularBaseTypes`, `recursiveMappedTypes`), which this walk does not
follow: a gap, not a wrong line. The "Circularity originates in type at this
location" related info is not produced (related info is not compared).

## §3 TS2359: invalid `instanceof` right operand

`resolveInstanceofExpression` (`checker.go:8800`). Ported where every arm is
decidable: (a) a primitive constituent (`getSymbolHasInstanceMethodOfObjectType`
only looks when every constituent is non-primitive, `flow.go:2093`; a primitive
has no union signatures and is not a `Function` subtype; `null`/`undefined`
count only under `strictNullChecks`), and (b) a single object type whose
type-shaping declarations (type literal, object literal without spread,
interface or class, no heritage) declare no computed member, no member named
`apply` (required by `Function`), and no signature. Merged value and namespace
declarations (`declare var Object`) are skipped because they do not shape the
declared type. `declared_property_table` was tried for the `apply` test and
rejected: it cannot certify lib `Object`, losing `x instanceof o2`.

**Rejected:** using the relater for `isTypeSubtypeOf(t, Function)`. The
`Subtype` relation shares the assignable structural walk
(`relater.rs` `Relation::Subtype`); the syntactic `apply` test is exact on the
certified domain and cheaper.

## §4 TS6807: enum-member overshift

`checkBinaryLikeExpressionWorker`'s shift arm (`checker.go:12402`) uses
`errorOrSuggestion`; only the enum-member arm is an error, and suggestions are
not baseline diagnostics. The call sits in `check.rs`'s numeric-operator
dispatch arm behind its `operands_ok` (upstream's `leftOk && rightOk`): a first
draft placed it in the generic `BinaryExpression` arm, which shifts never
reach because the earlier guarded arm is exclusive. The shift count uses the
symbol-free `evaluate_constant_expression`; a count naming a constant declines.
`GetTextOfNode(left)` is spelled only for numeric-literal and identifier left
operands (the checker holds no source text); others decline.

## §5 TS17013: `new.target` outside a function

`checkNewTargetMetaProperty` (`checker.go:10768`) with `GetNewTargetContainer`
over `GetThisContainer(node, false, false)` (`ast/utilities.go:1790`),
transcribed including the computed-property-name and decorator skips. Purely
syntactic; no declines.

## §6 TS2637: variance annotation on a non-object type alias

`checkTypeParameterDeferred` (`checker.go:2627`) tests the alias's declared
type for `ObjectFlagsAnonymous|ObjectFlagsMapped`. This port's
`get_declared_type_of_symbol` keeps a generic alias as an unresolved `Named`
reference (`VC<V>`, flags `OBJECT`), so the flags cannot be read. The
decision is made from the body syntax instead, following references to other
aliases with their type arguments substituted (`NumericConstraint<Value>`
resolves to the type parameter `Value`, so it is not anonymous). Conditional,
indexed-access, `typeof` and intersection bodies decline (an intersection with
`{}` can reduce to a single anonymous type). Would be replaced by the
declared-type test once generic alias declared types are resolved
(type-refs box).

## §7 TS2842 is not reported in declaration files

`checkSourceFile` collects renamed binding elements in body-less signatures
but runs `checkUnusedRenamedBindingElements` only
`if !sourceFile.IsDeclarationFile` (`checker.go:2212`). The port reported them
in `.d.ts` files too (`renamingDestructuredPropertyInFunctionType2`, 13 extra
lines). The gate is `file_is_ambient` (set from the declaration-file name) or
the module host's `is_declaration_file`.

## §8 TS2842's reference scan skips declaration names

§805 of `checker-notes-diag2.md` replaced upstream's `referenceKinds == 0`
with a syntactic scan of the containing signature for the renamed identifier.
The scan counted *name positions* as references: in
`([{ a: b }, { b: a }]) => void` the property name `b` of the second element
suppressed the first element's report (`destructuringInFunctionType`). A
binding element's identifier property name and bound name, and a property or
method signature's name, are declarations, so the scan skips them. Computed
property names and nested patterns are still scanned (they can hold real
references). Remaining approximation: any other same-text identifier in the
signature (e.g. a type reference named like the binding) still suppresses.

## §9 TS2777–TS2781: optional-chain reference targets

`checkReferenceExpression` (`checker.go:13130`) has two arms over
`SkipOuterExpressions(expr, OEKAssertions|OEKParentheses)`: a non-reference
reports the caller's "must be a variable" message, an access carrying
`NodeFlagsOptionalChain` the caller's "may not be an optional property access"
message, both at the unskipped `expr`.

**Forcing constraint.** §181 of `docs/architecture/checker-notes-diag2.md` ported only the first arm and
declined whenever a syntactic `?.` walk found a chain, because the parser did
not then set `NodeFlags::OPTIONAL_CHAIN`. It has since §748 of
`docs/architecture/checker-notes-callres.md`. The walk differed
from the flag through parentheses: `(a?.b).c = 1` is a plain access upstream
(the parenthesis ends the chain) but declined here.

**What was ported.** The second arm over the flag, at every caller:
assignment and compound assignment (TS2779) and `++`/`--` (TS2777) in
`check_reference_expression`; `for...in` (TS2780) inside
`check_for_in_variable_type`, because upstream runs it only in the `else` of
the TS2405 type test, so it sits exactly where that test passes; `for...of`
(both arms, TS2487/TS2781) and destructuring-assignment leaf targets
(`checkReferenceAssignment`, `checker.go:12703`: TS2364/TS2779, and
TS2701/TS2778 under an object rest) in `reference_target.rs`. `delete` keeps
no optional arm (`checkDeleteExpression` has none; `delete a?.b` is legal).

**Duplicate avoidance.** Upstream's `checkDestructuringAssignment` first runs
`checkBinaryExpression` on a `target = default`, which checks the same left
side again, and the diagnostic collection drops the repeat. This port's
traversal visits that nested `=` as its own node, so the destructuring walk
skips such targets instead of reporting twice. The rest-element arms mirror
upstream's early returns: a rest that is not last (TS2462) and an array rest
with an initializer (TS1186) are not reference-checked.

**Measured.** `propertyAccessChain.3`, `elementAccessChain.3`, `for-of3`,
`parserPrivateIdentifierInArrayAssignment` converted; partial lines in
`assignmentLHSIsValue` (2×TS2364) and `objectRestNegative` (TS2701); no
line lost.
