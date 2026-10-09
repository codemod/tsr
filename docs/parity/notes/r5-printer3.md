# r5-printer3: type-printer parity, second pass (`tsr-2zk`)

Lane `r5-printer3`, successor of r5-printer2 (`r5-printer2.md`): `printing.rs`
(except `prints_as_a_single_token`), the printers in `signatures.rs`,
`objects.rs`, `union_signatures.rs` and `literals.rs`. Pinned vendor
`5b1047d`.

## 1. Base and method

Frozen base: integration head `51d8da2` plus r5-printer2's branch (`1f1fdfd`)
and its four diffs (member optional `undefined`, member constraint reuse,
declared optional method, names as written), applied locally because they had
not landed yet. Types dump 549,285 RIGHT / 876 GAP / 6,130 WRONG of 556,291.

**A native `.types` oracle for one file.** The pinned compiler runner reads
local cases from `vendor/typescript-go/testdata/tests/cases/compiler`. Build
the runner once (`go test -modfile=<build-tsgo.sh's tsgo.mod> -c
./internal/testrunner`), copy a probe there, run
`testrunner -test.run 'TestLocal/<probe>.ts'` from `internal/testrunner`, and
read `testdata/baselines/local/compiler/<probe>.types`; then delete both. The
port's side is `examples/probefile`. Every claim below about what native
prints for a probe was read this way.

## 2. Optional parameters of synthetic signatures (item 1): nothing to port

r5-printer2 §2 left the parameters built by union and contextual signature
synthesis (`Parameter::new`, no `question`). Native's synthetic symbols
(`combineUnionOrIntersectionParameters`, `checker.go:21222`) take
`tryGetTypeAtPosition`'s type, which for a `?` parameter is
`getTypeOfSymbol` and already carries the `undefined`; they have no
declaration, so the printer serializes that type as is.

The port reads the same position type: `combine_union_signature`
(`union_signatures.rs`) and `combine_contextual_overload_signatures`
(`contextual.rs`) build each slot from `signature_type_at_position` or from
an `optional` parameter that `signature_type_at_position` widens with
`undefined` on every later read (`signature_parameter_includes_undefined`).
So the synthesized slot already holds `X | undefined` where native's symbol
does, and `question` (which only re-adds what a reused annotation dropped)
is correctly unset. Probes of union-typed and overload-typed contextual
function expressions, arrows and rest parameters print identically to native
(one unrelated difference: native labels a contextual rest tuple from a
combined overload `args_0`, the port `a`; contextual.rs, not measured).

**Measured at the base:** no WRONG line in the corpus prints a synthetic
parameter without the `undefined` native prints. The 14 lines whose native
text has `name?: X | undefined` where the port has `name?: X` are:

