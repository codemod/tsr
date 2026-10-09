# r5-printer2: type-printer parity (`tsr-2zk`)

Lane `r5-printer2`, the type printer: `printing.rs`, the signature printers
in `signatures.rs`, and `objects.rs`. Pinned vendor `5b1047d`. Frozen base
`6cdb344`: types dump 548,747 RIGHT / 900 GAP / 6,644 WRONG of 556,291.

## 1. Measurement at head

The typetriage tables regenerated at `6cdb344` (`classify.py`/`report.py`
of `r5-typetriage`, run into a scratch directory, not committed). The
lane's targets, as cases solely blocked:

| cause | lines | cases touched | solely blocked |
|---|---:|---:|---:|
| `optional-param-undefined-not-printed` | 74 | 21 | 9 |
| `string-escape-printing` | 35 | 9 | 7 |
| `array-sugar` | 36 | 7 | 4 |
| `signature-type-params-dropped` | 19 | 9 | 3 |
| `enum-member-vs-enum` | 9 | 6 | 3 |
| `parenthesization` | 20 | 8 | 1 |

## 2. An instantiated optional parameter prints `| undefined` (`.16.60`)

**Forcing constraint.** `symbolToParameterDeclaration`
(`nodebuilderimpl.go:1654`) hands `getTypeOfSymbol(parameter)` to
`serializeTypeForDeclaration` (`:2181`). For a `?` parameter that type
carries the `undefined` `getTypeForVariableLikeDeclaration` adds
(`checker.go:16675`, `addOptionalityEx`). The reuse arm keeps the written
annotation when `pseudoTypeEquivalentToType` holds, and its optional flag
(`:2249`) forgives exactly the added `undefined`, so a declaration prints
`(x?: string)`. Once the signature is instantiated the annotation's type
is no longer the symbol's, equivalence fails, and `typeToTypeNode(t)`
prints the symbol's type: lib's `sort` on `IOptions[]` is
`(compareFn?: ((a: IOptions, b: IOptions) => number) | undefined) => …`.

This port's `Parameter` slot holds the annotation's type **without** the
optionality (the reuse arms compare against it), so every printer's last,
serializing arm printed the bare type.

**Port.** `Parameter::question` records whether the symbol's type carries
the optionality, and `Checker::serialized_parameter_type` adds it back in
the serializing arm only:
- it is `isOptionalDeclaration` (a `?` token) **and** a type source that
  adds it: an annotation or initializer (`checker.go:16693`, `:16742`), or
  a function expression / arrow / object-literal method, whose parameters
  `assignParameterType` assigns (`checker.go:10418`, which adds it to any
  type, so `(...arg?) => 102` prints `any[] | undefined`). A declaration's
  unannotated name or binding pattern returns its type without it
  (`:16791`): `function d1([a, b, c]?)` prints `[any, any, any]`
  (`destructuringParameterDeclaration2`).
- the gate is **type equivalence**, not whether this port's site visitor
  managed to print the annotation. Native prints an equivalent annotation
  through `pseudoTypeToNodeWithCheckerFallback`, re-serializing only the
  refused sub-nodes, and never adds `undefined` there. A first cut that
  added it whenever the printer fell through lost
  `stackDepthLimitCastingType:0:2` (`options?: CombinedModelConstructorOptions<E, this>`,
  an equivalent annotation the site visitor refuses).
- JSDoc's bracketed `@param` already puts the optional type in the slot
  (`signatures.rs`, the JS arm); it sets no `question`, and
  `getOptionalType` is idempotent anyway.

Applied in the arrow printer, `signature_to_string` and
`objects::signature_member_text`.

**Measured** (types dump, unfiltered, against the frozen base): +64 lines,
0 lost; cases flipped: `arrayconcat`, `classReferencedInContextualParameterWithinItsOwnBaseExpression`,
`doYouNeedToChangeYourTargetLibraryES2023(target=esnext)`,
`forOfTransformsExpression`, `interfaceAssignmentCompat`,
`propagationOfPromiseInitialization`, `reverseInferenceInContextualInstantiation`.
Diagnostics unchanged. slowcases clean. Median child CPU vs base binary
(21 samples): domain-model 1.002, generic-imports 1.007.

**Diff for main (`checker.rs`)**: `r5-printer2-member-optional-undefined.diff`
applies the same arm to `Checker::signature_member_text_at`, the member
form (`{ <U>(success?: …): … }`). On top of the commit: +34 lines, 0 lost,
+6 cases (`duplicateOverloadInTypeAugmentation1`, `ipromise3`,
`promiseTest`, `genericCallToOverloadedMethodWithOverloadedArguments`,
`genericCallWithOverloadedConstructorTypedArguments`,
`genericCallWithOverloadedConstructorTypedArguments2`).

**Remaining in the cause.** `mappedTypeIndexedAccessConstraint` (an
optional-chain `| undefined`, not a parameter) and parameters built by
union/contextual signature synthesis (`union_signatures.rs`,
`contextual.rs`), whose `Parameter::new` carries no `question`. Native's
synthetic symbols there take `getTypeAtPosition`'s type; unmeasured.

**How this would be wrong.** A `?` parameter whose slot is not the
annotation's own type but still prints the annotation natively would gain
a spurious `| undefined`; the zero-loss run is the check.

