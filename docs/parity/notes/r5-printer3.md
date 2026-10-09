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

## 4. A context-sensitive function takes the contextual signature's type parameters (item 4)

**Forcing constraint.** `assignContextualParameterTypes` (`checker.go:10349`)
opens with: if the contextual signature has type parameters and the
function's own signature has none, the function's signature takes them
(`sig.typeParameters = context.typeParameters`). So
`foo((t, u: number) => t.a)` against
`arg: <T extends { a: number }>(t: T, ...rest: A) => number` prints the arrow
as `<T extends { a: number; }>(t: T, u: number) => number`; the port printed
`(t: T, u: number) => number`, a type parameter with no binder in the print.

`get_signature_from_declaration` (`signatures.rs`) already ports that
function's `this` arm (the contextual `this` slot for a context-sensitive
function with no type parameters of its own). The type-parameter arm goes
beside it, under the same gate, and both read one `contextual_signature`
call: computing it twice (a first cut) cost domain-model +0.76% Ir, because
this port rebuilds a declaration's signature on every read rather than
caching it.

r5-printer2 §7 routed this to `contextual.rs`; the contextual signature is
already right there (the parameters print `T`), and only the construction
dropped its type parameters, so no contextual.rs diff is needed.

Measured on top of §3.1, unfiltered: +25 type lines, 0 lost; +4 cases
(`contextuallyTypedGenericAssignment`, `genericCallWithinOwnBodyCastTypeParameterIdentity`,
`importTypeGenericArrowTypeParenthesized`, `promisePermutations2`); lines also
in `contextualOuterTypeParameters`, `genericFunctionParameters` (11),
`promisePermutations{,3}`, `typeTagOnFunctionReferencesGeneric`. Against the
frozen base: 0 lost on both dumps. Diagnostics unchanged; slowcases clean.
Ir: domain-model 1,124,499,997 → 1,124,528,536 (+0.003%), generic-imports
342,949,224 → 342,944,464; CLI output identical.

