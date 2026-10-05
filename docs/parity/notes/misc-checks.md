# Lane notes: misc-checks (`tsr-2zk.15`)

Judgment calls made while porting the remaining per-code checker diagnostics.
Upstream anchors are `vendor/typescript-go` @ `5b1047d`.

## §1 TS2344 at call sites: `checkTypeArguments` from `resolveCall`

**Forcing constraint.** TS2344 was reported only for written type references
(`check_type_argument_constraints`, `checkTypeReferenceOrImport`). A call or
`new` with written type arguments (`f<string, { a: number }>()`) reaches
`checkTypeArguments` (`checker.go:9222`) through `chooseOverload`
(`checker.go:9049`) and is reported by `reportCallResolutionErrors`'
`candidateForTypeArgumentError` arm (`checker.go:9671`). Nothing in the port
owned that position; 4 lane cases plus 18 correct lines elsewhere were missing
only that diagnostic.

**What was ported.** `Checker::check_call_type_argument_constraints`
(`crates/tsr-checker/src/constraints.rs`), dispatched from `check.rs` next to
the call-site arity check. It walks the callee's call (or construct)
signatures in order, skips those failing `hasCorrectTypeArgumentArity` or
value arity, and asks each remaining candidate's type arguments. It reports
only when *every* candidate was skipped or definitely failed, re-running the
last failure, because that is the only state in which upstream's diagnostic
does not depend on argument checking: any candidate that passes its type
arguments goes on to `isSignatureApplicable`, and an argument failure
(`candidatesForArgumentError`) outranks the type-argument one.

**Alternatives rejected.**
- *Hook the check into `choose_overload` (`calls.rs`).* That is the faithful
  home, but `calls.rs` is the calls box's file and its overload walk is a set
  of partial roads (§273, §391, §463 there) rather than upstream's single
  loop; the error state is not recorded. Would win once `choose_overload`
  carries upstream's `CallState` (candidate-for-error fields).
- *Report from a single candidate only.* Strictly weaker: overloaded
  constructors whose every generic overload fails (`overloadResolution*`)
  are decided the same way, and measured correct.

**Declines (accepted gaps).** Like the type-reference check: a generic written
argument, an undecidable side (`relation_undecidable_for_constraint`) or a
non-`NotRelated` relation ends the walk silently. Spread arguments, JS files,
`super(...)` calls, `any`/`unknown` callees and abstract construct signatures
are declined. The generic-argument decline runs *before* the callee type is
queried: querying `reduce`'s type from inside its own initializer
(`declarationsWithRecursiveInternalTypesProduceUniqueTypeParams`) surfaced two
spurious TS7024 lines, and a generic argument would have declined anyway.
`getTypeWithThisArgument` on the instantiated constraint is not applied (the
type-reference check omits it too).

**Missing-property head.** `reportRelationError` (`relater.go:4751`) drops the
TS2344 head when the chain ends in the pair's missing-property message, so
`compare<ComparableString>(a, b)` is TS2741 (`genericConstraint2`). The rule
lives in `assignreport.rs`' private `missing_required_property` /
`report_missing_properties`; this lane restates their non-fresh half in
`constraints.rs` rather than editing the relate box's file. Integrator: making
those two `pub(crate)` lets the restatement go.

**Falsifier.** A corpus line where TSR reports TS2344 at a call's type
argument and tsgo reports TS2345 (an argument failure) would mean a candidate
was judged "definitely failed" here that passes upstream.

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