## 3. A type literal's property reuses its written annotation

**Forcing constraint.** `addPropertyToElementList` prints a property
signature's type through `serializeTypeForDeclaration`
(`nodebuilderimpl.go:2231`). Its reuse arm re-emits the written annotation
whenever `pseudoTypeEquivalentToType` holds: the annotation's type is the
property's, with the optional flag forgiving the `undefined` an optional
property adds (`:2249`). The re-emitted node goes through the existing-node
visitor, which gives an unannotated parameter `: any` (`nodecopy.go:660`).
So `{ func1: (...rest) => void }` prints `(...rest: any) => void`, while
the signature's own serialization (and `var v1: (...rest) => void`, which
reuses nothing at that level) prints `any[]`.

`type_literal_text_at` (the site printer of a type-literal image) had two
annotation arms: an alias the annotation names, and the producer's baked
text for an unresolved annotation or a written type-literal, array or
union node. Any other annotation, a function type included, was serialized
from the type.

**Port.** `reused_property_annotation_text_at` is the reuse arm: the same
equivalence test, then the written node through the site visitor
(`written_annotation_text_at`, which already ports `nodecopy.go`). It runs
first, as native's single arm does. The two older arms stay behind it for
the nodes the visitor refuses. Removing them was measured: 12 lines lost
(`thisTypeErrors`, `genericTypeReferenceWithoutTypeArgument*`,
`typeofInObjectLiteralType`), whose annotations do not resolve
(`errorType`), which the equivalence gate (correctly) does not cover here.

**Measured** on top of §2: +106 type lines, 0 lost, +18 cases:
`collisionArgumentsInType(alwaysstrict=true)`, `collisionRestParameterInType`,
`declarationEmitBindingPatternsUnused`, `declarationEmitComputedPropertyName1`,
`declarationEmitNoInvalidCommentReuse3`, `jsxFragmentWrongType`, `typeName1`,
`widenedTypes`, `assignmentCompatWithObjectMembers`, `callChain.2`,
`destructuringParameterDeclaration10(strict=false|true)`,
`discriminatedUnionTypes4`, `elementAccessChain.2`,
`emitRestParametersFunctionProperty(target=es2015)`,
`emitRestParametersFunctionPropertyES6`, `generatedContextualTyping`,
`propertyAccessChain.2`. Running the new arm after the older two gave the
identical dump; first is the faithful order. Diagnostics unchanged,
slowcases clean, CPU vs base 1.009 / 0.973.

**How this would be wrong.** An annotation whose type is the property's
but which native refuses to reuse (`pseudoTypeEquivalentToType`'s other
conditions, `RequiresWidening`) would print as written here. None showed
in the dump.

## 4. Small member-printing rules

### 4.1 A type literal's divergent accessor pair prints as accessors

`addPropertyToElementList` (`nodebuilderimpl.go:2524`) prints an accessor
property whose read type differs from `getWriteTypeOfSymbol` as its getter
and setter signatures, each built from the accessor's own declaration by
`signatureToSignatureDeclarationHelper`, so their annotations are reused
(`set foo(v: number | string)` keeps the written order). Either side an
error type keeps the property form. The object-literal and spread printers
already had this arm (`spreads.rs::anonymous_property_members`); the
type-literal site printer did not. `type_literal_accessor_pair_at` adds it
(a type literal has no class parent, so the class-only arms do not apply).
+`divergentAccessors1` (4 lines).

`circularObjectLiteralAccessors` is not this rule: native prints the
accessor form for a `string`/`string` pair there, which needs native's
object-literal property symbols' read/write identity; not investigated.

### 4.2 A merged property name is string-named only if every spelling is

`getPropertyNameNodeForSymbol` (`nodebuilderimpl.go:2426`) classifies a
symbol's name over **every** declaration: string-named only if all are
string literals (`isStringNamed`, `:2405`), single-quoted only if all are
(`:2421`). The object-literal duplicate merge (`objects.rs`) kept the first
spelling instead, so `{ "0": '', 0: '' }` printed `{ "0": string; }` where
native prints `{ 0: string; }`. `merged_written_name` folds the two
spellings pairwise; each printed spelling already encodes its
classification (unquoted, `"`, `'`), so the fold equals the whole-list
rule. +2 lines; the case still fails on `0:7`, the same rule in the
type-literal producer (`declared.rs`, `var a: { "1": number; 1.0: string }`
prints `{ "1": number; }`): routed to r5-declared3.

Measured together on top of §3: +8 lines, 0 lost, +1 case
(`divergentAccessors1`); diagnostics unchanged, slowcases clean.

### 4.3 Diff for declared.rs: an overloaded method is optional if any overload is

`r5-printer2-declared-optional-method.diff`. The binder ORs
`SymbolFlagsOptional` across a merged symbol's declarations and
`addPropertyToElementList` reads the symbol's flag for every overload, so
`{ func4?(x: number): number; func4(s: string): string; }` prints
`func4?` twice. `get_type_from_type_literal` replaced the property on
each overload, keeping the last declaration's `?`. Measured on top of §3:
+1 line, 0 lost, +1 case (`methodSignaturesWithOverloads`).
