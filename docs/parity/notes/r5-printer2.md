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