- `arrayFlatNoCrashInference{,Declarations}` and the `flat` member inside
  `mappedTypeWithAsClauseAndLateBoundProperty{,2}`'s array expansion (11
  lines; those two cases also differ in member order and symbol-keyed
  members). Native prints `flat`'s `depth?: D | undefined` on `T[]` because
  instantiating `Array<T>`'s members clones the method's own type parameters
  (`instantiateSignature`'s fresh type parameters), so `D` is no longer the
  annotation's `D` and the reuse arm fails. Whether the method is
  instantiated at all depends on `isTypeParameterPossiblyReferenced`:
  `Array`'s `T` has several declarations (the lib files merge), so it is
  "possibly referenced" and every member is instantiated; a single-declaration
  interface `Arr<T>` whose method does not mention `T` is not, and native
  prints the bare `D` there (probe). The port's `instantiate_signature`
  (`inference.rs`) keeps the signature's own type parameters, so the reuse
  arm still matches. Routed: main's `inference.rs` / `members.rs`.
- `checkJsdocParamOnVariableDeclaredFunctionExpression` (3 lines): JSDoc.

**How this would be wrong.** A synthetic slot that bypasses
`signature_type_at_position` (a future builder reading `parameter_type`
directly) would lose the `undefined`; the corpus scan above is the check.

## 3. A single-value enum prints as its enum only in the regular form (item 2)

**Forcing constraint.** `typeToTypeNodeHelper` prints an enum literal whose
symbol is a member as its parent enum's name exactly when
`getDeclaredTypeOfSymbol(parent) == t` (`nodebuilderimpl.go:3266`). That
holds for an enum whose members fold to one value (`enum E { A }`, or
`Flag`'s `A = 1 >> 1, B = 2 >> 2, …`, all `0`): `getDeclaredTypeOfEnum`'s
union of one regular literal is that literal (`checker.go:23899`). The member
symbol's declared type is the **fresh** form (`:23891`), which is not the
enum's type and prints `E.A`. So the same value prints both ways:

- a declaration line, `getTypeOfSymbol`: fresh, `>A : E.A`;
- an expression line, `getTypeOfNode`'s `IsExpressionNode` arm:
  `getRegularTypeOfExpression` (`checker.go:32111`), regular, `>E.A : E`;
- a value that keeps the fresh literal (an object-literal property under a
  contextual type, an inference candidate): `{ type: E.CAT; … }`,
  `(x: E1.X) => E1`.

The port already mints the two forms with their two spellings (`declared.rs`,
§55.1 of `checker-notes-narrow.md`). What it lacked is the place native
decides between them: instead of the baseline writer reading the regular
form, three value roads swap the fresh member for the regular twin
(`enum_access_spelling`: property access in `members.rs`, identifier
reference in `expressions.rs`, a two-segment type reference in
`declared.rs`). That answers the enum on the roads it covers, misses the
others (element access `Flag["A"]`, a `typeof E.A` query), and changes the
value itself, so a fresh member flowing into an object literal or an
inference prints the enum where native prints the member.

### 3.1 Commit: `getRegularTypeOfLiteralType`'s union arm (`literals.rs`)

Native maps a union member-wise (`mapType(t, getRegularTypeOfLiteralType)`,
`checker.go:25277`); the port's function returned a union unchanged. Ported,
with `mapType`'s hand-back of the input when nothing changed (an alias-named
union must not be rebuilt, see `get_widened_unique_es_symbol_type`). Native
caches the answer on the union (`regularType`); the port has no slot on the
type, so a union is answered by a scan for a fresh or enum-twin member and
rebuilt only when one is found. The arm is outlined (`#[inline(never)]`):
inlined, the function's own cost doubled on domain-model.

Measured against the frozen base, unfiltered: +3 type lines, 0 lost
(`constAssertions:0:237` `0 | 1`, `literalTypeWidening:0:144` `"a" | 10`,
`templateLiteralTypes2:0:143`); +1 case (`constAssertions`). Diagnostics
unchanged; slowcases clean on both dumps. Ir (callgrind, `--singleThreaded`):
domain-model 1,123,808,581 → 1,124,499,997 (+0.06%), generic-imports
342,943,617 → 342,949,224 (+0.002%); CLI output identical.

### 3.2 Diff: the baseline writer reads the regular form

`r5-printer3-regular-expression-type.diff` (harness `types_producer.rs`,
main's `members.rs` and `expressions.rs`; none of them this lane's):

- `type_id_at_location_arm` answers `get_regular_type_of_literal_type` for
  the arms that stand for `IsExpressionNode` (`Expression`,
  `PropertyAccessName`, `TypeQueryName`, `QualifiedNameLeft`), as
  `getRegularTypeOfExpression` does;
- the property-access and identifier swaps are removed: those roads return
  the fresh member, as native's `checkPropertyAccessExpression` and
  `checkIdentifier` do. The `declared.rs` swap is left: a type reference
  reads `get_regular_type_of_literal_type` first, which already answers the
  enum-spelled twin, so it is inert.

It needs §3.1: without the union arm, `true ? E.A : x` (a union holding the
fresh member) printed `E.A | T` (2 lines lost in `subtypesOfTypeParameter`,
measured).

Measured on top of §3.1, unfiltered: +11 type lines, 0 lost; +4 cases
(`isolatedDeclarationErrorsEnums`, `noImplicitAnyIndexing`,
`typeArgumentInferenceWithObjectLiteral`, `typeofANonExportedType`);
`typeofAnExportedType` gains its 2 lines. Diagnostics unchanged; slowcases
clean. Ir: domain-model 1,124,489,803 (−0.001% against §3.1),
generic-imports 342,947,299.

**Remaining in the cause.** `arrayLiteralInference` (3 lines, native
`[AppType.Standard, …]`, the port `[AppType, …]`) and
`mappedTypeOverlappingStringEnumKeys` (5 lines) are multi-member enums whose
member literal the port widens or replaces elsewhere; not investigated.

**How this would be wrong.** A port arm that is not `IsExpressionNode`
natively but is listed above would print a regular form where native prints
`getTypeOfSymbol`'s fresh one; for every type but a single-value enum's
member the two print the same text, so only an enum line could show it, and
none did.