**What is not ported.** Native's instantiated contextual signature clones
its own type parameters (`instantiateSignature`'s fresh type parameters), so
the function owns `T'`; the port hands over the contextual signature's own
`T`. They print the same; a relation that compares the function against its
contextual type could tell them apart, and none in the corpus moved.

**How this would be wrong.** Native skips the arm when the signature already
has type parameters from an earlier contextual assignment ("already has a
contextual inference performed and cached on it"); the port recomputes the
signature, so a function checked under two contexts takes the second one's.
No line in the corpus shows it.

## 5. `circularObjectLiteralAccessors` is node reuse, not accessor identity (item 3)

Native prints `a`'s declaration as
`{ b: { get foo(): string; set foo(value: string); }; foo: string; }`, and
the object-literal expression lines as `{ foo: string; }`. r5-printer2 §4.1
guessed at the read/write identity of object-literal accessor symbols. It is
not: a scratch tsgo with a print in `addPropertyToElementList`'s accessor arm
(`nodebuilderimpl.go:2522`; built, run, and the vendor file restored) shows
the read and write types identical (`string`, the same type id) for every
`foo`, so that arm prints the property form, as the port does.

The accessor form comes from declaration reuse: `serializeTypeForDeclaration`
reuses the initializer's pseudo type when `pseudoTypeEquivalentToType` holds
(its object-literal arm, `pseudotypenodebuilder.go:418`, counts a get/set
pair as two elements), and `pseudoTypeToNode` re-emits the written accessors
(`:292`). Probes: the form appears only for an object literal **nested** in
the declaration's initializer and only with annotated accessors
(`const m = { x: 1, b: { get foo(): string …, set foo(v: string) … } }`); a
top-level pair (`const k = { get foo(): string …, set … }`) and an unannotated
pair print the property form natively. Why the top level differs was not
established. Routed to r5-nodereuse (`node_reuse.rs`).

## 6. Scanner and parser items, filed with witnesses (item 5; not this lane's files)

`bd` cannot run in these containers; these are for the integrator to file.

### 6.1 Octal escapes in an untagged template cook to their character

Witness `compiler/octalLiteralAndEscapeSequence` (14 type lines at the
base). Native, for untagged templates:

```
>`\5` : "\u0005"        >`\55` : "-"        >`\5${0}` : "\u00050"
```

the port: `"\\5"`, `"\\55"`, `"\\50"`. `scanEscapeSequence`
(`scanner.go:1700-1730`) returns the character (`string(rune(code))`) when
`ReportInvalidEscapeErrors` is set, and the raw text otherwise. The parser
re-scans an untagged template with reporting on (`reScanTemplateToken(false)`,
`parser.go:3692`, `:3728`), so the cooked value is the character. The port's
`scan_escape_into` (`tsr-scanner`) already has both arms (§147 of
`checker-notes-narrow.md`); the value the checker reads is the unreported
one, so either the rescan does not reach these tokens or its value is not
the one stored on the node. Owner: the parser/scanner (main).

### 6.2 Lone surrogates need a string representation that can hold them

Witnesses `conformance/unicodeExtendedEscapesIn{Strings,Templates}1{0,1}(target=es6)`
(4 type lines): `"\u{D800}"` prints `"\uD800"` natively, `"�"` in the port.
A JavaScript string is a sequence of UTF-16 code units and may hold an
unpaired surrogate; native's Go `string` holds it as WTF-8 bytes, and the
printer escapes it (`\uD800`). A Rust `String` must be valid UTF-8, so the
scanner substitutes U+FFFD, which loses the value: two different lone
surrogates become one literal type, and the printer cannot recover the code
unit.

Design options, for a decision record before anyone builds one:

1. **Literal values as WTF-8** (`Vec<u8>` with a WTF-8 view, or a `JsString`
   newtype) from the scanner's token value through the AST's literal text and
   the checker's `StringLiteral` payload. Faithful (it is native's
   representation), but touches every consumer of a literal's text.
2. **Literal values as UTF-16** (`Vec<u16>`): the JS model exactly, but every
   comparison with source text and every print converts.
3. **A side flag**: keep U+FFFD in the `String` and record the original code
   units in a side table keyed by node. Cheap, but type identity stays wrong
   (`"\uD800"` and `"\uDC00"` intern to one type), so it only fixes printing.

Option 1 is the one that matches native; the measured payoff today is 4 type
lines, so it waits on a cause that needs literal identity. Owner: the
scanner (main).

## 7. A required parameter with an initializer prints `| undefined`

**Forcing constraint.** `serializeTypeForDeclaration`
(`nodebuilderimpl.go:2219`) asks the emit resolver whether a parameter
`requiresAddingImplicitUndefined` (`emitresolver.go:601`): a parameter that
is **not** optional (`isOptionalParameter`: no `?`, and a later parameter is
required) but has an initializer, under strictNullChecks, whose annotation
does not already contain `undefined` (`isRequiredInitializedParameter`,
`:625`; `declaredParameterTypeContainsUndefined`, `:605`, an error type
counting as containing it). The printed type then takes `getOptionalType`,
although the symbol's type does not: `function g(x = "J", y: number)` prints
`(x: string | undefined, y: number) => number`; with an annotation the reuse
arm fails against the widened type and prints the annotation's node plus
`| undefined` (`:2263-2276`). The port printed the bare type.

**Port.** `Parameter::implicit_undefined`, set beside `question` in
`get_signature_from_declaration` from the declaration, and
`Checker::implicit_undefined_parameter_type`, which every signature printer
consults before its reuse arms (the arrow printer and `signature_to_string`
in `signatures.rs`, `objects::signature_member_text`). It serializes the
widened type rather than re-emitting the annotation with `| undefined`
appended; the two print alike for an annotation that prints as its type,
and no corpus line reaches the difference.

Not ported: a parameter property (`constructor(public x = 1, y: number)`).
`isRequiredInitializedParameter` answers by the printing site there
(`enclosingDeclaration` function-like, `:629`); the flag stays off for a
property modifier.

**Measured** on top of §4, unfiltered: +7 type lines, 0 lost; +1 case
(`defaultParameterAddsUndefinedWithStrictNullChecks`). Diagnostics
unchanged; slowcases clean.
Ir: domain-model 1,124,528,536 → 1,124,059,226 (−0.04%), generic-imports
342,944,464 → 342,930,624; CLI output identical. The unit test
`a_defaulted_parameter_is_optional_only_from_the_minimum_argument_count`
checks under strictNullChecks and now expects the native
`(x: number | undefined, y: number) => void`.

The same arm in main's member printer (`Checker::signature_member_text_at`,
`checker.rs`) measured **zero** transitions on top, so it is not shipped.

## 8. Diff for declared.rs: a readonly array element is parenthesized

`r5-printer3-readonly-array-element.diff`. `wrap_array_element_text`
(`declared.rs`) wraps the `keyof`, `typeof` and `unique` operator spellings
of an array element, and its comment held that `readonly`, the fourth,
never reaches it. It does once the element is itself a readonly array or
tuple: `ReadonlyArray<readonly [K, V]>` printed `readonly readonly [K, V][]`,
native `readonly (readonly [K, V])[]` (the parenthesizer wraps a
`TypeOperator` element of an array type). Measured on top of §4: +1 type
line, 0 lost, +1 case (`overrideInterfaceProperty`); diagnostics unchanged,
slowcases clean.
