# r5-typetriage: checker_types WRONG lines by root cause

Lane `r5-typetriage` (epic `tsr-2zk`), measured at integration head
`ccb48e7` against the pinned vendor `5b1047d`. The integrator's clusters are
built from diagnostic codes, and type-print failures have no code. This note
groups every non-RIGHT `.types` line by **why** it differs, and ranks the
groups by the number of cases each one would flip.

## 1. What was measured

| population | count |
|---|---:|
| aligned assertion lines (plain + configured) | 552,533 |
| RIGHT | 544,166 |
| WRONG (port printed a different type) | 7,379 |
| GAP (port printed `error`) | 988 |
| cases with a WRONG or GAP line | 1,517 |
| plain cases failing with **no** WRONG/GAP line (unaligned lines only) | 52 |
| cases in the triage (union of the two) | 1,569 |

The plain `checker_types` suite reads 8,236/9,538 at this head (`casequery
--list`: 1,302 FAIL). Of these, 1,250 have a WRONG/GAP row in the dump. The
other 52 fail only on lines the dump cannot show: the expression text did
not align, or a file has no assertions. A further 29 cases have both. That
set (`unaligned-lines`) is reported as its own cause.

A case flips only when **every** line in it is RIGHT. So the ranking column
is **cases solely blocked**: cases whose every non-RIGHT line has this one
cause. Lines and cases touched are reported beside it, but they do not rank
the table.

## 2. Method

The classifier and the report are kept with the data, so the table can be
regenerated at any head:

```bash
B=/tmp/box/base
TSR_VERDICT_EXPR=1 cargo run -q --release -p tsr-conformance --example verdictdump > $B/typesx.tsv
TSR_ANY_DUMP=1 cargo run -q --release -p tsr-conformance --example any_audit; cp target/any_lost_lines.tsv $B/anylost.tsv
cargo run -q --release -p tsr-conformance --example casequery -- checker_types --list > $B/ctlist.tsv
# unaligned_cases.txt: FAIL cases whose ctlist total != dump row count, plus FAIL cases absent from the dump
python3 docs/parity/notes/r5-typetriage/classify.py           # lines.tsv, cases.tsv, summary
python3 docs/parity/notes/r5-typetriage/report.py             # the cause table (section 4)
python3 docs/parity/notes/r5-typetriage/report.py --claims    # the lane rollup (section 3)
```

- **Lines.** Each line is classified by an ordered rule table over
  `(want, got, expression)` (`classify.py: classify`). The first rule that
  fires owns the line. The rules compare *normalised* strings. For example,
  if the two sides match after widening every literal, the cause is
  `literal-widened`. If they match after dropping qualifiers, it is
  `qualified-name-choice`. If they match after sorting union members at
  every depth, it is `union-order`. Every rule is a falsifiable equality, not
  a judgement.
- **`any` lines.** A line where the port printed `any` is attributed to its
  producer, the rule `any_audit` reports as having minted the `any` at that
  node. These producers are grouped into families (`ANY_FAMILIES`). Two of
  `any_audit`'s labels did not match their witnesses, and the families are
  renamed to what the witnesses show:
  - `shorthand ambient module <- ERROR` covers ordinary function
    declarations whose type answered error, so it is reported as
    `any:function-declaration-error`.
  - `self-referential initialiser <- ERROR` covers ordinary initialisers
    that answered error, so it is reported as
    `any:variable-initialiser-error`.

  `any_audit` skips configured cases, so their `any` lines are grouped by
  expression shape (`any:configured:*`) and attributed by case below.
- **JavaScript files.** Any cause on a line from a `.js` file is prefixed
  `js:` and routed to the JSDoc/JS lane. Running the TypeScript-side rule
  on those lines would hide that the producer is a JSDoc path.
- **GAP lines.** These are split by expression shape (`gap:*`). They all
  belong to the errorType-producer work (`.944`/`.1038`).

The causes are **symptom families with an owning subsystem**. They are not
proven single roots. A case can carry several causes that share one root
(for example, an `any` call result followed by `any` property reads). Its
"cases solely blocked" count is then zero for every one of them, even
though one fix would flip it. The `dominant_cause` column of `cases.tsv`
(the cause holding most of the case's lines) is the better routing key for
those cases.

## 3. By lane

From `report.py --claims`. Each cause is routed to its lane, and a case
counts as solely blocked by a lane when all of its causes route there.


| lane | lines | cases touched | cases solely blocked |
|---|---:|---:|---:|
| UNCLAIMED | 1335 | 370 | 175 |
| r5-declared | 1251 | 387 | 155 |
| r5-jsdoc3 / main .5 (JS files) | 599 | 119 | 113 |
| mixed / follows another producer | 1475 | 426 | 108 |
| r5-errorsplit4 (.944/.1038) | 1073 | 304 | 93 |
| main .39 symbol-chain printer | 383 | 132 | 93 |
| r5-modexports remainder | 667 | 164 | 71 |
| r5-harness | 0 | 81 | 52 |
| main .9 | 556 | 198 | 42 |
| main .11 | 397 | 94 | 34 |
| r5-modules | 94 | 28 | 16 |
| r5-mapped4 | 147 | 46 | 12 |
| r5-tuples (Beads) | 100 | 37 | 10 |
| main .4 | 290 | 91 | 5 |

Reading it:
- **UNCLAIMED is the largest sole-blocking lane (175 cases).** Section 5 files
  it.
- **`r5-declared` is next (155).** Its alias-attachment causes alone are 69
  cases: `alias-port-kept` 30, `alias-native-kept` 20,
  `alias-name-native-reassigned` 19. That outranks its declared-reference
  `any` work (37).
- **The symbol-chain printer (main `.39`) solely blocks 93 cases.** That
  is the same order as all of `r5-errorsplit4`. Of the 93, 35 are
  `import("…")` qualifiers alone.
- **JS-file lines solely block 113 cases.** They need the JSDoc lane's own
  triage; this note does not split them further.

## 4. Cause table

`lines` = non-RIGHT lines with this cause; `cases touched` = cases holding at
least one; `cases solely blocked` = cases whose only cause this is. The
witness prefers a line from a solely-blocked case.

| cause | owner | claim | lines | cases touched | cases solely blocked | witness |
|---|---|---|---:|---:|---:|---|
| `unaligned-lines` | `types_producer` walker / case loading (lines not aligned, so not in the dump) | r5-harness (tsr-2zk.37/.46/.1017/.1041 lane) | 0 | 81 | 52 | `compiler/castOfYield`, `compiler/conflictMarkerTrivia3`, `compiler/deferredConditionalTypes2` |
| `any:type-reference-error` | `declared.rs` getTypeFromTypeNode answered error | r5-declared (.979/.1010/.1034) | 260 | 98 | 37 | `compiler/allowImportClausesToMergeWithTypes:2:1` want `zzz` got `any` |
| `import-qualifier-missing` | symbol-chain printer: `import("…").X` for a symbol not accessible at the print site | main tsr-2zk.39 (ADR-0044) | 177 | 56 | 35 | `compiler/constEnumPreserveEmitReexport:2:0` want `typeof import("./ConstEnum").MyConstEnum` got `typeof MyConstEnum` |
| `qualified-name-choice` | symbol-chain printer (getAccessibleSymbolChain) | main tsr-2zk.39 | 116 | 53 | 34 | `compiler/aliasBug:0:19` want `provide.Provide` got `foo.Provide` |
| `alias-port-kept` | alias attachment / reduction: port keeps an alias name native resolves through | r5-declared; main tsr-2zk.16.2 | 363 | 132 | 30 | `compiler/aliasInstantiationExpressionGenericIntersectionNoCrash1:0:4` want `{ new (): ErrImpl<U>; prototype: ErrImpl<any>; } & (() => U)` got `ErrAlias<U>` |
| `literal-widened` | literal widening / const contexts / reverse-mapped inference | main tsr-2zk.11 (widening), tsr-2zk.9 (inference) | 315 | 82 | 30 | `compiler/coAndContraVariantInferences6:0:16` want `{ value: "C"; }` got `{ value: string; }` |
| `gap:identifier` | errorType producers across the checker (the port answers `error`) | .944/.1038 r5-errorsplit4 (in_progress) | 402 | 157 | 29 | `compiler/asyncImportNestedYield:0:0` want `() => AsyncGenerator<string, void, string>` got `error` |
| `signature-differs` | mixed signature shape (return-type inference, parameter printing) | - | 415 | 145 | 26 | `compiler/bindingPatternCannotBeOnlyInferenceSource:0:40` want `<T extends IDestructuring<TFuncs1>>(destructuring: Destructuring<TFunc` got `<T extends IDestructuring<{ funcA: (a: boolean) => void; funcB: (b: st` |
| `undefined-optionality` | flow narrowing / optional-property `undefined` | UNCLAIMED (flow.rs is main's) | 178 | 51 | 26 | `compiler/contextuallyTypedOptionalProperty(exactoptionalpropertytypes=true):0:10` want `number` got `number \| undefined` |
| `any:configured:identifier` | configured cases (any_audit skips them): arbitraryModuleNamespaceIdentifiers_module, nodeModulesResolveJsonModule, nodeModulesJson dominate | r5-modexports .991/.992 for the top three; rest mixed | 349 | 84 | 24 | `compiler/accessorWithInitializer(target=es2015):0:2` want `number` got `any` |
| `different-name` | mixed naming: alias re-naming, enum, this, symbol chain | partly .39 / r5-declared | 148 | 71 | 21 | `compiler/aliasInstantiationExpressionGenericIntersectionNoCrash2:0:3` want `typeof Class<T>` got `ClassAlias<T>` |
| `type-param-rename` | printer typeParameterToName shadow renaming (`T_1`) | UNCLAIMED (printing.rs: r5-typetriage) | 86 | 27 | 21 | `compiler/chainedCallsWithTypeParameterConstrainedToOtherTypeParameter2:0:146` want `<S extends S_1>(cb: (x: S_1) => S) => Chain2<S>` got `<S extends S>(cb: (x: S) => S) => Chain2<S>` |
| `alias-native-kept` | alias attachment / union origin: native keeps a name the port expands | r5-declared; main tsr-2zk.16.2 | 192 | 66 | 20 | `compiler/commonJsImportClassExpression:0:0` want `typeof Chunk` got `typeof (Anonymous class)` |
| `typed-where-native-any` | errorType print (native prints any for errorType) | .944 r5-errorsplit4 | 159 | 51 | 20 | `compiler/classMemberInitializerScoping2(target=es2017,usedefineforclassfields=true):0:3` want `any` got `number` |
| `alias-name-native-reassigned` | `declared.rs` alias attachment: native names an instantiated alias by the declaring alias (`type Baz = Omit<…>` prints `Baz`) | r5-declared | 161 | 50 | 19 | `compiler/conditionalTypeGenericInSignatureTypeParameterConstraint:0:0` want `x` got `H_inline1<x>` |
| `object-members-differ` | object member tables (late-bound, quoted, accessor) | - | 258 | 64 | 17 | `compiler/circularObjectLiteralAccessors(target=es2015):0:0` want `{ b: { get foo(): string; set foo(value: string); }; foo: string; }` got `{ b: { foo: string; }; foo: string; }` |
| `import-specifier-differs` | module specifier generation (`module_specifiers.rs`) | r5-modules .989/.999 | 94 | 28 | 16 | `compiler/constEnumNoPreserveDeclarationReexport:2:0` want `typeof import("./ConstEnum").MyConstEnum` got `typeof import("./ConstEnum.d").MyConstEnum` |
| `any:alias-target` | alias resolution (`resolution.rs`/module exports) | main tsr-2zk.6; r5-modexports .991/.992 | 68 | 42 | 15 | `compiler/aliasDoesNotDuplicateSignatures:1:0` want `() => void` got `any` |
| `union-order` | union member ordering (type id order) | UNCLAIMED | 53 | 23 | 15 | `compiler/baseClassImprovedMismatchErrors:0:12` want `() => number \| string` got `() => string \| number` |
| `partial-any` | an `any` nested in a structured answer (inner producer) | - | 226 | 82 | 14 | `compiler/genericFunctionsAndConditionalInference:0:44` want `<F extends Target>(at: Ops<F>) => { lr: Result<F, LR<F, string, number` got `<F extends keyof Targets<any>>(at: Ops<F>) => { lr: Result<F, LR<F, st` |
| `any:function-declaration-error` | type of a function/method declaration answered error (`signatures.rs`/`symbols.rs`; any_audit's label says shorthand ambient module, the witnesses are ordinary functions) | UNCLAIMED | 65 | 34 | 14 | `compiler/arrayBindingPatternOmittedExpressions:0:8` want `([, a, , b, , , , s, , ,]?: string[]) => void` got `any` |
| `type-argument-differs` | instantiation / inference picks different type arguments | main tsr-2zk.9 | 209 | 70 | 12 | `compiler/awaitedTypeNoLib:0:16` want `Thenable<NotPromise<TResult>> \| NotPromise<TResult>` got `Thenable<NotPromise<TResult>>` |
| `any:call-error` | `calls.rs` resolveCall / chooseOverload answered error | main tsr-2zk.9 | 169 | 91 | 12 | `compiler/abstractClassInLocalScopeIsAbstract:0:6` want `A` got `any` |
| `unknown-where-native-typed` | inference: no candidates → unknown | main tsr-2zk.9 | 132 | 31 | 12 | `compiler/localTypeParameterInferencePriority:0:1` want `Schema` got `Record<string, unknown>` |
| `import-qualifier-extra` | symbol-chain printer: accessible symbol printed through `import("…")` | main tsr-2zk.39 | 54 | 25 | 11 | `compiler/jsxPartialSpread:0:7` want `typeof React` got `typeof import("react").React` |
| `this-type` | `this` typing in object literals / methods | UNCLAIMED | 47 | 16 | 11 | `compiler/checkingObjectWithThisInNamePositionNoCrash:0:6` want `{ doit(): { [x: number]: string; }; }` got `this` |
| `tuple-vs-array` | tuple from array binding pattern / array literal context | claude-cloud-r5-tuples (.16.79 etc., Beads in_progress) | 100 | 37 | 10 | `compiler/intersectionsAndOptionalProperties:0:33` want `[number]` got `number[]` |
| `string-escape-printing` | printer escaping (escapeString/escapeNonAsciiString) and name-as-written; scanner for octal/surrogates | UNCLAIMED (printing.rs part: r5-typetriage) | 48 | 12 | 10 | `compiler/enumWithUnicodeEscape1:0:1` want `(typeof E)["gold \u2730"]` got `(typeof E)["gold ✰"]` |
| `mapped-type-unresolved` | `mapped.rs` resolveMappedTypeMembers / print-from-parts | r5-mapped4 (.1033 family) | 82 | 25 | 9 | `compiler/declarationAssertionNodeNotReusedWhenTypeNotEquivalent1:0:16` want `{ prop1: "hello"; }` got `{ [Key in keyof T["_type"]]: Unwrap<T["_type"][Key]>; }` |
| `optional-param-undefined-not-printed` | signature printing of an instantiated optional parameter (`?: X \| undefined`) | UNCLAIMED | 74 | 21 | 9 | `compiler/arrayconcat:0:16` want `(compareFn?: ((a: IOptions, b: IOptions) => number) \| undefined) => IO` got `(compareFn?: (a: IOptions, b: IOptions) => number) => IOptions[]` |
| `type-param-constraint` | `declared.rs` circular type-parameter constraints / constraint printing | r5-declared | 63 | 20 | 9 | `compiler/declFileRestParametersOfFunctionAndFunctionType:0:10` want `<T extends { (...args: any): void; }>() => void` got `<T extends { (...args: any[]): void; }>() => void` |
| `any:configured:call` | configured cases (any_audit skips them): arbitraryModuleNamespaceIdentifiers_module, nodeModulesResolveJsonModule, nodeModulesJson dominate | r5-modexports .991/.992 for the top three; rest mixed | 58 | 20 | 9 | `conformance/dynamicImportDefer(module=commonjs):1:0` want `Promise<void>` got `any` |
| `any:function-expression-error` | `contextual.rs`/`function_types.rs` checkFunctionExpressionOrObjectLiteralMethod answered error | UNCLAIMED (tsr-2zk.14 has no lane) | 109 | 61 | 8 | `compiler/contextualSignatureInstantiation2:0:15` want `(x: U) => S` got `any` |
| `any:other` | mixed | - | 95 | 50 | 8 | `compiler/circularInlineMappedGenericTupleTypeNoCrash:0:13` want `{ bar: unknown; }` got `any` |
| `union-members-differ` | union construction / narrowing | - | 89 | 40 | 8 | `compiler/enumLiteralAssignableToEnumInsideUnion:0:42` want `boolean \| X.Foo` got `boolean \| X.Foo.A \| X.Foo.B` |
| `typeof-name-choice` | symbol-chain printer for `typeof` targets (alias site naming) | main tsr-2zk.39 | 36 | 11 | 8 | `compiler/es6ExportEqualsInterop:0:20` want `typeof z4` got `typeof y4` |
| `other` | unclassified | - | 134 | 41 | 7 | `compiler/expandoFunctionExpressionsWithDynamicNames2:0:19` want `{ (): void; test: true; }` got `() => void` |
| `optional-property-undefined-printed` | node reuse of optional property annotations | UNCLAIMED (node_reuse.rs) | 16 | 8 | 7 | `compiler/declarationEmitComputedPropertyNameEnum1:1:1` want `{ x?: { a: 0; }; }` got `{ x?: { a: 0; } \| undefined; }` |
| `js:different-name` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 23 | 12 | 6 | `compiler/jsdocResolveNameFailureInTypedef:0:1` want `CantResolveThis` got `Ty` |
| `any:property-lookup-error` | `members.rs` getPropertyOfType on the receiver | main tsr-2zk.4 | 290 | 91 | 5 | `compiler/declarationEmitExpressionInExtends:0:7` want `string` got `any` |
| `gap:function-expression` | errorType producers across the checker (the port answers `error`) | .944/.1038 r5-errorsplit4 (in_progress) | 88 | 58 | 5 | `compiler/contextuallyTypeGeneratorReturnTypeFromUnion:0:11` want `() => AsyncGenerator<string, string, string[]>` got `error` |
| `any:binding-element` | `destructure.rs`/`binding_patterns.rs` getTypeForBindingElement | UNCLAIMED (tsr-2zk.10 has no lane) | 57 | 19 | 5 | `compiler/arrayDestructuringInSwitch2:0:17` want `never` got `any` |
| `js:any:uncontextual-parameter` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 61 | 20 | 4 | `compiler/accessorDeclarationEmitJs:0:19` want `string` got `any` |
| `literal-mixed` | mixed literal text: union literal order, template text | - | 49 | 18 | 4 | `conformance/discriminatedUnionTypes4:0:103` want `{ type: `${AnimalType.dog}`; bark: string; }` got `{ type: "dog"; bark: string; }` |
| `typed-where-native-unknown` | inference / declared unknown | main tsr-2zk.9 | 46 | 15 | 4 | `compiler/builtinIterator:0:101` want `Iterator<number, undefined, unknown>` got `IteratorConstructor` |
| `js:literal-widened` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 38 | 8 | 4 | `compiler/checkJsdocTypeTagOnExportAssignment8:1:0` want `{ a: string; b: "b"; }` got `{ a: string; b: string; }` |
| `array-sugar` | printer `Array<T>` vs `T[]` (node reuse of written form) | UNCLAIMED (node_reuse.rs) | 36 | 7 | 4 | `compiler/importExportInternalComments:1:0` want `T[]` got `Array<T>` |
| `js:signature-differs` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 34 | 16 | 4 | `conformance/checkJsdocTypeTag7:0:4` want `(optional?: any) => void` got `() => void` |
| `never-where-native-typed` | narrowing / reduction to never | UNCLAIMED | 23 | 15 | 4 | `compiler/keyofObjectWithGlobalSymbolIncluded:0:7` want `unique symbol` got `never` |
| `binding-pattern-parameter-type` | binding-pattern implied type for parameters (tsr-2zk.16.47) | UNCLAIMED (.16.47 has no lane) | 14 | 8 | 4 | `compiler/restParameterWithBindingPattern2:0:0` want `(a: any, b: any) => void` got `(...[a, b]: any[]) => void` |
| `js:any:configured:call` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 4 | 4 | 4 | `compiler/ambientRequireFunction(module=commonjs):0:1` want `typeof fs` got `any` |
| `mapped-type-resolved-where-native-deferred` | `mapped.rs` generic mapped type kept deferred natively | r5-mapped4 | 65 | 23 | 3 | `compiler/declarationEmitNestedAnonymousMappedType:0:0` want `<const Members extends readonly string[]>() => { [Property in keyof { ` got `<const Members extends readonly string[]>() => Part2` |
| `union-extra-members` | union construction / subtype reduction | - | 37 | 14 | 3 | `conformance/constEnum3:0:3` want `"bar" \| "foo"` got `number \| "bar" \| "foo"` |
| `any:configured:object-literal` | configured cases (any_audit skips them): arbitraryModuleNamespaceIdentifiers_module, nodeModulesResolveJsonModule, nodeModulesJson dominate | r5-modexports .991/.992 for the top three; rest mixed | 20 | 8 | 3 | `compiler/jsxSpreadTag(target=es2015):0:19` want `{ wrong: any; }` got `any` |
| `signature-type-params-dropped` | signature printing drops the type-parameter list of an instantiated/aliased generic signature | UNCLAIMED | 19 | 9 | 3 | `compiler/contextuallyTypedGenericAssignment:0:7` want `<T extends { a: number; }>(t: T, u: number) => number` got `(t: T, u: number) => number` |
| `conditional-type-unresolved` | `declared.rs` conditional producers | r5-declared (.1034) | 17 | 10 | 3 | `compiler/circularTypeArgumentsLocalAndOuterNoCrash1:0:1` want `f.NumArray<any>` got `NumArray<X extends {} ? number : number>` |
| `js:any:unannotated-declaration` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 17 | 5 | 3 | `compiler/jsDeclarationsInheritedTypes:0:1` want `A` got `any` |
| `js:any:alias-target` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 12 | 5 | 3 | `compiler/checkJsdocTypeTagOnExportAssignment1:2:0` want `import("./a").Foo` got `any` |
| `enum-member-vs-enum` | enum literal vs enum printing / widening | UNCLAIMED | 9 | 6 | 3 | `compiler/isolatedDeclarationErrorsEnums:0:51` want `Flag` got `Flag.A` |
| `js:any:function-declaration-error` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 4 | 4 | 3 | `compiler/unreachableJavascriptChecked:0:0` want `() => void \| 2 \| 3 \| 4` got `any` |
| `any:configured:array-literal` | configured cases (any_audit skips them): arbitraryModuleNamespaceIdentifiers_module, nodeModulesResolveJsonModule, nodeModulesJson dominate | r5-modexports .991/.992 for the top three; rest mixed | 3 | 3 | 3 | `compiler/arrayIterationLibES5TargetDifferent(nolib=true,target=es2015):0:3` want `{}` got `any` |
| `any:configured:new` | configured cases (any_audit skips them): arbitraryModuleNamespaceIdentifiers_module, nodeModulesResolveJsonModule, nodeModulesJson dominate | r5-modexports .991/.992 for the top three; rest mixed | 3 | 3 | 3 | `conformance/privateNameComputedPropertyName3(target=es2015):0:31` want `error` got `any` |
| `gap:property-access` | errorType producers across the checker (the port answers `error`) | .944/.1038 r5-errorsplit4 (in_progress) | 157 | 65 | 2 | `compiler/newLexicalEnvironmentForConvertedLoop(target=es2015):0:26` want `never` got `error` |
| `any:literal-expression-error` | `objects.rs`/`array_literals.rs` checkObjectLiteral/checkArrayLiteral answered error | UNCLAIMED | 97 | 33 | 2 | `compiler/objectLitArrayDeclNoNew:0:10` want `{ tokens: any; endState: IState; }` got `any` |
| `module-object-default-shape` | synthetic default / module object (`{ default: … }`) | r5-modexports .992 | 71 | 17 | 2 | `compiler/augmentExportEquals7:1:0` want `{ default: () => void; }` got `() => void` |
| `literal-not-widened` | literal widening | main tsr-2zk.11 | 67 | 7 | 2 | `compiler/typeNamedUndefined1:0:9` want `(p: ns.undefined) => symbol` got `(p: ns.undefined) => unique symbol` |
| `gap:other-expression` | errorType producers across the checker (the port answers `error`) | .944/.1038 r5-errorsplit4 (in_progress) | 58 | 29 | 2 | `conformance/autoAccessor10:0:15` want `any` got `error` |
| `js:partial-any` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 41 | 15 | 2 | `conformance/jsDeclarationsTypeReassignmentFromDeclaration:1:0` want `Item[]` got `any[]` |
| `js:gap:identifier` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 24 | 14 | 2 | `compiler/declarationEmitClassSetAccessorParamNameInJs3:0:2` want `string` got `error` |
| `union-missing-members` | union construction / narrowing | - | 21 | 6 | 2 | `compiler/mappedTypeNotMistakenlyHomomorphic:0:26` want `"a" \| "v"` got `"v"` |
| `gap:array-literal` | errorType producers across the checker (the port answers `error`) | .944/.1038 r5-errorsplit4 (in_progress) | 20 | 14 | 2 | `compiler/controlFlowInstanceofWithSymbolHasInstance:0:67` want `<T>(this: T, value: unknown) => value is (T extends (abstract new (...` got `error` |
| `duplicate-type-parameters` | `declared.rs` type-parameter gathering keeps duplicate declarations (native appendIfUnique) | r5-declared | 16 | 3 | 2 | `compiler/duplicateTypeParameters1:0:0` want `<X>() => void` got `<X, X>() => void` |
| `conditional-type-resolved-where-native-deferred` | `declared.rs` conditional producers | r5-declared (.1034) | 14 | 8 | 2 | `compiler/recursiveConditionalCrash1:0:0` want `T extends string ? T extends infer T_1 ? T_1 extends T ? T_1 extends s` got `C1<T>` |
| `binding-pattern-print` | printer of binding-pattern parameter names | UNCLAIMED | 10 | 4 | 2 | `compiler/noImplicitAnyDestructuringInPrivateMethod:0:3` want `({ a, }: Arg) => number` got `({ a }: Arg) => number` |
| `js:import-qualifier-missing` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 9 | 3 | 2 | `compiler/importTypeResolutionJSDocEOF:1:0` want `import("./interfaces").Bar` got `Bar` |
| `js:any:variable-initialiser-error` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 5 | 3 | 2 | `compiler/controlFlowInstanceof:1:13` want `{}` got `any` |
| `js:union-extra-members` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 3 | 2 | 2 | `compiler/arrowExpressionBodyJSDoc:0:11` want `{}` got `T \| {}` |
| `gap:call` | errorType producers across the checker (the port answers `error`) | .944/.1038 r5-errorsplit4 (in_progress) | 141 | 73 | 1 | `compiler/tupleTypeInference2:0:18` want `unknown` got `error` |
| `any:variable-initialiser-error` | variable whose initialiser answered error (any_audit's label says reportCircularityError; witnesses are ordinary initialisers, so the producer is the initialiser's) | follows the initialiser's producer | 119 | 51 | 1 | `compiler/controlFlowNoImplicitAny:0:24` want `string \| number \| undefined` got `any` |
| `any:configured:property-access` | configured cases (any_audit skips them): arbitraryModuleNamespaceIdentifiers_module, nodeModulesResolveJsonModule, nodeModulesJson dominate | r5-modexports .991/.992 for the top three; rest mixed | 67 | 29 | 1 | `conformance/trailingCommasInFunctionParametersAndArguments(target=es2015):0:11` want `never` got `any` |
| `js:object-members-differ` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 38 | 4 | 1 | `compiler/declarationEmitObjectLiteralAccessorsJs1:0:6` want `{ get x(): string; set x(a: number); }` got `{ x: string; }` |
| `js:alias-port-kept` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 25 | 8 | 1 | `compiler/lateBoundAssignmentCandidateJS1:0:11` want `string \| null` got `null` |
| `parenthesization` | printer parenthesization | UNCLAIMED | 24 | 11 | 1 | `conformance/overrideInterfaceProperty:0:1` want `readonly (readonly [K, V])[] \| null \| undefined` got `readonly readonly [K, V][] \| null \| undefined` |
| `js:gap:call` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 22 | 5 | 1 | `conformance/inferingFromAny:1:13` want `any` got `error` |
| `js:type-argument-differs` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 20 | 11 | 1 | `conformance/extendsTag5:0:10` want `A<{ a: string; b: string; }>` got `A<{}>` |
| `any:unannotated-declaration` | `symbols.rs` unannotated declaration widening | main tsr-2zk.11 | 15 | 7 | 1 | `compiler/staticPrototypeProperty:0:3` want `C2` got `any` |
| `js:other` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 13 | 5 | 1 | `conformance/esmModuleExports2(esmoduleinterop=true):0:1` want `typeof import("./exporter.mjs", { with: { "resolution-mode": "import" ` got `typeof import("./exporter.mjs")` |
| `js:import-specifier-differs` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 10 | 2 | 1 | `conformance/jsDeclarationsExportAssignedClassExpressionAnonymous(target=es2015):0:0` want `typeof import(".")` got `typeof import("./index")` |
| `readonly-modifier` | readonly modifier printing | - | 9 | 3 | 1 | `compiler/duplicateObjectLiteralProperty(target=es2015):0:16` want `{ readonly a: number; }` got `{ a: number; }` |
| `js:tuple-vs-array` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 8 | 3 | 1 | `conformance/jsDeclarationsTypedefFunction:0:10` want `[(value: any) => void, (reason?: any) => void]` got `((reason?: any) => void)[]` |
| `gap:jsx-or-assertion` | errorType producers across the checker (the port answers `error`) | .944/.1038 r5-errorsplit4 (in_progress) | 7 | 4 | 1 | `conformance/inlineJsxAndJsxFragPragmaOverridesCompilerOptions:3:2` want `any` got `error` |
| `js:any:any-receiver-propagated` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 6 | 2 | 1 | `conformance/assertionTypePredicates2:0:4` want `number` got `any` |
| `js:gap:function-expression` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 4 | 4 | 1 | `conformance/instantiateTemplateTagTypeParameterOnVariableStatement:0:3` want `(b: T) => T` got `error` |
| `js:any:function-expression-error` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 3 | 2 | 1 | `conformance/asyncArrowFunction_allowJs:0:17` want `() => Promise<number>` got `any` |
| `js:any:literal-expression-error` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 3 | 1 | 1 | `compiler/selfReferentialDefaultNoStackOverflow:0:1` want `{ mixins: any[]; name: string; }` got `any` |
| `js:optional-param-undefined-not-printed` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 3 | 1 | 1 | `conformance/checkJsdocParamOnVariableDeclaredFunctionExpression:0:0` want `(n?: number \| undefined, s?: string) => void` got `(n?: number, s?: string) => void` |
| `js:gap:other-expression` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 2 | 2 | 1 | `conformance/privateIdentifierExpando:0:3` want `any` got `error` |
| `typedarray-buffer-arg` | TypedArray default type argument | - | 2 | 1 | 1 | `compiler/unionWithIndexSignature:0:11` want `(a: {}) => a is Int32Array \| Uint8Array` got `(a: {}) => a is Int32Array<ArrayBuffer> \| Uint8Array<ArrayBuffer>` |
| `js:alias-name-native-reassigned` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 1 | 1 | 1 | `conformance/jsdocTemplateTagNameResolution:0:4` want `number` got `Foo<{ a: number; }, "a">` |
| `js:array-sugar` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 1 | 1 | 1 | `compiler/jsdocTypeGenericInstantiationAttempt:0:0` want `(list: Array<any>) => any[]` got `(list: any[]) => any[]` |
| `any:contextual-parameter` | `contextual.rs` contextual parameter typing (getContextuallyTypedParameterType) | UNCLAIMED (tsr-2zk.14 has no lane) | 251 | 60 | 0 | `compiler/callbacksDontShareTypes:0:48` want `number` got `any` |
| `any:binary-operand` | `binary.rs` operand answered error | UNCLAIMED | 48 | 22 | 0 | `compiler/contextualTypingWithGenericAndNonGenericSignature:0:16` want `(x: any, y: any) => any` got `any` |
| `js:any:contextual-parameter` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 40 | 9 | 0 | `compiler/amdLikeInputDeclarationEmit:2:10` want `typeof import("deps/BaseClass")` got `any` |
| `gap:object-literal` | errorType producers across the checker (the port answers `error`) | .944/.1038 r5-errorsplit4 (in_progress) | 37 | 29 | 0 | `compiler/coAndContraVariantInferences3:0:134` want `{ 0: ([, modifiers, importClause, moduleSpecifier, assertClause, other` got `error` |
| `any:any-receiver-propagated` | downstream of an implicit-any parameter (see any:contextual-parameter) | UNCLAIMED (follows contextual) | 24 | 2 | 0 | `compiler/declarationsWithRecursiveInternalTypesProduceUniqueTypeParams:0:110` want `string` got `any` |
| `any:any-initialiser-propagated` | downstream of an `any` initialiser | follows the initialiser's producer | 21 | 10 | 0 | `compiler/controlFlowArrayErrors:0:18` want `any[]` got `any` |
| `any:configured:other-expression` | configured cases (any_audit skips them): arbitraryModuleNamespaceIdentifiers_module, nodeModulesResolveJsonModule, nodeModulesJson dominate | r5-modexports .991/.992 for the top three; rest mixed | 21 | 17 | 0 | `compiler/commentsOnObjectLiteral3(target=es2015):0:13` want `number` got `any` |
| `any:jsx` | `jsx_*.rs` | UNCLAIMED | 19 | 9 | 0 | `compiler/jsxComplexSignatureHasApplicabilityError:0:20` want `Option<ExtractValueType<WrappedProps>> \| ExtractValueType<WrappedProps` got `any` |
| `js:gap:property-access` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 18 | 8 | 0 | `compiler/declarationEmitLateBoundJSAssignments:0:10` want `string` got `error` |
| `alias-name-port-reassigned` | `declared.rs` alias attachment (inverse) | r5-declared | 17 | 6 | 0 | `compiler/callsOnComplexSignatures:0:248` want `React.ReactType<any>` got `React.ReactType` |
| `js:any:binary-operand` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 16 | 8 | 0 | `compiler/amdLikeInputDeclarationEmit:2:18` want `new () => { f: () => "something"; } & import("deps/BaseClass")` got `any` |
| `js:any:call-error` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 14 | 10 | 0 | `compiler/amdLikeInputDeclarationEmit:2:8` want `new () => { f: () => "something"; } & import("deps/BaseClass")` got `any` |
| `js:typed-where-native-any` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 13 | 6 | 0 | `compiler/jsFileMethodOverloads3:0:3` want `any` got `string \| number` |
| `js:any:configured:identifier` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 12 | 4 | 0 | `conformance/jsDeclarationsClasses(target=es2015):0:11` want `T & U` got `any` |
| `rest-tuple-params-not-expanded` | signature printing of rest tuple parameters | UNCLAIMED | 12 | 3 | 0 | `compiler/instantiateContextualTypes:0:209` want `{ foo(bar: string): void; }` got `{ foo(...args: [bar: string]): void; }` |
| `noinfer-not-substituted` | NoInfer substitution | UNCLAIMED | 11 | 2 | 0 | `compiler/contextuallyTypedJsxChildren2:0:33` want `{ foo: number; }` got `NoInfer<{ foo: number; }>` |
| `js:any:property-lookup-error` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 8 | 4 | 0 | `compiler/amdLikeInputDeclarationEmit:2:11` want `<A>(a: A) => new () => A & import("deps/BaseClass")` got `any` |
| `js:union-missing-members` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 8 | 1 | 0 | `conformance/jsdocTemplateTag6:0:108` want `readonly [1, "x"] \| readonly [2, "y"]` got `readonly [1, "x"]` |
| `any:configured:function-expression` | configured cases (any_audit skips them): arbitraryModuleNamespaceIdentifiers_module, nodeModulesResolveJsonModule, nodeModulesJson dominate | r5-modexports .991/.992 for the top three; rest mixed | 7 | 7 | 0 | `compiler/duplicateIdentifierBindingElementInParameterDeclaration1(target=es2015):0:34` want `(a: any) => number` got `any` |
| `js:any:other` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 7 | 3 | 0 | `compiler/ensureNoCrashExportAssignmentDefineProperrtyPotentialMerge:1:5` want `typeof Q` got `any` |
| `js:signature-type-params-dropped` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 6 | 3 | 0 | `conformance/typeTagNoErasure:0:1` want `<T1 extends number>(dibbity: T1) => T1` got `(dibbity: any) => any` |
| `js:type-param-constraint` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 5 | 2 | 0 | `conformance/jsdocTemplateTag3:0:0` want `<T extends { a: number; b: string; }, U, V extends { c: boolean; }, W,` got `<T, U, V, W, X>(t: T, u: U, v: V, w: W, x: X) => W \| X` |
| `any:uncontextual-parameter` | `symbols.rs`/`contextual.rs` isContextSensitive decision for the container | UNCLAIMED | 4 | 3 | 0 | `compiler/decoratorReferences:0:11` want `(...args: any[]) => any` got `any` |
| `js:union-members-differ` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 4 | 2 | 0 | `compiler/jsFileMethodOverloads3:0:0` want `{ (x: number): any; (x: string): any; }` got `(x: number) => string \| number` |
| `gap:class-expression` | errorType producers across the checker (the port answers `error`) | .944/.1038 r5-errorsplit4 (in_progress) | 3 | 3 | 0 | `compiler/anonClassDeclarationEmitIsAnon:0:10` want `{ new (...args: any[]): (Anonymous class); prototype: Timestamped.(Ano` got `error` |
| `js:alias-native-kept` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 3 | 1 | 0 | `conformance/returnTagTypeGuard:0:28` want `Entry` got `Entry \| Group` |
| `js:any:any-initialiser-propagated` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 2 | 2 | 0 | `conformance/typeFromPrivatePropertyAssignmentJs:1:4` want `{ foo?: string; }` got `any` |
| `js:gap:object-literal` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 2 | 1 | 0 | `conformance/jsDeclarationsComputedNames(target=es2015):0:10` want `{ [TopLevelSym](x?: number): number; items: { [InnerSym]: (arg?: { x: ` got `error` |
| `js:typed-where-native-unknown` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 2 | 1 | 0 | `compiler/contravariantOnlyInferenceFromAnnotatedFunctionJs:0:0` want `<A, B extends Record<string, unknown>>(fns: Funcs<A, B>) => [A, B]` got `<A, B>(fns: Funcs<A, B>) => [A, B]` |
| `gap:new` | errorType producers across the checker (the port answers `error`) | .944/.1038 r5-errorsplit4 (in_progress) | 1 | 1 | 0 | `compiler/acceptSymbolAsWeakType:0:61` want `FinalizationRegistry<unknown>` got `error` |
| `js:any:configured:other-expression` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 1 | 1 | 0 | `conformance/jsDeclarationsFunctionJSDoc(target=es2015):0:6` want `null` got `any` |
| `js:gap:array-literal` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 1 | 1 | 0 | `conformance/jsDeclarationsComputedNames(target=es2015):0:18` want `(arg?: { x: number; }) => number` got `error` |
| `js:gap:new` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 1 | 1 | 0 | `compiler/jsDeclarationEmitDoesNotRenameImport:2:13` want `import("./Test.js").default` got `error` |
| `js:this-type` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 1 | 1 | 0 | `compiler/jsFileMethodOverloads:0:7` want `{ (this: Example<number>): 'number'; (this: Example<string>): 'string'` got `() => 'number'` |
| `js:union-order` | JavaScript file: JSDoc / JS checking | r5-jsdoc3 (.1046), main tsr-2zk.5 | 1 | 1 | 0 | `conformance/returnTagTypeGuard:0:40` want `(val: boolean \| number) => void` got `(val: number \| boolean) => void` |
| `spacing` | printer spacing | UNCLAIMED | 1 | 1 | 0 | `compiler/stringLiteralsErrors:0:29` want `" "` got `""` |

## 5. Claims, and the unclaimed causes to file

**Already claimed.** These causes have an owner in `.beads/issues.jsonl`, or
are the territory of an active lane. Route new findings to the owner listed:

| cause(s) | claimed by |
|---|---|
| `gap:*`, `typed-where-native-any` | `.944`/`.1038` (r5-errorsplit4, Beads in_progress) |
| `unaligned-lines` | r5-harness (walker/alignment; `.37`, `.46`, `.1017`, `.1041`) |
| `js:*` | r5-jsdoc3 (`.1046`) and main `.5` |
| `import-qualifier-missing`, `import-qualifier-extra`, `qualified-name-choice`, `typeof-name-choice` | main `.39` (symbol-chain printer, ADR-0044); open Beads roots `.16.64`, `.16.159`, `.16.190` |
| `import-specifier-differs` | r5-modules (`.989`, `.999`) |
| `any:type-reference-error`, `alias-*`, `type-param-constraint`, `duplicate-type-parameters`, `conditional-type-*` | r5-declared (`.979`, `.1010`, `.1034`); TYPE-ALIAS cluster `.16.2` (main) and its open root `.16.56` |
| `mapped-type-*` | r5-mapped4 (`.1033` family) |
| `any:configured:*` in `arbitraryModuleNamespaceIdentifiers_module`, `nodeModulesResolveJsonModule`, `nodeModulesJson`; `module-object-default-shape` | r5-modexports (`.991`, `.992`) |
| `any:alias-target` | main `.6`; r5-modexports |
| `any:call-error`, `type-argument-differs`, `unknown-where-native-typed` | main `.9` |
| `literal-widened`, `literal-not-widened`, `any:unannotated-declaration` | main `.11` (widening). Reverse-mapped and const-context inference witnesses go to `.9` |
| `any:property-lookup-error` | main `.4` |
| `tuple-vs-array` | claude-cloud-r5-tuples (`.16.79` and siblings; Beads in_progress, not in the active lane list) |

**Unclaimed: filing lines for the integrator.** Each line gives the title,
the owner file, the counts (lines / cases touched / cases solely blocked)
and the witnesses. Where an *open, unassigned* Beads root already describes
the cause, it is named, and the integrator should claim it rather than file
a duplicate.

1. **flow: optional/nullable `undefined` kept or dropped where native narrows
   differently** — `flow.rs` (main-owned file). 178 / 51 / 26. Mixed:
   - exactOptionalPropertyTypes reads
     (`contextuallyTypedOptionalProperty(exactoptionalpropertytypes=true):0:10`:
     want `number`, got `number | undefined`);
   - destructuring guards (`destructuringTypeGuardFlow:0:45`: want `string`,
     got `string | null`);
   - noUncheckedIndexedAccess narrowing
     (`inKeywordNarrowingWithNoUncheckedIndexedAccess:0:23`).

   Open roots: `.949`, `.16.298`, `.16.359`. Needs a split by construct before
   it can be dispatched.
2. **printer: shadowed type-parameter renaming (`T_1`)** —
   `printing.rs` `TypeParameterNames`, i.e. `typeParameterToName`.
   86 / 27 / 21. Existing open roots: `.16.65` SHADOWED-TYPEPARAM-RENAME (19),
   `.16.102`, `.16.111`, `.16.363`. Witnesses:
   - `chainedCallsWithTypeParameterConstrainedToOtherTypeParameter2:0:146`
     (`<S extends S_1>(cb: (x: S_1) => S)`);
   - `computedPropertyNames33_ES6:0:8` (`<T_1>() => string`);
   - `subtypesOfUnion:0:32`.
3. **union member order (type-id creation order and origin order)**. The
   type is minted in a different order, or the union's origin is not kept.
   53 / 23 / 15. Open root `.16.143` ORIGIN-ENTRY-ORDER. Witnesses:
   - `baseClassImprovedMismatchErrors:0:12` (`number | string` vs
     `string | number`);
   - `typePredicatesOptionalChaining3:0:2`;
   - `mappedTypeIndexedAccess:0:13`.

   Diffuse: each witness has its own producer.
4. **function/method declaration type answers error** — `signatures.rs` /
   `symbols.rs` getTypeOfFuncClassEnumModule.
   65 / 34 / 14. Witnesses:
   - `arrayBindingPatternOmittedExpressions:0:8`;
   - `arguments:0:13` (`(args: typeof arguments) => void`);
   - `generatorReturnTypeInference*`.
5. **`this` type in object literals and methods** — `contextual.rs`
   getContextualThisParameterType / object-literal `this`. 47 / 16 / 11.
   Witnesses:
   - `thisTypeInObjectLiterals:0:24` (port prints `this`, native the
     literal's type);
   - `contextualThisType`;
   - `esDecorators-classDeclaration-outerThisReference(target=*)`
     (6 lines × 4 configurations).
6. **printer: string escaping and names as written**. 48 / 12 / 10. Open
   roots: `.16.233` ESCAPE-STRING-LINE-TERMINATORS and `.16.125`
   SYMBOL-NAME-AS-WRITTEN-SOURCE-TEXT. It has three parts with different
   owners:
   - **Literal escaping** (`printing.rs`, taken by this lane, section 6):
     ` `, ` ` and `\u0085` in string literal types. Cases:
     `allowUnescapedParagraphAndLineSeparatorsInStringLiteral`,
     `fileWithNextLine1`, `sourceMap-LineBreaks(target=es2015)`. Also
     non-ASCII text in element-access names: `enumWithUnicodeEscape1`.
   - **Names as written** (`escapedIdentifiers`, `parserClassDeclaration23`,
     `classType2`). The symbol-name-as-written path belongs with main
     `.39`.
   - **Scanner** (parser lane, not ownable here):
     - octal escapes in templates (`octalLiteralAndEscapeSequence`);
     - lone surrogates (`unicodeExtendedEscapesIn{Strings,Templates}1{0,1}`),
       which a Rust `String` cannot hold, so they need a scanner
       representation decision.
7. **signature printing of an instantiated optional parameter omits
   `| undefined`** — node builder / `signatures.rs`. 74 / 21 / 9. Open root
   `.16.60` PARAM-SERIALIZED-TYPE-IMPLICIT-UNDEFINED (22). Witnesses:
   - `arrayconcat:0:16` (`compareFn?: ((a, b) => number) | undefined`);
   - `interfaceAssignmentCompat:0:62`;
   - `genericCallToOverloadedMethodWithOverloadedArguments`.
8. **function expression type answers error** — `contextual.rs` /
   `function_types.rs`. 109 / 61 / 8. Open root `.16.70`
   FUNCEXPR-GROUNDED-GATE-OUTER-TYPEPARAMS. Witness:
   `contextualSignatureInstantiation2:0:15`.
9. **implicit-any parameter where native has a contextual type** —
   `contextual.rs` getContextuallyTypedParameterType. 251 / 60 / 0. It
   never blocks a case alone, but it co-occurs in 60, and it feeds the
   `any:any-receiver-propagated` lines. Open roots `.16.216`, `.16.290`.
10. **node reuse of optional property annotations prints `| undefined`** —
    `node_reuse.rs`. 16 / 8 / 7. Witness:
    `declarationEmitComputedPropertyNameEnum1:1:1` (`{ x?: { a: 0; } |
    undefined; }`). Open `tsr-8.2`.
11. **binding-element types answer error** — `destructure.rs` /
    `binding_patterns.rs`. 57 / 19 / 5. Binding-pattern parameter types:
    14 / 8 / 4 (open `.16.47`; witness
    `restParameterWithBindingPattern2:0:0`, `(...[a, b]: any[])` vs
    `(a: any, b: any)`).
12. **Small printer families.** Each needs a witness-level port:
    - `array-sugar` 36 / 7 / 4 (node reuse of `Array<T>`, open `.16.371`);
    - `signature-type-params-dropped` 19 / 9 / 3;
    - `enum-member-vs-enum` 9 / 6 / 3;
    - `binding-pattern-print` 10 / 4 / 2;
    - `parenthesization` 24 / 11 / 1;
    - `noinfer-not-substituted` 11 / 2 / 0 (open `.44` / `.16.167`).
13. **Object/array literal answers error** — `objects.rs` /
    `array_literals.rs`. 97 / 33 / 2.

Three rows are mixed buckets: `signature-differs` (420 / 148 / 26),
`object-members-differ` (262 / 66 / 18) and `other`. They are not filed as
causes. The witnesses printed in section 4 are their starting points.
Among the 26 cases `signature-differs` blocks alone:
- `functionsMissingReturnStatementsAndExpressions*`: return type
  `undefined` vs `void`;
- `declInput`/`declInput3`: `c: undefined` where native widens to `any`;
- `emitRestParametersFunctionProperty*`: rest `any` vs `any[]`;
- `multiExtendsSplitInterfaces1`, `arrowFunctionContexts`: intersection of
  identical signatures kept by native.

## 6. What this lane takes

The lane owns `spreads.rs` and the parts of `printing.rs` no one else owns.
Fixes, in order:
1. **The `spreads.rs` method form** (r5-modexports §5). The witness is
   `nodeModules{,AllowJs}SynchronousCallErrors`. The expected type
   `{ f(): Promise<void>; default: typeof mod2; }` also needs the
   import-site name `typeof mod2` for the module (printing site naming), so
   the fix is measured by lines, and the cases may not flip on it alone.
2. **Literal escaping** in string literal types (unclaimed item 6, the
   `printing.rs` part).
3. **Shadowed type-parameter renaming** (unclaimed item 2) if it fits in the
   remaining time. Otherwise it is left filed.

Results are recorded in section 8 as they land.

## 7. Limits of this triage, and how to tell it is wrong

- **The classes are string-level.** A rule fires on a normalised equality.
  That proves the *symptom*, not the producer. Two lines with the same slug
  can have different producers: `union-order` is diffuse by construction,
  and `partial-any` names only the outermost symptom. The falsifier is
  simple: pick a solely-blocked case and port the named producer. If the
  case does not flip, the cause was wrong. Correct the rule here, and say
  that it was corrected.
- **`any_audit`'s labels are last-step attributions.** Two were renamed in
  §2 because their witnesses contradict the label. The remaining
  `any:*` families name the node kind that answered error or `any`. The
  first native function that would compute the type is not proven by the
  probe.
- **Configured `any` lines carry no producer** (`any_audit` skips varied
  cases). They are attributed per case in §5. Three cases account for 248 of
  the 528 lines.
- **Unaligned lines are invisible to the dump.** 81 plain failing cases have
  some. Only the 52 that have nothing else are attributed to them. The
  configured suite has about 22 more (289 failing in the snapshot, 267 in
  the dump); `casequery` does not cover configured cases, so they are not
  listed individually.
- **"Cases solely blocked" understates shared roots** (§2). A routing
  decision should also read `dominant_cause` in `cases.tsv`.
